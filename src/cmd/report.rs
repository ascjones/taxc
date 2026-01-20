use crate::events;
use crate::tax::cgt::CgtReport;
use crate::tax::income::IncomeReport;
use crate::tax::{calculate_cgt, calculate_income_tax, TaxBand, TaxYear};
use clap::{Args, ValueEnum};
use rust_decimal::Decimal;
use std::{fs::File, io, path::PathBuf};

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum ReportType {
    /// Capital Gains Tax report only
    Cgt,
    /// Income Tax report only
    Income,
    /// Both CGT and Income Tax reports
    #[default]
    All,
}

#[derive(Args, Debug)]
pub struct ReportCommand {
    /// CSV file containing taxable events
    #[arg(short, long)]
    events: PathBuf,

    /// Tax year to report (e.g., 2025 for 2024/25)
    #[arg(short, long)]
    year: Option<i32>,

    /// Tax band for income tax calculation
    #[arg(short, long, value_enum, default_value_t = TaxBandArg::Basic)]
    tax_band: TaxBandArg,

    /// Type of report to generate
    #[arg(short, long, value_enum, default_value_t = ReportType::All)]
    report: ReportType,

    /// Output as CSV instead of formatted table
    #[arg(long)]
    csv: bool,
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

impl ReportCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let tax_band: TaxBand = self.tax_band.into();
        let tax_year = self.year.map(TaxYear);
        let all_events = events::read_csv(File::open(&self.events)?)?;

        match self.report {
            ReportType::Cgt => self.report_cgt(all_events, tax_year),
            ReportType::Income => self.report_income(all_events, tax_year, tax_band),
            ReportType::All => {
                self.report_cgt(all_events.clone(), tax_year)?;
                println!();
                self.report_income(all_events, tax_year, tax_band)
            }
        }
    }

    fn report_cgt(
        &self,
        events: Vec<events::TaxableEvent>,
        year: Option<TaxYear>,
    ) -> color_eyre::Result<()> {
        let report = calculate_cgt(events);

        if self.csv {
            report.write_csv(io::stdout(), year)
        } else {
            self.print_cgt_report(&report, year);
            Ok(())
        }
    }

    fn print_cgt_report(&self, report: &CgtReport, year: Option<TaxYear>) {
        let year_str = year.map_or("All Years".to_string(), |y| y.display());

        // Get disposals for the year
        let disposals: Vec<_> = report
            .disposals
            .iter()
            .filter(|d| year.map_or(true, |y| d.tax_year == y))
            .collect();

        // Calculate totals
        let total_proceeds = report.total_proceeds(year);
        let total_costs = report.total_allowable_costs(year);
        let total_gain = report.total_gain(year);

        // Get tax rates
        let (exempt_amount, basic_rate, higher_rate) = match year {
            Some(y) => (y.cgt_exempt_amount(), y.cgt_basic_rate(), y.cgt_higher_rate()),
            None => {
                let current = TaxYear(2025);
                (
                    current.cgt_exempt_amount(),
                    current.cgt_basic_rate(),
                    current.cgt_higher_rate(),
                )
            }
        };

        let taxable_gain = (total_gain - exempt_amount).max(Decimal::ZERO);
        let tax_basic = (taxable_gain * basic_rate).round_dp(2);
        let tax_higher = (taxable_gain * higher_rate).round_dp(2);

        // Print header
        println!("╔══════════════════════════════════════════════════════════════════════════════╗");
        println!("║                    CAPITAL GAINS TAX REPORT ({:^10})                    ║", year_str);
        println!("╠══════════════════════════════════════════════════════════════════════════════╣");

        if disposals.is_empty() {
            println!("║  No disposals found                                                          ║");
        } else {
            // Print disposals table header
            println!("║  Date       │ Asset │    Proceeds │        Cost │        Fees │        Gain ║");
            println!("╟─────────────┼───────┼─────────────┼─────────────┼─────────────┼─────────────╢");

            // Print each disposal
            for d in &disposals {
                println!(
                    "║  {} │ {:>5} │ {:>11} │ {:>11} │ {:>11} │ {:>11} ║",
                    d.date.format("%Y-%m-%d"),
                    truncate(&d.asset, 5),
                    format_gbp(d.proceeds_gbp),
                    format_gbp(d.allowable_cost_gbp),
                    format_gbp(d.fees_gbp),
                    format_gbp_signed(d.gain_gbp),
                );
            }

            // Print totals row
            println!("╟─────────────┴───────┼─────────────┼─────────────┼─────────────┼─────────────╢");
            println!(
                "║              TOTALS │ {:>11} │ {:>11} │             │ {:>11} ║",
                format_gbp(total_proceeds),
                format_gbp(total_costs),
                format_gbp_signed(total_gain),
            );
        }

        // Print summary section
        println!("╠══════════════════════════════════════════════════════════════════════════════╣");
        println!("║  SUMMARY                                                                     ║");
        println!("╟──────────────────────────────────────────────────────────────────────────────╢");
        println!("║  Total Gain/Loss:          {:>12}                                      ║", format_gbp_signed(total_gain));
        println!("║  Annual Exempt Amount:     {:>12}                                      ║", format_gbp(exempt_amount));
        println!("║  Taxable Gain:             {:>12}                                      ║", format_gbp_signed(taxable_gain));
        println!("╟──────────────────────────────────────────────────────────────────────────────╢");
        println!(
            "║  Tax @ {:>5.1}% (basic):    {:>12}                                      ║",
            basic_rate * rust_decimal_macros::dec!(100),
            format_gbp(tax_basic)
        );
        println!(
            "║  Tax @ {:>5.1}% (higher):   {:>12}                                      ║",
            higher_rate * rust_decimal_macros::dec!(100),
            format_gbp(tax_higher)
        );
        println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    }

    fn report_income(
        &self,
        events: Vec<events::TaxableEvent>,
        year: Option<TaxYear>,
        band: TaxBand,
    ) -> color_eyre::Result<()> {
        let report = calculate_income_tax(events);

        if self.csv {
            report.write_csv(io::stdout(), year)
        } else {
            self.print_income_report(&report, year, band);
            Ok(())
        }
    }

    fn print_income_report(&self, report: &IncomeReport, year: Option<TaxYear>, band: TaxBand) {
        let years = match year {
            Some(y) => vec![y],
            None => report.tax_years(),
        };

        for tax_year in years {
            let tax = report.calculate_tax(tax_year, band);
            let band_str = match band {
                TaxBand::Basic => "Basic",
                TaxBand::Higher => "Higher",
                TaxBand::Additional => "Additional",
            };

            println!("╔══════════════════════════════════════════════════════════════════════════════╗");
            println!("║                      INCOME TAX REPORT ({:^10})                        ║", tax_year.display());
            println!("║                           Tax Band: {:^10}                              ║", band_str);
            println!("╠══════════════════════════════════════════════════════════════════════════════╣");

            // Staking section
            println!("║  STAKING REWARDS                                                             ║");
            println!("╟──────────────────────────────────────────────────────────────────────────────╢");
            println!("║  Total Staking Income:     {:>12}                                      ║", format_gbp(tax.staking_income));
            println!(
                "║  Tax @ {:>5.1}%:            {:>12}                                      ║",
                tax_year.income_rate(band) * rust_decimal_macros::dec!(100),
                format_gbp(tax.staking_tax)
            );

            // Dividends section
            println!("╠══════════════════════════════════════════════════════════════════════════════╣");
            println!("║  DIVIDENDS                                                                   ║");
            println!("╟──────────────────────────────────────────────────────────────────────────────╢");
            println!("║  Total Dividend Income:    {:>12}                                      ║", format_gbp(tax.dividend_income));
            println!("║  Dividend Allowance Used:  {:>12}                                      ║", format_gbp(tax.dividend_allowance_used));
            println!("║  Taxable Dividends:        {:>12}                                      ║", format_gbp(tax.taxable_dividends));
            println!(
                "║  Tax @ {:>5.2}%:           {:>12}                                      ║",
                tax_year.dividend_rate(band) * rust_decimal_macros::dec!(100),
                format_gbp(tax.dividend_tax)
            );

            // Total
            println!("╠══════════════════════════════════════════════════════════════════════════════╣");
            println!("║  TOTAL INCOME TAX:         {:>12}                                      ║", format_gbp(tax.total_tax));
            println!("╚══════════════════════════════════════════════════════════════════════════════╝");
        }
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

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        format!("{:>width$}", s, width = max_len)
    } else {
        s[..max_len].to_string()
    }
}
