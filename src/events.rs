use chrono::{DateTime, FixedOffset, NaiveDate};
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Type of taxable event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub enum EventType {
    #[default]
    Acquisition,
    Disposal,
}

/// Classification label for a taxable event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub enum Label {
    /// Unclassified event - needs review
    #[default]
    Unclassified,
    Trade,
    StakingReward,
}

/// Asset class for tax treatment
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub enum AssetClass {
    #[default]
    Crypto,
    Stock,
}

/// A taxable event (acquisition, disposal, or income)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaxableEvent {
    /// Optional identifier to link back to source data
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "date")]
    #[schemars(with = "String")]
    pub datetime: DateTime<FixedOffset>,
    pub event_type: EventType,
    #[serde(default)]
    pub label: Label,
    pub asset: String,
    pub asset_class: AssetClass,
    #[schemars(with = "f64")]
    pub quantity: Decimal,
    #[schemars(with = "f64")]
    pub value_gbp: Decimal,
    #[serde(default)]
    #[schemars(with = "Option<f64>")]
    pub fee_gbp: Option<Decimal>,
    #[serde(default)]
    pub description: Option<String>,
}

impl TaxableEvent {
    /// Get just the date portion for tax calculations
    pub fn date(&self) -> NaiveDate {
        self.datetime.date().naive_utc()
    }

    pub fn total_cost_gbp(&self) -> Decimal {
        self.value_gbp + self.fee_gbp.unwrap_or(Decimal::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn total_cost_includes_fees() {
        let event = TaxableEvent {
            id: None,
            datetime: DateTime::parse_from_rfc3339("2024-01-15T00:00:00+00:00").unwrap(),
            event_type: EventType::Acquisition,
            label: Label::Trade,
            asset: "GBP".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1000),
            value_gbp: dec!(1000),
            fee_gbp: Some(dec!(50)),
            description: None,
        };
        assert_eq!(event.total_cost_gbp(), dec!(1050));
    }

    #[test]
    fn total_cost_without_fees() {
        let event = TaxableEvent {
            id: None,
            datetime: DateTime::parse_from_rfc3339("2024-01-15T00:00:00+00:00").unwrap(),
            event_type: EventType::Acquisition,
            label: Label::Trade,
            asset: "GBP".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1000),
            value_gbp: dec!(1000),
            fee_gbp: None,
            description: None,
        };
        assert_eq!(event.total_cost_gbp(), dec!(1000));
    }
}
