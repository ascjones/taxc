use super::events::{AssetClass, EventType, Tag, TaxableEvent};
use super::price::Price;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime};
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TransactionError {
    #[error("duplicate transaction id: {0}")]
    DuplicateTransactionId(String),
    #[error("linked transaction not found: {id} -> {linked_id}")]
    LinkedTransactionNotFound { id: String, linked_id: String },
    #[error("linked transaction type mismatch: {id} -> {linked_id}")]
    LinkedTransactionTypeMismatch { id: String, linked_id: String },
    #[error("linked transaction is not reciprocal: {id} -> {linked_id}")]
    LinkedTransactionNotReciprocal { id: String, linked_id: String },
    #[error("price required when neither side is GBP: {id}")]
    MissingTradePrice { id: String },
    #[error("price required for {tag} {tx_type}: {id}")]
    MissingTaggedPrice {
        id: String,
        tag: String,
        tx_type: String,
    },
    #[error("tagged deposit cannot have linked_withdrawal: {id}")]
    TaggedDepositLinked { id: String },
    #[error("tagged withdrawal cannot have linked_deposit: {id}")]
    TaggedWithdrawalLinked { id: String },
    #[error("airdrop deposit must not include price: {id}")]
    AirdropPriceNotAllowed { id: String },
    #[error("GBP {tag} deposit must not include price: {id}")]
    GbpIncomePriceNotAllowed { id: String, tag: String },
    #[error("price is not needed for GBP trades, value is derived from quantities: {id}")]
    GbpTradePriceNotAllowed { id: String },
    #[error("{tag} tag not allowed on {tx_type}: {id}")]
    InvalidTagForType {
        id: String,
        tag: String,
        tx_type: String,
    },
    #[error("price base '{base}' does not match expected asset '{expected}': {id}")]
    PriceBaseMismatch {
        id: String,
        base: String,
        expected: String,
    },
    #[error("fee price required for non-GBP fee asset: {asset}")]
    MissingFeePrice { asset: String },
    #[error("invalid price configuration: {0}")]
    InvalidPrice(String),
    #[error("invalid datetime: {0}")]
    InvalidDatetime(String),
    #[error("undefined asset symbol: {symbol}")]
    UndefinedAsset { symbol: String },
    #[error("duplicate asset symbol: {symbol}")]
    DuplicateAsset { symbol: String },
}

/// Input root for transaction JSON
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TransactionInput {
    pub assets: Vec<Asset>,
    pub transactions: Vec<Transaction>,
}

/// Transaction record with common fields + type-specific data
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Transaction {
    /// Unique identifier for this transaction
    pub id: String,
    /// When the transaction occurred (RFC3339 with offset; date-only assumes UTC)
    #[serde(deserialize_with = "deserialize_datetime")]
    #[schemars(with = "String")]
    pub datetime: DateTime<FixedOffset>,
    /// Account/wallet where this happened (e.g., "kraken", "ledger")
    pub account: String,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Optional price for valuation
    #[serde(default)]
    pub price: Option<Price>,
    /// Optional fee for this transaction
    #[serde(default)]
    pub fee: Option<Fee>,
    /// Optional transaction tag used for classification
    #[serde(default)]
    pub tag: Tag,
    /// The transaction details
    #[serde(flatten)]
    pub details: TransactionType,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum TransactionType {
    /// Trade one asset for another (includes fiat and crypto-to-crypto)
    Trade { sold: Amount, bought: Amount },

    /// Deposit - assets received INTO an account
    Deposit {
        amount: Amount,
        #[serde(default)]
        linked_withdrawal: Option<String>,
    },

    /// Withdrawal - assets sent FROM an account
    Withdrawal {
        amount: Amount,
        #[serde(default)]
        linked_deposit: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Asset {
    pub symbol: String,
    pub asset_class: AssetClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Amount {
    pub asset: String,
    #[schemars(with = "f64")]
    pub quantity: Decimal,
}

pub type AssetRegistry = HashMap<String, Asset>;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Fee {
    pub asset: String,
    #[schemars(with = "f64")]
    pub amount: Decimal,
    #[serde(default)]
    pub price: Option<Price>,
}

impl Fee {
    /// Compute fee in GBP, using transaction price if the fee asset matches the priced asset.
    ///
    /// If the fee has an explicit price, use it. Otherwise, if the fee asset
    /// matches the priced_asset and a price is provided, use that price.
    ///
    /// - For Trade: priced_asset is the bought asset, price is the trade price
    /// - For tagged Deposit/Withdrawal: priced_asset is the transaction asset, price is tx price
    /// - For Deposit/Withdrawal: no price available, fee must have explicit price or be GBP
    pub fn to_gbp_with_context(
        &self,
        priced_asset: Option<&str>,
        tx_price: Option<&Price>,
    ) -> Result<Decimal, TransactionError> {
        // GBP fees need no conversion
        if is_gbp(&self.asset) {
            return Ok(self.amount);
        }

        // Explicit fee price takes precedence
        if let Some(price) = &self.price {
            return price.to_gbp(self.amount);
        }

        // Use transaction price if fee asset matches the priced asset
        if let (Some(asset), Some(price)) = (priced_asset, tx_price) {
            let fee_asset_normalized = normalize_currency(&self.asset);
            if fee_asset_normalized == normalize_currency(asset) {
                return price.to_gbp(self.amount);
            }
        }

        // Fee asset doesn't match or no price available - require explicit price
        Err(TransactionError::MissingFeePrice {
            asset: self.asset.clone(),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConversionOptions {
    pub exclude_unlinked: bool,
}

/// Read transactions from JSON
pub fn read_transactions_json<R: Read>(
    reader: R,
) -> anyhow::Result<(Vec<Transaction>, AssetRegistry)> {
    let input: TransactionInput = serde_json::from_reader(reader)?;
    let mut assets = input.assets;
    let mut transactions = input.transactions;
    normalize_assets(&mut assets);
    normalize_transactions(&mut transactions);
    let registry = validate_assets(&assets, &transactions)?;
    transactions.sort_by_key(|t| t.datetime);
    Ok((transactions, registry))
}

/// Convert transactions to taxable events
pub fn transactions_to_events(
    transactions: &[Transaction],
    registry: &AssetRegistry,
    options: ConversionOptions,
) -> Result<Vec<TaxableEvent>, TransactionError> {
    validate_links(transactions)?;

    let mut events = Vec::new();

    for tx in transactions {
        let mut tx_events = tx.to_taxable_events(registry, options.exclude_unlinked)?;
        events.append(&mut tx_events);
    }

    events.sort_by_key(|e| e.datetime);
    for (idx, event) in events.iter_mut().enumerate() {
        event.id = idx + 1;
    }
    Ok(events)
}

impl Transaction {
    pub fn to_taxable_events(
        &self,
        registry: &AssetRegistry,
        exclude_unlinked: bool,
    ) -> Result<Vec<TaxableEvent>, TransactionError> {
        let Transaction {
            id,
            datetime,
            description,
            price,
            fee,
            tag,
            details,
            ..
        } = self;

        let mut event_index = 1usize;
        let mut next_event_id = || {
            let event_id = event_index;
            event_index += 1;
            event_id
        };

        match details {
            TransactionType::Trade { sold, bought } => {
                if !matches!(tag, Tag::Unclassified | Tag::Trade) {
                    return Err(TransactionError::InvalidTagForType {
                        id: id.clone(),
                        tag: tag_name(*tag).to_string(),
                        tx_type: "trade".to_string(),
                    });
                }

                let value_gbp = if is_gbp(&sold.asset) || is_gbp(&bought.asset) {
                    if price.is_some() {
                        return Err(TransactionError::GbpTradePriceNotAllowed { id: id.clone() });
                    }
                    if is_gbp(&sold.asset) {
                        sold.quantity
                    } else {
                        bought.quantity
                    }
                } else {
                    let p = price
                        .as_ref()
                        .ok_or_else(|| TransactionError::MissingTradePrice { id: id.clone() })?;
                    validate_price_base(id, p, &bought.asset)?;
                    p.to_gbp(bought.quantity)?
                };

                let mut events = Vec::new();

                let has_disposal = !is_gbp(&sold.asset);
                let has_acquisition = !is_gbp(&bought.asset);

                // Fee uses trade price if fee asset matches bought asset
                let fee_gbp = match fee {
                    Some(f) => Some(f.to_gbp_with_context(Some(&bought.asset), price.as_ref())?),
                    None => None,
                };

                if has_disposal {
                    events.push(TaxableEvent {
                        id: next_event_id(),
                        source_transaction_id: id.clone(),
                        event_type: EventType::Disposal,
                        tag: Tag::Trade,
                        datetime: *datetime,
                        asset: normalize_currency(&sold.asset),
                        asset_class: asset_class_for(registry, &sold.asset),
                        quantity: sold.quantity,
                        value_gbp,
                        fee_gbp,
                        description: description.clone(),
                    });
                }

                if has_acquisition {
                    let acquisition_fee = if !has_disposal { fee_gbp } else { None };
                    events.push(TaxableEvent {
                        id: next_event_id(),
                        source_transaction_id: id.clone(),
                        event_type: EventType::Acquisition,
                        tag: Tag::Trade,
                        datetime: *datetime,
                        asset: normalize_currency(&bought.asset),
                        asset_class: asset_class_for(registry, &bought.asset),
                        quantity: bought.quantity,
                        value_gbp,
                        fee_gbp: acquisition_fee,
                        description: description.clone(),
                    });
                }

                Ok(events)
            }

            TransactionType::Deposit {
                amount,
                linked_withdrawal,
            } => {
                if *tag != Tag::Unclassified {
                    if linked_withdrawal.is_some() {
                        return Err(TransactionError::TaggedDepositLinked { id: id.clone() });
                    }

                    let value_gbp = match tag {
                        Tag::Dividend | Tag::Interest if is_gbp(&amount.asset) => {
                            if price.is_some() {
                                return Err(TransactionError::GbpIncomePriceNotAllowed {
                                    id: id.clone(),
                                    tag: tag_name(*tag).to_string(),
                                });
                            }
                            amount.quantity
                        }
                        Tag::StakingReward
                        | Tag::Salary
                        | Tag::OtherIncome
                        | Tag::AirdropIncome
                        | Tag::Dividend
                        | Tag::Interest => {
                            let tx_price =
                                require_tagged_price(id, *tag, "deposit", price.as_ref())?;
                            validate_price_base(id, tx_price, &amount.asset)?;
                            tx_price.to_gbp(amount.quantity)?
                        }
                        Tag::Gift => {
                            let tx_price =
                                require_tagged_price(id, *tag, "deposit", price.as_ref())?;
                            validate_price_base(id, tx_price, &amount.asset)?;
                            tx_price.to_gbp(amount.quantity)?
                        }
                        Tag::Airdrop => {
                            if price.is_some() {
                                return Err(TransactionError::AirdropPriceNotAllowed {
                                    id: id.clone(),
                                });
                            }
                            Decimal::ZERO
                        }
                        Tag::Trade | Tag::Unclassified => {
                            return Err(TransactionError::InvalidTagForType {
                                id: id.clone(),
                                tag: tag_name(*tag).to_string(),
                                tx_type: "deposit".to_string(),
                            });
                        }
                    };

                    // For airdrops and GBP income, there is no price context for fee resolution.
                    let (priced_asset, tx_price) = if *tag == Tag::Airdrop || price.is_none() {
                        (None, None)
                    } else {
                        (Some(amount.asset.as_str()), price.as_ref())
                    };
                    let fee_gbp = match fee {
                        Some(f) => Some(f.to_gbp_with_context(priced_asset, tx_price)?),
                        None => None,
                    };

                    return Ok(vec![TaxableEvent {
                        id: next_event_id(),
                        source_transaction_id: id.clone(),
                        event_type: EventType::Acquisition,
                        tag: *tag,
                        datetime: *datetime,
                        asset: normalize_currency(&amount.asset),
                        asset_class: asset_class_for(registry, &amount.asset),
                        quantity: amount.quantity,
                        value_gbp,
                        fee_gbp,
                        description: description.clone(),
                    }]);
                }

                if linked_withdrawal.is_some() || is_gbp(&amount.asset) {
                    return Ok(vec![]);
                }

                if exclude_unlinked {
                    log::warn!(
                        "Skipping unlinked deposit: id={} asset={}",
                        id,
                        amount.asset
                    );
                    return Ok(vec![]);
                }

                let fee_gbp = match fee {
                    Some(f) => Some(f.to_gbp_with_context(
                        price.as_ref().map(|_| amount.asset.as_str()),
                        price.as_ref(),
                    )?),
                    None => None,
                };

                let value_gbp = match price {
                    Some(p) => {
                        validate_price_base(id, p, &amount.asset)?;
                        p.to_gbp(amount.quantity)?
                    }
                    None => Decimal::ZERO,
                };

                log::warn!(
                    "Unlinked deposit treated as acquisition: id={} asset={}",
                    id,
                    amount.asset
                );
                Ok(vec![TaxableEvent {
                    id: next_event_id(),
                    source_transaction_id: id.clone(),
                    event_type: EventType::Acquisition,
                    tag: Tag::Unclassified,
                    datetime: *datetime,
                    asset: normalize_currency(&amount.asset),
                    asset_class: asset_class_for(registry, &amount.asset),
                    quantity: amount.quantity,
                    value_gbp,
                    fee_gbp,
                    description: description.clone(),
                }])
            }

            TransactionType::Withdrawal {
                amount,
                linked_deposit,
            } => {
                if *tag != Tag::Unclassified {
                    if linked_deposit.is_some() {
                        return Err(TransactionError::TaggedWithdrawalLinked { id: id.clone() });
                    }

                    if *tag != Tag::Gift {
                        return Err(TransactionError::InvalidTagForType {
                            id: id.clone(),
                            tag: tag_name(*tag).to_string(),
                            tx_type: "withdrawal".to_string(),
                        });
                    }

                    let tx_price = require_tagged_price(id, *tag, "withdrawal", price.as_ref())?;
                    validate_price_base(id, tx_price, &amount.asset)?;
                    let fee_gbp = match fee {
                        Some(f) => {
                            Some(f.to_gbp_with_context(Some(&amount.asset), Some(tx_price))?)
                        }
                        None => None,
                    };

                    return Ok(vec![TaxableEvent {
                        id: next_event_id(),
                        source_transaction_id: id.clone(),
                        event_type: EventType::Disposal,
                        tag: Tag::Gift,
                        datetime: *datetime,
                        asset: normalize_currency(&amount.asset),
                        asset_class: asset_class_for(registry, &amount.asset),
                        quantity: amount.quantity,
                        value_gbp: tx_price.to_gbp(amount.quantity)?,
                        fee_gbp,
                        description: description.clone(),
                    }]);
                }

                if linked_deposit.is_some() || is_gbp(&amount.asset) {
                    return Ok(vec![]);
                }

                if exclude_unlinked {
                    log::warn!(
                        "Skipping unlinked withdrawal: id={} asset={}",
                        id,
                        amount.asset
                    );
                    return Ok(vec![]);
                }

                let fee_gbp = match fee {
                    Some(f) => Some(f.to_gbp_with_context(
                        price.as_ref().map(|_| amount.asset.as_str()),
                        price.as_ref(),
                    )?),
                    None => None,
                };

                let value_gbp = match price {
                    Some(p) => {
                        validate_price_base(id, p, &amount.asset)?;
                        p.to_gbp(amount.quantity)?
                    }
                    None => Decimal::ZERO,
                };

                log::warn!(
                    "Unlinked withdrawal treated as disposal: id={} asset={}",
                    id,
                    amount.asset
                );
                Ok(vec![TaxableEvent {
                    id: next_event_id(),
                    source_transaction_id: id.clone(),
                    event_type: EventType::Disposal,
                    tag: Tag::Unclassified,
                    datetime: *datetime,
                    asset: normalize_currency(&amount.asset),
                    asset_class: asset_class_for(registry, &amount.asset),
                    quantity: amount.quantity,
                    value_gbp,
                    fee_gbp,
                    description: description.clone(),
                }])
            }
        }
    }
}

fn tag_name(tag: Tag) -> &'static str {
    match tag {
        Tag::Unclassified => "Unclassified",
        Tag::Trade => "Trade",
        Tag::StakingReward => "StakingReward",
        Tag::Salary => "Salary",
        Tag::OtherIncome => "OtherIncome",
        Tag::Airdrop => "Airdrop",
        Tag::AirdropIncome => "AirdropIncome",
        Tag::Dividend => "Dividend",
        Tag::Interest => "Interest",
        Tag::Gift => "Gift",
    }
}

fn require_tagged_price<'a>(
    id: &str,
    tag: Tag,
    tx_type: &str,
    price: Option<&'a Price>,
) -> Result<&'a Price, TransactionError> {
    price.ok_or_else(|| TransactionError::MissingTaggedPrice {
        id: id.to_string(),
        tag: tag_name(tag).to_string(),
        tx_type: tx_type.to_string(),
    })
}

fn validate_price_base(
    id: &str,
    price: &Price,
    expected_asset: &str,
) -> Result<(), TransactionError> {
    let price_base = normalize_currency(&price.base);
    let expected = normalize_currency(expected_asset);
    if price_base != expected {
        return Err(TransactionError::PriceBaseMismatch {
            id: id.to_string(),
            base: price.base.clone(),
            expected: expected_asset.to_string(),
        });
    }
    Ok(())
}

fn asset_class_for(registry: &AssetRegistry, symbol: &str) -> AssetClass {
    if is_gbp(symbol) {
        return AssetClass::Fiat;
    }
    let normalized = normalize_currency(symbol);
    registry
        .get(normalized.as_str())
        .map(|asset| asset.asset_class.clone())
        .expect("asset validated")
}

fn validate_assets(
    assets: &[Asset],
    transactions: &[Transaction],
) -> Result<AssetRegistry, TransactionError> {
    let mut registry: AssetRegistry = HashMap::new();

    for asset in assets {
        if is_gbp(&asset.symbol) {
            continue;
        }
        if registry.contains_key(asset.symbol.as_str()) {
            return Err(TransactionError::DuplicateAsset {
                symbol: asset.symbol.clone(),
            });
        }
        registry.insert(asset.symbol.clone(), asset.clone());
    }

    for tx in transactions {
        match &tx.details {
            TransactionType::Trade { sold, bought } => {
                validate_symbol(&registry, sold.asset.as_str())?;
                validate_symbol(&registry, bought.asset.as_str())?;
            }
            TransactionType::Deposit { amount, .. }
            | TransactionType::Withdrawal { amount, .. } => {
                validate_symbol(&registry, amount.asset.as_str())?;
            }
        }

        if let Some(fee) = &tx.fee {
            validate_symbol(&registry, fee.asset.as_str())?;
            if let Some(price) = &fee.price {
                validate_symbol(&registry, price.base.as_str())?;
            }
        }

        if let Some(price) = &tx.price {
            validate_symbol(&registry, price.base.as_str())?;
        }
    }

    Ok(registry)
}

fn validate_symbol(registry: &AssetRegistry, symbol: &str) -> Result<(), TransactionError> {
    if is_gbp(symbol) || registry.contains_key(symbol) {
        return Ok(());
    }
    Err(TransactionError::UndefinedAsset {
        symbol: symbol.to_string(),
    })
}

fn validate_links(transactions: &[Transaction]) -> Result<(), TransactionError> {
    let mut seen = HashSet::new();
    let mut index: HashMap<&str, &Transaction> = HashMap::new();

    for tx in transactions {
        if !seen.insert(tx.id.clone()) {
            return Err(TransactionError::DuplicateTransactionId(tx.id.clone()));
        }
        index.insert(&tx.id, tx);
    }

    for tx in transactions {
        match &tx.details {
            TransactionType::Deposit {
                linked_withdrawal: Some(withdrawal_id),
                ..
            } if tx.tag == Tag::Unclassified => {
                let withdrawal = index.get(withdrawal_id.as_str()).ok_or_else(|| {
                    TransactionError::LinkedTransactionNotFound {
                        id: tx.id.clone(),
                        linked_id: withdrawal_id.clone(),
                    }
                })?;
                if !matches!(withdrawal.details, TransactionType::Withdrawal { .. }) {
                    return Err(TransactionError::LinkedTransactionTypeMismatch {
                        id: tx.id.clone(),
                        linked_id: withdrawal_id.clone(),
                    });
                }
                if !matches!(
                    withdrawal.details,
                    TransactionType::Withdrawal {
                        linked_deposit: Some(ref deposit_id),
                        ..
                    } if deposit_id == &tx.id
                ) {
                    return Err(TransactionError::LinkedTransactionNotReciprocal {
                        id: tx.id.clone(),
                        linked_id: withdrawal_id.clone(),
                    });
                }
            }
            TransactionType::Withdrawal {
                linked_deposit: Some(deposit_id),
                ..
            } if tx.tag == Tag::Unclassified => {
                let deposit = index.get(deposit_id.as_str()).ok_or_else(|| {
                    TransactionError::LinkedTransactionNotFound {
                        id: tx.id.clone(),
                        linked_id: deposit_id.clone(),
                    }
                })?;
                if !matches!(deposit.details, TransactionType::Deposit { .. }) {
                    return Err(TransactionError::LinkedTransactionTypeMismatch {
                        id: tx.id.clone(),
                        linked_id: deposit_id.clone(),
                    });
                }
                if !matches!(
                    deposit.details,
                    TransactionType::Deposit {
                        linked_withdrawal: Some(ref withdrawal_id),
                        ..
                    } if withdrawal_id == &tx.id
                ) {
                    return Err(TransactionError::LinkedTransactionNotReciprocal {
                        id: tx.id.clone(),
                        linked_id: deposit_id.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn normalize_transactions(transactions: &mut [Transaction]) {
    for tx in transactions {
        // Normalize price at transaction level
        if let Some(p) = tx.price.as_mut() {
            p.base = normalize_currency(&p.base);
            if let Some(quote) = p.quote.as_mut() {
                *quote = normalize_currency(quote);
            }
        }

        // Normalize fee at transaction level
        if let Some(f) = tx.fee.as_mut() {
            f.asset = normalize_currency(&f.asset);
            if let Some(fp) = f.price.as_mut() {
                fp.base = normalize_currency(&fp.base);
                if let Some(quote) = fp.quote.as_mut() {
                    *quote = normalize_currency(quote);
                }
            }
        }

        // Normalize type-specific fields
        match &mut tx.details {
            TransactionType::Trade { sold, bought } => {
                sold.asset = normalize_currency(&sold.asset);
                bought.asset = normalize_currency(&bought.asset);
            }
            TransactionType::Deposit { amount, .. } => {
                amount.asset = normalize_currency(&amount.asset);
            }
            TransactionType::Withdrawal { amount, .. } => {
                amount.asset = normalize_currency(&amount.asset);
            }
        }
    }
}

fn normalize_assets(assets: &mut [Asset]) {
    for asset in assets {
        asset.symbol = normalize_currency(&asset.symbol);
    }
}

fn normalize_currency(s: &str) -> String {
    s.trim().to_uppercase()
}

fn is_gbp(s: &str) -> bool {
    s.eq_ignore_ascii_case("GBP")
}

fn parse_datetime(s: &str) -> Result<DateTime<FixedOffset>, TransactionError> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(dt.and_utc().fixed_offset());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(dt.and_utc().fixed_offset());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Ok(dt.and_utc().fixed_offset());
    }
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(date.and_hms_opt(0, 0, 0).unwrap().and_utc().fixed_offset());
    }
    Err(TransactionError::InvalidDatetime(s.to_string()))
}

fn deserialize_datetime<'de, D>(deserializer: D) -> Result<DateTime<FixedOffset>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    parse_datetime(&s).map_err(|err| serde::de::Error::custom(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn dt(s: &str) -> DateTime<FixedOffset> {
        parse_datetime(s).unwrap()
    }

    /// Helper to create a direct GBP price
    fn gbp_price(base: &str, rate: Decimal) -> Price {
        Price {
            base: base.to_string(),
            rate,
            source: None,
            quote: None,
            fx_rate: None,
        }
    }

    /// Helper to create an FX price
    fn fx_price(base: &str, rate: Decimal, quote: &str, fx_rate: Decimal) -> Price {
        Price {
            base: base.to_string(),
            rate,
            source: None,
            quote: Some(quote.to_string()),
            fx_rate: Some(fx_rate),
        }
    }

    fn test_registry() -> AssetRegistry {
        let mut registry = AssetRegistry::new();
        for symbol in ["BTC", "ETH", "USDT"] {
            registry.insert(
                symbol.to_string(),
                Asset {
                    symbol: symbol.to_string(),
                    asset_class: AssetClass::Crypto,
                },
            );
        }
        registry.insert(
            "AAPL".to_string(),
            Asset {
                symbol: "AAPL".to_string(),
                asset_class: AssetClass::Stock,
            },
        );
        registry
    }

    #[test]
    fn price_gbp_multiplies_rate() {
        let price = gbp_price("BTC", dec!(2000));
        assert_eq!(price.to_gbp(dec!(0.5)).unwrap(), dec!(1000));
    }

    #[test]
    fn price_fx_chain_applies_fx() {
        let price = fx_price("BTC", dec!(40000), "USD", dec!(0.79));
        assert_eq!(price.to_gbp(dec!(0.5)).unwrap(), dec!(15800));
    }

    #[test]
    fn trade_crypto_to_crypto_generates_two_events() {
        let tx = Transaction {
            id: "tx-1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(fx_price("ETH", dec!(2000), "USD", dec!(0.79))),
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.01),
                },
                bought: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(0.5),
                },
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, EventType::Disposal);
        assert_eq!(events[1].event_type, EventType::Acquisition);
        assert_eq!(events[0].value_gbp, events[1].value_gbp);
    }

    #[test]
    fn transactions_to_events_assigns_sequential_event_ids() {
        let tx1 = Transaction {
            id: "tx-1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(fx_price("ETH", dec!(2000), "USD", dec!(0.79))),
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.01),
                },
                bought: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(0.5),
                },
            },
        };

        let tx2 = Transaction {
            id: "tx-2".to_string(),
            datetime: dt("2024-01-02T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(2000))),
            fee: None,
            tag: Tag::StakingReward,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(0.01),
                },
                linked_withdrawal: None,
            },
        };

        let events = transactions_to_events(
            &[tx1, tx2],
            &test_registry(),
            ConversionOptions {
                exclude_unlinked: false,
            },
        )
        .unwrap();

        let ids: Vec<usize> = events.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn trade_gbp_to_crypto_only_acquisition() {
        let tx = Transaction {
            id: "tx-2".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "GBP".to_string(),
                    quantity: dec!(1000),
                },
                bought: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.02),
                },
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Acquisition);
        assert_eq!(events[0].value_gbp, dec!(1000));
    }

    #[test]
    fn trade_crypto_to_gbp_only_disposal() {
        let tx = Transaction {
            id: "tx-3".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.02),
                },
                bought: Amount {
                    asset: "GBP".to_string(),
                    quantity: dec!(1000),
                },
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Disposal);
        assert_eq!(events[0].value_gbp, dec!(1000));
    }

    #[test]
    fn trade_without_price_no_gbp_errors() {
        let tx = Transaction {
            id: "tx-4".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.02),
                },
                bought: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(0.5),
                },
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::MissingTradePrice {
                id: "tx-4".to_string()
            }
        );
    }

    #[test]
    fn linked_deposit_withdrawal_no_events() {
        let deposit = Transaction {
            id: "d1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                linked_withdrawal: Some("w1".to_string()),
            },
        };
        let withdrawal = Transaction {
            id: "w1".to_string(),
            datetime: dt("2024-01-01T09:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Withdrawal {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                linked_deposit: Some("d1".to_string()),
            },
        };

        let events = transactions_to_events(
            &[deposit, withdrawal],
            &test_registry(),
            ConversionOptions {
                exclude_unlinked: false,
            },
        )
        .unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn unlinked_crypto_deposit_warns_and_creates_acquisition() {
        let deposit = Transaction {
            id: "d1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                linked_withdrawal: None,
            },
        };

        let events = transactions_to_events(
            &[deposit],
            &test_registry(),
            ConversionOptions {
                exclude_unlinked: false,
            },
        )
        .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Acquisition);
        assert_eq!(events[0].tag, Tag::Unclassified);
    }

    #[test]
    fn exclude_unlinked_flag_skips_events() {
        let withdrawal = Transaction {
            id: "w1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Withdrawal {
                amount: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(1),
                },
                linked_deposit: None,
            },
        };

        let events = transactions_to_events(
            &[withdrawal],
            &test_registry(),
            ConversionOptions {
                exclude_unlinked: true,
            },
        )
        .unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn staking_reward_generates_income_event() {
        let tx = Transaction {
            id: "s1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(2000))),
            fee: None,
            tag: Tag::StakingReward,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(0.01),
                },
                linked_withdrawal: None,
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Acquisition);
        assert_eq!(events[0].tag, Tag::StakingReward);
        assert_eq!(events[0].value_gbp, dec!(20));
    }

    #[test]
    fn fee_allocated_to_disposal() {
        let tx = Transaction {
            id: "t1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(1000))),
            fee: Some(Fee {
                asset: "GBP".to_string(),
                amount: dec!(5),
                price: None,
            }),
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(1),
                },
                bought: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(10),
                },
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].fee_gbp, Some(dec!(5)));
        assert_eq!(events[1].fee_gbp, None);
    }

    #[test]
    fn fee_uses_trade_price_when_asset_matches_bought() {
        // Trade: 1 ETH -> 0.05 BTC at £15000/BTC
        // Fee: 0.0001 BTC (no explicit price, but matches bought asset)
        // Fee value = 0.0001 * 15000 = £1.50
        let tx = Transaction {
            id: "t1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("BTC", dec!(15000))),
            fee: Some(Fee {
                asset: "BTC".to_string(),
                amount: dec!(0.0001),
                price: None,
            }),
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                bought: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.05),
                },
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 2);
        // Fee uses trade price directly: 0.0001 * 15000 = £1.50
        assert_eq!(events[0].fee_gbp, Some(dec!(1.50)));
    }

    #[test]
    fn fee_in_sold_asset_requires_explicit_price() {
        // Trade: 1 ETH -> 0.05 BTC at £15000/BTC
        // Fee: 0.01 ETH (no explicit price, doesn't match bought asset)
        // Should error - sold asset doesn't get automatic price
        let tx = Transaction {
            id: "t1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("BTC", dec!(15000))),
            fee: Some(Fee {
                asset: "ETH".to_string(),
                amount: dec!(0.01),
                price: None,
            }),
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                bought: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.05),
                },
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::MissingFeePrice {
                asset: "ETH".to_string()
            }
        );
    }

    #[test]
    fn fee_explicit_price_takes_precedence() {
        // Fee has explicit price even though asset matches traded asset
        let tx = Transaction {
            id: "t1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("BTC", dec!(15000))),
            fee: Some(Fee {
                asset: "BTC".to_string(),
                amount: dec!(0.0001),
                // Explicit price overrides trade price
                price: Some(gbp_price("BTC", dec!(20000))),
            }),
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                bought: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.05),
                },
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 2);
        // Fee should use explicit price: 0.0001 * 20000 = £2.00
        assert_eq!(events[0].fee_gbp, Some(dec!(2)));
    }

    #[test]
    fn fee_unrelated_asset_requires_price() {
        // Fee in USDT but trade is ETH/BTC - should error
        let tx = Transaction {
            id: "t1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("BTC", dec!(15000))),
            fee: Some(Fee {
                asset: "USDT".to_string(),
                amount: dec!(5),
                price: None,
            }),
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                bought: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.05),
                },
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::MissingFeePrice {
                asset: "USDT".to_string()
            }
        );
    }

    #[test]
    fn fee_asset_match_is_case_insensitive() {
        // Fee asset "btc" should match bought asset "BTC"
        let tx = Transaction {
            id: "t1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("BTC", dec!(15000))),
            fee: Some(Fee {
                asset: "btc".to_string(), // lowercase
                amount: dec!(0.0001),
                price: None,
            }),
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                bought: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.05),
                },
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].fee_gbp, Some(dec!(1.50)));
    }

    #[test]
    fn staking_reward_requires_price() {
        let tx = Transaction {
            id: "s1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::StakingReward,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(0.01),
                },
                linked_withdrawal: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::MissingTaggedPrice {
                id: "s1".to_string(),
                tag: "StakingReward".to_string(),
                tx_type: "deposit".to_string(),
            }
        );
    }

    #[test]
    fn income_deposit_with_mismatched_price_base_errors() {
        let tx = Transaction {
            id: "s1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: Some(gbp_price("BTC", dec!(2000))),
            fee: None,
            tag: Tag::StakingReward,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(0.01),
                },
                linked_withdrawal: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::PriceBaseMismatch {
                id: "s1".to_string(),
                base: "BTC".to_string(),
                expected: "ETH".to_string(),
            }
        );
    }

    #[test]
    fn tagged_deposit_with_linked_withdrawal_errors() {
        let tx = Transaction {
            id: "s1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(2000))),
            fee: None,
            tag: Tag::StakingReward,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(0.01),
                },
                linked_withdrawal: Some("w1".to_string()),
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::TaggedDepositLinked {
                id: "s1".to_string()
            }
        );
    }

    #[test]
    fn tagged_withdrawal_with_linked_deposit_errors() {
        let tx = Transaction {
            id: "w1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(2000))),
            fee: None,
            tag: Tag::Gift,
            details: TransactionType::Withdrawal {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(0.01),
                },
                linked_deposit: Some("d1".to_string()),
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::TaggedWithdrawalLinked {
                id: "w1".to_string()
            }
        );
    }

    #[test]
    fn income_tag_on_withdrawal_errors() {
        let tx = Transaction {
            id: "w1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(2000))),
            fee: None,
            tag: Tag::StakingReward,
            details: TransactionType::Withdrawal {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(0.01),
                },
                linked_deposit: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::InvalidTagForType {
                id: "w1".to_string(),
                tag: "StakingReward".to_string(),
                tx_type: "withdrawal".to_string(),
            }
        );
    }

    #[test]
    fn airdrop_tag_on_withdrawal_errors() {
        let tx = Transaction {
            id: "w1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(2000))),
            fee: None,
            tag: Tag::Airdrop,
            details: TransactionType::Withdrawal {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(0.01),
                },
                linked_deposit: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::InvalidTagForType {
                id: "w1".to_string(),
                tag: "Airdrop".to_string(),
                tx_type: "withdrawal".to_string(),
            }
        );
    }

    #[test]
    fn non_trade_tag_on_trade_errors() {
        let tx = Transaction {
            id: "t1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("BTC", dec!(2000))),
            fee: None,
            tag: Tag::StakingReward,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                bought: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.05),
                },
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::InvalidTagForType {
                id: "t1".to_string(),
                tag: "StakingReward".to_string(),
                tx_type: "trade".to_string(),
            }
        );
    }

    #[test]
    fn gift_deposit_missing_price_errors() {
        let tx = Transaction {
            id: "d1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Gift,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                linked_withdrawal: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::MissingTaggedPrice {
                id: "d1".to_string(),
                tag: "Gift".to_string(),
                tx_type: "deposit".to_string(),
            }
        );
    }

    #[test]
    fn gift_withdrawal_missing_price_errors() {
        let tx = Transaction {
            id: "w1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Gift,
            details: TransactionType::Withdrawal {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                linked_deposit: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::MissingTaggedPrice {
                id: "w1".to_string(),
                tag: "Gift".to_string(),
                tx_type: "withdrawal".to_string(),
            }
        );
    }

    #[test]
    fn trade_tag_on_deposit_errors() {
        let tx = Transaction {
            id: "d1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Trade,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                linked_withdrawal: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::InvalidTagForType {
                id: "d1".to_string(),
                tag: "Trade".to_string(),
                tx_type: "deposit".to_string(),
            }
        );
    }

    #[test]
    fn trade_tag_on_withdrawal_errors() {
        let tx = Transaction {
            id: "w1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Trade,
            details: TransactionType::Withdrawal {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                linked_deposit: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::InvalidTagForType {
                id: "w1".to_string(),
                tag: "Trade".to_string(),
                tx_type: "withdrawal".to_string(),
            }
        );
    }

    #[test]
    fn airdrop_deposit_with_price_errors() {
        let tx = Transaction {
            id: "d1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(1000))),
            fee: None,
            tag: Tag::Airdrop,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                linked_withdrawal: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::AirdropPriceNotAllowed {
                id: "d1".to_string(),
            }
        );
    }

    #[test]
    fn gift_deposit_creates_gift_in() {
        let tx = Transaction {
            id: "d1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(1000))),
            fee: None,
            tag: Tag::Gift,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(2),
                },
                linked_withdrawal: None,
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Acquisition);
        assert_eq!(events[0].tag, Tag::Gift);
        assert_eq!(events[0].value_gbp, dec!(2000));
    }

    #[test]
    fn gift_withdrawal_creates_gift_out() {
        let tx = Transaction {
            id: "w1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(1000))),
            fee: None,
            tag: Tag::Gift,
            details: TransactionType::Withdrawal {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(2),
                },
                linked_deposit: None,
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Disposal);
        assert_eq!(events[0].tag, Tag::Gift);
        assert_eq!(events[0].value_gbp, dec!(2000));
    }

    #[test]
    fn airdrop_deposit_creates_zero_cost_acquisition() {
        let tx = Transaction {
            id: "d1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Airdrop,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(2),
                },
                linked_withdrawal: None,
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tag, Tag::Airdrop);
        assert_eq!(events[0].value_gbp, Decimal::ZERO);
    }

    #[test]
    fn airdrop_income_deposit_requires_price_and_counts_as_income_tag() {
        let tx = Transaction {
            id: "d1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(1000))),
            fee: None,
            tag: Tag::AirdropIncome,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(2),
                },
                linked_withdrawal: None,
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tag, Tag::AirdropIncome);
        assert_eq!(events[0].value_gbp, dec!(2000));
    }

    #[test]
    fn salary_other_dividend_and_interest_deposits_are_supported() {
        let cases = [
            ("d1", Tag::Salary),
            ("d2", Tag::OtherIncome),
            ("d3", Tag::Dividend),
            ("d4", Tag::Interest),
        ];

        for (id, tag) in cases {
            let tx = Transaction {
                id: id.to_string(),
                datetime: dt("2024-01-01T10:00:00+00:00"),
                account: "ledger".to_string(),
                description: None,
                price: Some(gbp_price("ETH", dec!(1000))),
                fee: None,
                tag,
                details: TransactionType::Deposit {
                    amount: Amount {
                        asset: "ETH".to_string(),
                        quantity: dec!(1),
                    },
                    linked_withdrawal: None,
                },
            };

            let events = tx.to_taxable_events(&test_registry(), false).unwrap();
            assert_eq!(events[0].tag, tag);
        }
    }

    #[test]
    fn dividend_and_interest_tags_on_trade_error() {
        let cases = [(Tag::Dividend, "Dividend"), (Tag::Interest, "Interest")];

        for (tag, tag_name) in cases {
            let tx = Transaction {
                id: "t1".to_string(),
                datetime: dt("2024-01-01T10:00:00+00:00"),
                account: "kraken".to_string(),
                description: None,
                price: Some(gbp_price("BTC", dec!(2000))),
                fee: None,
                tag,
                details: TransactionType::Trade {
                    sold: Amount {
                        asset: "ETH".to_string(),
                        quantity: dec!(1),
                    },
                    bought: Amount {
                        asset: "BTC".to_string(),
                        quantity: dec!(0.05),
                    },
                },
            };

            let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
            assert_eq!(
                err,
                TransactionError::InvalidTagForType {
                    id: "t1".to_string(),
                    tag: tag_name.to_string(),
                    tx_type: "trade".to_string(),
                }
            );
        }
    }

    #[test]
    fn dividend_and_interest_tags_on_withdrawal_error() {
        let cases = [(Tag::Dividend, "Dividend"), (Tag::Interest, "Interest")];

        for (tag, tag_name) in cases {
            let tx = Transaction {
                id: "w1".to_string(),
                datetime: dt("2024-01-01T10:00:00+00:00"),
                account: "kraken".to_string(),
                description: None,
                price: Some(gbp_price("ETH", dec!(2000))),
                fee: None,
                tag,
                details: TransactionType::Withdrawal {
                    amount: Amount {
                        asset: "ETH".to_string(),
                        quantity: dec!(0.01),
                    },
                    linked_deposit: None,
                },
            };

            let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
            assert_eq!(
                err,
                TransactionError::InvalidTagForType {
                    id: "w1".to_string(),
                    tag: tag_name.to_string(),
                    tx_type: "withdrawal".to_string(),
                }
            );
        }
    }

    #[test]
    fn dividend_and_interest_deposits_require_price() {
        let cases = [(Tag::Dividend, "Dividend"), (Tag::Interest, "Interest")];

        for (tag, tag_name) in cases {
            let tx = Transaction {
                id: "d1".to_string(),
                datetime: dt("2024-01-01T10:00:00+00:00"),
                account: "ledger".to_string(),
                description: None,
                price: None,
                fee: None,
                tag,
                details: TransactionType::Deposit {
                    amount: Amount {
                        asset: "ETH".to_string(),
                        quantity: dec!(1),
                    },
                    linked_withdrawal: None,
                },
            };

            let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
            assert_eq!(
                err,
                TransactionError::MissingTaggedPrice {
                    id: "d1".to_string(),
                    tag: tag_name.to_string(),
                    tx_type: "deposit".to_string(),
                }
            );
        }
    }

    #[test]
    fn gbp_dividend_and_interest_deposits_no_price_needed() {
        let cases = [(Tag::Dividend, "Dividend"), (Tag::Interest, "Interest")];

        for (tag, _tag_name) in cases {
            let tx = Transaction {
                id: "d1".to_string(),
                datetime: dt("2024-01-01T10:00:00+00:00"),
                account: "bank".to_string(),
                description: None,
                price: None,
                fee: None,
                tag,
                details: TransactionType::Deposit {
                    amount: Amount {
                        asset: "GBP".to_string(),
                        quantity: dec!(500),
                    },
                    linked_withdrawal: None,
                },
            };

            let events = tx.to_taxable_events(&test_registry(), false).unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].value_gbp, dec!(500));
            assert_eq!(events[0].asset_class, AssetClass::Fiat);
        }
    }

    #[test]
    fn gbp_dividend_and_interest_deposits_reject_price() {
        let cases = [(Tag::Dividend, "Dividend"), (Tag::Interest, "Interest")];

        for (tag, tag_name) in cases {
            let tx = Transaction {
                id: "d1".to_string(),
                datetime: dt("2024-01-01T10:00:00+00:00"),
                account: "bank".to_string(),
                description: None,
                price: Some(gbp_price("GBP", dec!(1))),
                fee: None,
                tag,
                details: TransactionType::Deposit {
                    amount: Amount {
                        asset: "GBP".to_string(),
                        quantity: dec!(500),
                    },
                    linked_withdrawal: None,
                },
            };

            let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
            assert_eq!(
                err,
                TransactionError::GbpIncomePriceNotAllowed {
                    id: "d1".to_string(),
                    tag: tag_name.to_string(),
                }
            );
        }
    }

    #[test]
    fn trade_sell_to_gbp_rejects_price() {
        let tx = Transaction {
            id: "t1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "broker".to_string(),
            description: None,
            price: Some(gbp_price("AAPL", dec!(150))),
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "AAPL".to_string(),
                    quantity: dec!(10),
                },
                bought: Amount {
                    asset: "GBP".to_string(),
                    quantity: dec!(1500),
                },
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::GbpTradePriceNotAllowed {
                id: "t1".to_string()
            }
        );
    }

    #[test]
    fn trade_buy_from_gbp_rejects_price() {
        let tx = Transaction {
            id: "t2".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "broker".to_string(),
            description: None,
            price: Some(gbp_price("AAPL", dec!(150))),
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "GBP".to_string(),
                    quantity: dec!(1500),
                },
                bought: Amount {
                    asset: "AAPL".to_string(),
                    quantity: dec!(10),
                },
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::GbpTradePriceNotAllowed {
                id: "t2".to_string()
            }
        );
    }

    #[test]
    fn price_base_must_match_bought_asset() {
        let tx = Transaction {
            id: "t1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(2000))), // Wrong base - should be BTC
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                bought: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.05),
                },
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::PriceBaseMismatch {
                id: "t1".to_string(),
                base: "ETH".to_string(),
                expected: "BTC".to_string(),
            }
        );
    }

    #[test]
    fn validate_assets_detects_undefined_symbol() {
        let json = r#"{
          "assets": [{ "symbol": "BTC", "asset_class": "Crypto" }],
          "transactions": [
            {
              "id": "tx-1",
              "datetime": "2024-01-01T00:00:00+00:00",
              "account": "kraken",
              "type": "Trade",
              "sold": { "asset": "ETH", "quantity": 1.0 },
              "bought": { "asset": "BTC", "quantity": 0.05 },
              "price": { "base": "BTC", "rate": 1000 }
            }
          ]
        }"#;

        let err = read_transactions_json(std::io::Cursor::new(json)).unwrap_err();
        assert_eq!(
            err.downcast_ref::<TransactionError>(),
            Some(&TransactionError::UndefinedAsset {
                symbol: "ETH".to_string()
            })
        );
    }

    #[test]
    fn validate_assets_detects_duplicate_symbol() {
        let json = r#"{
          "assets": [{ "symbol": "BTC", "asset_class": "Crypto" }, { "symbol": "BTC", "asset_class": "Crypto" }],
          "transactions": []
        }"#;

        let err = read_transactions_json(std::io::Cursor::new(json)).unwrap_err();
        assert_eq!(
            err.downcast_ref::<TransactionError>(),
            Some(&TransactionError::DuplicateAsset {
                symbol: "BTC".to_string()
            })
        );
    }

    #[test]
    fn validate_assets_gbp_implicit() {
        let json = r#"{
          "assets": [{ "symbol": "BTC", "asset_class": "Crypto" }],
          "transactions": [
            {
              "id": "tx-1",
              "datetime": "2024-01-01T00:00:00+00:00",
              "account": "kraken",
              "type": "Trade",
              "sold": { "asset": "GBP", "quantity": 1000 },
              "bought": { "asset": "BTC", "quantity": 0.05 }
            }
          ]
        }"#;

        assert!(read_transactions_json(std::io::Cursor::new(json)).is_ok());
    }

    #[test]
    fn validate_assets_gbp_in_assets_list_allowed() {
        let json = r#"{
          "assets": [{ "symbol": "gbp", "asset_class": "Stock" }, { "symbol": "BTC", "asset_class": "Crypto" }],
          "transactions": [
            {
              "id": "tx-1",
              "datetime": "2024-01-01T00:00:00+00:00",
              "account": "kraken",
              "type": "Trade",
              "sold": { "asset": "GBP", "quantity": 1000 },
              "bought": { "asset": "BTC", "quantity": 0.05 }
            }
          ]
        }"#;

        assert!(read_transactions_json(std::io::Cursor::new(json)).is_ok());
    }

    #[test]
    fn validate_assets_case_insensitive_duplicate() {
        let json = r#"{
          "assets": [{ "symbol": "btc", "asset_class": "Crypto" }, { "symbol": "BTC", "asset_class": "Crypto" }],
          "transactions": []
        }"#;

        let err = read_transactions_json(std::io::Cursor::new(json)).unwrap_err();
        assert_eq!(
            err.downcast_ref::<TransactionError>(),
            Some(&TransactionError::DuplicateAsset {
                symbol: "BTC".to_string()
            })
        );
    }

    #[test]
    fn validate_assets_checks_fee_and_price_symbols() {
        let invalid_fee_json = r#"{
          "assets": [{ "symbol": "BTC", "asset_class": "Crypto" }],
          "transactions": [
            {
              "id": "tx-1",
              "datetime": "2024-01-01T00:00:00+00:00",
              "account": "kraken",
              "type": "Trade",
              "sold": { "asset": "GBP", "quantity": 1000 },
              "bought": { "asset": "BTC", "quantity": 0.05 },
              "fee": { "asset": "ETH", "amount": 0.001 }
            }
          ]
        }"#;
        let err = read_transactions_json(std::io::Cursor::new(invalid_fee_json)).unwrap_err();
        assert_eq!(
            err.downcast_ref::<TransactionError>(),
            Some(&TransactionError::UndefinedAsset {
                symbol: "ETH".to_string()
            })
        );

        let invalid_price_json = r#"{
          "assets": [{ "symbol": "BTC", "asset_class": "Crypto" }],
          "transactions": [
            {
              "id": "tx-1",
              "datetime": "2024-01-01T00:00:00+00:00",
              "account": "kraken",
              "type": "Trade",
              "sold": { "asset": "GBP", "quantity": 1000 },
              "bought": { "asset": "BTC", "quantity": 0.05 },
              "price": { "base": "ETH", "rate": 2000 }
            }
          ]
        }"#;
        let err = read_transactions_json(std::io::Cursor::new(invalid_price_json)).unwrap_err();
        assert_eq!(
            err.downcast_ref::<TransactionError>(),
            Some(&TransactionError::UndefinedAsset {
                symbol: "ETH".to_string()
            })
        );
    }

    #[test]
    fn validate_assets_missing_field_errors() {
        let json = r#"{
          "transactions": []
        }"#;

        let err = read_transactions_json(std::io::Cursor::new(json)).unwrap_err();
        assert!(err.to_string().contains("missing field `assets`"));
    }

    #[test]
    fn validate_assets_empty_with_non_gbp_errors() {
        let json = r#"{
          "assets": [],
          "transactions": [
            {
              "id": "tx-1",
              "datetime": "2024-01-01T00:00:00+00:00",
              "account": "kraken",
              "type": "Trade",
              "sold": { "asset": "BTC", "quantity": 1.0 },
              "bought": { "asset": "GBP", "quantity": 1000.0 }
            }
          ]
        }"#;

        let err = read_transactions_json(std::io::Cursor::new(json)).unwrap_err();
        assert_eq!(
            err.downcast_ref::<TransactionError>(),
            Some(&TransactionError::UndefinedAsset {
                symbol: "BTC".to_string()
            })
        );
    }

    #[test]
    fn stock_asset_class_from_registry() {
        let tx = Transaction {
            id: "tx-1".to_string(),
            datetime: dt("2024-01-01T00:00:00+00:00"),
            account: "broker".to_string(),
            description: None,
            price: None,
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "GBP".to_string(),
                    quantity: dec!(1000),
                },
                bought: Amount {
                    asset: "AAPL".to_string(),
                    quantity: dec!(10),
                },
            },
        };

        let mut registry = AssetRegistry::new();
        registry.insert(
            "AAPL".to_string(),
            Asset {
                symbol: "AAPL".to_string(),
                asset_class: AssetClass::Stock,
            },
        );
        let events = tx.to_taxable_events(&registry, false).unwrap();
        assert_eq!(events[0].asset_class, AssetClass::Stock);
    }

    #[test]
    fn unclassified_deposit_price_base_mismatch_errors() {
        let tx = Transaction {
            id: "d1".to_string(),
            datetime: dt("2024-01-01T00:00:00+00:00"),
            account: "wallet".to_string(),
            description: None,
            price: Some(gbp_price("BTC", dec!(1000))),
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                linked_withdrawal: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::PriceBaseMismatch {
                id: "d1".to_string(),
                base: "BTC".to_string(),
                expected: "ETH".to_string(),
            }
        );
    }

    #[test]
    fn unclassified_withdrawal_price_base_mismatch_errors() {
        let tx = Transaction {
            id: "w1".to_string(),
            datetime: dt("2024-01-01T00:00:00+00:00"),
            account: "wallet".to_string(),
            description: None,
            price: Some(gbp_price("BTC", dec!(1000))),
            fee: None,
            tag: Tag::Unclassified,
            details: TransactionType::Withdrawal {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                linked_deposit: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::PriceBaseMismatch {
                id: "w1".to_string(),
                base: "BTC".to_string(),
                expected: "ETH".to_string(),
            }
        );
    }
}
