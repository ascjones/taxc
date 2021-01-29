mod exchanges;

use crate::trades::{
    Trade,
    TradeRecord,
};
use self::exchanges::binance::BinanceApiCommand;
use argh::FromArgs;
use serde::de::DeserializeOwned;
use std::{
    convert::TryInto,
    fs::File,
    io,
    path::PathBuf,
};
use std::path::Path;

/// Import trades from a csv file
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "import")]
pub struct ImportTradesCommand {
    #[argh(subcommand)]
    sub: ImportTradesSubCommand
}

impl ImportTradesCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        self.sub.exec()
    }
}

/// Import trades from a csv file
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
pub enum ImportTradesSubCommand {
    Api(ImportApiCommand),
    Csv(ImportExchangeCsvCommand),
}

impl ImportTradesSubCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        match self {
            Self::Api(api) => api.exec(),
            Self::Csv(csv) => csv.exec(),
        }
    }
}

/// Import trades from an API
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "api")]
pub struct ImportApiCommand {
    #[argh(subcommand)]
    sub: ImportApiSubCommand,
}

impl ImportApiCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        self.sub.exec()
    }
}

/// Import trades from a csv file
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
pub enum ImportApiSubCommand {
    Binance(BinanceApiCommand),
}

impl ImportApiSubCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        match self {
            Self::Binance(binance) => binance.exec(),
        }
    }
}

/// Import trades from a csv file for the given exchange
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "csv")]
pub struct ImportExchangeCsvCommand {
    #[argh(subcommand)]
    sub: ImportExchangeCsvSubCommand
}

impl ImportExchangeCsvCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        self.sub.exec()
    }
}

/// Import trades from a csv file for the given exchange
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
pub enum ImportExchangeCsvSubCommand {
    Binance(ImportCsvCommand),
    Bittrex(ImportCsvCommand),
    Coinbase(ImportCsvCommand),
    Poloniex(ImportCsvCommand),
    Uphold(ImportCsvCommand),
}

impl ImportExchangeCsvSubCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        match self {
            Self::Uphold(csv) => csv.exec::<exchanges::uphold::Record, _, _>(&csv.file),
            Self::Poloniex(csv) => csv.exec::<exchanges::poloniex::Record, _, _>(&csv.file),
            Self::Bittrex(csv) => csv.exec::<exchanges::bittrex::Record, _, _>(&csv.file),
            Self::Binance(csv) => csv.exec::<exchanges::binance::CsvRecord, _, _>(&csv.file),
            Self::Coinbase(csv) => csv.exec::<exchanges::coinbase::Record, _, _>(&csv.file),
        }
    }
}

/// Import trades from a csv file
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "csv")]
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

impl ImportCsvCommand {
    fn exec<'a, CsvRecord, P, E>(&self, path: P) -> color_eyre::Result<()>
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

        let trades = if self.group_by_day {
            crate::trades::group_trades_by_day(&trades)
        } else {
            trades
        };

        let trade_records = trades
            .iter()
            .map(|t| TradeRecord::from(t))
            .collect();
        crate::utils::write_csv(trade_records, io::stdout())
    }
}
