use std::error::Error;
use std::io::Read;

use chrono::NaiveDateTime;
use serde_derive::Deserialize;

use crate::trades::{Trade, TradeKind};
use crate::{amount, Account, AccountKind, Entry, Transaction};

#[derive(Debug, Deserialize, Clone)]
#[allow(non_snake_case)]
struct Record {
    // Date(UTC),Market,Type,Price,Amount,Total,Fee,Fee Coin
    #[serde(rename = "Date(UTC)")]
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
    #[serde(rename = "Fee")]
    fee: f64,
    #[serde(rename = "Fee Coin")]
    fee_coin: String,
}

impl Into<Option<Trade>> for Record {
    fn into(self) -> Option<Trade> {
        let date_time =
            NaiveDateTime::parse_from_str(self.date.as_ref(), "%Y-%m-%d %H:%M:%S").unwrap();

        let (base_currency, quote_currency) = self.market.split_at(3);

        let base_amount = amount(base_currency, self.amount);
        let quote_amount = amount(quote_currency, self.total);

        let (kind, sell, buy) = match self.order_type.as_ref() {
            "BUY" => (TradeKind::Buy, quote_amount, base_amount),
            "SELL" => (TradeKind::Sell, base_amount, quote_amount),
            _ => panic!("Invalid order_type {}", self.order_type),
        };
        let fee = amount(self.fee_coin.as_ref(), self.fee);

        Some(Trade {
            date_time,
            kind,
            buy,
            sell,
            fee,
            rate: self.price,
            exchange: Some("Binance".into()),
        })
    }
}

impl Into<Vec<Transaction>> for Record {
    fn into(self) -> Vec<Transaction> {
        let date_time =
            NaiveDateTime::parse_from_str(self.date.as_ref(), "%Y-%m-%d %H:%M:%S").unwrap();

        let (base_currency, quote_currency) = self.market.split_at(3);

        let base_amount = amount(base_currency, self.amount);
        let quote_amount = amount(quote_currency, self.total);

        let (debit_amt, credit_amt) = match self.order_type.as_ref() {
            "BUY" => (quote_amount, base_amount),
            "SELL" => (base_amount, quote_amount),
            _ => panic!("Invalid order_type {}", self.order_type),
        };

        let fee = amount(&self.fee_coin, self.fee);

        let acct = Account::new("Binance", AccountKind::Exchange);

        let tx = Transaction::new(
            Some("binance-trade".into()),
            date_time,
            Entry::new(acct.clone(), debit_amt),
            Entry::new(acct.clone(), credit_amt),
            fee,
        );

        vec![tx]
    }
}

pub fn import_trades<R>(reader: R) -> Result<Vec<Trade>, Box<Error>>
where
    R: Read,
{
    super::csv_to_trades::<R, Record>(reader)
}
