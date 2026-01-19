use crate::events;
use crate::tax::{calculate_cgt, calculate_income_tax, TaxBand, TaxYear};
use argh::FromArgs;
use std::{fs::File, io, path::PathBuf};

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "report")]
/// Calculate UK taxes from taxable events CSV
pub struct ReportCommand {
    /// the CSV file containing taxable events
    #[argh(option)]
    events: PathBuf,

    /// tax year to report (e.g., 2025 for 2024/25)
    #[argh(option)]
    year: Option<i32>,

    /// tax band for income tax: basic, higher, or additional (default: basic)
    #[argh(option, default = "String::from(\"basic\")")]
    tax_band: String,

    /// report type: cgt, income, or all (default: all)
    #[argh(option, default = "String::from(\"all\")")]
    report: String,
}

impl ReportCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let tax_band = TaxBand::from_str(&self.tax_band)
            .ok_or_else(|| color_eyre::eyre::eyre!("Invalid tax band: {}", self.tax_band))?;

        let tax_year = self.year.map(TaxYear);

        let all_events = events::read_csv(File::open(&self.events)?)?;

        match self.report.as_str() {
            "cgt" => self.report_cgt(all_events, tax_year),
            "income" => self.report_income(all_events, tax_year, tax_band),
            "all" | _ => {
                self.report_cgt(all_events.clone(), tax_year)?;
                eprintln!(); // Separator
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

        let disposal_count = report.disposal_count(year);
        let total_proceeds = report.total_proceeds(year);
        let total_costs = report.total_allowable_costs(year);
        let total_gain = report.total_gain(year);

        // Calculate tax liability
        let (exempt_amount, basic_rate, higher_rate) = match year {
            Some(y) => (y.cgt_exempt_amount(), y.cgt_basic_rate(), y.cgt_higher_rate()),
            None => {
                // Use current year rates as default
                let current = TaxYear(2025);
                (
                    current.cgt_exempt_amount(),
                    current.cgt_basic_rate(),
                    current.cgt_higher_rate(),
                )
            }
        };

        let taxable_gain = (total_gain - exempt_amount).max(rust_decimal::Decimal::ZERO);
        let tax_at_basic = (taxable_gain * basic_rate).round_dp(2);
        let tax_at_higher = (taxable_gain * higher_rate).round_dp(2);

        // Log summary
        let year_str = year.map_or("All years".to_string(), |y| y.display());
        log::info!("=== Capital Gains Tax Report ({}) ===", year_str);
        log::info!("Disposals: {}", disposal_count);
        log::info!("Total proceeds: £{:.2}", total_proceeds);
        log::info!("Total allowable costs: £{:.2}", total_costs);
        log::info!("Total gain/loss: £{:.2}", total_gain);
        log::info!("Annual exempt amount: £{:.2}", exempt_amount);
        log::info!("Taxable gain: £{:.2}", taxable_gain);
        log::info!(
            "Tax liability (basic rate {}%): £{:.2}",
            basic_rate * rust_decimal_macros::dec!(100),
            tax_at_basic
        );
        log::info!(
            "Tax liability (higher rate {}%): £{:.2}",
            higher_rate * rust_decimal_macros::dec!(100),
            tax_at_higher
        );

        // Write CSV to stdout
        report.write_csv(io::stdout(), year)
    }

    fn report_income(
        &self,
        events: Vec<events::TaxableEvent>,
        year: Option<TaxYear>,
        band: TaxBand,
    ) -> color_eyre::Result<()> {
        let report = calculate_income_tax(events);

        let years = match year {
            Some(y) => vec![y],
            None => report.tax_years(),
        };

        for tax_year in years {
            let tax = report.calculate_tax(tax_year, band);

            log::info!("=== Income Tax Report ({}) ===", tax_year.display());
            log::info!("Tax band: {:?}", band);
            log::info!("");
            log::info!("Staking income: £{:.2}", tax.staking_income);
            log::info!(
                "Staking tax ({}%): £{:.2}",
                tax_year.income_rate(band) * rust_decimal_macros::dec!(100),
                tax.staking_tax
            );
            log::info!("");
            log::info!("Dividend income: £{:.2}", tax.dividend_income);
            log::info!("Dividend allowance used: £{:.2}", tax.dividend_allowance_used);
            log::info!("Taxable dividends: £{:.2}", tax.taxable_dividends);
            log::info!(
                "Dividend tax ({}%): £{:.2}",
                tax_year.dividend_rate(band) * rust_decimal_macros::dec!(100),
                tax.dividend_tax
            );
            log::info!("");
            log::info!("Total income tax: £{:.2}", tax.total_tax);
        }

        // Write income events CSV to stderr (since CGT goes to stdout)
        report.write_csv(io::stderr(), year)
    }
}
