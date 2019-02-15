use std::error::Error;
use std::io::{Read};

use chrono::NaiveDateTime;
use serde_derive::Deserialize;

use crate::{Transaction, Account, AccountKind, Entry, amount};
use crate::trades::{Trade, TradeKind};

#[derive(Debug, Deserialize, Clone)]
#[allow(non_snake_case)]
struct OrderRecord {
    #[serde(rename = "OrderUuid")]
    order_id: String,
    #[serde(rename = "Exchange")]
    exchange: String,
    #[serde(rename = "Type")]
    order_type: String,
    #[serde(rename = "Quantity")]
    quantity: f64,
    #[serde(rename = "Limit")]
    limit: f64,
    #[serde(rename = "CommissionPaid")]
    commission_paid: f64,
    #[serde(rename = "Price")]
    price: f64,
    #[serde(rename = "Opened")]
    opened: String,
    #[serde(rename = "Closed")]
    closed: String,
}

impl Into<Option<Trade>> for OrderRecord {
    fn into(self) -> Option<Trade> {
        let date_time = NaiveDateTime::parse_from_str(
            self.closed.as_ref(), "%m/%d/%Y %-I:%M:%S %p").unwrap();

        let mut market_parts = self.exchange.split('-');
        let quote_currency = market_parts.next().expect("quote currency");
        let base_currency = market_parts.next().expect("base currency");

        let base_amount = amount(base_currency, self.quantity);
        let quote_amount = amount(quote_currency, self.price);

        let (kind, sell, buy) =
            match self.order_type.as_ref() {
                "LIMIT_BUY" => {
                    (TradeKind::Buy, quote_amount, base_amount)
                },
                "LIMIT_SELL" => {
                    (TradeKind::Sell, base_amount, quote_amount)
                },
                _ => {
                    panic!("Invalid order_type {}", self.order_type)
                }
            };
        let fee = amount(quote_currency, self.commission_paid);

        Some(Trade {
            date_time,
            buy,
            sell,
            fee,
            rate: self.limit,
            exchange: Some("Bittrex".into()),
            kind,
        })
    }
}

impl Into<Vec<Transaction>> for OrderRecord {
    fn into(self) -> Vec<Transaction> {
        let date_time = NaiveDateTime::parse_from_str(
            self.closed.as_ref(), "%m/%d/%Y %-I:%M:%S %p").unwrap();

        let mut market_parts = self.exchange.split('-');
        let quote_currency = market_parts.next().expect("quote currency");
        let base_currency = market_parts.next().expect("base currency");

        let base_amount = amount(base_currency, self.quantity);
        let quote_amount = amount(quote_currency, self.price);

        let (debit_amt, credit_amt) =
            match self.order_type.as_ref() {
                "LIMIT_BUY" => {
                    (quote_amount, base_amount)
                },
                "LIMIT_SELL" => {
                    (base_amount, quote_amount)
                },
                _ => {
                    panic!("Invalid order_type {}", self.order_type)
                }
            };

        let fee = amount("£", 0.); // todo: fee;

        let acct = Account::new("Bittrex", AccountKind::Exchange);

        let tx = Transaction::new(
            Some(self.order_id),
            date_time,
            Entry::new(acct.clone(), debit_amt),
            Entry::new(acct.clone(), credit_amt),
            fee);

        vec![tx]
    }
}

pub fn import_trades<R>(reader: R) -> Result<Vec<Trade>, Box<Error>> where R: Read {
    super::csv_to_trades::<R, OrderRecord>(reader)
}