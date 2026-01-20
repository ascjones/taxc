use crate::events::{EventType, TaxableEvent};
use crate::tax::uk::{TaxBand, TaxYear};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;

/// Income tax report
#[derive(Debug)]
pub struct IncomeReport {
    /// Staking rewards grouped by tax year
    pub staking_by_year: HashMap<TaxYear, Decimal>,
    /// Dividends grouped by tax year
    pub dividends_by_year: HashMap<TaxYear, Decimal>,
    /// Individual staking events
    pub staking_events: Vec<IncomeEvent>,
    /// Individual dividend events
    pub dividend_events: Vec<IncomeEvent>,
}

/// Individual income event record
#[derive(Debug, Clone)]
pub struct IncomeEvent {
    pub date: chrono::NaiveDate,
    pub tax_year: TaxYear,
    pub asset: String,
    pub quantity: Decimal,
    pub value_gbp: Decimal,
    pub income_type: IncomeType,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncomeType {
    StakingReward,
    Dividend,
}

/// Tax calculation for a specific year
#[derive(Debug, Clone)]
pub struct YearlyIncomeTax {
    #[allow(dead_code)]
    pub tax_year: TaxYear,
    #[allow(dead_code)]
    pub tax_band: TaxBand,
    pub staking_income: Decimal,
    pub staking_tax: Decimal,
    pub dividend_income: Decimal,
    pub dividend_allowance_used: Decimal,
    pub taxable_dividends: Decimal,
    pub dividend_tax: Decimal,
    pub total_tax: Decimal,
}

impl IncomeReport {
    /// Calculate tax liability for a specific year
    pub fn calculate_tax(&self, year: TaxYear, band: TaxBand) -> YearlyIncomeTax {
        let staking_income = self
            .staking_by_year
            .get(&year)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let dividend_income = self
            .dividends_by_year
            .get(&year)
            .copied()
            .unwrap_or(Decimal::ZERO);

        // Staking is taxed as miscellaneous income at marginal rate
        let staking_tax = (staking_income * year.income_rate(band)).round_dp(2);

        // Dividends have an allowance
        let dividend_allowance = year.dividend_allowance();
        let dividend_allowance_used = dividend_allowance.min(dividend_income);
        let taxable_dividends = (dividend_income - dividend_allowance_used).max(Decimal::ZERO);
        let dividend_tax = (taxable_dividends * year.dividend_rate(band)).round_dp(2);

        YearlyIncomeTax {
            tax_year: year,
            tax_band: band,
            staking_income,
            staking_tax,
            dividend_income,
            dividend_allowance_used,
            taxable_dividends,
            dividend_tax,
            total_tax: staking_tax + dividend_tax,
        }
    }

    /// Get all tax years with income
    pub fn tax_years(&self) -> Vec<TaxYear> {
        let mut years: Vec<TaxYear> = self
            .staking_by_year
            .keys()
            .chain(self.dividends_by_year.keys())
            .copied()
            .collect();
        years.sort();
        years.dedup();
        years
    }

    #[cfg(test)]
    pub fn staking_total(&self, year: Option<TaxYear>) -> Decimal {
        match year {
            Some(y) => self
                .staking_by_year
                .get(&y)
                .copied()
                .unwrap_or(Decimal::ZERO),
            None => self.staking_by_year.values().sum(),
        }
    }

    #[cfg(test)]
    pub fn dividend_total(&self, year: Option<TaxYear>) -> Decimal {
        match year {
            Some(y) => self
                .dividends_by_year
                .get(&y)
                .copied()
                .unwrap_or(Decimal::ZERO),
            None => self.dividends_by_year.values().sum(),
        }
    }

    /// Write income events to CSV
    pub fn write_csv<W: Write>(&self, writer: W, year: Option<TaxYear>) -> color_eyre::Result<()> {
        let mut wtr = csv::Writer::from_writer(writer);

        let all_events: Vec<&IncomeEvent> = self
            .staking_events
            .iter()
            .chain(self.dividend_events.iter())
            .filter(|e| year.is_none_or(|y| e.tax_year == y))
            .collect();

        for event in all_events {
            let record: IncomeCsvRecord = event.into();
            wtr.serialize(record)?;
        }
        wtr.flush()?;
        Ok(())
    }
}

/// CSV record for income output
#[derive(Debug, Serialize, Deserialize)]
pub struct IncomeCsvRecord {
    pub date: String,
    pub tax_year: String,
    pub income_type: String,
    pub asset: String,
    pub quantity: String,
    pub value_gbp: String,
    pub description: String,
}

impl From<&IncomeEvent> for IncomeCsvRecord {
    fn from(e: &IncomeEvent) -> Self {
        IncomeCsvRecord {
            date: e.date.format("%Y-%m-%d").to_string(),
            tax_year: e.tax_year.display(),
            income_type: match e.income_type {
                IncomeType::StakingReward => "Staking",
                IncomeType::Dividend => "Dividend",
            }
            .to_string(),
            asset: e.asset.clone(),
            quantity: e.quantity.to_string(),
            value_gbp: e.value_gbp.round_dp(2).to_string(),
            description: e.description.clone().unwrap_or_default(),
        }
    }
}

/// Calculate income tax from taxable events
pub fn calculate_income_tax(events: Vec<TaxableEvent>) -> IncomeReport {
    let mut staking_by_year: HashMap<TaxYear, Decimal> = HashMap::new();
    let mut dividends_by_year: HashMap<TaxYear, Decimal> = HashMap::new();
    let mut staking_events: Vec<IncomeEvent> = Vec::new();
    let mut dividend_events: Vec<IncomeEvent> = Vec::new();

    for event in events {
        let tax_year = TaxYear::from_date(event.date);

        match event.event_type {
            EventType::StakingReward => {
                *staking_by_year.entry(tax_year).or_insert(Decimal::ZERO) += event.value_gbp;
                staking_events.push(IncomeEvent {
                    date: event.date,
                    tax_year,
                    asset: event.asset,
                    quantity: event.quantity,
                    value_gbp: event.value_gbp,
                    income_type: IncomeType::StakingReward,
                    description: event.description,
                });
            }
            EventType::Dividend => {
                *dividends_by_year.entry(tax_year).or_insert(Decimal::ZERO) += event.value_gbp;
                dividend_events.push(IncomeEvent {
                    date: event.date,
                    tax_year,
                    asset: event.asset,
                    quantity: event.quantity,
                    value_gbp: event.value_gbp,
                    income_type: IncomeType::Dividend,
                    description: event.description,
                });
            }
            // Non-income events are ignored
            EventType::Acquisition | EventType::Disposal => {}
        }
    }

    IncomeReport {
        staking_by_year,
        dividends_by_year,
        staking_events,
        dividend_events,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::AssetClass;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    fn staking(date: &str, asset: &str, qty: Decimal, value: Decimal) -> TaxableEvent {
        TaxableEvent {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            event_type: EventType::StakingReward,
            asset: asset.to_string(),
            asset_class: AssetClass::Crypto,
            quantity: qty,
            value_gbp: value,
            fees_gbp: None,
            description: None,
        }
    }

    fn dividend(date: &str, asset: &str, qty: Decimal, value: Decimal) -> TaxableEvent {
        TaxableEvent {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            event_type: EventType::Dividend,
            asset: asset.to_string(),
            asset_class: AssetClass::Stock,
            quantity: qty,
            value_gbp: value,
            fees_gbp: None,
            description: None,
        }
    }

    #[test]
    fn staking_income_summed() {
        let events = vec![
            staking("2024-06-01", "ETH", dec!(0.1), dec!(250)),
            staking("2024-07-01", "ETH", dec!(0.1), dec!(260)),
            staking("2024-08-01", "DOT", dec!(10), dec!(50)),
        ];

        let report = calculate_income_tax(events);

        assert_eq!(report.staking_total(Some(TaxYear(2025))), dec!(560));
        assert_eq!(report.staking_events.len(), 3);
    }

    #[test]
    fn staking_tax_basic_rate() {
        let events = vec![staking("2024-06-01", "ETH", dec!(1), dec!(1000))];

        let report = calculate_income_tax(events);
        let tax = report.calculate_tax(TaxYear(2025), TaxBand::Basic);

        // 20% of £1,000 = £200
        assert_eq!(tax.staking_income, dec!(1000));
        assert_eq!(tax.staking_tax, dec!(200));
    }

    #[test]
    fn staking_tax_higher_rate() {
        let events = vec![staking("2024-06-01", "ETH", dec!(1), dec!(1000))];

        let report = calculate_income_tax(events);
        let tax = report.calculate_tax(TaxYear(2025), TaxBand::Higher);

        // 40% of £1,000 = £400
        assert_eq!(tax.staking_tax, dec!(400));
    }

    #[test]
    fn dividend_allowance_applied() {
        // Dividend allowance for 2024/25 is £500
        let events = vec![dividend("2024-06-01", "AAPL", dec!(100), dec!(800))];

        let report = calculate_income_tax(events);
        let tax = report.calculate_tax(TaxYear(2025), TaxBand::Basic);

        assert_eq!(tax.dividend_income, dec!(800));
        assert_eq!(tax.dividend_allowance_used, dec!(500));
        assert_eq!(tax.taxable_dividends, dec!(300));
        // 8.75% of £300 = £26.25
        assert_eq!(tax.dividend_tax, dec!(26.25));
    }

    #[test]
    fn dividend_under_allowance() {
        let events = vec![dividend("2024-06-01", "AAPL", dec!(100), dec!(400))];

        let report = calculate_income_tax(events);
        let tax = report.calculate_tax(TaxYear(2025), TaxBand::Basic);

        assert_eq!(tax.dividend_income, dec!(400));
        assert_eq!(tax.dividend_allowance_used, dec!(400));
        assert_eq!(tax.taxable_dividends, Decimal::ZERO);
        assert_eq!(tax.dividend_tax, Decimal::ZERO);
    }

    #[test]
    fn dividend_higher_rate() {
        let events = vec![dividend("2024-06-01", "AAPL", dec!(100), dec!(1000))];

        let report = calculate_income_tax(events);
        let tax = report.calculate_tax(TaxYear(2025), TaxBand::Higher);

        // £1000 - £500 allowance = £500 taxable
        // 33.75% of £500 = £168.75
        assert_eq!(tax.dividend_tax, dec!(168.75));
    }

    #[test]
    fn mixed_income_total_tax() {
        let events = vec![
            staking("2024-06-01", "ETH", dec!(1), dec!(500)),
            dividend("2024-06-01", "AAPL", dec!(100), dec!(1000)),
        ];

        let report = calculate_income_tax(events);
        let tax = report.calculate_tax(TaxYear(2025), TaxBand::Basic);

        // Staking: 20% of £500 = £100
        assert_eq!(tax.staking_tax, dec!(100));

        // Dividend: 8.75% of (£1000 - £500) = £43.75
        assert_eq!(tax.dividend_tax, dec!(43.75));

        assert_eq!(tax.total_tax, dec!(143.75));
    }

    #[test]
    fn income_grouped_by_tax_year() {
        let events = vec![
            staking("2024-04-05", "ETH", dec!(1), dec!(100)), // 2023/24
            staking("2024-04-06", "ETH", dec!(1), dec!(200)), // 2024/25
            staking("2024-12-01", "ETH", dec!(1), dec!(300)), // 2024/25
        ];

        let report = calculate_income_tax(events);

        assert_eq!(report.staking_total(Some(TaxYear(2024))), dec!(100));
        assert_eq!(report.staking_total(Some(TaxYear(2025))), dec!(500));
    }

    #[test]
    fn multiple_dividends_summed() {
        let events = vec![
            dividend("2024-06-01", "AAPL", dec!(100), dec!(150)),
            dividend("2024-09-01", "AAPL", dec!(100), dec!(150)),
            dividend("2024-12-01", "MSFT", dec!(50), dec!(100)),
        ];

        let report = calculate_income_tax(events);

        assert_eq!(report.dividend_total(Some(TaxYear(2025))), dec!(400));
        assert_eq!(report.dividend_events.len(), 3);
    }

    #[test]
    fn acquisitions_and_disposals_ignored() {
        use crate::events::EventType;

        let events = vec![
            TaxableEvent {
                date: NaiveDate::from_ymd_opt(2024, 6, 1).unwrap(),
                event_type: EventType::Acquisition,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(1),
                value_gbp: dec!(50000),
                fees_gbp: None,
                description: None,
            },
            TaxableEvent {
                date: NaiveDate::from_ymd_opt(2024, 7, 1).unwrap(),
                event_type: EventType::Disposal,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(0.5),
                value_gbp: dec!(30000),
                fees_gbp: None,
                description: None,
            },
            staking("2024-06-01", "ETH", dec!(1), dec!(100)),
        ];

        let report = calculate_income_tax(events);

        // Only staking should be counted
        assert_eq!(report.staking_total(None), dec!(100));
        assert_eq!(report.dividend_total(None), Decimal::ZERO);
        assert_eq!(report.staking_events.len(), 1);
        assert_eq!(report.dividend_events.len(), 0);
    }

    #[test]
    fn tax_years_list() {
        let events = vec![
            staking("2023-06-01", "ETH", dec!(1), dec!(100)), // 2023/24
            dividend("2024-06-01", "AAPL", dec!(100), dec!(200)), // 2024/25
            staking("2025-01-01", "ETH", dec!(1), dec!(300)), // 2024/25 (Jan is still 2024/25)
        ];

        let report = calculate_income_tax(events);
        let years = report.tax_years();

        assert_eq!(years.len(), 2);
        assert!(years.contains(&TaxYear(2024)));
        assert!(years.contains(&TaxYear(2025)));
    }
}
