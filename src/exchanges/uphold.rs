use std::error::Error;
use std::io::Read;

use chrono::DateTime;

use serde_derive::Deserialize;
use steel_cent::currency;

use crate::amount;
use crate::trades::{Trade, TradeKind};

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
        if currency::with_code(&self.origin_currency).is_some()
            && currency::with_code(&self.destination_currency).is_some()
        {
            return None;
        }
        if self.origin_currency == self.destination_currency {
            return None;
        }

        let date_time = DateTime::parse_from_rfc3339(&self.date)
            .expect("invalid rcf3339 date")
            .naive_utc();

        let sell = amount(&self.origin_currency, self.origin_amount);
        let buy = amount(&self.destination_currency, self.destination_amount);

        let (base_currency, _quote_currency) = self.pair.split_at(3);
        let kind = if self.origin_currency == base_currency {
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

pub fn import_trades<R>(reader: R) -> Result<Vec<Trade>, Box<dyn Error>>
where
    R: Read,
{
    super::csv_to_trades::<R, Record>(reader)
}
