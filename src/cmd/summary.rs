//! Summary command - aggregated totals and tax calculations.

use super::filter::{EventFilter, FilterArgs};
use super::format::{format_gbp, format_gbp_signed};
use super::read_events;
use crate::core::fmt::{iso_date, pence_string};
use crate::core::{
    calculate_cgt, summarize_by_year, CgtReport, DisposalRecord, TaxBand, TaxSummary, TaxYear,
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

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum TaxBandArg {
    #[default]
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
    /// `None` when the years summed apply different rates.
    cgt_rate_pct: Option<u8>,
    estimated_cgt: String,
    income: String,
    salary_income: String,
    dividend_income: String,
    interest_income: String,
    income_rate_pct: Option<u8>,
    /// Dividend rate in percent, to two places (e.g. 8.75).
    dividend_rate_pct: Option<Decimal>,
    dividend_allowance: String,
    estimated_income_tax: String,
    estimated_total_tax: String,
}

impl Figures {
    /// Sum the figures of one or more tax years.
    fn total(years: &[TaxSummary]) -> Self {
        let sum = |f: fn(&TaxSummary) -> Decimal| pence_string(years.iter().map(f).sum());
        let common_pct = |f: fn(&TaxSummary) -> Decimal| {
            let first = f(&years[0]);
            years
                .iter()
                .all(|y| f(y) == first)
                .then(|| decimal_pct(first))
        };
        Figures {
            disposal_count: years.iter().map(|y| y.cgt.disposal_count).sum(),
            gross_gains: sum(|y| y.cgt.summary.gross_gains),
            in_year_losses: sum(|y| y.cgt.summary.in_year_losses),
            net_gain_before_aea: sum(|y| y.cgt.summary.net_gain_before_aea),
            aea: sum(|y| y.cgt.summary.aea),
            taxable_gain: sum(|y| y.cgt.summary.taxable_gain),
            cgt_rate_pct: common_pct(|y| y.cgt.rate),
            estimated_cgt: sum(|y| y.cgt.estimated_cgt),
            income: sum(|y| y.income.taxable),
            salary_income: sum(|y| y.income.salary),
            dividend_income: sum(|y| y.income.dividend),
            interest_income: sum(|y| y.income.interest),
            income_rate_pct: common_pct(|y| y.income.rate),
            dividend_rate_pct: {
                let first = years[0].income.dividend_rate;
                years
                    .iter()
                    .all(|y| y.income.dividend_rate == first)
                    .then(|| (first * dec!(100)).normalize())
            },
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
        let years = summarize_by_year(
            &filtered_events,
            &disposals,
            tax_band,
            filter.rate_year(&filtered_events),
        );

        if self.json {
            self.print_json(&years, &filter)
        } else {
            self.print_summary(&years, &filter, tax_band);
            Ok(())
        }
    }

    fn print_summary(&self, years: &[TaxSummary], filter: &EventFilter, band: TaxBand) {
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
    }

    fn print_json(&self, years: &[TaxSummary], filter: &EventFilter) -> anyhow::Result<()> {
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
    if summary.tax_year == TaxYear(2025) {
        // Rates changed on 30 Oct 2024, so one percentage would mislead.
        println!(
            "  CGT basic rate: {} | higher rate: {} (10%/20% before 30 Oct 2024, {:.0}%/{:.0}% after)",
            format_gbp(cgt.estimated_cgt_basic),
            format_gbp(cgt.estimated_cgt_higher),
            basic_rate * dec!(100),
            higher_rate * dec!(100),
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

pub(crate) fn filtered_classified_disposals<'a>(
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
