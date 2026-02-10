use super::events::{AssetClass, EventType, Label, TaxableEvent};
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
    #[error("price required for staking reward: {id}")]
    MissingStakingPrice { id: String },
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
}

/// Input root for transaction JSON
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TransactionInput {
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
    /// Optional price for valuation (required for crypto-to-crypto trades and staking rewards)
    #[serde(default)]
    pub price: Option<Price>,
    /// Optional fee for this transaction
    #[serde(default)]
    pub fee: Option<Fee>,
    /// The transaction details
    #[serde(flatten)]
    pub details: TransactionType,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum TransactionType {
    /// Trade one asset for another (includes fiat and crypto-to-crypto)
    Trade { sold: Asset, bought: Asset },

    /// Deposit - assets received INTO an account
    Deposit {
        asset: Asset,
        #[serde(default)]
        linked_withdrawal: Option<String>,
    },

    /// Withdrawal - assets sent FROM an account
    Withdrawal {
        asset: Asset,
        #[serde(default)]
        linked_deposit: Option<String>,
    },

    /// Staking reward (income + acquisition)
    StakingReward { asset: Asset },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Asset {
    pub symbol: String,
    #[schemars(with = "f64")]
    pub quantity: Decimal,
    #[serde(default)]
    pub asset_class: AssetClass,
}

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
    /// - For StakingReward: priced_asset is the reward asset, price is the staking price
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
pub fn read_transactions_json<R: Read>(reader: R) -> anyhow::Result<Vec<Transaction>> {
    let input: TransactionInput = serde_json::from_reader(reader)?;
    let mut transactions = input.transactions;
    normalize_transactions(&mut transactions);
    transactions.sort_by_key(|t| t.datetime);
    Ok(transactions)
}

/// Convert transactions to taxable events
pub fn transactions_to_events(
    transactions: &[Transaction],
    options: ConversionOptions,
) -> Result<Vec<TaxableEvent>, TransactionError> {
    validate_links(transactions)?;

    let mut events = Vec::new();

    for tx in transactions {
        let mut tx_events = tx.to_taxable_events(options.exclude_unlinked)?;
        events.append(&mut tx_events);
    }

    events.sort_by_key(|e| e.datetime);
    Ok(events)
}

impl Transaction {
    pub fn to_taxable_events(
        &self,
        exclude_unlinked: bool,
    ) -> Result<Vec<TaxableEvent>, TransactionError> {
        let Transaction {
            id,
            datetime,
            description,
            price,
            fee,
            details,
            ..
        } = self;

        match details {
            TransactionType::Trade { sold, bought } => {
                // Validate price.base matches bought asset if price is provided
                if let Some(p) = price {
                    let price_base = normalize_currency(&p.base);
                    let bought_symbol = normalize_currency(&bought.symbol);
                    if price_base != bought_symbol {
                        return Err(TransactionError::PriceBaseMismatch {
                            id: id.clone(),
                            base: p.base.clone(),
                            expected: bought.symbol.clone(),
                        });
                    }
                }

                let value_gbp = match price {
                    Some(p) => p.to_gbp(bought.quantity)?,
                    None if is_gbp(&sold.symbol) => sold.quantity,
                    None if is_gbp(&bought.symbol) => bought.quantity,
                    None => return Err(TransactionError::MissingTradePrice { id: id.clone() }),
                };

                let mut events = Vec::new();

                let has_disposal = !is_gbp(&sold.symbol);
                let has_acquisition = !is_gbp(&bought.symbol);

                // Fee uses trade price if fee asset matches bought asset
                let fee_gbp = match fee {
                    Some(f) => Some(f.to_gbp_with_context(Some(&bought.symbol), price.as_ref())?),
                    None => None,
                };

                if has_disposal {
                    events.push(TaxableEvent {
                        id: Some(format!("{id}-disposal")),
                        event_type: EventType::Disposal,
                        label: Label::Trade,
                        datetime: *datetime,
                        asset: normalize_currency(&sold.symbol),
                        asset_class: sold.asset_class.clone(),
                        quantity: sold.quantity,
                        value_gbp,
                        fee_gbp,
                        description: description.clone(),
                    });
                }

                if has_acquisition {
                    let acquisition_fee = if !has_disposal { fee_gbp } else { None };
                    events.push(TaxableEvent {
                        id: Some(format!("{id}-acquisition")),
                        event_type: EventType::Acquisition,
                        label: Label::Trade,
                        datetime: *datetime,
                        asset: normalize_currency(&bought.symbol),
                        asset_class: bought.asset_class.clone(),
                        quantity: bought.quantity,
                        value_gbp,
                        fee_gbp: acquisition_fee,
                        description: description.clone(),
                    });
                }

                Ok(events)
            }

            TransactionType::Deposit {
                asset,
                linked_withdrawal,
            } => {
                if linked_withdrawal.is_some() || is_gbp(&asset.symbol) {
                    return Ok(vec![]);
                }

                if exclude_unlinked {
                    log::warn!(
                        "Skipping unlinked deposit: id={} asset={}",
                        id,
                        asset.symbol
                    );
                    return Ok(vec![]);
                }

                // Deposits can optionally have a price for valuation
                let fee_gbp = match fee {
                    Some(f) => Some(f.to_gbp_with_context(
                        price.as_ref().map(|p| p.base.as_str()),
                        price.as_ref(),
                    )?),
                    None => None,
                };

                let value_gbp = match price {
                    Some(p) => p.to_gbp(asset.quantity)?,
                    None => Decimal::ZERO,
                };

                log::warn!(
                    "Unlinked deposit treated as acquisition: id={} asset={}",
                    id,
                    asset.symbol
                );
                Ok(vec![TaxableEvent {
                    id: Some(id.clone()),
                    event_type: EventType::Acquisition,
                    label: Label::Unclassified,
                    datetime: *datetime,
                    asset: normalize_currency(&asset.symbol),
                    asset_class: asset.asset_class.clone(),
                    quantity: asset.quantity,
                    value_gbp,
                    fee_gbp,
                    description: description.clone(),
                }])
            }

            TransactionType::Withdrawal {
                asset,
                linked_deposit,
            } => {
                if linked_deposit.is_some() || is_gbp(&asset.symbol) {
                    return Ok(vec![]);
                }

                if exclude_unlinked {
                    log::warn!(
                        "Skipping unlinked withdrawal: id={} asset={}",
                        id,
                        asset.symbol
                    );
                    return Ok(vec![]);
                }

                // Withdrawals can optionally have a price for valuation
                let fee_gbp = match fee {
                    Some(f) => Some(f.to_gbp_with_context(
                        price.as_ref().map(|p| p.base.as_str()),
                        price.as_ref(),
                    )?),
                    None => None,
                };

                let value_gbp = match price {
                    Some(p) => p.to_gbp(asset.quantity)?,
                    None => Decimal::ZERO,
                };

                log::warn!(
                    "Unlinked withdrawal treated as disposal: id={} asset={}",
                    id,
                    asset.symbol
                );
                Ok(vec![TaxableEvent {
                    id: Some(id.clone()),
                    event_type: EventType::Disposal,
                    label: Label::Unclassified,
                    datetime: *datetime,
                    asset: normalize_currency(&asset.symbol),
                    asset_class: asset.asset_class.clone(),
                    quantity: asset.quantity,
                    value_gbp,
                    fee_gbp,
                    description: description.clone(),
                }])
            }

            TransactionType::StakingReward { asset } => {
                // Staking rewards require a price
                let price = price
                    .as_ref()
                    .ok_or_else(|| TransactionError::MissingStakingPrice { id: id.clone() })?;

                // Validate price.base matches asset
                let price_base = normalize_currency(&price.base);
                let asset_symbol = normalize_currency(&asset.symbol);
                if price_base != asset_symbol {
                    return Err(TransactionError::PriceBaseMismatch {
                        id: id.clone(),
                        base: price.base.clone(),
                        expected: asset.symbol.clone(),
                    });
                }

                // Fee uses staking price if fee asset matches reward asset
                let fee_gbp = match fee {
                    Some(f) => Some(f.to_gbp_with_context(Some(&asset.symbol), Some(price))?),
                    None => None,
                };

                Ok(vec![TaxableEvent {
                    id: Some(id.clone()),
                    event_type: EventType::Acquisition,
                    label: Label::StakingReward,
                    datetime: *datetime,
                    asset: normalize_currency(&asset.symbol),
                    asset_class: asset.asset_class.clone(),
                    quantity: asset.quantity,
                    value_gbp: price.to_gbp(asset.quantity)?,
                    fee_gbp,
                    description: description.clone(),
                }])
            }
        }
    }
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
            } => {
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
            } => {
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
                sold.symbol = normalize_currency(&sold.symbol);
                bought.symbol = normalize_currency(&bought.symbol);
            }
            TransactionType::Deposit { asset, .. } => {
                asset.symbol = normalize_currency(&asset.symbol);
            }
            TransactionType::Withdrawal { asset, .. } => {
                asset.symbol = normalize_currency(&asset.symbol);
            }
            TransactionType::StakingReward { asset } => {
                asset.symbol = normalize_currency(&asset.symbol);
            }
        }
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
            details: TransactionType::Trade {
                sold: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(0.01),
                    asset_class: AssetClass::Crypto,
                },
                bought: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(0.5),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let events = tx.to_taxable_events(false).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, EventType::Disposal);
        assert_eq!(events[1].event_type, EventType::Acquisition);
        assert_eq!(events[0].value_gbp, events[1].value_gbp);
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
            details: TransactionType::Trade {
                sold: Asset {
                    symbol: "GBP".to_string(),
                    quantity: dec!(1000),
                    asset_class: AssetClass::Crypto,
                },
                bought: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(0.02),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let events = tx.to_taxable_events(false).unwrap();
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
            details: TransactionType::Trade {
                sold: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(0.02),
                    asset_class: AssetClass::Crypto,
                },
                bought: Asset {
                    symbol: "GBP".to_string(),
                    quantity: dec!(1000),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let events = tx.to_taxable_events(false).unwrap();
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
            details: TransactionType::Trade {
                sold: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(0.02),
                    asset_class: AssetClass::Crypto,
                },
                bought: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(0.5),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let err = tx.to_taxable_events(false).unwrap_err();
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
            details: TransactionType::Deposit {
                asset: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
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
            details: TransactionType::Withdrawal {
                asset: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                linked_deposit: Some("d1".to_string()),
            },
        };

        let events = transactions_to_events(
            &[deposit, withdrawal],
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
            details: TransactionType::Deposit {
                asset: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                linked_withdrawal: None,
            },
        };

        let events = transactions_to_events(
            &[deposit],
            ConversionOptions {
                exclude_unlinked: false,
            },
        )
        .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Acquisition);
        assert_eq!(events[0].label, Label::Unclassified);
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
            details: TransactionType::Withdrawal {
                asset: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                linked_deposit: None,
            },
        };

        let events = transactions_to_events(
            &[withdrawal],
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
            details: TransactionType::StakingReward {
                asset: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(0.01),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let events = tx.to_taxable_events(false).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Acquisition);
        assert_eq!(events[0].label, Label::StakingReward);
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
            details: TransactionType::Trade {
                sold: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                bought: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(10),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let events = tx.to_taxable_events(false).unwrap();
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
            details: TransactionType::Trade {
                sold: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                bought: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(0.05),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let events = tx.to_taxable_events(false).unwrap();
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
            details: TransactionType::Trade {
                sold: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                bought: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(0.05),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let err = tx.to_taxable_events(false).unwrap_err();
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
            details: TransactionType::Trade {
                sold: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                bought: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(0.05),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let events = tx.to_taxable_events(false).unwrap();
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
            details: TransactionType::Trade {
                sold: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                bought: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(0.05),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let err = tx.to_taxable_events(false).unwrap_err();
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
            details: TransactionType::Trade {
                sold: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                bought: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(0.05),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let events = tx.to_taxable_events(false).unwrap();
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
            details: TransactionType::StakingReward {
                asset: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(0.01),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let err = tx.to_taxable_events(false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::MissingStakingPrice {
                id: "s1".to_string()
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
            details: TransactionType::Trade {
                sold: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                bought: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(0.05),
                    asset_class: AssetClass::Crypto,
                },
            },
        };

        let err = tx.to_taxable_events(false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::PriceBaseMismatch {
                id: "t1".to_string(),
                base: "ETH".to_string(),
                expected: "BTC".to_string(),
            }
        );
    }
}
