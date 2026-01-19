use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Tax band for income tax calculations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TaxBand {
    #[default]
    Basic,
    Higher,
    Additional,
}

impl TaxBand {
    pub fn from_str(s: &str) -> Option<TaxBand> {
        match s.to_lowercase().as_str() {
            "basic" => Some(TaxBand::Basic),
            "higher" => Some(TaxBand::Higher),
            "additional" => Some(TaxBand::Additional),
            _ => None,
        }
    }
}

/// UK Tax Year (runs 6 April to 5 April)
/// The year value represents the end year (e.g., 2025 = 2024/25 tax year)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaxYear(pub i32);

impl TaxYear {
    /// Create a tax year from a date
    pub fn from_date(date: NaiveDate) -> Self {
        let year = date.year();
        // Tax year starts 6 April
        // If date is 6 April or later, it's in the tax year ending next April
        // If date is before 6 April, it's in the current tax year ending this April
        if date >= NaiveDate::from_ymd_opt(year, 4, 6).unwrap() {
            TaxYear(year + 1)
        } else {
            TaxYear(year)
        }
    }

    /// Start date of the tax year (6 April of previous year)
    pub fn start_date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.0 - 1, 4, 6).unwrap()
    }

    /// End date of the tax year (5 April)
    pub fn end_date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.0, 4, 5).unwrap()
    }

    /// Display as "2024/25" format
    pub fn display(&self) -> String {
        format!("{}/{}", self.0 - 1, self.0 % 100)
    }

    /// Get CGT annual exempt amount for this tax year
    pub fn cgt_exempt_amount(&self) -> Decimal {
        match self.0 {
            // 2024/25 onwards: £3,000
            2025.. => dec!(3000),
            // 2023/24: £6,000
            2024 => dec!(6000),
            // 2022/23: £12,300
            2023 => dec!(12300),
            // Earlier years: £12,300 (approximate)
            _ => dec!(12300),
        }
    }

    /// Get CGT basic rate for this tax year
    pub fn cgt_basic_rate(&self) -> Decimal {
        match self.0 {
            // From April 2025: 18%
            2026.. => dec!(0.18),
            // 2024/25 and earlier: 10% for most assets, but crypto/property is 18%
            // For crypto specifically, it's been 18% since 2016
            _ => dec!(0.18),
        }
    }

    /// Get CGT higher rate for this tax year
    pub fn cgt_higher_rate(&self) -> Decimal {
        match self.0 {
            // From April 2025: 24%
            2026.. => dec!(0.24),
            // 2024/25 and earlier: 20% for most assets, but crypto/property is 24%
            // For crypto specifically, it's been 20% (now 24%)
            _ => dec!(0.20),
        }
    }

    /// Get dividend allowance for this tax year
    pub fn dividend_allowance(&self) -> Decimal {
        match self.0 {
            // 2024/25 onwards: £500
            2025.. => dec!(500),
            // 2023/24: £1,000
            2024 => dec!(1000),
            // Earlier: £2,000
            _ => dec!(2000),
        }
    }

    /// Get dividend tax rate for a given tax band
    pub fn dividend_rate(&self, band: TaxBand) -> Decimal {
        // Dividend rates have been stable for several years
        match band {
            TaxBand::Basic => dec!(0.0875),     // 8.75%
            TaxBand::Higher => dec!(0.3375),    // 33.75%
            TaxBand::Additional => dec!(0.3935), // 39.35%
        }
    }

    /// Get income tax rate for miscellaneous income (e.g., staking rewards)
    pub fn income_rate(&self, band: TaxBand) -> Decimal {
        match band {
            TaxBand::Basic => dec!(0.20),      // 20%
            TaxBand::Higher => dec!(0.40),     // 40%
            TaxBand::Additional => dec!(0.45), // 45%
        }
    }
}

impl std::fmt::Display for TaxYear {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tax_year_from_date_before_april_6() {
        // 5 April 2024 is in 2023/24 tax year
        let date = NaiveDate::from_ymd_opt(2024, 4, 5).unwrap();
        assert_eq!(TaxYear::from_date(date), TaxYear(2024));
    }

    #[test]
    fn tax_year_from_date_on_april_6() {
        // 6 April 2024 is in 2024/25 tax year
        let date = NaiveDate::from_ymd_opt(2024, 4, 6).unwrap();
        assert_eq!(TaxYear::from_date(date), TaxYear(2025));
    }

    #[test]
    fn tax_year_from_date_after_april_6() {
        // 7 April 2024 is in 2024/25 tax year
        let date = NaiveDate::from_ymd_opt(2024, 4, 7).unwrap();
        assert_eq!(TaxYear::from_date(date), TaxYear(2025));
    }

    #[test]
    fn tax_year_from_date_january() {
        // 15 January 2024 is in 2023/24 tax year
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        assert_eq!(TaxYear::from_date(date), TaxYear(2024));
    }

    #[test]
    fn tax_year_from_date_december() {
        // 31 December 2024 is in 2024/25 tax year
        let date = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        assert_eq!(TaxYear::from_date(date), TaxYear(2025));
    }

    #[test]
    fn tax_year_display() {
        assert_eq!(TaxYear(2024).display(), "2023/24");
        assert_eq!(TaxYear(2025).display(), "2024/25");
        assert_eq!(TaxYear(2026).display(), "2025/26");
    }

    #[test]
    fn tax_year_start_end_dates() {
        let ty = TaxYear(2025);
        assert_eq!(ty.start_date(), NaiveDate::from_ymd_opt(2024, 4, 6).unwrap());
        assert_eq!(ty.end_date(), NaiveDate::from_ymd_opt(2025, 4, 5).unwrap());
    }

    #[test]
    fn cgt_exempt_amounts() {
        assert_eq!(TaxYear(2025).cgt_exempt_amount(), dec!(3000));
        assert_eq!(TaxYear(2026).cgt_exempt_amount(), dec!(3000));
        assert_eq!(TaxYear(2024).cgt_exempt_amount(), dec!(6000));
        assert_eq!(TaxYear(2023).cgt_exempt_amount(), dec!(12300));
    }

    #[test]
    fn cgt_rates_2025_26_onwards() {
        let ty = TaxYear(2026);
        assert_eq!(ty.cgt_basic_rate(), dec!(0.18));
        assert_eq!(ty.cgt_higher_rate(), dec!(0.24));
    }

    #[test]
    fn cgt_rates_2024_25() {
        let ty = TaxYear(2025);
        assert_eq!(ty.cgt_basic_rate(), dec!(0.18));
        assert_eq!(ty.cgt_higher_rate(), dec!(0.20));
    }

    #[test]
    fn dividend_allowances() {
        assert_eq!(TaxYear(2025).dividend_allowance(), dec!(500));
        assert_eq!(TaxYear(2026).dividend_allowance(), dec!(500));
        assert_eq!(TaxYear(2024).dividend_allowance(), dec!(1000));
    }

    #[test]
    fn dividend_rates() {
        let ty = TaxYear(2025);
        assert_eq!(ty.dividend_rate(TaxBand::Basic), dec!(0.0875));
        assert_eq!(ty.dividend_rate(TaxBand::Higher), dec!(0.3375));
        assert_eq!(ty.dividend_rate(TaxBand::Additional), dec!(0.3935));
    }

    #[test]
    fn income_rates() {
        let ty = TaxYear(2025);
        assert_eq!(ty.income_rate(TaxBand::Basic), dec!(0.20));
        assert_eq!(ty.income_rate(TaxBand::Higher), dec!(0.40));
        assert_eq!(ty.income_rate(TaxBand::Additional), dec!(0.45));
    }

    #[test]
    fn tax_band_from_str() {
        assert_eq!(TaxBand::from_str("basic"), Some(TaxBand::Basic));
        assert_eq!(TaxBand::from_str("Basic"), Some(TaxBand::Basic));
        assert_eq!(TaxBand::from_str("BASIC"), Some(TaxBand::Basic));
        assert_eq!(TaxBand::from_str("higher"), Some(TaxBand::Higher));
        assert_eq!(TaxBand::from_str("additional"), Some(TaxBand::Additional));
        assert_eq!(TaxBand::from_str("invalid"), None);
    }
}
