use crate::events::{AssetClass, EventType, TaxableEvent};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime};
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

/// Price - either direct GBP or foreign currency with FX conversion
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Price {
    /// Direct GBP price per unit
    /// value_gbp = quantity * rate
    Gbp {
        #[schemars(with = "f64")]
        rate: Decimal,
        #[serde(default)]
        source: Option<String>,
    },

    /// Foreign currency price with FX conversion to GBP
    /// value_gbp = quantity * rate * fx_rate
    FxChain {
        #[schemars(with = "f64")]
        rate: Decimal,
        quote: String,
        #[schemars(with = "f64")]
        fx_rate: Decimal,
        #[serde(default)]
        source: Option<String>,
    },
}

impl Price {
    pub fn to_gbp(&self, quantity: Decimal) -> Result<Decimal, TransactionError> {
        match self {
            Price::Gbp { rate, .. } => Ok(quantity * rate),
            Price::FxChain {
                rate,
                fx_rate,
                quote,
                ..
            } => {
                if quote.trim().is_empty() {
                    return Err(TransactionError::InvalidPrice(
                        "quote is required and cannot be empty".to_string(),
                    ));
                }
                Ok(quantity * rate * fx_rate)
            }
        }
    }
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
    /// The transaction details
    #[serde(flatten)]
    pub details: TransactionType,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum TransactionType {
    /// Trade one asset for another (includes fiat and crypto-to-crypto)
    Trade {
        sold: Asset,
        bought: Asset,
        #[serde(default)]
        price: Option<Price>,
        #[serde(default)]
        fee: Option<Fee>,
    },

    /// Deposit - assets received INTO an account
    Deposit {
        asset: Asset,
        #[serde(default)]
        linked_withdrawal: Option<String>,
        #[serde(default)]
        fee: Option<Fee>,
    },

    /// Withdrawal - assets sent FROM an account
    Withdrawal {
        asset: Asset,
        #[serde(default)]
        linked_deposit: Option<String>,
        #[serde(default)]
        fee: Option<Fee>,
    },

    /// Staking reward (income + acquisition)
    StakingReward { asset: Asset, price: Price },
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
    pub fn to_gbp(&self) -> Result<Decimal, TransactionError> {
        if is_gbp(&self.asset) {
            return Ok(self.amount);
        }
        match &self.price {
            Some(price) => price.to_gbp(self.amount),
            None => Err(TransactionError::MissingFeePrice {
                asset: self.asset.clone(),
            }),
        }
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
            details,
            ..
        } = self;

        match details {
            TransactionType::Trade {
                sold,
                bought,
                price,
                fee,
            } => {
                let value_gbp = match price {
                    Some(p) => p.to_gbp(bought.quantity)?,
                    None if is_gbp(&sold.symbol) => sold.quantity,
                    None if is_gbp(&bought.symbol) => bought.quantity,
                    None => return Err(TransactionError::MissingTradePrice { id: id.clone() }),
                };

                let mut events = Vec::new();

                let has_disposal = !is_gbp(&sold.symbol);
                let has_acquisition = !is_gbp(&bought.symbol);

                let fee_gbp = match fee {
                    Some(f) => Some(f.to_gbp()?),
                    None => None,
                };

                if has_disposal {
                    events.push(TaxableEvent {
                        id: Some(format!("{id}-disposal")),
                        event_type: EventType::Disposal,
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
                ..
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

                log::warn!(
                    "Unlinked deposit treated as acquisition: id={} asset={}",
                    id,
                    asset.symbol
                );
                Ok(vec![TaxableEvent {
                    id: Some(id.clone()),
                    event_type: EventType::UnclassifiedIn,
                    datetime: *datetime,
                    asset: normalize_currency(&asset.symbol),
                    asset_class: asset.asset_class.clone(),
                    quantity: asset.quantity,
                    value_gbp: Decimal::ZERO,
                    fee_gbp: None,
                    description: description.clone(),
                }])
            }

            TransactionType::Withdrawal {
                asset,
                linked_deposit,
                ..
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

                log::warn!(
                    "Unlinked withdrawal treated as disposal: id={} asset={}",
                    id,
                    asset.symbol
                );
                Ok(vec![TaxableEvent {
                    id: Some(id.clone()),
                    event_type: EventType::UnclassifiedOut,
                    datetime: *datetime,
                    asset: normalize_currency(&asset.symbol),
                    asset_class: asset.asset_class.clone(),
                    quantity: asset.quantity,
                    value_gbp: Decimal::ZERO,
                    fee_gbp: None,
                    description: description.clone(),
                }])
            }

            TransactionType::StakingReward { asset, price } => Ok(vec![TaxableEvent {
                id: Some(id.clone()),
                event_type: EventType::StakingReward,
                datetime: *datetime,
                asset: normalize_currency(&asset.symbol),
                asset_class: asset.asset_class.clone(),
                quantity: asset.quantity,
                value_gbp: price.to_gbp(asset.quantity)?,
                fee_gbp: None,
                description: description.clone(),
            }]),
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
        match &mut tx.details {
            TransactionType::Trade {
                sold,
                bought,
                price,
                fee,
            } => {
                sold.symbol = normalize_currency(&sold.symbol);
                bought.symbol = normalize_currency(&bought.symbol);
                if let Some(Price::FxChain { quote, .. }) = price.as_mut() {
                    *quote = normalize_currency(quote);
                }
                if let Some(f) = fee.as_mut() {
                    f.asset = normalize_currency(&f.asset);
                    if let Some(Price::FxChain { quote, .. }) = f.price.as_mut() {
                        *quote = normalize_currency(quote);
                    }
                }
            }
            TransactionType::Deposit { asset, fee, .. } => {
                asset.symbol = normalize_currency(&asset.symbol);
                if let Some(f) = fee.as_mut() {
                    f.asset = normalize_currency(&f.asset);
                    if let Some(Price::FxChain { quote, .. }) = f.price.as_mut() {
                        *quote = normalize_currency(quote);
                    }
                }
            }
            TransactionType::Withdrawal { asset, fee, .. } => {
                asset.symbol = normalize_currency(&asset.symbol);
                if let Some(f) = fee.as_mut() {
                    f.asset = normalize_currency(&f.asset);
                    if let Some(Price::FxChain { quote, .. }) = f.price.as_mut() {
                        *quote = normalize_currency(quote);
                    }
                }
            }
            TransactionType::StakingReward { asset, price } => {
                asset.symbol = normalize_currency(&asset.symbol);
                if let Price::FxChain { quote, .. } = price {
                    *quote = normalize_currency(quote);
                }
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
        return Ok(DateTime::from_utc(dt, FixedOffset::east_opt(0).unwrap()));
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(DateTime::from_utc(dt, FixedOffset::east_opt(0).unwrap()));
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Ok(DateTime::from_utc(dt, FixedOffset::east_opt(0).unwrap()));
    }
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        return Ok(DateTime::from_utc(dt, FixedOffset::east_opt(0).unwrap()));
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

    #[test]
    fn price_gbp_multiplies_rate() {
        let price = Price::Gbp {
            rate: dec!(2000),
            source: None,
        };
        assert_eq!(price.to_gbp(dec!(0.5)).unwrap(), dec!(1000));
    }

    #[test]
    fn price_fx_chain_applies_fx() {
        let price = Price::FxChain {
            rate: dec!(40000),
            quote: "USD".to_string(),
            fx_rate: dec!(0.79),
            source: None,
        };
        assert_eq!(price.to_gbp(dec!(0.5)).unwrap(), dec!(15800));
    }

    #[test]
    fn trade_crypto_to_crypto_generates_two_events() {
        let tx = Transaction {
            id: "tx-1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
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
                price: Some(Price::FxChain {
                    rate: dec!(2000),
                    quote: "USD".to_string(),
                    fx_rate: dec!(0.79),
                    source: None,
                }),
                fee: None,
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
                price: None,
                fee: None,
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
                price: None,
                fee: None,
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
                price: None,
                fee: None,
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
            details: TransactionType::Deposit {
                asset: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                linked_withdrawal: Some("w1".to_string()),
                fee: None,
            },
        };
        let withdrawal = Transaction {
            id: "w1".to_string(),
            datetime: dt("2024-01-01T09:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            details: TransactionType::Withdrawal {
                asset: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                linked_deposit: Some("d1".to_string()),
                fee: None,
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
            details: TransactionType::Deposit {
                asset: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                linked_withdrawal: None,
                fee: None,
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
        assert_eq!(events[0].event_type, EventType::UnclassifiedIn);
    }

    #[test]
    fn exclude_unlinked_flag_skips_events() {
        let withdrawal = Transaction {
            id: "w1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            details: TransactionType::Withdrawal {
                asset: Asset {
                    symbol: "BTC".to_string(),
                    quantity: dec!(1),
                    asset_class: AssetClass::Crypto,
                },
                linked_deposit: None,
                fee: None,
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
            details: TransactionType::StakingReward {
                asset: Asset {
                    symbol: "ETH".to_string(),
                    quantity: dec!(0.01),
                    asset_class: AssetClass::Crypto,
                },
                price: Price::Gbp {
                    rate: dec!(2000),
                    source: None,
                },
            },
        };

        let events = tx.to_taxable_events(false).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::StakingReward);
        assert_eq!(events[0].value_gbp, dec!(20));
    }

    #[test]
    fn fee_allocated_to_disposal() {
        let tx = Transaction {
            id: "t1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
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
                price: Some(Price::Gbp {
                    rate: dec!(1000),
                    source: None,
                }),
                fee: Some(Fee {
                    asset: "GBP".to_string(),
                    amount: dec!(5),
                    price: None,
                }),
            },
        };

        let events = tx.to_taxable_events(false).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].fee_gbp, Some(dec!(5)));
        assert_eq!(events[1].fee_gbp, None);
    }
}
