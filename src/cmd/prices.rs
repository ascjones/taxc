use std::{collections::HashMap, fmt, io::Read};

use crate::currencies::{self, Currency, BTC, ETH, USDC};
use chrono::{DateTime, NaiveDate, NaiveDateTime};
use color_eyre::eyre;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Eq, PartialEq, Clone)]
pub struct CurrencyPair<'a> {
    pub base: &'a Currency,
    pub quote: &'a Currency,
}

impl<'a> Hash for CurrencyPair<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.base.code.hash(state);
        self.base.code.hash(state);
    }
}

impl<'a> fmt::Display for CurrencyPair<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.base.code, self.quote.code)
    }
}

#[derive(Clone, PartialEq)]
pub struct Price<'a> {
    pub pair: CurrencyPair<'a>,
    pub date_time: NaiveDateTime,
    pub rate: Decimal,
}

#[derive(Default)]
pub struct Prices<'a> {
    prices: HashMap<CurrencyPair<'a>, Vec<Price<'a>>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Record {
    base_currency: String,
    quote_currency: String,
    date_time: String,
    rate: Decimal,
}

impl<'a> Prices<'a> {
    /// Initializes the prices database from the coingecko api
    pub fn from_coingecko_api(quote_currency: &'a Currency) -> eyre::Result<Prices<'a>> {
        let mut prices = HashMap::new();

        let mut fetch_prices = |coin, base| -> eyre::Result<()> {
            let pair = CurrencyPair { base, quote: quote_currency };
            let pair_prices = crate::coingecko::fetch_daily_prices(coin, quote_currency)?
                .iter()
                .map(|price| {
                    Price {
                        pair: pair.clone(),
                        date_time: price.timestamp,
                        rate: price.price,
                    }
                })
                .collect();
            prices.insert(pair, pair_prices);
            Ok(())
        };

        fetch_prices("bitcoin", BTC)?;
        fetch_prices("ethereum", ETH)?;
        fetch_prices("usd-coin", USDC)?;

        Ok(Prices { prices })
    }

    /// Initialize the prices database from the supplied CSV file
    pub fn read_csv<R>(reader: R) -> color_eyre::Result<Prices<'a>>
    where
        R: Read,
    {
        let mut rdr = csv::Reader::from_reader(reader);
        let result: Result<Vec<_>, _> = rdr.deserialize::<Record>().collect();
        let mut prices = HashMap::new();
        for record in result? {
            let base = currencies::find(&record.base_currency)
                .expect(format!("invalid base currency {}", record.base_currency).as_ref());
            let quote = currencies::find(&record.quote_currency)
                .expect(format!("invalid quote currency {}", record.quote_currency).as_ref());
            let date_time = parse_date(&record.date_time);
            let pair = CurrencyPair { base, quote };
            let price = Price {
                pair: pair.clone(),
                date_time,
                rate: record.rate,
            };
            let pair_prices = prices.entry(pair).or_insert_with(Vec::new);
            pair_prices.push(price);
        }

        Ok(Prices { prices })
    }

    /// gets daily price if exists
    pub fn get(&self, pair: CurrencyPair<'a>, at: NaiveDate) -> Option<Price<'a>> {
        self.prices.get(&pair).and_then(|prices| {
            prices
                .iter()
                .find(|price| price.date_time.date() == at)
                .cloned()
        })
    }
}

fn parse_date(s: &str) -> NaiveDateTime {
    DateTime::parse_from_rfc3339(s)
        .expect(format!("Invalid date_time {}", s).as_ref())
        .naive_utc()
}
