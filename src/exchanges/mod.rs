pub mod binance;
pub mod bittrex;
pub mod coinbase;
pub mod poloniex;
pub mod uphold;

use serde::de::DeserializeOwned;
use std::convert::TryInto;
use std::error::Error;
use std::io::Read;

use crate::trades::Trade;

#[derive(Debug, derive_more::From, derive_more::Display)]
pub enum ExchangeError {
    DateParse(chrono::format::ParseError),
    InvalidRecord(&'static str),
}

impl std::error::Error for ExchangeError {}

pub fn csv_to_trades<CsvRecord, R, E>(reader: R) -> Result<Vec<Trade>, Box<dyn Error>>
where
    CsvRecord: Clone + DeserializeOwned + TryInto<Trade, Error = E>,
    R: Read,
    E: std::error::Error + 'static,
{
    let mut rdr = csv::Reader::from_reader(reader);
    let result: Result<Vec<CsvRecord>, _> = rdr.deserialize().collect();
    let result = result?;
    log::info!("Read {} csv records", result.len());
    let mut trades = result
        .iter()
        .cloned()
        .map(|record: CsvRecord| TryInto::try_into(record))
        .collect::<Result<Vec<Trade>, _>>()?;
    trades.sort_by(|tx1, tx2| tx1.date_time.cmp(&tx2.date_time));
    Ok(trades)
}
