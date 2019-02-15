use std::error::Error;
use std::io::{Read};

use csv;
use chrono::DateTime;
use serde_derive::Deserialize;
use steel_cent::currency::{self, Currency};

use crate::{Transaction, Account, AccountKind, Network, Entry, amount};
use crate::trades::{Trade, TradeKind};
use super::*;

#[derive(Clone, Debug, Deserialize)]
#[allow(non_snake_case)]
struct Record {
    date: String,
    id: String,
    #[serde(rename = "type")]
    tx_type: String,
    value_in_GBP: f64,
    commission_in_GBP: f64,
    pair: String,
    rate: f64,
    origin_currency: String,
    origin_amount: f64,
    origin_commission: String,
    destination_currency: String,
    destination_amount: f64,
    destination_commission: String,
}

impl Into<Option<Trade>> for Record {
    fn into(self) -> Option<Trade> {
        // check to see if this is a crypto trade - either are unknown currencies
        if currency::with_code(&self.origin_currency).is_some() &&
            currency::with_code(&self.destination_currency).is_some() {
            return None
        }
        if self.origin_currency == self.destination_currency {
            return None
        }

        let date_time = DateTime::parse_from_rfc3339(&self.date)
            .expect("invalid rcf3339 date").naive_utc();

        let sell = amount(&self.origin_currency, self.origin_amount);
        let buy = amount(&self.destination_currency, self.destination_amount);

        let (base_currency, _quote_currency) = self.pair.split_at(3);
        let kind =
            if self.origin_currency == base_currency {
                TradeKind::Sell
            } else if self.destination_currency == base_currency {
                TradeKind::Buy
            } else {
                panic!("Either source or destination should be the base currency")
            };
        let fee = amount("£", self.commission_in_GBP);

        Some(Trade {
            date_time,
            buy,
            sell,
            fee,
            rate: self.rate,
            exchange: Some("Uphold".into()),
            kind,
        })
    }
}

fn crypto_account(currency: &Currency) -> Account {
    match currency.code().as_ref() {
        "BTC" =>
            Account::new("Uphold", AccountKind::Crypto(Network::Bitcoin, None)),
        "ETH" =>
            Account::new("Uphold", AccountKind::Crypto(Network::Ethereum, None)),
        code => panic!("Unknown crypto {}", code),
    }
}

fn record_to_transactions(record: &Record) -> Vec<Transaction> {
    let date_time = DateTime::parse_from_rfc3339(&record.date)
        .expect("invalid rcf3339 date").naive_utc();
    let debit_amt = amount(&record.origin_currency, record.origin_amount);
    let credit_amt = amount(&record.destination_currency, record.destination_amount);
    let fee = amount("£", record.commission_in_GBP);

    let uphold_acct = &Account::new("Uphold", AccountKind::Exchange);
    let bank_account = &Account::new("Uphold", AccountKind::Bank);

    let entry = |acc: &Account, amt|
        Entry::new(acc.clone(), amt);

    let source_id = &record.id;
    let tx = move |dt, deb, cr, fee|
        Transaction::new(Some(source_id.clone()), dt, deb, cr, fee);

    match record.tx_type.as_ref() {
        "deposit" => {
            let debit = entry(bank_account, debit_amt);
            let credit = entry(uphold_acct, credit_amt);
            vec![tx(date_time, debit, credit, fee)]
        },
        "withdrawal" => {
            if currency::with_code(&credit_amt.currency.code()).is_some() {
                if debit_amt.currency == credit_amt.currency {
                    // straight fiat withdrawal, no conversion
                    let debit = entry(uphold_acct, debit_amt);
                    let credit = entry(bank_account, credit_amt);
                    vec![tx(date_time, debit, credit, fee)]
                } else {
                    // conversion before withdrawal
                    let debit = entry(uphold_acct, debit_amt);
                    let credit = entry(uphold_acct, credit_amt);
                    let conversion =
                        tx(date_time, debit, credit, fee);
                    // withdrawal to fiat
                    let debit = entry(uphold_acct, credit_amt);
                    let credit = entry(bank_account, credit_amt);
                    let withdrawal =
                        tx(date_time, debit, credit, fee);
                    vec![conversion, withdrawal]
                }
            } else {
                panic!("Expecting withdrawal to known fiat currency. Debit {}, Credit {}, id {}",
                       debit_amt.currency.code(), credit_amt.currency.code(), source_id)
            }
        },
        "in" => {
            let debit_acct = crypto_account(&debit_amt.currency);
            let debit = entry(&debit_acct, debit_amt);
            let credit = entry(uphold_acct, credit_amt);
            vec![tx(date_time, debit, credit, fee)]
        },
        "out" => {
            let credit_acct = crypto_account(&credit_amt.currency);
            let debit = entry(uphold_acct, debit_amt);
            let credit = entry(&credit_acct, credit_amt);
            vec![tx(date_time, debit, credit, fee)]
        },
        "transfer" => {
            // todo: refactor duplicate with wihdrawal
            let debit = entry(uphold_acct, debit_amt);
            let credit = entry(uphold_acct, credit_amt);
            vec![tx(date_time, debit, credit, fee)]
        },
        tx_ty => panic!("Invalid tx type {}", tx_ty)
    }
}

pub fn read_csv<R>(reader: R) -> Result<Vec<Transaction>, Box<Error>> where R: Read {
    let mut rdr = csv::Reader::from_reader(reader);
    let result: Result<Vec<_>, _> = rdr.deserialize::<Record>().collect();
    let mut txs: Vec<_> = result?
        .iter()
        .flat_map(record_to_transactions)
        .collect();
    txs.sort_by(|tx1, tx2| tx1.date_time.cmp(&tx2.date_time));
    Ok(txs)
}

pub fn import_trades<R>(reader: R) -> Result<Vec<Trade>, Box<Error>> where R: Read {
    super::csv_to_trades::<R, Record>(reader)
}