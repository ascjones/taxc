//! Summary command - aggregated totals and tax calculations.

use super::filter::{EventFilter, FilterArgs};
use super::read_events;
use crate::core::{
    calculate_cgt, CgtReport, DisposalRecord, EventType, Tag, TaxBand, TaxYear, TaxableEvent,
};
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
        let cgt_report = calculate_cgt(all_events.clone())?;
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
        let scope = summary_scope_label(filter);
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

        let rate_year = filter.rate_year();

        let disposals = filtered_classified_disposals(cgt_report, filter);
        let total_proceeds: Decimal = disposals.iter().map(|d| d.proceeds_gbp).sum();
        let total_costs: Decimal = disposals
            .iter()
            .map(|d| d.allowable_cost_gbp + d.fees_gbp)
            .sum();
        let total_gain: Decimal = disposals.iter().map(|d| d.gain_gbp).sum();

        let exempt_amount = rate_year.cgt_exempt_amount();
        let basic_rate = rate_year.cgt_basic_rate();
        let higher_rate = rate_year.cgt_higher_rate();

        let taxable_gain = (total_gain - exempt_amount).max(Decimal::ZERO);
        let tax_basic = (taxable_gain * basic_rate).round_dp(2);
        let tax_higher = (taxable_gain * higher_rate).round_dp(2);

        println!("CAPITAL GAINS");
        println!("  Disposals: {}", disposals.len());
        println!(
            "  Proceeds: {} | Costs: {} | Gain: {}",
            format_gbp(total_proceeds),
            format_gbp(total_costs),
            format_gbp_signed(total_gain)
        );
        println!(
            "  Exempt: {} | Taxable: {}",
            format_gbp(exempt_amount),
            format_gbp_signed(taxable_gain)
        );
        println!(
            "  CGT @ {:.0}%: {} | @ {:.0}%: {}",
            basic_rate * dec!(100),
            format_gbp(tax_basic),
            higher_rate * dec!(100),
            format_gbp(tax_higher)
        );
        println!();

        let income_rate = rate_year.income_rate(band);
        let (income, dividend_income, interest_income) = income_totals(events);
        let income_tax = (income * income_rate).round_dp(2);

        println!("INCOME");
        if income > Decimal::ZERO {
            println!(
                "  Income: {} (Tax @ {:.0}%: {})",
                format_gbp(income),
                income_rate * dec!(100),
                format_gbp(income_tax)
            );
        } else {
            println!("  Income: £0.00");
        }
        println!("  Dividend: {}", format_gbp(dividend_income));
        println!("  Interest: {}", format_gbp(interest_income));
        println!();

        let cgt_tax = match band {
            TaxBand::Basic => tax_basic,
            TaxBand::Higher | TaxBand::Additional => tax_higher,
        };
        let total_tax = cgt_tax + income_tax;

        println!(
            "TOTAL TAX LIABILITY: {} ({})",
            format_gbp(total_tax),
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
        let rate_year = filter.rate_year();
        let cgt_rate = match band {
            TaxBand::Basic => rate_year.cgt_basic_rate(),
            TaxBand::Higher | TaxBand::Additional => rate_year.cgt_higher_rate(),
        };
        let income_rate = rate_year.income_rate(band);

        let disposals = filtered_classified_disposals(cgt_report, filter);
        let gross_gains: Decimal = disposals
            .iter()
            .filter(|d| d.gain_gbp > Decimal::ZERO)
            .map(|d| d.gain_gbp)
            .sum();
        let in_year_losses: Decimal = disposals
            .iter()
            .filter(|d| d.gain_gbp < Decimal::ZERO)
            .map(|d| d.gain_gbp.abs())
            .sum();
        let net_gain_before_aea = gross_gains - in_year_losses;
        let aea = rate_year.cgt_exempt_amount();
        let taxable_gain = (net_gain_before_aea - aea).max(Decimal::ZERO);
        let estimated_cgt = (taxable_gain * cgt_rate).round_dp(2);

        let (income, dividend_income, interest_income) = income_totals(events);
        let estimated_income_tax = (income * income_rate).round_dp(2);
        let estimated_total_tax = estimated_cgt + estimated_income_tax;

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
            disposal_count: disposals.len(),
            gross_gains: decimal_to_f64(gross_gains),
            in_year_losses: decimal_to_f64(in_year_losses),
            net_gain_before_aea: decimal_to_f64(net_gain_before_aea),
            aea: decimal_to_f64(aea),
            taxable_gain: decimal_to_f64(taxable_gain),
            cgt_rate_pct: decimal_pct(cgt_rate),
            estimated_cgt: decimal_to_f64(estimated_cgt),
            income: decimal_to_f64(income),
            dividend_income: decimal_to_f64(dividend_income),
            interest_income: decimal_to_f64(interest_income),
            income_rate_pct: decimal_pct(income_rate),
            estimated_income_tax: decimal_to_f64(estimated_income_tax),
            estimated_total_tax: decimal_to_f64(estimated_total_tax),
            currency: "GBP",
        };

        println!("{}", serde_json::to_string_pretty(&data)?);
        Ok(())
    }
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

fn income_totals(events: &[&TaxableEvent]) -> (Decimal, Decimal, Decimal) {
    let mut income = Decimal::ZERO;
    let mut dividend = Decimal::ZERO;
    let mut interest = Decimal::ZERO;

    for event in events {
        if event.event_type != EventType::Acquisition || !event.tag.is_income() {
            continue;
        }
        income += event.value_gbp;
        match event.tag {
            Tag::Dividend => dividend += event.value_gbp,
            Tag::Interest => interest += event.value_gbp,
            _ => {}
        }
    }

    (income, dividend, interest)
}

fn summary_scope_label(filter: &EventFilter) -> String {
    match (filter.from, filter.to) {
        (None, None) => "All Years".to_string(),
        (Some(from), Some(to)) => {
            let tax_year = TaxYear::from_date(from);
            if from == tax_year_start(tax_year) && to == tax_year_end(tax_year) {
                tax_year.display()
            } else {
                format!("{} to {}", date_str(from), date_str(to))
            }
        }
        (Some(from), None) => format!("From {}", date_str(from)),
        (None, Some(to)) => format!("Up to {}", date_str(to)),
    }
}

fn tax_year_start(year: TaxYear) -> NaiveDate {
    NaiveDate::from_ymd_opt(year.0 - 1, 4, 6).unwrap()
}

fn tax_year_end(year: TaxYear) -> NaiveDate {
    NaiveDate::from_ymd_opt(year.0, 4, 5).unwrap()
}

fn band_label(band: TaxBand) -> &'static str {
    match band {
        TaxBand::Basic => "basic",
        TaxBand::Higher => "higher",
        TaxBand::Additional => "additional",
    }
}

fn decimal_to_f64(d: Decimal) -> f64 {
    format!("{:.2}", d).parse::<f64>().unwrap_or(0.0)
}

fn decimal_pct(rate: Decimal) -> u8 {
    format!("{:.0}", rate * dec!(100))
        .parse::<u8>()
        .unwrap_or_default()
}

fn date_str(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn format_gbp(amount: Decimal) -> String {
    format!("£{:.2}", amount)
}

fn format_gbp_signed(amount: Decimal) -> String {
    if amount < Decimal::ZERO {
        format!("-£{:.2}", amount.abs())
    } else {
        format!("£{:.2}", amount)
    }
}
