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

    /// Demerger treated as a share reorganisation: no disposal. The stated
    /// fraction of the original shares' pool cost moves to the new holding,
    /// which is treated as held since the original shares were.
    ///
    /// Covers an exempt distribution (TCGA 1992 s.192; CTA 2010 s.1076) or a
    /// scheme of reconstruction (TCGA 1992 s.136), as the company's tax
    /// guidance states. `cost_fraction` is the share of the original cost
    /// apportioned to the new holding by market value on the first dealing
    /// day (s.130). HMRC CG45620, CG51702, CG51890, CG52742.
    ///
    /// A demerger taxed as a dividend in specie is not a `Demerger`: record
    /// it as a Dividend-tagged Deposit of the new shares at market value.
    Demerger {
        /// Symbol of the original shares, whose pool gives up the cost.
        original: String,
        /// The new holding received.
        new_holding: Amount,
        /// Fraction of the original pool cost moved, strictly between 0 and 1.
        #[schemars(with = "DecimalJson")]
        cost_fraction: Decimal,
    },

    /// Take-up of the holder's own pro-rata rights entitlement in the same
    /// company: a reorganisation, not an acquisition (TCGA 1992 s.126(2)(a),
    /// s.127, s.128; HMRC CG51746, CG51590). The shares and the consideration
    /// paid (plus any fee) join the existing pool, and are never matched
    /// under the same-day or 30-day rules.
    ///
    /// Shares from purchased rights or excess applications, and rights to
    /// shares in another company (CG52065), are a Trade acquisition.
    RightsIssue {
        /// The new shares taken up.
        new_shares: Amount,
        /// GBP paid for the new shares, excluding any fee.
        #[schemars(with = "DecimalJson")]
        consideration: Decimal,
    },

    /// Small capital distribution: the amount reduces the pool's allowable
    /// cost instead of being a disposal (TCGA 1992 s.122(2); HMRC CG57835).
    /// Includes cash for fractional entitlements on a reorganisation
    /// (s.128(3); HMRC CG57855). Choosing this type asserts the distribution
    /// is small: HMRC's practice is 5% or less of the holding's value, or
    /// £3,000 or less.
    ///
    /// An amount above the pool's cost zeroes the cost, and the excess is a
    /// chargeable gain, as under a s.122(4) election (HMRC CG57847).
    SmallCapitalDistribution {
        /// Symbol of the shares the distribution was made on.
        asset: String,
        /// GBP received.
        #[schemars(with = "DecimalJson")]
        amount: Decimal,
    },

    /// A fee paid with nothing else moving. The transaction's `fee` is
    /// required: tokens spent on it are disposed of at market value (HMRC
    /// CRYPTO22100, CRYPTO22280), so a non-GBP fee needs a price. A GBP fee
    /// produces no event.
    Fee {},
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
