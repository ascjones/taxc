use chrono::NaiveDateTime;
use serde::Deserialize;
use std::convert::TryFrom;

use crate::coins::amount;
use crate::trades::{Trade, TradeKind};

#[derive(Debug, Deserialize, Clone)]
#[allow(non_snake_case)]
pub struct Record {
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

impl TryFrom<Record> for Trade {
    type Error = super::ExchangeError;

    fn try_from(value: Record) -> Result<Trade, Self::Error> {
        let date_time =
            NaiveDateTime::parse_from_str(value.closed.as_ref(), "%m/%d/%Y %-I:%M:%S %p").unwrap();

        let mut market_parts = value.exchange.split('-');
        let quote_currency = market_parts.next().expect("quote currency");
        let base_currency = market_parts.next().expect("base currency");

        let base_amount = amount(base_currency, value.quantity);
        let quote_amount = amount(quote_currency, value.price);

        let (kind, sell, buy) = match value.order_type.as_ref() {
            "LIMIT_BUY" => (TradeKind::Buy, quote_amount, base_amount),
            "LIMIT_SELL" => (TradeKind::Sell, base_amount, quote_amount),
            _ => panic!("Invalid order_type {}", value.order_type),
        };
        let fee = amount(quote_currency, value.commission_paid);

        Ok(Trade {
            date_time,
            buy,
            sell,
            fee,
            rate: value.limit,
            exchange: Some("Bittrex".into()),
            kind,
        })
    }
}
