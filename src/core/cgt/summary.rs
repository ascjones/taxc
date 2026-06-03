use rust_decimal::Decimal;

/// Aggregated capital-gains position for a set of disposals: gains netted
/// against in-year losses, then reduced by the Annual Exempt Amount (AEA).
///
/// Pure domain math, independent of any tax band or output format. Callers
/// supply the per-disposal gains and the year's AEA; `estimated_cgt` applies a
/// rate to the taxable gain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CgtSummary {
    /// Sum of positive gains only.
    pub gross_gains: Decimal,
    /// Sum of losses as a positive magnitude.
    pub in_year_losses: Decimal,
    /// `gross_gains - in_year_losses` (may be negative).
    pub net_gain_before_aea: Decimal,
    /// The Annual Exempt Amount applied.
    pub aea: Decimal,
    /// `(net_gain_before_aea - aea)` floored at zero.
    pub taxable_gain: Decimal,
}

impl CgtSummary {
    /// Net the supplied gains (positive and negative) against each other, then
    /// subtract the AEA, clamping the taxable gain at zero.
    pub fn calculate(gains: impl IntoIterator<Item = Decimal>, aea: Decimal) -> Self {
        let mut gross_gains = Decimal::ZERO;
        let mut in_year_losses = Decimal::ZERO;
        for gain in gains {
            if gain > Decimal::ZERO {
                gross_gains += gain;
            } else if gain < Decimal::ZERO {
                in_year_losses += gain.abs();
            }
        }
        let net_gain_before_aea = gross_gains - in_year_losses;
        let taxable_gain = (net_gain_before_aea - aea).max(Decimal::ZERO);
        CgtSummary {
            gross_gains,
            in_year_losses,
            net_gain_before_aea,
            aea,
            taxable_gain,
        }
    }

    /// Estimated CGT for the taxable gain at the given rate, rounded to pence.
    pub fn estimated_cgt(&self, rate: Decimal) -> Decimal {
        (self.taxable_gain * rate).round_dp(2)
    }
}
