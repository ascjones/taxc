use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Domain warning types emitted during conversion/calculation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Warning {
    /// Event was unclassified and may need manual review.
    UnclassifiedEvent,
    /// Pool had insufficient quantity to cover the disposal.
    /// When `available = 0`, this means no cost basis at all.
    InsufficientCostBasis {
        #[schemars(with = "f64")]
        available: Decimal,
        #[schemars(with = "f64")]
        required: Decimal,
    },
    /// Income airdrop had no market price available.
    MissingAirdropPrice,
    /// Non-income airdrop provided a price that was ignored.
    IgnoredAirdropPrice,
}
