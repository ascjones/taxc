use std::error::Error;
use std::io::{Read};

use csv;
use chrono::NaiveDateTime;
use serde_derive::Deserialize;

use crate::{Transaction, Account, AccountKind, Network, Entry, amount};

#[derive(Debug, Deserialize)]
struct Record {
    #[serde(rename = "Txhash")]
    tx_hash: String,
    #[serde(rename = "Blockno")]
    block_no: u64,
    #[serde(rename = "UnixTimestamp")]
    unix_timestamp: i64,
    #[serde(rename = "DateTime")]
    date_time: String,
    #[serde(rename = "From")]
    from: String,
    #[serde(rename = "To")]
    to: String,
    #[serde(rename = "ContractAddress")]
    contract_address: String,
    #[serde(rename = "Value_IN(ETH)")]
    value_in: f64,
    #[serde(rename = "Value_OUT(ETH)")]
    value_out: f64,
    #[serde(rename = "TxnFee(ETH)")]
    txn_fee_eth: f64,
    #[serde(rename = "Historical $Price/Eth")]
    historical_price_eth: f64,
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "ErrCode")]
    err_code: String,
}

fn record_to_transaction(record: &Record) -> Option<Transaction> {
//    let date_time = Utc.timestamp_millis(record.unix_timestamp).naive_utc();
    let date_time = NaiveDateTime::parse_from_str(
        record.date_time.as_ref(), "%m/%d/%Y %-I:%M:%S %p").unwrap();
//    println!("{} {:?}", record.unix_timestamp, date_time);
    let amt =
        if record.value_in == 0. && record.value_out > 0. {
            record.value_out
        } else if record.value_in > 0. && record.value_out == 0. {
            record.value_in
        } else if record.value_in == 0. && record.value_out == 0. {
//            println!("Ignoring token tx {}", record.tx_hash);
            return None
        } else {
            panic!("in and out both have no zero values: tx {}", record.tx_hash)
        };

    let entry = |address: &str, amt: f64| {
        let amt = amount("ETH", amt); // todo should subtract fees?
        let acct = Account::new(
            "ethereum",
            AccountKind::Crypto(Network::Ethereum, Some(address.into()))
        );
        Entry::new(acct, amt)
    };

    let debit = entry(&record.from, amt);
    let credit = entry(&record.to, amt);

    let fee = amount("ETH", record.txn_fee_eth);
    let source_id = Some(record.tx_hash.clone());

    Some(Transaction::new(source_id, date_time, debit, credit, fee))
}

pub fn read_csv<R>(reader: R) -> Result<Vec<Transaction>, Box<Error>> where R: Read {
    let mut rdr = csv::Reader::from_reader(reader);
    let result: Result<Vec<_>, _> = rdr.deserialize::<Record>().collect();
    let mut txs: Vec<_> = result?
        .iter()
        .filter_map(|record|record_to_transaction(record))
        .collect();
    txs.sort_by(|tx1, tx2| tx1.date_time.cmp(&tx2.date_time));
    Ok(txs)
}