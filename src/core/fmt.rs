//! Canonical rendering of domain values.
//!
//! Money, quantities and dates must render identically wherever they leave
//! the crate — stdout tables, report JSON, and serde impls on domain types —
//! so the rules live here rather than being restated at each output site.

use chrono::NaiveDate;
use rust_decimal::{Decimal, RoundingStrategy};

/// Quantities are rendered to at most this many decimal places.
const QUANTITY_DP: u32 = 8;

/// Round to pence, half away from zero.
///
/// `{:.2}` alone truncates `Decimal` values rather than rounding, so every
/// monetary output rounds through here first.
pub fn round_pence(amount: Decimal) -> Decimal {
    amount.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// Render a monetary amount as a plain 2dp string (no currency symbol).
pub fn pence_string(amount: Decimal) -> String {
    format!("{:.2}", round_pence(amount))
}

/// Render a quantity with up to 8 decimal places, trailing zeros trimmed.
pub fn quantity_string(quantity: Decimal) -> String {
    quantity.round_dp(QUANTITY_DP).normalize().to_string()
}

/// Render a date as ISO `YYYY-MM-DD`.
pub fn iso_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn round_pence_rounds_half_away_from_zero() {
        assert_eq!(round_pence(dec!(12.345)), dec!(12.35));
        assert_eq!(round_pence(dec!(-12.345)), dec!(-12.35));
        assert_eq!(round_pence(dec!(99.999)), dec!(100));
    }

    #[test]
    fn pence_string_always_two_places() {
        assert_eq!(pence_string(dec!(1234.5)), "1234.50");
        assert_eq!(pence_string(dec!(0)), "0.00");
        assert_eq!(pence_string(dec!(99.999)), "100.00");
    }

    #[test]
    fn quantity_string_trims_trailing_zeros() {
        assert_eq!(quantity_string(dec!(1.50000000)), "1.5");
        assert_eq!(quantity_string(dec!(2)), "2");
        assert_eq!(quantity_string(dec!(0.00000001)), "0.00000001");
        assert_eq!(quantity_string(dec!(0)), "0");
    }

    #[test]
    fn quantity_string_keeps_integer_zeros() {
        // A trim-based implementation over "100.00000000" must not eat the
        // zeros in the integer part.
        assert_eq!(quantity_string(dec!(100)), "100");
        assert_eq!(quantity_string(dec!(1000.10)), "1000.1");
    }

    #[test]
    fn quantity_string_caps_at_eight_places() {
        assert_eq!(quantity_string(dec!(1.123456789)), "1.12345679");
    }

    #[test]
    fn iso_date_formats_yyyy_mm_dd() {
        assert_eq!(
            iso_date(NaiveDate::from_ymd_opt(2024, 4, 6).unwrap()),
            "2024-04-06"
        );
    }
}
