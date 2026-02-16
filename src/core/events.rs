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

/// Classification tag for a taxable event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub enum Tag {
    /// Unclassified event - needs review
    #[default]
    Unclassified,
    Trade,
    StakingReward,
    Salary,
    OtherIncome,
    Airdrop,
    AirdropIncome,
    Gift,
}

impl Tag {
    pub fn is_income(self) -> bool {
        matches!(
            self,
            Tag::StakingReward | Tag::Salary | Tag::OtherIncome | Tag::AirdropIncome
        )
    }
}

/// Display string for event type and tag (used in reports and summaries)
pub fn display_event_type(event_type: EventType, tag: Tag) -> &'static str {
    match (event_type, tag) {
        (EventType::Acquisition, Tag::StakingReward) => "StakingReward",
        (EventType::Acquisition, Tag::Salary) => "Salary",
        (EventType::Acquisition, Tag::OtherIncome) => "OtherIncome",
        (EventType::Acquisition, Tag::Airdrop) => "Airdrop",
        (EventType::Acquisition, Tag::AirdropIncome) => "AirdropIncome",
        (EventType::Acquisition, Tag::Gift) => "GiftIn",
        (EventType::Disposal, Tag::Gift) => "GiftOut",
        (EventType::Acquisition, Tag::Unclassified) => "UnclassifiedIn",
        (EventType::Disposal, Tag::Unclassified) => "UnclassifiedOut",
        (EventType::Acquisition, _) => "Acquisition",
        (EventType::Disposal, _) => "Disposal",
    }
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
    /// Sequential event identifier assigned during conversion
    pub id: usize,
    /// Original input transaction ID for this event
    pub source_transaction_id: String,
    #[serde(rename = "date")]
    #[schemars(with = "String")]
    pub datetime: DateTime<FixedOffset>,
    pub event_type: EventType,
    #[serde(default)]
    pub tag: Tag,
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
        self.datetime.date_naive()
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
            id: 1,
            source_transaction_id: "tx-1".to_string(),
            datetime: DateTime::parse_from_rfc3339("2024-01-15T00:00:00+00:00").unwrap(),
            event_type: EventType::Acquisition,
            tag: Tag::Trade,
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
            id: 1,
            source_transaction_id: "tx-1".to_string(),
            datetime: DateTime::parse_from_rfc3339("2024-01-15T00:00:00+00:00").unwrap(),
            event_type: EventType::Acquisition,
            tag: Tag::Trade,
            asset: "GBP".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1000),
            value_gbp: dec!(1000),
            fee_gbp: None,
            description: None,
        };
        assert_eq!(event.total_cost_gbp(), dec!(1000));
    }

    #[test]
    fn display_event_type_mappings() {
        assert_eq!(
            display_event_type(EventType::Acquisition, Tag::Trade),
            "Acquisition"
        );
        assert_eq!(
            display_event_type(EventType::Disposal, Tag::Trade),
            "Disposal"
        );
        assert_eq!(
            display_event_type(EventType::Acquisition, Tag::StakingReward),
            "StakingReward"
        );
        assert_eq!(
            display_event_type(EventType::Acquisition, Tag::Unclassified),
            "UnclassifiedIn"
        );
        assert_eq!(
            display_event_type(EventType::Disposal, Tag::Unclassified),
            "UnclassifiedOut"
        );
        assert_eq!(
            display_event_type(EventType::Acquisition, Tag::Gift),
            "GiftIn"
        );
        assert_eq!(
            display_event_type(EventType::Disposal, Tag::Gift),
            "GiftOut"
        );
        assert_eq!(
            display_event_type(EventType::Acquisition, Tag::Salary),
            "Salary"
        );
        assert_eq!(
            display_event_type(EventType::Acquisition, Tag::OtherIncome),
            "OtherIncome"
        );
        assert_eq!(
            display_event_type(EventType::Acquisition, Tag::Airdrop),
            "Airdrop"
        );
        assert_eq!(
            display_event_type(EventType::Acquisition, Tag::AirdropIncome),
            "AirdropIncome"
        );
    }

    #[test]
    fn income_tags_identified() {
        assert!(Tag::StakingReward.is_income());
        assert!(Tag::Salary.is_income());
        assert!(Tag::OtherIncome.is_income());
        assert!(Tag::AirdropIncome.is_income());
        assert!(!Tag::Trade.is_income());
        assert!(!Tag::Gift.is_income());
        assert!(!Tag::Airdrop.is_income());
        assert!(!Tag::Unclassified.is_income());
    }
}
