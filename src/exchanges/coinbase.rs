use std::io::Read;

use chrono::NaiveDateTime;
use serde_derive::Deserialize;
use std::convert::TryFrom;

use crate::amount;
use crate::trades::{Trade, TradeKind};

// trade id,product,side,created at,size,size unit,price,fee,total,price/fee/total unit
// 155157,ETH-GBP,SELL,2018-11-20T21:39:45.667Z,5.41307455,ETH,101.86,1.654127320989,549.721646342011,GBP

#[derive(Debug, Deserialize, Clone)]
#[allow(non_snake_case)]
struct Record {
    #[serde(rename = "trade id")]
    trade_id: String,
    product: String,
    side: String,
    #[serde(rename = "created at")]
    created_at: String,
    size: f64,
    #[serde(rename = "size unit")]
    size_unit: String,
    price: f64,
    fee: f64,
    total: f64,
    #[serde(rename = "price/fee/total unit")]
    unit: String,
}

impl TryFrom<Record> for Trade {
    type Error = super::ExchangeError;

    fn try_from(value: Record) -> Result<Trade, Self::Error> {
        // 2018-11-20T21:39:45.667Z
        let date_time =
            NaiveDateTime::parse_from_str(value.created_at.as_ref(), "%Y-%m-%dT%H:%M:%S%z").unwrap();

        let mut market_parts = value.product.split('-');
        let quote_currency = market_parts.next().expect("quote currency");
        let base_currency = market_parts.next().expect("base currency");

        let base_amount = amount(base_currency, value.size);
        let quote_amount = amount(quote_currency, value.total);

        let (kind, sell, buy) = match value.side.as_ref() {
            "BUY" => (TradeKind::Buy, quote_amount, base_amount),
            "SELL" => (TradeKind::Sell, base_amount, quote_amount),
            _ => panic!("Invalid order_type {}", value.side),
        };
        let fee = amount(value.unit.as_ref(), value.fee);

        Ok(Trade {
            date_time,
            kind,
            buy,
            sell,
            fee,
            rate: value.price,
            exchange: Some("Coinbase Pro".into()),
        })
    }
}
