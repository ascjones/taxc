mod exchanges;

use crate::{
    cmd::import::exchanges::{binance::BinanceApiCommand, ExchangeError},
    trades::{Trade, TradeRecord},
};
use argh::FromArgs;
use serde::de::DeserializeOwned;
use std::{convert::TryInto, fs::File, io, path::PathBuf};

/// Import trades from a csv file
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "import")]
pub struct ImportTradesCommand {
    #[argh(subcommand)]
    sub: ImportTradesSubCommand,
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
    /// the exchange to import csv from
    #[argh(positional)]
    exchange: Exchange,
    /// the csv file containing trades to import
    #[argh(positional)]
    file: PathBuf,
}

impl ImportExchangeCsvCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        match self.exchange {
            Exchange::Uphold => self.import_csv::<exchanges::uphold::Record, _>(),
            Exchange::Poloniex => self.import_csv::<exchanges::poloniex::Record, _>(),
            Exchange::Bittrex => self.import_csv::<exchanges::bittrex::Record, _>(),
            Exchange::Binance => self.import_csv::<exchanges::binance::CsvRecord, _>(),
            Exchange::Coinbase => self.import_csv::<exchanges::coinbase::Record, _>(),
        }
    }

    fn import_csv<'a, CsvRecord, E>(&self) -> color_eyre::Result<()>
    where
        CsvRecord: Clone + DeserializeOwned + TryInto<Trade<'a>, Error = E>,
        E: std::error::Error + 'static + Send + Sync,
    {
        let file = File::open(&self.file)?;
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

        let trade_records = trades.iter().map(|t| TradeRecord::from(t));
        crate::utils::write_csv(trade_records, io::stdout())
    }
}

/// Import trades from a csv file for the given exchange
#[derive(PartialEq, Debug)]
pub enum Exchange {
    Binance,
    Bittrex,
    Coinbase,
    Poloniex,
    Uphold,
}

impl std::str::FromStr for Exchange {
    type Err = ExchangeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "binance" => Ok(Self::Binance),
            "bittrex" => Ok(Self::Bittrex),
            "coinbase" => Ok(Self::Coinbase),
            "poloniex" => Ok(Self::Poloniex),
            "uphold" => Ok(Self::Uphold),
            e => Err(ExchangeError::UnsupportedExchange(e.into())),
        }
    }
}
