use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Serialize, Serializer};

/// Tax band for income tax calculations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TaxBand {
    #[default]
    Basic,
    Higher,
    Additional,
}

impl TaxBand {
    /// Income tax rate for miscellaneous income (e.g. staking rewards).
    /// The band alone determines the rate; it does not vary by tax year.
    pub fn income_rate(self) -> Decimal {
        match self {
            TaxBand::Basic => dec!(0.20),
            TaxBand::Higher => dec!(0.40),
            TaxBand::Additional => dec!(0.45),
        }
    }
}

/// UK Tax Year (runs 6 April to 5 April)
/// The year value represents the end year (e.g., 2025 = 2024/25 tax year)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaxYear(pub i32);

impl Serialize for TaxYear {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.display())
    }
}

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

    /// The tax year's 6 April / 5 April bounds, or `None` when the year is
    /// too large or small for a representable date.
    ///
    /// The single place these dates are derived; `start_date`, `end_date` and
    /// every caller that needs to reject an out-of-range year go through it.
    pub fn try_bounds(&self) -> Option<(NaiveDate, NaiveDate)> {
        let start = self
            .0
            .checked_sub(1)
            .and_then(|y| NaiveDate::from_ymd_opt(y, 4, 6))?;
        let end = NaiveDate::from_ymd_opt(self.0, 4, 5)?;
        Some((start, end))
    }

    /// First day of the tax year (6 April).
    ///
    /// Panics if the year has no representable bounds; use [`TaxYear::try_bounds`]
    /// for a year that came from untrusted input.
    pub fn start_date(&self) -> NaiveDate {
        self.try_bounds().expect("tax year out of range").0
    }

    /// Last day of the tax year (5 April).
    ///
    /// Panics if the year has no representable bounds; use [`TaxYear::try_bounds`]
    /// for a year that came from untrusted input.
    pub fn end_date(&self) -> NaiveDate {
        self.try_bounds().expect("tax year out of range").1
    }

    /// Display as "2024/25" format. The end year is zero-padded, so
    /// `TaxYear(2005)` is "2004/05" rather than "2004/5".
    pub fn display(&self) -> String {
        format!("{}/{:02}", self.0 - 1, self.0.rem_euclid(100))
    }

    /// Get CGT annual exempt amount for this tax year
    pub fn cgt_exempt_amount(&self) -> Decimal {
        match self.0 {
            // 2024/25 onwards: £3,000
            2025.. => dec!(3000),
            // 2023/24: £6,000
            2024 => dec!(6000),
            // 2020/21 to 2022/23: £12,300
            2021..=2023 => dec!(12300),
            // 2019/20: £12,000
            2020 => dec!(12000),
            // 2018/19: £11,700
            2019 => dec!(11700),
            // 2017/18: £11,300
            2018 => dec!(11300),
            // 2015/16 and 2016/17: £11,100
            2016..=2017 => dec!(11100),
            // 2014/15: £11,000 (approximate for earlier years)
            _ => dec!(11000),
        }
    }

    /// Get CGT basic rate for this tax year (non-residential-property assets,
    /// e.g. crypto and shares).
    ///
    /// Rates changed mid-year on 30 October 2024 (10% -> 18%); for 2024/25
    /// this returns the post-change rate, so gains realised before that date
    /// are over-estimated.
    pub fn cgt_basic_rate(&self) -> Decimal {
        match self.0 {
            // 2024/25 onwards: 18% (from 30 October 2024)
            2025.. => dec!(0.18),
            // 2016/17 to 2023/24: 10%
            2017..=2024 => dec!(0.10),
            // 2010/11 to 2015/16: 18% (approximate for earlier years)
            _ => dec!(0.18),
        }
    }

    /// Get CGT higher rate for this tax year (non-residential-property assets,
    /// e.g. crypto and shares).
    ///
    /// Rates changed mid-year on 30 October 2024 (20% -> 24%); for 2024/25
    /// this returns the post-change rate, so gains realised before that date
    /// are over-estimated.
    pub fn cgt_higher_rate(&self) -> Decimal {
        match self.0 {
            // 2024/25 onwards: 24% (from 30 October 2024)
            2025.. => dec!(0.24),
            // 2016/17 to 2023/24: 20%
            2017..=2024 => dec!(0.20),
            // 2010/11 to 2015/16: 28% (approximate for earlier years)
            _ => dec!(0.28),
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
    fn tax_year_display_pads_single_digit_end_year() {
        // Years ending 2000-2009 must keep the leading zero: "2004/05",
        // never "2004/5".
        assert_eq!(TaxYear(2000).display(), "1999/00");
        assert_eq!(TaxYear(2005).display(), "2004/05");
        assert_eq!(TaxYear(2009).display(), "2008/09");
        assert_eq!(TaxYear(2010).display(), "2009/10");
    }

    #[test]
    fn tax_year_start_end_dates() {
        let ty = TaxYear(2025);
        assert_eq!(
            ty.start_date(),
            NaiveDate::from_ymd_opt(2024, 4, 6).unwrap()
        );
        assert_eq!(ty.end_date(), NaiveDate::from_ymd_opt(2025, 4, 5).unwrap());
    }

    #[test]
    fn cgt_exempt_amounts() {
        assert_eq!(TaxYear(2026).cgt_exempt_amount(), dec!(3000));
        assert_eq!(TaxYear(2025).cgt_exempt_amount(), dec!(3000));
        assert_eq!(TaxYear(2024).cgt_exempt_amount(), dec!(6000));
        assert_eq!(TaxYear(2023).cgt_exempt_amount(), dec!(12300));
        assert_eq!(TaxYear(2021).cgt_exempt_amount(), dec!(12300));
        assert_eq!(TaxYear(2020).cgt_exempt_amount(), dec!(12000));
        assert_eq!(TaxYear(2019).cgt_exempt_amount(), dec!(11700));
        assert_eq!(TaxYear(2018).cgt_exempt_amount(), dec!(11300));
        assert_eq!(TaxYear(2017).cgt_exempt_amount(), dec!(11100));
        assert_eq!(TaxYear(2016).cgt_exempt_amount(), dec!(11100));
        assert_eq!(TaxYear(2015).cgt_exempt_amount(), dec!(11000));
    }

    #[test]
    fn cgt_rates_2024_25_onwards() {
        // 18%/24% apply from 30 October 2024; the tool uses them for the
        // whole of 2024/25.
        for year in [2025, 2026, 2027] {
            let ty = TaxYear(year);
            assert_eq!(ty.cgt_basic_rate(), dec!(0.18));
            assert_eq!(ty.cgt_higher_rate(), dec!(0.24));
        }
    }

    #[test]
    fn cgt_rates_2016_17_to_2023_24() {
        for year in [2017, 2020, 2024] {
            let ty = TaxYear(year);
            assert_eq!(ty.cgt_basic_rate(), dec!(0.10));
            assert_eq!(ty.cgt_higher_rate(), dec!(0.20));
        }
    }

    #[test]
    fn cgt_rates_2010_11_to_2015_16() {
        for year in [2011, 2016] {
            let ty = TaxYear(year);
            assert_eq!(ty.cgt_basic_rate(), dec!(0.18));
            assert_eq!(ty.cgt_higher_rate(), dec!(0.28));
        }
    }

    #[test]
    fn income_rates() {
        assert_eq!(TaxBand::Basic.income_rate(), dec!(0.20));
        assert_eq!(TaxBand::Higher.income_rate(), dec!(0.40));
        assert_eq!(TaxBand::Additional.income_rate(), dec!(0.45));
    }

    #[test]
    fn try_bounds_rejects_unrepresentable_year() {
        assert!(TaxYear(2025).try_bounds().is_some());
        assert!(TaxYear(i32::MIN).try_bounds().is_none());
        assert!(TaxYear(i32::MAX).try_bounds().is_none());
    }
}
