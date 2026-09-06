//! Shared stdout formatting helpers for CLI commands.

use crate::core::fmt::{pence_string, quantity_string};
use rust_decimal::Decimal;

/// Format a monetary amount with the currency symbol, rounded to pence.
pub fn format_gbp(amount: Decimal) -> String {
    format!("£{}", pence_string(amount))
}

pub fn format_gbp_signed(amount: Decimal) -> String {
    if amount < Decimal::ZERO {
        format!("-{}", format_gbp(amount.abs()))
    } else {
        format_gbp(amount)
    }
}

/// Format a quantity with up to 8 decimal places, trimming trailing zeros.
pub fn format_quantity(qty: Decimal) -> String {
    quantity_string(qty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn gbp_two_decimal_places() {
        assert_eq!(format_gbp(dec!(1234.5)), "£1234.50");
        assert_eq!(format_gbp(dec!(0)), "£0.00");
    }

    #[test]
    fn gbp_rounds_rather_than_truncates() {
        assert_eq!(format_gbp(dec!(99.999)), "£100.00");
        assert_eq!(format_gbp(dec!(12.346)), "£12.35");
    }

    #[test]
    fn gbp_signed_places_sign_before_symbol() {
        assert_eq!(format_gbp_signed(dec!(-12.345)), "-£12.35");
        assert_eq!(format_gbp_signed(dec!(12.34)), "£12.34");
    }

    #[test]
    fn quantity_trims_trailing_zeros() {
        assert_eq!(format_quantity(dec!(1.50000000)), "1.5");
        assert_eq!(format_quantity(dec!(2)), "2");
        assert_eq!(format_quantity(dec!(0.00000001)), "0.00000001");
    }
}
