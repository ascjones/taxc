use std::error::Error;
use std::io::{Read};

use chrono::NaiveDateTime;
use serde_derive::Deserialize;

use crate::{Transaction, Account, AccountKind, Entry, amount};
use crate::trades::{Trade, TradeKind};

#[derive(Debug, Deserialize, Clone)]
#[allow(non_snake_case)]
struct TradeHistoryRecord {
    #[serde(rename = "Date")]
    date: String,
    #[serde(rename = "Market")]
    market: String,
    #[serde(rename = "Type")]
    order_type: String,
    #[serde(rename = "Price")]
    price: f64,
    #[serde(rename = "Amount")]
    amount: f64,
    #[serde(rename = "Total")]
    total: f64,
    #[serde(rename = "Order Number")]
    order_number: String,
    #[serde(rename = "Base Total Less Fee")]
    base_total_less_fee: f64,
    #[serde(rename = "Quote Total Less Fee")]
    quote_total_less_fee: f64,
}

impl Into<Option<Trade>> for TradeHistoryRecord {
    fn into(self) -> Option<Trade> {
        let date_time = NaiveDateTime::parse_from_str(
            self.date.as_ref(), "%Y-%m-%d %H:%M:%S").unwrap();

        let mut market_parts = self.market.split('/');
        let base_currency = market_parts.next().expect("base currency");
        let quote_currency = market_parts.next().expect("quote currency");

        // note that the poloniex data seems to have base and quote the wrong way around

        let (kind, sell, buy, fee) =
            match self.order_type.as_ref() {
                "Buy" => {
                    let buy = amount(base_currency, self.amount);
                    let sell = amount(quote_currency, self.total);
                    let fee_base = self.amount - self.quote_total_less_fee;
                    let fee = amount(quote_currency, fee_base * self.price);
                    (TradeKind::Buy, sell, buy, fee)
                },
                "Sell" => {
                    let buy = amount(quote_currency, self.base_total_less_fee);
                    let sell = amount(base_currency, self.amount);
                    let fee = amount(quote_currency, self.total - self.base_total_less_fee);
                    (TradeKind::Sell, sell, buy, fee)
                },
                _ => {
                    panic!("Invalid order_type {}", self.order_type)
                }
            };

        Some(Trade {
            date_time,
            kind,
            buy,
            sell,
            fee,
            rate: self.price,
            exchange: Some("Poloniex".into())
        })
    }
}

impl Into<Vec<Transaction>> for TradeHistoryRecord {
    fn into(self) -> Vec<Transaction> {
        let date_time = NaiveDateTime::parse_from_str(
            self.date.as_ref(), "%Y-%m-%d %H:%M:%S").unwrap();

        let mut market_parts = self.market.split('/');
        let base_currency = market_parts.next().expect("base currency");
        let quote_currency = market_parts.next().expect("quote currency");

        let base_amount = amount(base_currency, self.amount);
        let quote_amount = amount(quote_currency, self.total);

        let (debit_amt, credit_amt) =
            match self.order_type.as_ref() {
                "Buy" => {
                    (quote_amount, base_amount)
                },
                "Sell" => {
                    (base_amount, quote_amount)
                },
                _ => {
                    panic!("Invalid order_type {}", self.order_type)
                }
            };

        let fee = amount("£", 0.); // todo: fee;

        let acct = Account::new("Poloniex", AccountKind::Exchange);

        let tx = Transaction::new(
            Some(self.order_number),
            date_time,
            Entry::new(acct.clone(), debit_amt),
            Entry::new(acct.clone(), credit_amt),
            fee);

        vec![tx]
    }
}

pub fn import_trades<R>(reader: R) -> Result<Vec<Trade>, Box<Error>> where R: Read {
    super::csv_to_trades::<R, TradeHistoryRecord>(reader)
}