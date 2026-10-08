use rust_decimal::Decimal;
use std::collections::BTreeMap;

use super::cgt::{CgtSummary, DisposalRecord, DisposalTotals};
use super::events::{EventType, Tag, TaxableEvent};
use super::fmt::round_tax;
use super::uk::{cgt_rate_on, TaxBand, TaxYear};
use super::warnings::Warning;

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
    /// CGT rate for the chosen band at the end of the tax year -- a headline
    /// figure only. In 2024/25 gains realised before 30 October 2024 were
    /// taxed at the earlier rate, so `taxable_gain * rate` is not the estimate
    /// there; use `estimated_cgt`.
    pub rate: Decimal,
    /// Estimated CGT for the chosen band.
    pub estimated_cgt: Decimal,
    /// Estimated CGT for a basic-rate taxpayer.
    pub estimated_cgt_basic: Decimal,
    /// Estimated CGT for a higher- or additional-rate taxpayer.
    pub estimated_cgt_higher: Decimal,
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
    /// Income tax rate applied to non-dividend income for the chosen band.
    pub rate: Decimal,
    /// Dividend tax rate for the chosen band.
    pub dividend_rate: Decimal,
    /// Dividends up to this amount are taxed at 0%.
    pub dividend_allowance: Decimal,
    /// Non-dividend income at `rate`, plus dividends above the allowance
    /// at `dividend_rate`.
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
    let summary = CgtSummary::calculate(
        disposals.iter().map(|d| d.gain_gbp),
        rate_year.cgt_exempt_amount(),
    );
    let estimate = |band| estimate_cgt(disposals, rate_year.cgt_exempt_amount(), band);
    let estimated_cgt_basic = estimate(TaxBand::Basic);
    let estimated_cgt_higher = estimate(TaxBand::Higher);
    let estimated_cgt = match band {
        TaxBand::Basic => estimated_cgt_basic,
        TaxBand::Higher | TaxBand::Additional => estimated_cgt_higher,
    };
    let totals: DisposalTotals = disposals.iter().copied().collect();
    let cgt = CgtPosition {
        disposal_count: disposals.len(),
        total_proceeds: totals.proceeds,
        total_costs: totals.costs,
        total_gain: totals.gain,
        summary,
        rate: match band {
            TaxBand::Basic => rate_year.cgt_basic_rate(),
            TaxBand::Higher | TaxBand::Additional => rate_year.cgt_higher_rate(),
        },
        estimated_cgt,
        estimated_cgt_basic,
        estimated_cgt_higher,
    };

    let income_rate = band.income_rate();
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
    let dividend = tag_total(Tag::Dividend);
    let dividend_rate = rate_year.dividend_rate(band);
    let dividend_allowance = rate_year.dividend_allowance();
    let estimated_income_tax = round_tax((taxable - dividend) * income_rate)
        + round_tax((dividend - dividend_allowance).max(Decimal::ZERO) * dividend_rate);
    let income = IncomePosition {
        total,
        taxable,
        salary,
        dividend,
        interest: tag_total(Tag::Interest),
        by_tag,
        rate: income_rate,
        dividend_rate,
        dividend_allowance,
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

/// Estimated CGT on one tax year's disposals at a band's rates.
///
/// Each gain is taxed at the rate in force on its disposal date (which only
/// differs within 2024/25). Losses and the AEA are set against the
/// highest-rate gains first -- HMRC lets the taxpayer allocate them, and
/// that allocation gives the lowest liability.
fn estimate_cgt(disposals: &[&DisposalRecord], aea: Decimal, band: TaxBand) -> Decimal {
    let mut gains_by_rate: BTreeMap<Decimal, Decimal> = BTreeMap::new();
    let mut deductions = aea;
    for d in disposals {
        if d.gain_gbp > Decimal::ZERO {
            *gains_by_rate.entry(cgt_rate_on(d.date, band)).or_default() += d.gain_gbp;
        } else {
            deductions -= d.gain_gbp;
        }
    }
    let mut tax = Decimal::ZERO;
    for (rate, gains) in gains_by_rate.into_iter().rev() {
        let offset = deductions.min(gains);
        deductions -= offset;
        tax += (gains - offset) * rate;
    }
    round_tax(tax)
}

/// Summarise each tax year the events and disposals fall in, in year order.
///
/// Each year gets its own AEA and rates; netting losses or applying one AEA
/// across years would be wrong. When there is nothing to summarise, the
/// `fallback` year is summarised so callers still get that year's allowances.
pub fn summarize_by_year(
    events: &[&TaxableEvent],
    disposals: &[&DisposalRecord],
    band: TaxBand,
    fallback: TaxYear,
) -> Vec<TaxSummary> {
    let mut years: BTreeMap<TaxYear, (Vec<&TaxableEvent>, Vec<&DisposalRecord>)> = BTreeMap::new();
    for &event in events {
        years
            .entry(TaxYear::from_date(event.date()))
            .or_default()
            .0
            .push(event);
    }
    for &disposal in disposals {
        years
            .entry(TaxYear::from_date(disposal.date))
            .or_default()
            .1
            .push(disposal);
    }
    if years.is_empty() {
        years.insert(fallback, Default::default());
    }
    years
        .into_iter()
        .map(|(year, (events, disposals))| summarize(&events, &disposals, year, band))
        .collect()
}

/// Warnings attached to one event: an unclassified tag, plus whatever CGT
/// matching recorded on its disposal (deduplicated).
pub fn event_warnings(event: &TaxableEvent, disposal: Option<&DisposalRecord>) -> Vec<Warning> {
    let mut warnings = if event.tag == Tag::Unclassified {
        vec![Warning::UnclassifiedEvent]
    } else {
        Vec::new()
    };
    if let Some(d) = disposal {
        for warning in &d.warnings {
            if !warnings.contains(warning) {
                warnings.push(warning.clone());
            }
        }
    }
    warnings
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
        // The £200 dividend is inside the £500 dividend allowance; only the
        // £50 interest is taxed, at 20%.
        assert_eq!(s.income.estimated_income_tax, dec!(10.00));
        assert_eq!(s.estimated_total_tax, dec!(10.00));
    }

    #[test]
    fn event_warnings_dedups_unclassified_seed_against_disposal_warnings() {
        let mut d = disp("2024-09-01", "BTC", dec!(1), dec!(20000));
        d.tag = Tag::Unclassified;
        let report = calculate_cgt(vec![d.clone()]);
        let record = &report.disposals[0];
        assert!(record.warnings.contains(&Warning::UnclassifiedEvent));

        let warnings = event_warnings(&d, Some(record));
        assert_eq!(
            warnings,
            vec![
                Warning::UnclassifiedEvent,
                Warning::InsufficientCostBasis {
                    available: dec!(0),
                    required: dec!(1),
                },
            ]
        );
        assert!(event_warnings(&d, None).contains(&Warning::UnclassifiedEvent));
    }

    #[test]
    fn summarize_by_year_applies_each_years_own_aea() {
        // £10,000 gain in 2022/23 (AEA £12,300) and £8,000 in 2024/25
        // (AEA £3,000). One AEA over both would be wrong either way.
        let events = vec![
            acq("2022-05-01", "BTC", dec!(1), dec!(1000)),
            disp("2022-06-01", "BTC", dec!(1), dec!(11000)),
            acq("2024-06-01", "BTC", dec!(1), dec!(1000)),
            disp("2024-12-01", "BTC", dec!(1), dec!(9000)),
        ];
        let report = calculate_cgt(events.clone());
        let refs: Vec<&TaxableEvent> = events.iter().collect();
        let disposals: Vec<&DisposalRecord> = report.disposals.iter().collect();

        let years = summarize_by_year(&refs, &disposals, TaxBand::Basic, TaxYear(2025));
        assert_eq!(years.len(), 2);
        assert_eq!(years[0].tax_year, TaxYear(2023));
        assert_eq!(years[0].cgt.summary.aea, dec!(12300));
        assert_eq!(years[0].cgt.estimated_cgt, dec!(0));
        assert_eq!(years[1].tax_year, TaxYear(2025));
        assert_eq!(years[1].cgt.summary.taxable_gain, dec!(5000));
        assert_eq!(years[1].cgt.estimated_cgt, dec!(900.00));
    }

    #[test]
    fn summarize_rounds_tax_down_to_the_penny() {
        // HMRC rounds tax down to the whole penny: 1000.3056 x 18% = 180.055008.
        let events = vec![
            acq("2024-11-01", "BTC", dec!(1), dec!(1000)),
            disp("2024-12-01", "BTC", dec!(1), dec!(5000.3056)),
        ];
        let report = calculate_cgt(events.clone());
        let refs: Vec<&TaxableEvent> = events.iter().collect();
        let disposals: Vec<&DisposalRecord> = report.disposals.iter().collect();
        let s = summarize(&refs, &disposals, TaxYear(2025), TaxBand::Basic);
        assert_eq!(s.cgt.estimated_cgt, dec!(180.05));

        // 1,000.07 of interest at 20% = 200.014 -> 200.01
        let interest = [income("2024-07-01", Tag::Interest, dec!(1000.07))];
        let refs: Vec<&TaxableEvent> = interest.iter().collect();
        let s = summarize(&refs, &[], TaxYear(2025), TaxBand::Basic);
        assert_eq!(s.income.estimated_income_tax, dec!(200.01));
    }

    #[test]
    fn summarize_additional_rate_uses_the_higher_cgt_rate() {
        let events = vec![
            acq("2024-11-01", "BTC", dec!(1), dec!(1000)),
            disp("2024-12-01", "BTC", dec!(1), dec!(11000)),
        ];
        let report = calculate_cgt(events.clone());
        let refs: Vec<&TaxableEvent> = events.iter().collect();
        let disposals: Vec<&DisposalRecord> = report.disposals.iter().collect();
        let s = summarize(&refs, &disposals, TaxYear(2025), TaxBand::Additional);
        // (10,000 - 3,000 AEA) x 24%
        assert_eq!(s.cgt.estimated_cgt, dec!(1680.00));
    }

    #[test]
    fn summarize_by_year_with_no_events_uses_fallback_year() {
        let years = summarize_by_year(&[], &[], TaxBand::Basic, TaxYear(2025));
        assert_eq!(years.len(), 1);
        assert_eq!(years[0].tax_year, TaxYear(2025));
        assert_eq!(years[0].estimated_total_tax, dec!(0));
    }

    #[test]
    fn summarize_2024_25_taxes_gains_at_the_rate_on_their_disposal_date() {
        // £10,000 gain before 30 Oct 2024 (10% basic) and £10,000 after (18%).
        // The £3,000 AEA goes against the 18% gain, the cheaper allocation.
        let events = vec![
            acq("2024-05-01", "BTC", dec!(2), dec!(2000)),
            disp("2024-09-01", "BTC", dec!(1), dec!(11000)),
            disp("2024-11-01", "BTC", dec!(1), dec!(11000)),
        ];
        let report = calculate_cgt(events.clone());
        let refs: Vec<&TaxableEvent> = events.iter().collect();
        let disposals: Vec<&DisposalRecord> = report.disposals.iter().collect();

        let basic = summarize(&refs, &disposals, TaxYear(2025), TaxBand::Basic);
        assert_eq!(basic.cgt.summary.taxable_gain, dec!(17000));
        // 10,000 x 10% + 7,000 x 18%
        assert_eq!(basic.cgt.estimated_cgt, dec!(2260.00));
        assert_eq!(basic.cgt.estimated_cgt_basic, dec!(2260.00));
        // 10,000 x 20% + 7,000 x 24%
        assert_eq!(basic.cgt.estimated_cgt_higher, dec!(3680.00));

        let higher = summarize(&refs, &disposals, TaxYear(2025), TaxBand::Higher);
        assert_eq!(higher.cgt.estimated_cgt, dec!(3680.00));
    }

    #[test]
    fn summarize_losses_offset_the_highest_rate_gains_first() {
        // Pre-change gain 10,000 (10%), post-change gain 2,000 (18%), a
        // post-change loss of 4,000. Losses + AEA (7,000) wipe the 18% gain
        // first and the remaining 5,000 comes off the 10% gain.
        let events = vec![
            acq("2024-05-01", "BTC", dec!(1), dec!(1000)),
            disp("2024-09-01", "BTC", dec!(1), dec!(11000)),
            acq("2024-05-01", "ETH", dec!(2), dec!(10000)),
            disp("2024-11-01", "ETH", dec!(1), dec!(7000)),
            disp("2024-11-02", "ETH", dec!(1), dec!(1000)),
        ];
        let report = calculate_cgt(events.clone());
        let refs: Vec<&TaxableEvent> = events.iter().collect();
        let disposals: Vec<&DisposalRecord> = report.disposals.iter().collect();
        let s = summarize(&refs, &disposals, TaxYear(2025), TaxBand::Basic);
        assert_eq!(s.cgt.summary.taxable_gain, dec!(5000));
        assert_eq!(s.cgt.estimated_cgt, dec!(500.00));
    }

    #[test]
    fn summarize_taxes_dividends_at_dividend_rates_after_the_allowance() {
        // 2024/25 basic rate: £1,000 dividends, £500 allowance, 8.75%.
        // £200 interest at 20%.
        let events = [
            income("2024-07-01", Tag::Dividend, dec!(1000)),
            income("2024-07-02", Tag::Interest, dec!(200)),
        ];
        let refs: Vec<&TaxableEvent> = events.iter().collect();
        let s = summarize(&refs, &[], TaxYear(2025), TaxBand::Basic);
        assert_eq!(s.income.dividend_allowance, dec!(500));
        assert_eq!(s.income.dividend_rate, dec!(0.0875));
        // 500 x 8.75% = 43.75, plus 200 x 20% = 40.00
        assert_eq!(s.income.estimated_income_tax, dec!(83.75));
    }

    #[test]
    fn summarize_dividends_within_the_allowance_are_untaxed() {
        let events = [income("2024-07-01", Tag::Dividend, dec!(400))];
        let refs: Vec<&TaxableEvent> = events.iter().collect();
        let s = summarize(&refs, &[], TaxYear(2025), TaxBand::Higher);
        assert_eq!(s.income.estimated_income_tax, dec!(0));
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
        // Disposed of on 1 Sep 2024, before the 30 Oct rate change: 20%.
        assert_eq!(s.cgt.estimated_cgt, dec!(1400.00));
        assert_eq!(s.estimated_total_tax, dec!(1400.00));
    }
}
