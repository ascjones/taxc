use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io::{Read, Write};

use crate::coins::{BTC, ETH};
use crate::get_currency;
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Utc};
use serde::de::DeserializeOwned;
use serde_derive::{Deserialize, Serialize};
use steel_cent::currency::{Currency, GBP, USD};

#[derive(Eq, PartialEq, Hash, Clone)]
pub struct CurrencyPair {
    pub base: Currency,
    pub quote: Currency,
}
impl fmt::Display for CurrencyPair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.base.code(), self.quote.code())
    }
}

#[derive(Clone, PartialEq)]
pub struct Price {
    pub pair: CurrencyPair,
    pub date_time: NaiveDateTime,
    pub rate: f64,
}

#[derive(Default)]
pub struct Prices {
    prices: HashMap<CurrencyPair, Vec<Price>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Record {
    base_currency: String,
    quote_currency: String,
    date_time: String,
    rate: f64,
}

impl Prices {
    /// gets daily price if exists
    pub fn get(&self, pair: CurrencyPair, at: NaiveDate) -> Option<Price> {
        self.prices.get(&pair).and_then(|prices| {
            prices
                .iter()
                .find(|price| price.date_time.date() == at)
                .cloned()
        })
    }

    pub fn read_csv<'a, R>(reader: R) -> Result<Prices, Box<Error>>
    where
        R: Read,
    {
        let mut rdr = csv::Reader::from_reader(reader);
        let result: Result<Vec<_>, _> = rdr.deserialize::<Record>().collect();
        let mut prices = HashMap::new();
        for record in result? {
            let base = get_currency(&record.base_currency)
                .expect(format!("invalid base currency {}", record.base_currency).as_ref());
            let quote = get_currency(&record.quote_currency)
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

    pub fn write_csv<W>(&self, writer: W) -> Result<(), Box<Error>>
    where
        W: Write,
    {
        let mut wtr = csv::Writer::from_writer(writer);
        for (_pair, prices) in self.prices.iter() {
            for price in prices.iter() {
                let date_time = DateTime::<Utc>::from_utc(price.date_time, Utc)
                    .to_rfc3339()
                    .clone();
                let record = Record {
                    base_currency: price.pair.base.code(),
                    quote_currency: price.pair.quote.code(),
                    date_time,
                    rate: price.rate,
                };
                wtr.serialize(record)?;
            }
        }
        wtr.flush()?;
        Ok(())
    }
}

#[derive(Deserialize, Clone)]
struct FiatPriceRecord {
    #[serde(rename = "Date")]
    date: String,
    #[serde(rename = "Price")]
    price: f64,
}

#[derive(Deserialize, Clone)]
struct CoindeskPriceRecord {
    #[serde(rename = "Date")]
    date: String,
    #[serde(rename = "Closing Price (USD)")]
    closing_price: f64,
}

#[derive(Deserialize, Clone)]
struct EtherscanPriceRecord {
    #[serde(rename = "Date(UTC)")]
    date: String,
    #[serde(rename = "Value")]
    value: f64,
}

impl Into<Price> for FiatPriceRecord {
    fn into(self) -> Price {
        let pair = CurrencyPair {
            base: GBP,
            quote: USD,
        };
        // e.g. Jan 31, 2019
        let date_time = NaiveDate::parse_from_str(&self.date, "%b %d, %Y")
            .expect(format!("Invalid gbp/usd date {}", self.date).as_ref())
            .and_hms(23, 59, 59);
        Price {
            pair,
            date_time,
            rate: self.price,
        }
    }
}

impl Into<Price> for CoindeskPriceRecord {
    fn into(self) -> Price {
        let pair = CurrencyPair {
            base: *BTC,
            quote: USD,
        };
        let date_time = parse_date(&self.date);
        Price {
            pair,
            date_time,
            rate: self.closing_price,
        }
    }
}

impl Into<Price> for EtherscanPriceRecord {
    fn into(self) -> Price {
        let pair = CurrencyPair {
            base: *ETH,
            quote: USD,
        };
        let date_time = NaiveDate::parse_from_str(&self.date, "%m/%d/%Y")
            .expect(format!("Invalid etherscan date {}", self.date).as_ref())
            .and_hms(23, 59, 59);
        Price {
            pair,
            date_time,
            rate: self.value,
        }
    }
}

pub fn import_prices<R>(
    gbp_usd_file: R,
    btc_usd_file: R,
    eth_usd_file: R,
) -> Result<Prices, Box<Error>>
where
    R: Read,
{
    let gbp_usd_prices = read_records::<FiatPriceRecord, R>(gbp_usd_file)?;
    let btc_usd_prices = read_records::<CoindeskPriceRecord, R>(btc_usd_file)?;
    let eth_usd_prices = read_records::<EtherscanPriceRecord, R>(eth_usd_file)?;

    let mut prices = HashMap::new();

    let btc_gbp = CurrencyPair {
        base: *BTC,
        quote: GBP,
    };
    let btc_gbp_prices = usd_to_gbp(&btc_gbp, &gbp_usd_prices, btc_usd_prices);
    prices.insert(btc_gbp, btc_gbp_prices);

    let eth_gbp = CurrencyPair {
        base: *ETH,
        quote: GBP,
    };
    let eth_gbp_prices = usd_to_gbp(&eth_gbp, &gbp_usd_prices, eth_usd_prices);
    prices.insert(eth_gbp, eth_gbp_prices);

    Ok(Prices { prices })
}

fn usd_to_gbp(
    pair: &CurrencyPair,
    gbp_usd_prices: &Vec<Price>,
    crypto_usd_prices: Vec<Price>,
) -> Vec<Price> {
    let mut gbp_prices = Vec::new();
    let mut last_price = None;
    for price in crypto_usd_prices.iter() {
        let price_date = price.date_time.date();
        let gbp_usd = gbp_usd_prices.iter().find(|gbp_usd_price| {
            //                println!("Searching {}", gbp_usd_price.date_time.date());
            price_date == gbp_usd_price.date_time.date()
        });
        let gbp_usd_rate = if let Some(gbp_usd) = gbp_usd {
            last_price = Some(gbp_usd.clone());
            gbp_usd.rate
        } else {
            if let Some(last_price) = last_price.clone() {
                // handle missing currency prices over weekends/holidays
                if price_date - last_price.date_time.date() > Duration::days(4) {
                    panic!("No GBP/USD price in the last 4 days since {}", price_date)
                } else {
                    last_price.rate
                }
            } else {
                panic!("No GBP/USD price found for {}", price_date)
            }
        };
        let gbp_crypto_rate = price.rate / gbp_usd_rate;
        let gbp_price = Price {
            pair: pair.clone(),
            date_time: price.date_time,
            rate: gbp_crypto_rate,
        };
        gbp_prices.push(gbp_price);
    }
    gbp_prices
}

fn read_records<'de, T, R>(reader: R) -> Result<Vec<Price>, csv::Error>
where
    R: Read,
    T: DeserializeOwned,
    T: Into<Price>,
    T: Clone,
{
    let mut rdr = csv::Reader::from_reader(reader);
    let result: Result<Vec<_>, _> = rdr.deserialize::<T>().collect();
    result.map(|records| records.iter().cloned().map(Into::into).collect())
}

fn parse_date(s: &str) -> NaiveDateTime {
    DateTime::parse_from_rfc3339(s)
        .expect(format!("Invalid date_time {}", s).as_ref())
        .naive_utc()
}
