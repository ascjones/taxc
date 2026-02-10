use super::events::{EventType, Label, TaxableEvent};
use super::uk::TaxYear;
use rust_decimal::Decimal;

/// Income tax report
#[derive(Debug)]
pub struct IncomeReport {
    /// Individual staking events
    pub staking_events: Vec<IncomeEvent>,
}

/// Individual income event record
#[derive(Debug, Clone)]
pub struct IncomeEvent {
    pub tax_year: TaxYear,
    pub value_gbp: Decimal,
}

/// Calculate income tax from taxable events
pub fn calculate_income_tax(events: Vec<TaxableEvent>) -> anyhow::Result<IncomeReport> {
    let mut staking_events: Vec<IncomeEvent> = Vec::new();

    for event in events {
        let tax_year = TaxYear::from_date(event.date());

        if event.event_type == EventType::Acquisition && event.label == Label::StakingReward {
            staking_events.push(IncomeEvent {
                tax_year,
                value_gbp: event.value_gbp,
            });
        }
    }

    Ok(IncomeReport { staking_events })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{AssetClass, Label};
    use chrono::DateTime;
    use rust_decimal_macros::dec;

    fn dt(date: &str) -> chrono::DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339(&format!("{date}T00:00:00+00:00")).unwrap()
    }

    fn staking(date: &str, value: Decimal) -> TaxableEvent {
        TaxableEvent {
            id: None,
            datetime: dt(date),
            event_type: EventType::Acquisition,
            label: Label::StakingReward,
            asset: "GBP".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: value,
            value_gbp: value,
            fee_gbp: None,
            description: None,
        }
    }

    #[test]
    fn staking_events_collected() {
        let events = vec![
            staking("2024-06-01", dec!(250)),
            staking("2024-07-01", dec!(260)),
            staking("2024-08-01", dec!(50)),
        ];

        let report = calculate_income_tax(events).unwrap();
        assert_eq!(report.staking_events.len(), 3);
    }

    #[test]
    fn acquisitions_and_disposals_ignored() {
        let events = vec![
            TaxableEvent {
                id: None,
                datetime: dt("2024-06-01"),
                event_type: EventType::Acquisition,
                label: Label::Trade,
                asset: "GBP".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(50000),
                value_gbp: dec!(50000),
                fee_gbp: None,
                description: None,
            },
            TaxableEvent {
                id: None,
                datetime: dt("2024-07-01"),
                event_type: EventType::Disposal,
                label: Label::Trade,
                asset: "GBP".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(30000),
                value_gbp: dec!(30000),
                fee_gbp: None,
                description: None,
            },
            staking("2024-06-01", dec!(100)),
        ];

        let report = calculate_income_tax(events).unwrap();
        assert_eq!(report.staking_events.len(), 1);
    }

    #[test]
    fn gifts_not_counted_as_income() {
        let events = vec![
            TaxableEvent {
                id: None,
                datetime: dt("2024-06-01"),
                event_type: EventType::Acquisition,
                label: Label::Gift,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(1),
                value_gbp: dec!(50000),
                fee_gbp: None,
                description: None,
            },
            TaxableEvent {
                id: None,
                datetime: dt("2024-07-01"),
                event_type: EventType::Disposal,
                label: Label::Gift,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(0.5),
                value_gbp: dec!(25000),
                fee_gbp: None,
                description: None,
            },
            staking("2024-06-01", dec!(100)),
        ];

        let report = calculate_income_tax(events).unwrap();
        // Only staking counts as income, not gifts
        assert_eq!(report.staking_events.len(), 1);
        assert_eq!(report.staking_events[0].value_gbp, dec!(100));
    }
}
