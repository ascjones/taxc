//! Summary command - aggregated totals and tax calculations.

use super::filter::{EventFilter, FilterArgs};
use super::format::{format_gbp, format_gbp_signed};
use super::read_events;
use crate::core::fmt::{iso_date, pence_string, round_tax};
use crate::core::{
    calculate_cgt, cgt_rate_change_2024, cgt_rate_on, summarize_by_year, CgtReport, DisposalRecord,
    TaxBand, TaxSummary,
};
use clap::{Args, ValueEnum};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct SummaryCommand {
    /// Transactions file (JSON). Reads from stdin if not specified.
    #[arg(default_value = "-")]
    file: PathBuf,

    /// Filter by asset (e.g., BTC, ETH, DOT).
    #[arg(short, long)]
    asset: Option<String>,

    /// Tax band for income tax calculation.
    #[arg(short, long, value_enum, default_value_t = TaxBandArg::Basic)]
    tax_band: TaxBandArg,

    /// Output as JSON instead of formatted text.
    #[arg(long)]
    json: bool,

    /// Don't include unlinked deposits/withdrawals in calculations.
    #[arg(long)]
    exclude_unlinked: bool,

    #[command(flatten)]
    filter: FilterArgs,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TaxBandArg {
    Basic,
    Higher,
    Additional,
}

impl From<TaxBandArg> for TaxBand {
    fn from(arg: TaxBandArg) -> Self {
        match arg {
            TaxBandArg::Basic => TaxBand::Basic,
            TaxBandArg::Higher => TaxBand::Higher,
            TaxBandArg::Additional => TaxBand::Additional,
        }
    }
}

/// `summary --json`. The top-level figures are totals over `years`; with a
/// single tax year they are that year's figures.
#[derive(Debug, Serialize)]
struct SummaryJson {
    tax_year: String,
    filters: SummaryFilters,
    tax_band: String,
    #[serde(flatten)]
    totals: Figures,
    /// Unclassified disposals in range, which the figures leave out.
    unclassified_disposal_count: usize,
    years: Vec<YearJson>,
    currency: &'static str,
}

#[derive(Debug, Serialize)]
struct YearJson {
    tax_year: String,
    #[serde(flatten)]
    figures: Figures,
}

#[derive(Debug, Serialize)]
struct Figures {
    disposal_count: usize,
    gross_gains: String,
    in_year_losses: String,
    net_gain_before_aea: String,
    aea: String,
    taxable_gain: String,
    /// `None` when no single rate explains `estimated_cgt`: the years summed
    /// apply different rates, or a year's rate changed mid-year (2024/25).
    cgt_rate_pct: Option<u8>,
    estimated_cgt: String,
    income: String,
    salary_income: String,
    dividend_income: String,
    interest_income: String,
    income_rate_pct: Option<u8>,
    /// Dividend rate in percent, e.g. 8.75; `None` when the years differ.
    dividend_rate_pct: Option<f64>,
    dividend_allowance: String,
    estimated_income_tax: String,
    estimated_total_tax: String,
}

impl Figures {
    /// Sum the figures of one or more tax years.
    fn total(years: &[TaxSummary]) -> Self {
        let sum = |f: fn(&TaxSummary) -> Decimal| pence_string(years.iter().map(f).sum());
        // A rate is reported only when every year shares it.
        let common = |f: fn(&TaxSummary) -> Decimal| {
            let first = f(&years[0]);
            years.iter().all(|y| f(y) == first).then_some(first)
        };
        // ...and, for CGT, only when it actually reproduces each estimate.
        let cgt_rate = common(|y| y.cgt.rate).filter(|&rate| {
            years
                .iter()
                .all(|y| round_tax(y.cgt.summary.taxable_gain * rate) == y.cgt.estimated_cgt)
        });
        Figures {
            disposal_count: years.iter().map(|y| y.cgt.disposal_count).sum(),
            gross_gains: sum(|y| y.cgt.summary.gross_gains),
            in_year_losses: sum(|y| y.cgt.summary.in_year_losses),
            net_gain_before_aea: sum(|y| y.cgt.summary.net_gain_before_aea),
            aea: sum(|y| y.cgt.summary.aea),
            taxable_gain: sum(|y| y.cgt.summary.taxable_gain),
            cgt_rate_pct: cgt_rate.map(decimal_pct),
            estimated_cgt: sum(|y| y.cgt.estimated_cgt),
            income: sum(|y| y.income.taxable),
            salary_income: sum(|y| y.income.salary),
            dividend_income: sum(|y| y.income.dividend),
            interest_income: sum(|y| y.income.interest),
            income_rate_pct: common(|y| y.income.rate).map(decimal_pct),
            dividend_rate_pct: common(|y| y.income.dividend_rate)
                .and_then(|r| (r * dec!(100)).to_f64()),
            dividend_allowance: sum(|y| y.income.dividend_allowance),
            estimated_income_tax: sum(|y| y.income.estimated_income_tax),
            estimated_total_tax: sum(|y| y.estimated_total_tax),
        }
    }
}

#[derive(Debug, Serialize)]
struct SummaryFilters {
    from: Option<String>,
    to: Option<String>,
    asset: Option<String>,
    event_kind: Option<String>,
    exclude_unlinked: bool,
}

impl SummaryCommand {
    pub fn exec(&self) -> anyhow::Result<()> {
        let tax_band: TaxBand = self.tax_band.into();
        let filter = self.filter.build(self.asset.clone())?;
        let all_events = read_events(&self.file, self.exclude_unlinked)?;

        // Keep HMRC matching correct by calculating CGT from all events.
        let cgt_report = calculate_cgt(all_events.clone());
        let filtered_events = filter.apply(&all_events);

        let disposals = filtered_classified_disposals(&cgt_report, &filter);
        let unclassified = cgt_report
            .disposals
            .iter()
            .filter(|d| d.is_unclassified() && filter.matches_disposal(d))
            .count();
        let years = summarize_by_year(
            &filtered_events,
            &disposals,
            tax_band,
            filter.rate_year(&filtered_events),
        );

        if self.json {
            self.print_json(&years, &filter, unclassified)
        } else {
            self.print_summary(&years, &filter, tax_band, unclassified);
            Ok(())
        }
    }

    fn print_summary(
        &self,
        years: &[TaxSummary],
        filter: &EventFilter,
        band: TaxBand,
        unclassified: usize,
    ) {
        let scope = filter.scope_label();
        let band_str = band_label(band);

        println!();
        if let Some(ref asset) = self.asset {
            println!(
                "TAX SUMMARY ({}, {}) - {} rate",
                scope,
                asset.to_uppercase(),
                band_str
            );
        } else {
            println!("TAX SUMMARY ({}) - {} rate", scope, band_str);
        }
        println!();

        for summary in years {
            if years.len() > 1 {
                println!("TAX YEAR {}", summary.tax_year.display());
                println!();
            }
            print_year(summary);
        }

        let total: Decimal = years.iter().map(|y| y.estimated_total_tax).sum();
        println!("TOTAL TAX LIABILITY: {} ({})", format_gbp(total), band_str);
        println!();
        if unclassified > 0 {
            println!(
                "NOTE: {unclassified} unclassified disposal(s) are excluded from these figures; run `taxc report` to review them."
            );
            println!();
        }
    }

    fn print_json(
        &self,
        years: &[TaxSummary],
        filter: &EventFilter,
        unclassified: usize,
    ) -> anyhow::Result<()> {
        let first = &years[0];
        let last = &years[years.len() - 1];
        let tax_year = if years.len() == 1 {
            first.tax_year.display()
        } else {
            format!(
                "{} to {}",
                first.tax_year.display(),
                last.tax_year.display()
            )
        };

        let data = SummaryJson {
            tax_year,
            filters: SummaryFilters {
                from: filter.from.map(iso_date),
                to: filter.to.map(iso_date),
                asset: filter.asset.clone(),
                event_kind: filter.event_kind.map(|k| k.as_str().to_string()),
                exclude_unlinked: self.exclude_unlinked,
            },
            tax_band: band_label(first.tax_band).to_string(),
            totals: Figures::total(years),
            unclassified_disposal_count: unclassified,
            years: years
                .iter()
                .map(|y| YearJson {
                    tax_year: y.tax_year.display(),
                    figures: Figures::total(std::slice::from_ref(y)),
                })
                .collect(),
            currency: "GBP",
        };

        println!("{}", serde_json::to_string_pretty(&data)?);
        Ok(())
    }
}

/// The CAPITAL GAINS and INCOME blocks for one tax year.
fn print_year(summary: &TaxSummary) {
    let cgt = &summary.cgt;
    let income = &summary.income;
    let basic_rate = summary.tax_year.cgt_basic_rate();
    let higher_rate = summary.tax_year.cgt_higher_rate();

    println!("CAPITAL GAINS");
    println!("  Disposals: {}", cgt.disposal_count);
    println!(
        "  Proceeds: {} | Costs: {} | Gain: {}",
        format_gbp(cgt.total_proceeds),
        format_gbp(cgt.total_costs),
        format_gbp_signed(cgt.total_gain)
    );
    println!(
        "  Exempt: {} | Taxable: {}",
        format_gbp(cgt.summary.aea),
        format_gbp_signed(cgt.summary.taxable_gain)
    );
    if summary.tax_year.has_mid_year_cgt_rate_change() {
        // Rates changed part-way through the year, so one percentage would mislead.
        let change = cgt_rate_change_2024();
        let before = change.pred_opt().expect("valid date");
        let pct = |rate: Decimal| rate * dec!(100);
        println!(
            "  CGT basic rate: {} | higher rate: {} ({:.0}%/{:.0}% before {}, {:.0}%/{:.0}% after)",
            format_gbp(cgt.estimated_cgt_basic),
            format_gbp(cgt.estimated_cgt_higher),
            pct(cgt_rate_on(before, TaxBand::Basic)),
            pct(cgt_rate_on(before, TaxBand::Higher)),
            change.format("%-d %b %Y"),
            pct(basic_rate),
            pct(higher_rate),
        );
    } else {
        println!(
            "  CGT @ {:.0}%: {} | @ {:.0}%: {}",
            basic_rate * dec!(100),
            format_gbp(cgt.estimated_cgt_basic),
            higher_rate * dec!(100),
            format_gbp(cgt.estimated_cgt_higher)
        );
    }
    println!();

    println!("INCOME");
    if income.taxable > Decimal::ZERO {
        println!(
            "  Income: {} (Tax: {}; non-dividend income @ {:.0}%)",
            format_gbp(income.taxable),
            format_gbp(income.estimated_income_tax),
            income.rate * dec!(100)
        );
    } else {
        println!("  Income: £0.00");
    }
    println!(
        "  Salary (PAYE): {} (tax deducted at source)",
        format_gbp(income.salary)
    );
    println!(
        "  Dividend: {} (allowance {}, then {}%)",
        format_gbp(income.dividend),
        format_gbp(income.dividend_allowance),
        (income.dividend_rate * dec!(100)).normalize()
    );
    println!("  Interest: {}", format_gbp(income.interest));
    println!();
}

fn filtered_classified_disposals<'a>(
    cgt_report: &'a CgtReport,
    filter: &EventFilter,
) -> Vec<&'a DisposalRecord> {
    cgt_report
        .disposals
        .iter()
        .filter(|d| !d.is_unclassified())
        .filter(|d| filter.matches_disposal(d))
        .collect()
}

fn band_label(band: TaxBand) -> &'static str {
    match band {
        TaxBand::Basic => "basic",
        TaxBand::Higher => "higher",
        TaxBand::Additional => "additional",
    }
}

fn decimal_pct(rate: Decimal) -> u8 {
    (rate * dec!(100)).round().to_u8().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::events::builders::{acq, disp, event};
    use crate::core::{EventType, Tag, TaxYear, TaxableEvent};

    fn year(year: i32, events: &[TaxableEvent]) -> TaxSummary {
        let report = calculate_cgt(events.to_vec());
        let refs: Vec<&TaxableEvent> = events.iter().collect();
        let disposals: Vec<&DisposalRecord> = report.disposals.iter().collect();
        crate::core::summarize(&refs, &disposals, TaxYear(year), TaxBand::Basic)
    }

    fn dividend(date: &str, value: Decimal) -> TaxableEvent {
        event(
            EventType::Acquisition,
            Tag::Dividend,
            date,
            "GBP",
            dec!(1),
            value,
            None,
        )
    }

    #[test]
    fn figures_total_sums_years_and_keeps_shared_rates() {
        let a = year(2024, &[dividend("2023-07-01", dec!(100))]);
        let b = year(2026, &[dividend("2025-07-01", dec!(100))]);
        let f = Figures::total(&[a, b]);
        assert_eq!(f.dividend_income, "200.00");
        assert_eq!(f.aea, "9000.00"); // 6,000 + 3,000
        assert_eq!(f.dividend_rate_pct, Some(8.75));
        assert_eq!(f.income_rate_pct, Some(20));
        // 10% in 2023/24 vs 18% in 2025/26.
        assert_eq!(f.cgt_rate_pct, None);
    }

    #[test]
    fn figures_total_reports_no_cgt_rate_when_it_changed_mid_year() {
        // A 2024/25 gain before 30 Oct 2024 is taxed at 10%, not the 18%
        // year-end rate, so 18 would not reproduce the estimate.
        let events = [
            acq("2024-05-01", "BTC", dec!(1), dec!(1000)),
            disp("2024-09-01", "BTC", dec!(1), dec!(11000)),
        ];
        let f = Figures::total(&[year(2025, &events)]);
        assert_eq!(f.estimated_cgt, "700.00");
        assert_eq!(f.cgt_rate_pct, None);
    }

    #[test]
    fn figures_total_keeps_the_cgt_rate_when_it_explains_the_estimate() {
        let events = [
            acq("2024-11-01", "BTC", dec!(1), dec!(1000)),
            disp("2024-12-01", "BTC", dec!(1), dec!(11000)),
        ];
        let f = Figures::total(&[year(2025, &events)]);
        assert_eq!(f.estimated_cgt, "1260.00");
        assert_eq!(f.cgt_rate_pct, Some(18));
    }
}
