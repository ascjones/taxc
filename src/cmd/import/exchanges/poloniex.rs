use chrono::NaiveDateTime;
use serde::Deserialize;
use std::convert::TryFrom;

use crate::{
    assets::amount,
    trades::{
        Trade,
        TradeKind,
    },
};
use rust_decimal::Decimal;

#[derive(Debug, Deserialize, Clone)]
#[allow(non_snake_case)]
pub struct Record {
    #[serde(rename = "Date")]
    date: String,
    #[serde(rename = "Market")]
    market: String,
    #[serde(rename = "Type")]
    order_type: String,
    #[serde(rename = "Price")]
    price: Decimal,
    #[serde(rename = "Amount")]
    amount: Decimal,
    #[serde(rename = "Total")]
    total: Decimal,
    #[serde(rename = "Order Number")]
    order_number: String,
    #[serde(rename = "Base Total Less Fee")]
    base_total_less_fee: Decimal,
    #[serde(rename = "Quote Total Less Fee")]
    quote_total_less_fee: Decimal,
}

impl TryFrom<Record> for Trade {
    type Error = super::ExchangeError;

    fn try_from(value: Record) -> Result<Trade, Self::Error> {
        let date_time =
            NaiveDateTime::parse_from_str(value.date.as_ref(), "%Y-%m-%d %H:%M:%S")
                .unwrap();

        let mut market_parts = value.market.split('/');
        let base_currency = market_parts.next().expect("base currency");
        let quote_currency = market_parts.next().expect("quote currency");

        // note that the poloniex data seems to have base and quote the wrong way around

        let (kind, sell, buy, fee) = match value.order_type.as_ref() {
            "Buy" => {
                let buy = amount(base_currency, value.amount);
                let sell = amount(quote_currency, value.total);
                let fee_base = value.amount - value.quote_total_less_fee;
                let fee = amount(quote_currency, fee_base * value.price);
                (TradeKind::Buy, sell, buy, fee)
            }
            "Sell" => {
                let buy = amount(quote_currency, value.base_total_less_fee);
                let sell = amount(base_currency, value.amount);
                let fee = amount(quote_currency, value.total - value.base_total_less_fee);
                (TradeKind::Sell, sell, buy, fee)
            }
            _ => panic!("Invalid order_type {}", value.order_type),
        };

        Ok(Trade {
            date_time,
            kind,
            buy,
            sell,
            fee,
            rate: value.price,
            exchange: Some("Poloniex".into()),
        })
    }
}
