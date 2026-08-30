use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::price::Price;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Valuation {
    Price(Price),
    ValueGbp(#[schemars(with = "super::DecimalJson")] Decimal),
}

impl Valuation {
    pub fn price(&self) -> Option<&Price> {
        match self {
            Self::Price(price) => Some(price),
            Self::ValueGbp(_) => None,
        }
    }
}
