use std::io::{Read, Write};
use std::error::Error;
use chrono::{DateTime, Utc};
use steel_cent::{Money, formatting::uk_style};
use serde_derive::{Serialize, Deserialize};

use crate::transaction::{Transaction, Entry, Account, parse_money};

pub struct Journal(Vec<Transaction>);

#[derive(Debug, Serialize, Deserialize)]
struct CsvTransaction<'a> {
    date_time: &'a str,
    debit_acct: &'a str,
    debit_amt: &'a str,
    credit_acct: &'a str,
    credit_amt: &'a str,
    fee: &'a str,
    source_id: &'a str,
}

impl Journal {
    pub fn new(txs: Vec<Transaction>) -> Self {
        let mut journal = Journal(txs.clone());
        journal.sort_txs();
        journal
    }

    fn sort_txs(&mut self) {
        self.0.sort_by(|tx1,tx2| tx1.date_time.cmp(&tx2.date_time));
    }

    pub fn read_csv<'a, R>(reader: R) -> Result<Journal, Box<Error>> where R: Read {
        let mut rdr = csv::Reader::from_reader(reader);
        let mut raw_record = csv::StringRecord::new();
        let headers = rdr.headers()?.clone();
        let mut txs: Vec<Transaction> = Vec::new();

        while rdr.read_record(&mut raw_record)? {
            let record: CsvTransaction = raw_record.deserialize(Some(&headers))?;
            let source_id =
                if record.source_id == "" { None } else { Some(record.source_id.into()) };
            let date_time =
                DateTime::parse_from_rfc3339(record.date_time)
                    // todo: bring back error with detail
                    .expect(format!("Invalid date_time {}", record.date_time).as_ref()).naive_utc();
            let debit = parse_entry(record.debit_acct, record.debit_amt)?;
            let credit = parse_entry(record.credit_acct, record.credit_amt)?;
            let fee = parse_money(record.fee)?;
            let tx = Transaction::new(source_id, date_time, debit, credit, fee);
            txs.push(tx)
        }
        Ok(Journal::new(txs))
    }

    pub fn write_csv<W>(&self, writer: W) -> Result<(), Box<Error>> where W: Write {
        let mut wtr = csv::Writer::from_writer(writer);
        for tx in self.0.iter() {
            let date_time =
                DateTime::<Utc>::from_utc(tx.date_time, Utc).to_rfc3339().clone();
            let (debit_amt, debit_acct) = display_entry(&tx.debit);
            let (credit_amt, credit_acct) = display_entry(&tx.credit);
            let fee = display_amount(&tx.fee);
            let record = CsvTransaction {
                date_time: date_time.as_ref(),
                debit_acct: debit_acct.as_ref(),
                debit_amt: debit_amt.as_ref(),
                credit_acct: credit_acct.as_ref(),
                credit_amt: credit_amt.as_ref(),
                fee: fee.as_ref(),
                source_id: tx.source_id.as_ref().map_or("", String::as_ref),
            };
            wtr.serialize(record)?;
        }
        wtr.flush()?;
        Ok(())
    }

    pub fn transactions(&self) -> &[Transaction] {
        &self.0
    }

    pub fn merge(&mut self, other: &Journal) {
        for new_tx in other.transactions() {
            let existing_tx = self.transactions()
                .iter()
                .cloned()
                .find(|existing| existing == new_tx);
            if existing_tx.is_none() {
                self.0.push(new_tx.clone());
            }
        }
        self.sort_txs()
    }
}

fn display_entry<'a>(entry: &Entry) -> (String, String) {
    let amt = display_amount(&entry.amount);
    let acct = format!("{}", entry.account);
    (amt, acct)
}

fn display_amount(amount: &Money) -> String {
    format!("{}", uk_style().display_for(amount))
}

fn parse_entry(acct: &str, amt: &str) -> Result<Entry, Box<Error>> {
    let acct = Account::parse(acct)
        .map_err(|err| format!("Error parsing acct '{}':\n\t{}", acct, err))?;
    let amt = parse_money(amt)
        .map_err(|err| format!("Error parsing money '{}':\n\t{}", amt, err))?;
    Ok(Entry::new(acct, amt))
}