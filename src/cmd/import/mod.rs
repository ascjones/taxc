use crate::trades::Trade;
use serde::de::DeserializeOwned;
use std::{
    convert::TryInto,
    error::Error,
    fs::File,
    io::{self, Read},
    path::PathBuf,
};

mod exchanges;

pub fn import_csv(file: PathBuf, source: &str, group_by_day: bool) -> Result<(), Box<dyn Error>> {
    let csv_file = File::open(file)?;
    let trades = match source {
        "uphold" => csv_to_trades::<exchanges::uphold::Record, _, _>(csv_file),
        "poloniex" => csv_to_trades::<exchanges::poloniex::Record, _, _>(csv_file),
        "bittrex" => csv_to_trades::<exchanges::bittrex::Record, _, _>(csv_file),
        "binance" => csv_to_trades::<exchanges::binance::Record, _, _>(csv_file),
        "coinbase" => csv_to_trades::<exchanges::coinbase::Record, _, _>(csv_file),
        x => panic!("Unknown file source {}", x), // yes I know should be an error
    }?;
    let mut trades = if group_by_day {
        crate::trades::group_trades_by_day(&trades)
    } else {
        trades
    };

    trades.sort_by(|t1, t2| t1.date_time.cmp(&t2.date_time));
    crate::trades::write_csv(trades, io::stdout())
}

fn csv_to_trades<CsvRecord, R, E>(reader: R) -> Result<Vec<Trade>, Box<dyn Error>>
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
