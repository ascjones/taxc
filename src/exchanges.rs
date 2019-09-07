pub mod binance;
pub mod bittrex;
pub mod coinbase;
pub mod poloniex;
pub mod uphold;

use serde::de::DeserializeOwned;
use std::error::Error;
use std::io::Read;

use crate::trades::Trade;

fn csv_to_trades<R, CsvRecord>(reader: R) -> Result<Vec<Trade>, Box<dyn Error>>
where
    R: Read,
    CsvRecord: Into<Option<Trade>>,
    CsvRecord: DeserializeOwned,
    CsvRecord: Clone,
{
    let mut rdr = csv::Reader::from_reader(reader);
    let result: Result<Vec<_>, _> = rdr.deserialize::<CsvRecord>().collect();
    let mut trades: Vec<Trade> = result?
        .iter()
        .cloned()
        .flat_map(|record: CsvRecord| Into::into(record))
        .collect();
    trades.sort_by(|tx1, tx2| tx1.date_time.cmp(&tx2.date_time));
    Ok(trades)
}
