//! Summary command - aggregated totals and tax calculations.

use super::filter::{EventFilter, FilterArgs};
use super::format::{format_gbp, format_gbp_signed};
use super::read_events;
use crate::core::{calculate_cgt, summarize, CgtReport, DisposalRecord, TaxBand, TaxableEvent};
use chrono::NaiveDate;
use clap::{Args, ValueEnum};
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

#[derive(Debug, Serialize)]
struct SummaryJson {
    tax_year: String,
    filters: SummaryFilters,
    tax_band: String,
    disposal_count: usize,
    gross_gains: f64,
    in_year_losses: f64,
    net_gain_before_aea: f64,
    aea: f64,
    taxable_gain: f64,
    cgt_rate_pct: u8,
    estimated_cgt: f64,
    income: f64,
    salary_income: f64,
    dividend_income: f64,
    interest_income: f64,
    income_rate_pct: u8,
    estimated_income_tax: f64,
    estimated_total_tax: f64,
    currency: &'static str,
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

        if self.json {
            self.print_json(&filtered_events, &cgt_report, &filter, tax_band)
        } else {
            self.print_summary(&filtered_events, &cgt_report, &filter, tax_band);
            Ok(())
        }
    }

    fn print_summary(
        &self,
        events: &[&TaxableEvent],
        cgt_report: &CgtReport,
        filter: &EventFilter,
        band: TaxBand,
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

        let rate_year = filter.rate_year(events);
        let disposals = filtered_classified_disposals(cgt_report, filter);
        let summary = summarize(events, &disposals, rate_year, band);
        let cgt = &summary.cgt;
        let income = &summary.income;

        let basic_rate = rate_year.cgt_basic_rate();
        let higher_rate = rate_year.cgt_higher_rate();

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
        println!(
            "  CGT @ {:.0}%: {} | @ {:.0}%: {}",
            basic_rate * dec!(100),
            format_gbp(cgt.summary.estimated_cgt(basic_rate)),
            higher_rate * dec!(100),
            format_gbp(cgt.summary.estimated_cgt(higher_rate))
        );
        println!();

        println!("INCOME");
        if income.taxable > Decimal::ZERO {
            println!(
                "  Income: {} (Tax @ {:.0}%: {})",
                format_gbp(income.taxable),
                income.rate * dec!(100),
                format_gbp(income.estimated_income_tax)
            );
        } else {
            println!("  Income: £0.00");
        }
        println!(
            "  Salary (PAYE): {} (tax deducted at source)",
            format_gbp(income.salary)
        );
        println!("  Dividend: {}", format_gbp(income.dividend));
        println!("  Interest: {}", format_gbp(income.interest));
        println!();

        println!(
            "TOTAL TAX LIABILITY: {} ({})",
            format_gbp(summary.estimated_total_tax),
            band_str
        );
        println!();
    }

    fn print_json(
        &self,
        events: &[&TaxableEvent],
        cgt_report: &CgtReport,
        filter: &EventFilter,
        band: TaxBand,
    ) -> anyhow::Result<()> {
        let rate_year = filter.rate_year(events);
        let disposals = filtered_classified_disposals(cgt_report, filter);
        let summary = summarize(events, &disposals, rate_year, band);
        let cgt = &summary.cgt;
        let income = &summary.income;

        let data = SummaryJson {
            tax_year: rate_year.display(),
            filters: SummaryFilters {
                from: filter.from.map(date_str),
                to: filter.to.map(date_str),
                asset: filter.asset.clone(),
                event_kind: filter.event_kind.map(|k| k.as_str().to_string()),
                exclude_unlinked: self.exclude_unlinked,
            },
            tax_band: band_label(band).to_string(),
            disposal_count: cgt.disposal_count,
            gross_gains: decimal_to_f64(cgt.summary.gross_gains),
            in_year_losses: decimal_to_f64(cgt.summary.in_year_losses),
            net_gain_before_aea: decimal_to_f64(cgt.summary.net_gain_before_aea),
            aea: decimal_to_f64(cgt.summary.aea),
            taxable_gain: decimal_to_f64(cgt.summary.taxable_gain),
            cgt_rate_pct: decimal_pct(cgt.rate),
            estimated_cgt: decimal_to_f64(cgt.estimated_cgt),
            income: decimal_to_f64(income.taxable),
            salary_income: decimal_to_f64(income.salary),
            dividend_income: decimal_to_f64(income.dividend),
            interest_income: decimal_to_f64(income.interest),
            income_rate_pct: decimal_pct(income.rate),
            estimated_income_tax: decimal_to_f64(income.estimated_income_tax),
            estimated_total_tax: decimal_to_f64(summary.estimated_total_tax),
            currency: "GBP",
        };

        println!("{}", serde_json::to_string_pretty(&data)?);
        Ok(())
    }
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

fn decimal_to_f64(d: Decimal) -> f64 {
    use rust_decimal::prelude::ToPrimitive;
    d.round_dp_with_strategy(2, rust_decimal::RoundingStrategy::MidpointAwayFromZero)
        .to_f64()
        .unwrap_or(0.0)
}

fn decimal_pct(rate: Decimal) -> u8 {
    format!("{:.0}", rate * dec!(100))
        .parse::<u8>()
        .unwrap_or_default()
}

fn date_str(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}
