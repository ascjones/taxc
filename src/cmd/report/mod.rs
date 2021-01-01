use crate::cmd::prices::Prices;
use crate::trades;
use argh::FromArgs;
use std::{error::Error, fs::File, io, path::PathBuf};
use steel_cent::{currency::GBP, Money};

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
    pub fn exec(&self) -> Result<(), Box<dyn Error>> {
        let trades = trades::read_csv(File::open(&self.txs)?)?;
        let prices =
            match self.prices {
                None => Prices::from_coingecko_api()?,
                Some(ref path) => {
                    Prices::read_csv(File::open(path)?)?
                }
            };
        let report = cgt::calculate(trades, &prices)?;
        let gains = report.gains(self.year);

        let estimated_liability = (gains.total_gain() - Money::of_major(GBP, 11_300)) * 0.2;

        log::info!("Disposals {}", gains.len());
        log::info!("Proceeds {}", gains.total_proceeds());
        log::info!("Allowable Costs {}", gains.total_allowable_costs());
        log::info!("Gains {}", gains.total_gain());
        log::info!("Estimated Liability {}", estimated_liability);

        cgt::TaxEvent::write_csv(gains, io::stdout())
    }
}
