use rust_decimal::Decimal;
use std::collections::BTreeMap;

use super::cgt::{CgtSummary, DisposalRecord};
use super::events::{EventType, Tag, TaxableEvent};
use super::uk::{TaxBand, TaxYear};

/// Capital-gains position for a set of classified disposals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CgtPosition {
    pub disposal_count: usize,
    pub total_proceeds: Decimal,
    /// Allowable costs plus disposal fees.
    pub total_costs: Decimal,
    /// Net gain across disposals (may be negative).
    pub total_gain: Decimal,
    /// Gains netted against losses and reduced by the AEA.
    pub summary: CgtSummary,
    /// CGT rate applied for the chosen band.
    pub rate: Decimal,
    pub estimated_cgt: Decimal,
}

/// Income position: totals by tag and the flat-band estimate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomePosition {
    /// All income-tagged acquisitions, including salary.
    pub total: Decimal,
    /// Income subject to the flat-band estimate. Salary is always excluded:
    /// it is PAYE-settled at source, so estimating tax on it again would
    /// double-count.
    pub taxable: Decimal,
    pub salary: Decimal,
    pub dividend: Decimal,
    pub interest: Decimal,
    /// Income by tag for every income tag present.
    pub by_tag: BTreeMap<Tag, Decimal>,
    /// Income tax rate applied for the chosen band.
    pub rate: Decimal,
    pub estimated_income_tax: Decimal,
}

/// Tax position for one rate year at one band.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxSummary {
    /// Year whose AEA and rates were applied.
    pub tax_year: TaxYear,
    pub tax_band: TaxBand,
    pub cgt: CgtPosition,
    pub income: IncomePosition,
    pub estimated_total_tax: Decimal,
}

pub fn summarize(
    events: &[&TaxableEvent],
    disposals: &[&DisposalRecord],
    rate_year: TaxYear,
    band: TaxBand,
) -> TaxSummary {
    let cgt_rate = match band {
        TaxBand::Basic => rate_year.cgt_basic_rate(),
        TaxBand::Higher | TaxBand::Additional => rate_year.cgt_higher_rate(),
    };
    let summary = CgtSummary::calculate(
        disposals.iter().map(|d| d.gain_gbp),
        rate_year.cgt_exempt_amount(),
    );
    let estimated_cgt = summary.estimated_cgt(cgt_rate);
    let cgt = CgtPosition {
        disposal_count: disposals.len(),
        total_proceeds: disposals.iter().map(|d| d.proceeds_gbp).sum(),
        total_costs: disposals
            .iter()
            .map(|d| d.allowable_cost_gbp + d.fees_gbp)
            .sum(),
        total_gain: disposals.iter().map(|d| d.gain_gbp).sum(),
        summary,
        rate: cgt_rate,
        estimated_cgt,
    };

    let income_rate = rate_year.income_rate(band);
    let mut by_tag: BTreeMap<Tag, Decimal> = BTreeMap::new();
    let mut total = Decimal::ZERO;
    for event in events {
        if event.event_type != EventType::Acquisition || !event.tag.is_income() {
            continue;
        }
        total += event.value_gbp;
        *by_tag.entry(event.tag).or_default() += event.value_gbp;
    }
    let tag_total = |tag: Tag| by_tag.get(&tag).copied().unwrap_or_default();
    let salary = tag_total(Tag::Salary);
    let taxable = total - salary;
    let estimated_income_tax = (taxable * income_rate).round_dp(2);
    let income = IncomePosition {
        total,
        taxable,
        salary,
        dividend: tag_total(Tag::Dividend),
        interest: tag_total(Tag::Interest),
        by_tag,
        rate: income_rate,
        estimated_income_tax,
    };

    TaxSummary {
        tax_year: rate_year,
        tax_band: band,
        estimated_total_tax: estimated_cgt + estimated_income_tax,
        cgt,
        income,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::calculate_cgt;
    use crate::core::events::builders::{acq, disp, event};
    use rust_decimal_macros::dec;

    fn income(date: &str, tag: Tag, value: Decimal) -> TaxableEvent {
        event(
            EventType::Acquisition,
            tag,
            date,
            "GBP",
            dec!(1),
            value,
            None,
        )
    }

    #[test]
    fn summarize_salary_excluded_from_taxable_income_but_counted_in_total() {
        let events = [
            income("2024-06-30", Tag::Salary, dec!(1000)),
            income("2024-07-01", Tag::Dividend, dec!(200)),
            income("2024-07-02", Tag::Interest, dec!(50)),
        ];
        let refs: Vec<&TaxableEvent> = events.iter().collect();
        let s = summarize(&refs, &[], TaxYear(2025), TaxBand::Basic);

        assert_eq!(s.income.total, dec!(1250));
        assert_eq!(s.income.taxable, dec!(250));
        assert_eq!(s.income.salary, dec!(1000));
        assert_eq!(s.income.by_tag[&Tag::Dividend], dec!(200));
        assert_eq!(s.income.by_tag[&Tag::Interest], dec!(50));
        assert_eq!(s.income.rate, dec!(0.20));
        assert_eq!(s.income.estimated_income_tax, dec!(50.00));
        assert_eq!(s.estimated_total_tax, dec!(50.00));
    }

    #[test]
    fn summarize_cgt_applies_aea_and_band_rate() {
        let events = vec![
            acq("2024-05-01", "BTC", dec!(1), dec!(10000)),
            disp("2024-09-01", "BTC", dec!(1), dec!(20000)),
        ];
        let report = calculate_cgt(events.clone());
        let refs: Vec<&TaxableEvent> = events.iter().collect();
        let disposals: Vec<&DisposalRecord> = report.disposals.iter().collect();

        let s = summarize(&refs, &disposals, TaxYear(2025), TaxBand::Higher);
        assert_eq!(s.cgt.disposal_count, 1);
        assert_eq!(s.cgt.total_proceeds, dec!(20000));
        assert_eq!(s.cgt.total_costs, dec!(10000));
        assert_eq!(s.cgt.total_gain, dec!(10000));
        assert_eq!(s.cgt.summary.aea, TaxYear(2025).cgt_exempt_amount());
        assert_eq!(s.cgt.summary.taxable_gain, dec!(10000) - dec!(3000));
        assert_eq!(s.cgt.rate, TaxYear(2025).cgt_higher_rate());
        assert_eq!(s.cgt.estimated_cgt, s.cgt.summary.estimated_cgt(s.cgt.rate));
        assert_eq!(s.estimated_total_tax, s.cgt.estimated_cgt);
    }
}
