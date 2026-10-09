use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::Serialize;

/// Domain warning types emitted during conversion/calculation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Warning {
    /// Event was unclassified and may need manual review.
    UnclassifiedEvent,
    /// Pool had insufficient quantity to cover the disposal.
    /// When `available = 0`, this means no cost basis at all. A demerger or
    /// small capital distribution against an empty pool records it with
    /// `available` and `required` both zero.
    InsufficientCostBasis {
        #[schemars(with = "String")]
        available: Decimal,
        #[schemars(with = "String")]
        required: Decimal,
    },
    /// A small capital distribution exceeded the pool's allowable cost. The
    /// cost is reduced to zero and the excess is a chargeable gain, which
    /// assumes the taxpayer makes the s.122(4) TCGA 1992 election (HMRC
    /// CG57847).
    CapitalDistributionExceedsCost,
}
