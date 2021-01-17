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
    // Date(UTC),Market,Type,Price,Amount,Total,Fee,Fee Coin
    #[serde(rename = "Date(UTC)")]
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
    #[serde(rename = "Fee")]
    fee: Decimal,
    #[serde(rename = "Fee Coin")]
    fee_coin: String,
}

impl TryFrom<Record> for Trade {
    type Error = super::ExchangeError;

    fn try_from(value: Record) -> Result<Trade, Self::Error> {
        let date_time =
            NaiveDateTime::parse_from_str(value.date.as_ref(), "%Y-%m-%d %H:%M:%S")?;

        let (base_currency, quote_currency) = value.market.split_at(3);

        let base_amount = amount(base_currency, value.amount);
        let quote_amount = amount(quote_currency, value.total);

        let (kind, sell, buy) = match value.order_type.as_ref() {
            "BUY" => (TradeKind::Buy, quote_amount, base_amount),
            "SELL" => (TradeKind::Sell, base_amount, quote_amount),
            _ => panic!("Invalid order_type {}", value.order_type),
        };
        let fee = amount(value.fee_coin.as_ref(), value.fee);

        Ok(Trade {
            date_time,
            kind,
            buy,
            sell,
            fee,
            rate: value.price,
            exchange: Some("Binance".into()),
        })
    }
}
