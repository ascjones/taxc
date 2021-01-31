use crate::{cmd::prices::Prices, currencies::GBP, trades, Money};
use argh::FromArgs;
use rust_decimal::Decimal;
use std::{fs::File, io, path::PathBuf};

mod cgt;

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "report")]
/// Run a report to calculate CGT
pub struct ReportCommand {
    /// the csv file containing the transactions
    #[argh(option)]
    txs: PathBuf,
    /// optional csv file with prices in GBP for ETH and BTC, instead of fetching from Coingecko.
    #[argh(option)]
    prices: Option<PathBuf>,
    /// the tax year for which to produce the report
    #[argh(option)]
    year: Option<i32>,
}

impl ReportCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        // todo: in the future support other quote currencies
        let quote_currency = GBP;

        let trades = trades::read_csv(File::open(&self.txs)?)?;
        let prices = match self.prices {
            None => Prices::from_coingecko_api(quote_currency)?,
            Some(ref path) => Prices::read_csv(File::open(path)?)?,
        };
        let report = cgt::calculate(trades, &prices)?;
        let gains = report.gains(self.year);

        let estimated_liability =
            (gains.total_gain() - Money::from_major(11_300, GBP)) * Decimal::new(20, 2);

        log::info!("Disposals {}", gains.len());
        log::info!("Proceeds {}", gains.total_proceeds());
        log::info!("Allowable Costs {}", gains.total_allowable_costs());
        log::info!("Gains {}", gains.total_gain());
        log::info!("Estimated Liability {}", estimated_liability);

        cgt::Disposal::write_csv(gains, io::stdout())
    }
}
