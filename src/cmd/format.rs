//! Shared stdout formatting helpers for CLI commands.

use crate::core::fmt::pence_string;
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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn gbp_signed_places_sign_before_symbol() {
        assert_eq!(format_gbp_signed(dec!(-12.345)), "-£12.35");
        assert_eq!(format_gbp_signed(dec!(12.34)), "£12.34");
    }
}
