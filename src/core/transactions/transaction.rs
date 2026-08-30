use chrono::{DateTime, FixedOffset};
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::datetime::deserialize_datetime;
use super::valuation::Valuation;
use super::DecimalJson;
use crate::core::events::Tag;
use crate::core::price::Price;

/// Transaction record with common fields + type-specific data.
///
/// Serialization contract: optional fields are omitted when absent (never
/// `null`), the default `Unclassified` tag is omitted, and decimal quantities
/// are written as numeric strings (`"0.5"`) so they stay exact through any
/// JSON parser. On input a bare JSON number is also accepted.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional valuation: a price object or direct GBP total
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valuation: Option<Valuation>,
    /// Optional fee for this transaction
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee: Option<Fee>,
    /// Optional transaction tag used for classification
    #[serde(default, skip_serializing_if = "is_unclassified")]
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        linked_withdrawal: Option<String>,
    },

    /// Withdrawal - assets sent FROM an account
    Withdrawal {
        amount: Amount,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        linked_deposit: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Amount {
    pub asset: String,
    #[schemars(with = "DecimalJson")]
    pub quantity: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Fee {
    pub asset: String,
    #[schemars(with = "DecimalJson")]
    pub amount: Decimal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<Price>,
}

fn is_unclassified(tag: &Tag) -> bool {
    *tag == Tag::Unclassified
}
