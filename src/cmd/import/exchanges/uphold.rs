

use chrono::DateTime;

use serde_derive::Deserialize;
use std::convert::TryFrom;
use steel_cent::currency;

use super::ExchangeError;
use crate::coins::amount;
use crate::trades::{Trade, TradeKind};

#[derive(Clone, Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct Record {
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

impl TryFrom<Record> for Trade {
    type Error = ExchangeError;

    fn try_from(value: Record) -> Result<Trade, Self::Error> {
        // check to see if this is a crypto trade - either are unknown currencies
        if currency::with_code(&value.origin_currency).is_some()
            && currency::with_code(&value.destination_currency).is_some()
        {
            return Err("Either origin or destination currency should be a cryptocurrency".into());
        }
        if value.origin_currency == value.destination_currency {
            return Err("Origin and destination cannot be the same currency".into());
        }

        let date_time = DateTime::parse_from_rfc3339(&value.date)
            .expect("invalid rcf3339 date")
            .naive_utc();

        let sell = amount(&value.origin_currency, value.origin_amount);
        let buy = amount(&value.destination_currency, value.destination_amount);

        let (base_currency, _quote_currency) = value.pair.split_at(3);
        let kind = if value.origin_currency == base_currency {
            TradeKind::Sell
        } else if value.destination_currency == base_currency {
            TradeKind::Buy
        } else {
            panic!("Either source or destination should be the base currency")
        };
        let fee = amount("£", value.commission_in_GBP);

        Ok(Trade {
            date_time,
            buy,
            sell,
            fee,
            rate: value.rate,
            exchange: Some("Uphold".into()),
            kind,
        })
    }
}
