//! Summary command - aggregated totals and tax calculations

use crate::cmd::events::read_events;
use crate::tax::cgt::calculate_cgt;
use crate::tax::income::calculate_income_tax;
use crate::tax::{TaxBand, TaxYear};
use clap::{Args, ValueEnum};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct SummaryCommand {
    /// CSV or JSON file containing taxable events
    #[arg(short, long)]
    events: PathBuf,

    /// Tax year to report (e.g., 2025 for 2024/25)
    #[arg(short, long)]
    year: Option<i32>,

    /// Filter by asset (e.g., BTC, ETH, DOT)
    #[arg(short, long)]
    asset: Option<String>,

    /// Tax band for income tax calculation
    #[arg(short, long, value_enum, default_value_t = TaxBandArg::Basic)]
    tax_band: TaxBandArg,

    /// Output as JSON instead of formatted text
    #[arg(long)]
    json: bool,
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

/// Summary data for JSON output
#[derive(Debug, Serialize)]
struct SummaryData {
    tax_year: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    asset: Option<String>,
    tax_band: String,
    capital_gains: CapitalGainsSummary,
    income: IncomeSummary,
    total_tax_liability: String,
}

#[derive(Debug, Serialize)]
struct CapitalGainsSummary {
    disposal_count: usize,
    total_proceeds: String,
    total_costs: String,
    total_gain: String,
    exempt_amount: String,
    taxable_gain: String,
    tax_at_basic_rate: String,
    basic_rate_pct: String,
    tax_at_higher_rate: String,
    higher_rate_pct: String,
}

#[derive(Debug, Serialize)]
struct IncomeSummary {
    staking_income: String,
    staking_tax: String,
    staking_rate_pct: String,
    dividend_income: String,
    dividend_allowance: String,
    taxable_dividends: String,
    dividend_tax: String,
    dividend_rate_pct: String,
}

impl SummaryCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let tax_band: TaxBand = self.tax_band.into();
        let tax_year = self.year.map(TaxYear);
        let (all_events, opening_pools) = read_events(&self.events)?;

        // Filter by asset if specified
        let filtered_events: Vec<_> = if let Some(ref asset) = self.asset {
            all_events
                .into_iter()
                .filter(|e| e.asset.eq_ignore_ascii_case(asset))
                .collect()
        } else {
            all_events
        };

        let cgt_report = calculate_cgt(filtered_events.clone(), opening_pools.as_ref());
        let income_report = calculate_income_tax(filtered_events);

        if self.json {
            self.print_json(&cgt_report, &income_report, tax_year, tax_band)
        } else {
            self.print_summary(&cgt_report, &income_report, tax_year, tax_band);
            Ok(())
        }
    }

    fn print_summary(
        &self,
        cgt_report: &crate::tax::cgt::CgtReport,
        income_report: &crate::tax::income::IncomeReport,
        year: Option<TaxYear>,
        band: TaxBand,
    ) {
        let year_str = year.map_or("All Years".to_string(), |y| y.display());
        let band_str = match band {
            TaxBand::Basic => "basic",
            TaxBand::Higher => "higher",
            TaxBand::Additional => "additional",
        };

        println!();
        if let Some(ref asset) = self.asset {
            println!("TAX SUMMARY ({}, {}) - {} rate", year_str, asset.to_uppercase(), band_str);
        } else {
            println!("TAX SUMMARY ({}) - {} rate", year_str, band_str);
        }
        println!();

        // Get tax year for rates (use provided year or default to current)
        let rate_year = year.unwrap_or(TaxYear(2025));

        // Capital Gains section
        let disposals: Vec<_> = cgt_report
            .disposals
            .iter()
            .filter(|d| year.is_none_or(|y| d.tax_year == y))
            .collect();

        let total_proceeds = cgt_report.total_proceeds(year);
        let total_costs = cgt_report.total_allowable_costs(year);
        let total_gain = cgt_report.total_gain(year);

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

        // Income section
        let income_rate = rate_year.income_rate(band);
        let dividend_rate = rate_year.dividend_rate(band);
        let dividend_allowance = rate_year.dividend_allowance();

        // Calculate income totals
        let staking_income: Decimal = income_report
            .staking_events
            .iter()
            .filter(|e| year.is_none_or(|y| e.tax_year == y))
            .map(|e| e.value_gbp)
            .sum();

        let dividend_income: Decimal = income_report
            .dividend_events
            .iter()
            .filter(|e| year.is_none_or(|y| e.tax_year == y))
            .map(|e| e.value_gbp)
            .sum();

        let staking_tax = (staking_income * income_rate).round_dp(2);
        let dividend_allowance_used = dividend_allowance.min(dividend_income);
        let taxable_dividends = (dividend_income - dividend_allowance_used).max(Decimal::ZERO);
        let dividend_tax = (taxable_dividends * dividend_rate).round_dp(2);

        println!("INCOME");
        if staking_income > Decimal::ZERO {
            println!(
                "  Staking: {} (Tax @ {:.0}%: {})",
                format_gbp(staking_income),
                income_rate * dec!(100),
                format_gbp(staking_tax)
            );
        } else {
            println!("  Staking: £0.00");
        }

        if dividend_income > Decimal::ZERO {
            println!(
                "  Dividends: {} (Allowance: {}, Tax @ {:.2}%: {})",
                format_gbp(dividend_income),
                format_gbp(dividend_allowance_used),
                dividend_rate * dec!(100),
                format_gbp(dividend_tax)
            );
        } else {
            println!("  Dividends: £0.00");
        }
        println!();

        // Total liability
        // For CGT, use the rate that matches the band
        let cgt_tax = match band {
            TaxBand::Basic => tax_basic,
            TaxBand::Higher | TaxBand::Additional => tax_higher,
        };
        let total_tax = cgt_tax + staking_tax + dividend_tax;

        println!("TOTAL TAX LIABILITY: {} ({})", format_gbp(total_tax), band_str);
        println!();
    }

    fn print_json(
        &self,
        cgt_report: &crate::tax::cgt::CgtReport,
        income_report: &crate::tax::income::IncomeReport,
        year: Option<TaxYear>,
        band: TaxBand,
    ) -> color_eyre::Result<()> {
        let year_str = year.map_or("All Years".to_string(), |y| y.display());
        let band_str = match band {
            TaxBand::Basic => "basic",
            TaxBand::Higher => "higher",
            TaxBand::Additional => "additional",
        };

        let rate_year = year.unwrap_or(TaxYear(2025));

        // Calculate CGT values
        let disposals: Vec<_> = cgt_report
            .disposals
            .iter()
            .filter(|d| year.is_none_or(|y| d.tax_year == y))
            .collect();

        let total_proceeds = cgt_report.total_proceeds(year);
        let total_costs = cgt_report.total_allowable_costs(year);
        let total_gain = cgt_report.total_gain(year);

        let exempt_amount = rate_year.cgt_exempt_amount();
        let basic_rate = rate_year.cgt_basic_rate();
        let higher_rate = rate_year.cgt_higher_rate();

        let taxable_gain = (total_gain - exempt_amount).max(Decimal::ZERO);
        let tax_basic = (taxable_gain * basic_rate).round_dp(2);
        let tax_higher = (taxable_gain * higher_rate).round_dp(2);

        // Calculate income values
        let income_rate = rate_year.income_rate(band);
        let dividend_rate = rate_year.dividend_rate(band);
        let dividend_allowance = rate_year.dividend_allowance();

        let staking_income: Decimal = income_report
            .staking_events
            .iter()
            .filter(|e| year.is_none_or(|y| e.tax_year == y))
            .map(|e| e.value_gbp)
            .sum();

        let dividend_income: Decimal = income_report
            .dividend_events
            .iter()
            .filter(|e| year.is_none_or(|y| e.tax_year == y))
            .map(|e| e.value_gbp)
            .sum();

        let staking_tax = (staking_income * income_rate).round_dp(2);
        let dividend_allowance_used = dividend_allowance.min(dividend_income);
        let taxable_dividends = (dividend_income - dividend_allowance_used).max(Decimal::ZERO);
        let dividend_tax = (taxable_dividends * dividend_rate).round_dp(2);

        let cgt_tax = match band {
            TaxBand::Basic => tax_basic,
            TaxBand::Higher | TaxBand::Additional => tax_higher,
        };
        let total_tax = cgt_tax + staking_tax + dividend_tax;

        let data = SummaryData {
            tax_year: year_str,
            asset: self.asset.as_ref().map(|a| a.to_uppercase()),
            tax_band: band_str.to_string(),
            capital_gains: CapitalGainsSummary {
                disposal_count: disposals.len(),
                total_proceeds: format!("{:.2}", total_proceeds),
                total_costs: format!("{:.2}", total_costs),
                total_gain: format!("{:.2}", total_gain),
                exempt_amount: format!("{:.2}", exempt_amount),
                taxable_gain: format!("{:.2}", taxable_gain),
                tax_at_basic_rate: format!("{:.2}", tax_basic),
                basic_rate_pct: format!("{:.0}", basic_rate * dec!(100)),
                tax_at_higher_rate: format!("{:.2}", tax_higher),
                higher_rate_pct: format!("{:.0}", higher_rate * dec!(100)),
            },
            income: IncomeSummary {
                staking_income: format!("{:.2}", staking_income),
                staking_tax: format!("{:.2}", staking_tax),
                staking_rate_pct: format!("{:.0}", income_rate * dec!(100)),
                dividend_income: format!("{:.2}", dividend_income),
                dividend_allowance: format!("{:.2}", dividend_allowance_used),
                taxable_dividends: format!("{:.2}", taxable_dividends),
                dividend_tax: format!("{:.2}", dividend_tax),
                dividend_rate_pct: format!("{:.2}", dividend_rate * dec!(100)),
            },
            total_tax_liability: format!("{:.2}", total_tax),
        };

        println!("{}", serde_json::to_string_pretty(&data)?);
        Ok(())
    }
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
