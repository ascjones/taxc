use crate::{
    money::{currencies::Currency, display_amount, parse_money_parts, zero},
    Money,
};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::Read, ops::Add};

#[derive(Clone)]
pub struct TradeAmount<'a> {
    amount: Money<'a>,
    gbp_value: Money<'a>,
}

impl<'a> Add for TradeAmount<'a> {
    type Output = TradeAmount<'a>;

    fn add(self, other: TradeAmount<'a>) -> TradeAmount<'a> {
        TradeAmount {
            amount: self.amount + other.amount,
            gbp_value: self.gbp_value + other.gbp_value,
        }
    }
}

#[derive(Clone)]
pub struct Trade<'a> {
    pub date_time: NaiveDateTime,
    pub kind: TradeKind,
    pub buy: Money<'a>,
    pub sell: Money<'a>,
    pub fee: Money<'a>,
    pub rate: Decimal,
    pub exchange: Option<String>,
}

impl<'a> Trade<'a> {
    /// Unique key for Trade
    pub fn key(&self) -> TradeKey {
        TradeKey {
            date_time: self.date_time,
            buy: self.buy.to_string(),
            sell: self.sell.to_string(),
        }
    }

    /// Use to group similar trades on the same day
    pub fn key_by_day(&self) -> TradeByDayKey<'a> {
        TradeByDayKey {
            date: self.date_time.date(),
            kind: self.kind.clone(),
            exchange: self.exchange.clone(),
            buy: self.buy.currency(),
            sell: self.sell.currency(),
            fee: self.fee.currency(),
        }
    }
}

impl<'a> From<TradeRecord> for Trade<'a> {
    fn from(tr: TradeRecord) -> Self {
        let date_time = DateTime::parse_from_rfc3339(tr.date_time.as_ref())
            .expect(format!("Invalid date_time {}", tr.date_time).as_ref())
            .naive_utc();
        let exchange = if tr.exchange == "" {
            None
        } else {
            Some(tr.exchange.clone())
        };
        let buy = parse_money_parts(&tr.buy_asset, &tr.buy_amount)
            .expect(format!("BUY amount: {}", tr.buy_amount).as_ref());
        let sell = parse_money_parts(&tr.sell_asset, &tr.sell_amount)
            .expect(format!("SELL amount: {}", tr.sell_amount).as_ref());
        let fee = parse_money_parts(&tr.fee_asset, &tr.fee_amount)
            .expect(format!("FEE amount: {}", tr.fee_amount).as_ref());
        let kind = match tr.kind.as_ref() {
            "Buy" => TradeKind::Buy,
            "Sell" => TradeKind::Sell,
            x => panic!("Invalid trade kind {}", x),
        };
        Trade {
            date_time,
            buy,
            sell,
            fee,
            rate: tr.rate,
            exchange,
            kind,
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

#[derive(Eq, PartialEq)]
pub struct TradeByDayKey<'a> {
    date: NaiveDate,
    kind: TradeKind,
    exchange: Option<String>,
    buy: &'a Currency,
    sell: &'a Currency,
    fee: &'a Currency,
}

impl<'a> std::hash::Hash for TradeByDayKey<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.date.hash(state);
        self.kind.hash(state);
        self.exchange.hash(state);
        self.buy.code.hash(state);
        self.sell.code.hash(state);
        self.fee.code.hash(state);
    }
}

/// groups trades that occur for a currency on the same day/account
pub fn group_trades_by_day<'a>(trades: &'a [Trade<'a>]) -> Vec<Trade<'a>> {
    let mut days = HashMap::new();
    for trade in trades.iter() {
        let day = days.entry(trade.key_by_day()).or_insert(Vec::new());
        day.push(trade);
    }
    days.iter()
        .map(|(key, day_trades)| {
            let (total_buy, total_sell, total_fee) = day_trades.iter().fold(
                (zero(&key.buy), zero(&key.sell), zero(&key.fee)),
                |(buy, sell, fee), t| {
                    (
                        buy + t.buy.clone(),
                        sell + t.sell.clone(),
                        fee + t.fee.clone(),
                    )
                },
            );

            // todo: check if these are the correct way around
            let (quote_curr, base_curr) = match key.kind {
                TradeKind::Buy => (key.buy, key.sell),
                TradeKind::Sell => (key.sell, key.buy),
            };

            let average_rate = {
                let (count, total) = day_trades.iter().fold(
                    (zero(quote_curr), zero(quote_curr)),
                    |(count, total), trade| {
                        let (base, _quote) = if trade.buy.currency() == base_curr {
                            (trade.sell.clone(), trade.buy.clone())
                        } else if trade.sell.currency() == base_curr {
                            (trade.buy.clone(), trade.sell.clone())
                        } else {
                            panic!("Either buy or sell should be in quote currency")
                        };
                        (count + base.clone(), total + (base * trade.rate))
                    },
                );
                total.amount() / count.amount()
            };
            let latest_trade = day_trades
                .iter()
                .max_by(|e1, e2| e1.date_time.cmp(&e2.date_time))
                .expect(format!("Should have at least one event for {}", key.date).as_ref());
            Trade {
                date_time: latest_trade.date_time,
                exchange: key.exchange.clone(),
                buy: total_buy,
                sell: total_sell,
                fee: total_fee,
                rate: average_rate,
                kind: key.kind.clone(),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub date_time: String,
    pub kind: String,
    pub buy_asset: String,
    pub buy_amount: String,
    pub sell_asset: String,
    pub sell_amount: String,
    pub fee_asset: String,
    pub fee_amount: String,
    pub rate: Decimal,
    pub exchange: String,
}

impl<'a> From<&Trade<'a>> for TradeRecord {
    fn from(trade: &Trade) -> Self {
        let date_time = DateTime::<Utc>::from_utc(trade.date_time, Utc)
            .to_rfc3339();

        TradeRecord {
            date_time,
            buy_asset: trade.buy.currency().code.to_string(),
            buy_amount: display_amount(&trade.buy),
            sell_asset: trade.sell.currency().code.to_string(),
            sell_amount: display_amount(&trade.sell),
            fee_asset: trade.fee.currency().code.to_string(),
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

pub fn read_csv<'a, R>(reader: R) -> color_eyre::Result<Vec<Trade<'a>>>
where
    R: Read,
{
    let mut rdr = csv::Reader::from_reader(reader);
    let records: Result<Vec<TradeRecord>, _> = rdr.deserialize::<TradeRecord>().collect();
    let mut trades: Vec<Trade> = records?.into_iter().map(Into::into).collect();
    trades.sort_by(|tx1, tx2| tx1.date_time.cmp(&tx2.date_time));
    Ok(trades)
}
