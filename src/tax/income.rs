use crate::events::{EventType, TaxableEvent};
use crate::tax::uk::TaxYear;
use rust_decimal::Decimal;

/// Income tax report
#[derive(Debug)]
pub struct IncomeReport {
    /// Individual staking events
    pub staking_events: Vec<IncomeEvent>,
    /// Individual dividend events
    pub dividend_events: Vec<IncomeEvent>,
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
    let mut dividend_events: Vec<IncomeEvent> = Vec::new();

    for event in events {
        let tax_year = TaxYear::from_date(event.date());

        match event.event_type {
            EventType::StakingReward => {
                staking_events.push(IncomeEvent {
                    tax_year,
                    value_gbp: event.value_gbp,
                });
            }
            EventType::Dividend => {
                dividend_events.push(IncomeEvent {
                    tax_year,
                    value_gbp: event.value_gbp,
                });
            }
            // Non-income events are ignored
            EventType::Acquisition
            | EventType::Disposal
            | EventType::UnclassifiedIn
            | EventType::UnclassifiedOut => {}
        }
    }

    Ok(IncomeReport {
        staking_events,
        dividend_events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::AssetClass;
    use chrono::DateTime;
    use rust_decimal_macros::dec;

    fn dt(date: &str) -> chrono::DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339(&format!("{date}T00:00:00+00:00")).unwrap()
    }

    fn staking(date: &str, value: Decimal) -> TaxableEvent {
        TaxableEvent {
            id: None,
            datetime: dt(date),
            event_type: EventType::StakingReward,
            asset: "GBP".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: value,
            value_gbp: value,
            fee_gbp: None,
            description: None,
        }
    }

    fn dividend(date: &str, value: Decimal) -> TaxableEvent {
        TaxableEvent {
            id: None,
            datetime: dt(date),
            event_type: EventType::Dividend,
            asset: "GBP".to_string(),
            asset_class: AssetClass::Stock,
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
        assert_eq!(report.dividend_events.len(), 0);
    }

    #[test]
    fn dividend_events_collected() {
        let events = vec![
            dividend("2024-06-01", dec!(150)),
            dividend("2024-09-01", dec!(150)),
        ];

        let report = calculate_income_tax(events).unwrap();
        assert_eq!(report.staking_events.len(), 0);
        assert_eq!(report.dividend_events.len(), 2);
    }

    #[test]
    fn acquisitions_and_disposals_ignored() {
        let events = vec![
            TaxableEvent {
                id: None,
                datetime: dt("2024-06-01"),
                event_type: EventType::Acquisition,
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
        assert_eq!(report.dividend_events.len(), 0);
    }
}
