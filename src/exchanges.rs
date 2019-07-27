pub mod binance;
pub mod bittrex;
pub mod poloniex;
pub mod uphold;

use serde::de::DeserializeOwned;
use std::error::Error;
use std::io::Read;

use crate::trades::Trade;
use crate::Transaction;

fn read_csv<R, CsvRecord>(reader: R) -> Result<Vec<Transaction>, Box<Error>>
where
    R: Read,
    CsvRecord: Into<Vec<Transaction>>,
    CsvRecord: DeserializeOwned,
    CsvRecord: Clone,
{
    let mut rdr = csv::Reader::from_reader(reader);
    let result: Result<Vec<_>, _> = rdr.deserialize::<CsvRecord>().collect();
    let mut txs: Vec<Transaction> = result?
        .iter()
        .cloned()
        .flat_map(|record: CsvRecord| Into::into(record))
        .collect();
    txs.sort_by(|tx1, tx2| tx1.date_time.cmp(&tx2.date_time));
    Ok(txs)
}

fn csv_to_trades<R, CsvRecord>(reader: R) -> Result<Vec<Trade>, Box<Error>>
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

//fn crypto_account(exchange_name: &str, currency: &Currency) -> Account {
//    match currency.code().as_ref() {
//        "BTC" =>
//            Account::new(exchange_name, AccountKind::Crypto(Network::Bitcoin, None)),
//        "ETH" =>
//            Account::new(exchange_name, AccountKind::Crypto(Network::Ethereum, None)),
//        code => panic!("Unknown crypto {}", code),
//    }
//}
