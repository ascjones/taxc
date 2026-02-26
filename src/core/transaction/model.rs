use chrono::{DateTime, FixedOffset};
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::datetime::deserialize_datetime;
use crate::core::events::{AssetClass, Tag};
use crate::core::price::Price;

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

#[derive(Debug, Clone, Copy)]
pub struct ConversionOptions {
    pub exclude_unlinked: bool,
}
