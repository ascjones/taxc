mod exchanges;

use crate::trades::Trade;
use self::exchanges::binance::BinanceApiCommand;
use argh::FromArgs;
use serde::de::DeserializeOwned;
use std::{
    convert::TryInto,
    fs::File,
    io::{self, Read},
    path::PathBuf,
};
use std::path::Path;

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
/// Import trades from a csv file
pub enum ImportTradesCommand {
    Api(ImportApiCommand),
    Csv(ImportExchangeCsvCommand),
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
/// Import trades from a csv file
pub enum ImportApiCommand {
    Binance(BinanceApiCommand),
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
/// Import trades from a csv file for the given exchange
pub enum ImportExchangeCsvCommand {
    Binance(ImportCsvCommand),
    Bittrex(ImportCsvCommand),
    Coinbase(ImportCsvCommand),
    Poloniex(ImportCsvCommand),
    Uphold(ImportCsvCommand),
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "csv")]
/// Import trades from a csv file
pub struct ImportCsvCommand {
    /// the csv file containing trades to import
    #[argh(positional)]
    file: PathBuf,
    /// the source of the csv file, e.g. which exchange
    #[argh(option)]
    source: String,
    /// combines trades on the same pair on the same day into a single trade
    #[argh(switch, short = 'g')]
    group_by_day: bool,
}

impl ImportExchangeCsvCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let trades = match self {
            Self::Uphold(csv) => Self::csv_to_trades::<exchanges::uphold::Record, _, _>(&csv.file),
            Self::Poloniex(csv) => Self::csv_to_trades::<exchanges::poloniex::Record, _, _>(&csv.file),
            Self::Bittrex(csv) => Self::csv_to_trades::<exchanges::bittrex::Record, _, _>(&csv.file),
            Self::Binance(csv) => Self::csv_to_trades::<exchanges::binance::CsvRecord, _, _>(&csv.file),
            Self::Coinbase(csv) => Self::csv_to_trades::<exchanges::coinbase::Record, _, _>(&csv.file),
        }?;
        let mut trades = if self.group_by_day {
            crate::trades::group_trades_by_day(&trades)
        } else {
            trades
        };

        trades.sort_by(|t1, t2| t1.date_time.cmp(&t2.date_time));
        crate::trades::write_csv(trades, io::stdout())
    }

    fn csv_to_trades<'a, CsvRecord, P, E>(path: P) -> color_eyre::Result<Vec<Trade<'a>>>
    where
        CsvRecord: Clone + DeserializeOwned + TryInto<Trade<'a>, Error = E>,
        P: AsRef<Path>,
        E: std::error::Error + 'static + Send + Sync,
    {
        let file = File::open(path)?;
        let mut rdr = csv::Reader::from_reader(file);
        let result: Result<Vec<CsvRecord>, _> = rdr.deserialize().collect();
        let result = result?;
        log::info!("Read {} csv records", result.len());
        let mut trades = result
            .iter()
            .cloned()
            .map(|record: CsvRecord| TryInto::try_into(record).map_err(Into::into))
            .collect::<color_eyre::Result<Vec<Trade>>>()?;
        trades.sort_by(|tx1, tx2| tx1.date_time.cmp(&tx2.date_time));
        Ok(trades)
    }
}
