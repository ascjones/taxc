use std::collections::HashMap;
use std::error::Error;

use std::io::{Read, Write};
use std::ops::Add;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde_derive::{Deserialize, Serialize};
use steel_cent::Money;

use crate::coins::{display_amount, parse_money_parts};

#[derive(Clone, Copy)]
pub struct TradeAmount {
    amount: Money,
    gbp_value: Money,
}
impl Add for TradeAmount {
    type Output = TradeAmount;

    fn add(self, other: TradeAmount) -> TradeAmount {
        TradeAmount {
            amount: self.amount + other.amount,
            gbp_value: self.gbp_value + other.gbp_value,
        }
    }
}

#[derive(Clone)]
pub struct Trade {
    pub date_time: NaiveDateTime,
    pub kind: TradeKind,
    pub buy: Money,
    pub sell: Money,
    pub fee: Money,
    pub rate: f64,
    pub exchange: Option<String>,
}
impl Trade {
    /// Unique key for Trade
    pub fn key(&self) -> TradeKey {
        TradeKey {
            date_time: self.date_time,
            buy: self.buy.to_string(),
            sell: self.sell.to_string(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum TradeKind {
    Buy,
    Sell,
}

#[derive(Eq, PartialEq, Hash)]
pub struct TradeKey {
    date_time: NaiveDateTime,
    buy: String,
    sell: String,
}

/// groups trades that occur for a currency on the same day/account
pub fn group_trades_by_day(trades: &[Trade]) -> Vec<Trade> {
    let mut days = HashMap::new();
    for trade in trades.iter() {
        let key = (
            trade.date_time.date(),
            trade.kind.clone(),
            trade.exchange.clone(),
            trade.buy.currency,
            trade.sell.currency,
            trade.fee.currency,
        );
        let day = days.entry(key).or_insert(Vec::new());
        day.push(trade);
    }
    days.iter()
        .map(
            |((day, kind, exchange, buy_curr, sell_curr, fee_currency), day_trades)| {
                let (total_buy, total_sell, total_fee) = day_trades.iter().fold(
                    (
                        Money::zero(*buy_curr),
                        Money::zero(*sell_curr),
                        Money::zero(*fee_currency),
                    ),
                    |(buy, sell, fee), t| (buy + t.buy, sell + t.sell, fee + t.fee),
                );

                // todo: check if these are the correct way around
                let (quote_curr, base_curr) = match kind {
                    TradeKind::Buy => (buy_curr, sell_curr),
                    TradeKind::Sell => (sell_curr, buy_curr),
                };

                let average_rate = {
                    let (count, total) = day_trades.iter().fold(
                        (Money::zero(*quote_curr), Money::zero(*quote_curr)),
                        |(count, total), trade| {
                            let (base, _quote) = if trade.buy.currency == *base_curr {
                                (trade.sell, trade.buy)
                            } else if trade.sell.currency == *base_curr {
                                (trade.buy, trade.sell)
                            } else {
                                panic!("Either buy or sell should be in quote currency")
                            };
                            (count + base, total + (base * trade.rate))
                        },
                    );
                    total.minor_amount() as f64 / count.minor_amount() as f64
                };
                let latest_trade = day_trades
                    .iter()
                    .max_by(|e1, e2| e1.date_time.cmp(&e2.date_time))
                    .expect(format!("Should have at least one event for {}", day).as_ref());
                Trade {
                    date_time: latest_trade.date_time,
                    exchange: exchange.clone(),
                    buy: total_buy,
                    sell: total_sell,
                    fee: total_fee,
                    rate: average_rate,
                    kind: kind.clone(),
                }
            },
        )
        .collect()
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub date_time: String,
    pub kind: String,
    pub buy_asset: String,
    pub buy_amount: String,
    pub sell_asset: String,
    pub sell_amount: String,
    pub fee_asset: String,
    pub fee_amount: String,
    pub rate: f64,
    pub exchange: String,
}
impl From<&Trade> for TradeRecord {
    fn from(trade: &Trade) -> Self {
        let date_time = DateTime::<Utc>::from_utc(trade.date_time, Utc)
            .format("%d/%m/%Y %H:%M:%S")
            .to_string();

        TradeRecord {
            date_time,
            buy_asset: trade.buy.currency.code(),
            buy_amount: display_amount(&trade.buy),
            sell_asset: trade.sell.currency.code(),
            sell_amount: display_amount(&trade.sell),
            fee_asset: trade.fee.currency.code(),
            fee_amount: display_amount(&trade.fee),
            rate: trade.rate,
            exchange: trade.exchange.clone().unwrap_or(String::new()),
            kind: match &trade.kind {
                TradeKind::Buy => "Buy",
                TradeKind::Sell => "Sell",
            }
            .into(),
        }
    }
}
impl Into<Trade> for &TradeRecord {
    fn into(self) -> Trade {
        let date_time = NaiveDateTime::parse_from_str(self.date_time.as_ref(), "%d/%m/%Y %H:%M:%S")
            .expect(format!("Invalid date_time {}", self.date_time).as_ref());
        let exchange = if self.exchange == "" {
            None
        } else {
            Some(self.exchange.clone())
        };
        let buy = parse_money_parts(&self.buy_asset, &self.buy_amount)
            .expect(format!("BUY amount: {}", self.buy_amount).as_ref());
        let sell = parse_money_parts(&self.sell_asset, &self.sell_amount)
            .expect(format!("SELL amount: {}", self.sell_amount).as_ref());
        let fee = parse_money_parts(&self.fee_asset, &self.fee_amount)
            .expect(format!("FEE amount: {}", self.fee_amount).as_ref());
        let kind = match self.kind.as_ref() {
            "Buy" => TradeKind::Buy,
            "Sell" => TradeKind::Sell,
            x => panic!("Invalid trade kind {}", x),
        };
        Trade {
            date_time,
            buy,
            sell,
            fee,
            rate: self.rate,
            exchange,
            kind,
        }
    }
}

pub fn write_csv<W>(trades: Vec<Trade>, writer: W) -> Result<(), Box<dyn Error>>
where
    W: Write,
{
    let mut wtr = csv::Writer::from_writer(writer);
    for trade in trades.iter() {
        let record: TradeRecord = trade.into();
        wtr.serialize(record)?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn read_csv<R>(reader: R) -> Result<Vec<Trade>, Box<dyn Error>>
where
    R: Read,
{
    let mut rdr = csv::Reader::from_reader(reader);
    let records: Result<Vec<TradeRecord>, _> = rdr.deserialize::<TradeRecord>().collect();
    let mut trades: Vec<Trade> = records?.iter().map(Into::into).collect();
    trades.sort_by(|tx1, tx2| tx1.date_time.cmp(&tx2.date_time));
    Ok(trades)
}
