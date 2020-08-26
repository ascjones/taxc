use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io::Write;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};

use serde::{Deserialize, Serialize};
use steel_cent::{
    currency::{Currency, GBP},
    Money,
};

use crate::cmd::prices::{CurrencyPair, Price, Prices};
use crate::coins::{display_amount, BTC, ETH};
use crate::trades::{Trade, TradeKey, TradeKind, TradeRecord};

type Year = i32;

pub struct TaxYear {
    pub year: Year,
    pub events: Vec<TaxEvent>,
}
impl TaxYear {
    fn new(year: Year) -> Self {
        TaxYear {
            year,
            events: Vec::new(),
        }
    }
}

pub struct TaxReport {
    pub trades: Vec<Trade>,
    pub years: HashMap<Year, TaxYear>,
    pub pools: HashMap<Currency, Pool>,
}

impl TaxReport {
    pub(crate) fn gains(&self, year: Option<Year>) -> Gains {
        let mut gains = year
            .and_then(|y| self.years.get(&y).map(|ty| ty.events.clone()))
            .unwrap_or(
                self
                    .years
                    .iter()
                    .flat_map(|(_, y)| y.events.clone())
                    .collect::<Vec<_>>(),
            );
        gains.sort_by(|g1, g2| g1.trade.date_time.cmp(&g2.trade.date_time));
        Gains {
            year,
            gains
        }
    }
}

pub struct Gains {
    pub year: Option<Year>,
    pub gains: Vec<TaxEvent>,
}

impl IntoIterator for Gains {
    type Item = TaxEvent;
    type IntoIter = std::vec::IntoIter<TaxEvent>;

    fn into_iter(self) -> Self::IntoIter {
        self.gains.into_iter()
    }
}

impl Gains {
    pub(crate) fn len(&self) -> usize {
        self.gains.len()
    }

    pub(crate) fn total_proceeds(&self) -> Money {
        self.gains
            .iter()
            .fold(Money::zero(GBP), |acc, g| acc + g.proceeds())
    }

    pub(crate) fn total_allowable_costs(&self) -> Money {
        self.gains
            .iter()
            .fold(Money::zero(GBP), |acc, g| acc + g.allowable_costs())
    }

    pub(crate) fn total_gain(&self) -> Money {
        self.gains
            .iter()
            .fold(Money::zero(GBP), |acc, g| acc + g.gain())
    }
}

#[derive(Clone)]
pub struct TaxEvent {
    trade: Trade,
    tax_year: Year,
    buy_value: Money,
    sell_value: Money,
    fee_value: Money,
    price: Price,
    allowable_costs: Money,
    buy_pool: Option<Pool>,
    sell_pool: Option<Pool>,
}
impl TaxEvent {
    pub fn proceeds(&self) -> Money {
        self.sell_value // todo: fees
    }

    pub fn allowable_costs(&self) -> Money {
        self.allowable_costs
    }

    pub fn fee(&self) -> Money {
        self.fee_value
    }

    pub fn gain(&self) -> Money {
        self.sell_value - self.allowable_costs - self.fee()
    }

    pub fn write_csv<E, W>(tax_events: E, writer: W) -> Result<(), Box<dyn Error>>
    where
        E: IntoIterator<Item = TaxEvent>,
        W: Write,
    {
        let mut wtr = csv::Writer::from_writer(writer);
        for tax_event in tax_events.into_iter() {
            let record: TaxEventRecord = tax_event.into();
            wtr.serialize(record)?;
        }
        wtr.flush()?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct TaxEventRecord {
    date_time: String,
    tax_year: Year,
    exchange: String,
    buy_asset: String,
    buy_amt: String,
    sell_asset: String,
    sell_amt: String,
    price: String,
    rate: String,
    buy_gbp: String,
    sell_gbp: String,
    fee: String,
    allowable_cost: String,
    gain: String,
    buy_pool_total: String,
    buy_pool_cost: String,
    sell_pool_total: String,
    sell_pool_cost: String,
}
impl From<TaxEvent> for TaxEventRecord {
    fn from(tax_event: TaxEvent) -> Self {
        TaxEventRecord {
            date_time: tax_event.trade.date_time.date().to_string(),
            tax_year: tax_event.tax_year,
            exchange: tax_event.trade.exchange.clone().unwrap_or(String::new()),
            buy_asset: tax_event.trade.buy.currency.code(),
            buy_amt: display_amount(&tax_event.trade.buy),
            sell_asset: tax_event.trade.sell.currency.code(),
            sell_amt: display_amount(&tax_event.trade.sell),
            price: tax_event.price.pair.to_string(),
            rate: tax_event.price.rate.to_string(),
            buy_gbp: display_amount(&tax_event.buy_value),
            sell_gbp: display_amount(&tax_event.sell_value),
            fee: display_amount(&tax_event.fee()),
            allowable_cost: display_amount(&tax_event.allowable_costs()),
            gain: display_amount(&tax_event.gain()),
            buy_pool_total: tax_event
                .buy_pool
                .as_ref()
                .map_or("".to_string(), |p| display_amount(&p.total)),
            buy_pool_cost: tax_event
                .buy_pool
                .as_ref()
                .map_or("".to_string(), |p| format!("{:.2}", &p.cost_basis())),
            sell_pool_total: tax_event
                .sell_pool
                .as_ref()
                .map_or("".to_string(), |p| display_amount(&p.total)),
            sell_pool_cost: tax_event
                .sell_pool
                .as_ref()
                .map_or("".to_string(), |p| format!("{:.2}", &p.cost_basis())),
        }
    }
}

#[derive(Clone)]
pub struct Pool {
    currency: Currency,
    total: Money,
    costs: Money,
}
impl Pool {
    fn new(currency: Currency) -> Self {
        Pool {
            currency,
            total: Money::zero(currency),
            costs: Money::zero(GBP),
        }
    }

    fn buy(&mut self, buy: &Money, costs: Money) {
        self.total = self.total + buy;
        self.costs = self.costs + costs;
        log::debug!(
            "Pool BUY {}, costs: {}",
            display_amount(buy),
            display_amount(&costs)
        );
        log::debug!("Pool: {:?}", self);
    }

    fn sell(&mut self, sell: Money) -> Money {
        let (costs, new_total, new_costs) = if sell > self.total {
            // selling more than is in the pool
            (self.costs, Money::zero(self.currency), Money::zero(GBP))
        } else {
            let perc = sell.minor_amount() as f64 / self.total.minor_amount() as f64;
            let costs = self.costs * perc;
            let new_total = self.total - sell;
            let new_costs = self.costs - costs;
            (costs, new_total, new_costs)
        };
        self.total = new_total;
        self.costs = new_costs;
        log::debug!(
            "Pool SELL {}, costs: {}",
            display_amount(&sell),
            display_amount(&costs)
        );
        log::debug!("Pool: {:?}", self);
        costs
    }

    fn cost_basis(&self) -> f64 {
        // convert to GBP so minor units are equivalent - will lose some precision
        let total_as_gbp = self.total.convert_to(GBP, 1.0);
        self.costs.minor_amount() as f64 / total_as_gbp.minor_amount() as f64
    }
}
impl fmt::Debug for Pool {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "currency: {}, total: {}, costs: {}",
            self.currency.code(),
            display_amount(&self.total),
            display_amount(&self.costs)
        )
    }
}

pub fn calculate(trades: Vec<Trade>, prices: &Prices) -> Result<TaxReport, String> {
    let mut pools = HashMap::new();

    let convert_to_gbp = |money: &Money, price: &Price, trade_rate: f64| {
        if money.currency == price.pair.base {
            money.convert_to(price.pair.quote, price.rate)
        } else {
            money
                .convert_to(price.pair.base, trade_rate)
                .convert_to(price.pair.quote, price.rate)
        }
    };

    // todo: sort trades (test)

    let trades_with_prices = trades
        .iter()
        .map(|trade| {
            let price = get_price(trade, prices).expect(&format!(
                "Should have price for buy: {} sell: {} at {}",
                trade.buy, trade.sell, trade.date_time
            ));
            (trade, price)
        })
        .collect::<Vec<_>>();

    let mut special_buys: HashMap<TradeKey, Money> = HashMap::new();

    let gains: Vec<TaxEvent> = trades_with_prices
        .iter()
        .cloned()
        .map(|(trade, price)| {
            let trade_record: TradeRecord = trade.into();
            log::debug!("Trade: {:?}", trade_record);
            let mut buy_pool: Option<Pool> = None;
            let mut sell_pool: Option<Pool> = None;
            let mut allowable_costs = Money::zero(GBP);

            if trade.buy.currency != GBP {
                let _zero = Money::zero(trade.buy.currency);
                let buy_amount = special_buys.get(&trade.key()).unwrap_or(&trade.buy);
                let costs = convert_to_gbp(buy_amount, &price, trade.rate);
                let pool = pools
                    .entry(trade.buy.currency)
                    .or_insert(Pool::new(trade.buy.currency));
                pool.buy(buy_amount, costs);
                buy_pool = Some(pool.clone());
            }

            if trade.sell.currency != GBP {
                // find any buys of this asset within the next 30 days
                let special_rules_buy = trades_with_prices
                    .iter()
                    .filter(|(t, _)| {
                        t.buy.currency == trade.sell.currency
                            && t.date_time.date() >= trade.date_time.date()
                            && t.date_time < trade.date_time + Duration::days(30)
                    })
                    .cloned()
                    .collect::<Vec<_>>();

                let mut main_pool_sell = trade.sell;
                let mut special_allowable_costs = Money::zero(GBP);

                for (future_buy, buy_price) in special_rules_buy {
                    let remaining_buy_amount = special_buys
                        .entry(future_buy.key())
                        .or_insert(future_buy.buy);

                    if *remaining_buy_amount > Money::zero(remaining_buy_amount.currency) {
                        let (sell, special_buy_amt) = if *remaining_buy_amount <= main_pool_sell {
                            (
                                main_pool_sell - *remaining_buy_amount,
                                *remaining_buy_amount,
                            )
                        } else {
                            (Money::zero(trade.sell.currency), main_pool_sell)
                        };
                        *remaining_buy_amount = *remaining_buy_amount - special_buy_amt;
                        let costs = convert_to_gbp(&special_buy_amt, &buy_price, future_buy.rate);
                        log::debug!(
                            "Deducting SELL of {} from future BUY at {}, cost: {}",
                            display_amount(&special_buy_amt),
                            future_buy.date_time,
                            display_amount(&costs)
                        );
                        main_pool_sell = sell;
                        special_allowable_costs = special_allowable_costs + costs;
                    }
                }

                let pool = pools
                    .entry(trade.sell.currency)
                    .or_insert(Pool::new(trade.sell.currency));
                let main_pool_costs = pool.sell(main_pool_sell);
                allowable_costs = main_pool_costs + special_allowable_costs;
                sell_pool = Some(pool.clone());
            }

            let sell_value = if trade.sell.currency == GBP {
                trade.sell
            } else {
                convert_to_gbp(&trade.sell, &price, trade.rate)
            };

            let buy_value = if trade.buy.currency == GBP {
                trade.buy
            } else {
                convert_to_gbp(&trade.buy, &price, trade.rate)
            };

            let fee_value = if trade.fee.currency == GBP {
                trade.fee
            } else {
                convert_to_gbp(&trade.fee, &price, trade.rate)
            };

            let tax_year = uk_tax_year(trade.date_time);

            TaxEvent {
                trade: trade.clone(),
                buy_value,
                sell_value,
                fee_value,
                price: price.clone(),
                allowable_costs,
                tax_year,
                sell_pool: sell_pool,
                buy_pool: buy_pool,
            }
        })
        .collect();
    Ok(create_report(trades, gains, pools))
}

fn create_report(
    trades: Vec<Trade>,
    gains: Vec<TaxEvent>,
    pools: HashMap<Currency, Pool>,
) -> TaxReport {
    let mut tax_years = HashMap::new();
    for gain in gains.iter() {
        let year = gain.tax_year;
        let ty = tax_years.entry(year).or_insert(TaxYear::new(year));
        ty.events.push(gain.clone())
    }
    TaxReport {
        trades: trades.to_vec(),
        years: tax_years,
        pools,
    }
}

fn get_price(trade: &Trade, prices: &Prices) -> Option<Price> {
    // todo - extract and dedup this logic
    let (quote, base) = match trade.kind {
        TradeKind::Buy => (trade.sell.currency, trade.buy.currency),
        TradeKind::Sell => (trade.buy.currency, trade.sell.currency),
    };

    if quote == GBP {
        return Some(Price {
            pair: CurrencyPair { base, quote: GBP },
            date_time: trade.date_time,
            rate: trade.rate,
        });
    }

    // prefer BTC price, then ETH price
    let base = if quote == *BTC {
        *BTC
    } else if quote == *ETH {
        *ETH
    } else {
        panic!(
            "Expected quote price to be BTC or ETH or GBP for trade at {}. quote {}, base {}",
            trade.date_time,
            quote.code(),
            base.code()
        )
    };

    let pair = CurrencyPair { base, quote: GBP };
    prices.get(pair, trade.date_time.date())
}

fn uk_tax_year(date_time: NaiveDateTime) -> Year {
    let date = date_time.date();
    let year = date.year();
    if date > ymd(year, 4, 5) && date <= ymd(year, 12, 31) {
        year + 1
    } else {
        year
    }
}

fn ymd(y: Year, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd(y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trades::Trade;
    use chrono::NaiveDate;
    use steel_cent::{currency::GBP, Money};

    fn trade(dt: &str, kind: TradeKind, sell: Money, buy: Money, rate: f64) -> Trade {
        let date_time = NaiveDate::parse_from_str(dt, "%Y-%m-%d")
            .expect("DateTime string should match pattern")
            .and_hms(23, 59, 59);

        Trade {
            date_time,
            kind,
            sell,
            buy,
            rate,
            fee: gbp(0),
            exchange: None,
        }
    }

    fn gbp(major: i64) -> Money {
        Money::of_major(GBP, major)
    }

    fn btc(major: i64) -> Money {
        Money::of_major(*BTC, major)
    }

    macro_rules! assert_money_eq {
        ($left:expr, $right:expr, $($arg:tt)+) => {
            assert_eq!($left.to_string(), $right.to_string(), $($arg)+);
        };
        ($left:expr, $right:expr) => {
            assert_eq!($left.to_string(), $right.to_string());
        };
    }

    #[test]
    fn hmrc_pooling_example() {
        let acq1 = trade("2016-01-01", TradeKind::Buy, gbp(1000), btc(100), 10.);
        let acq2 = trade("2017-01-01", TradeKind::Buy, gbp(125000), btc(50), 2500.);
        let disp = trade("2018-01-01", TradeKind::Sell, btc(50), gbp(300000), 6000.);

        let trades = vec![acq1, acq2, disp];
        let report = calculate(trades, &Prices::default()).unwrap();

        let gains_2018 = report.gains(Some(2018));

        assert_money_eq!(gains_2018.total_proceeds(), gbp(300_000));
        assert_money_eq!(gains_2018.total_allowable_costs(), gbp(42_000));
        assert_money_eq!(gains_2018.total_gain(), gbp(258_000));
    }

    #[test]
    fn hmrc_acquiring_within_30_days_of_selling_example() {
        let buy1 = trade(
            "2018-01-01",
            TradeKind::Buy,
            gbp(200_000),
            btc(14_000),
            14.285714286,
        );
        let sell = trade("2018-08-30", TradeKind::Sell, btc(4000), gbp(160_000), 40.);
        let buy2 = trade("2018-09-11", TradeKind::Buy, gbp(17_500), btc(500), 35.);

        let trades = vec![buy1, sell, buy2];
        let report = calculate(trades, &Prices::default()).unwrap();

        let gains_2019 = report.gains(Some(2019));
        let gain = gains_2019.gains.get(0).unwrap();

        assert_money_eq!(gain.proceeds(), gbp(160_000), "Consideration");
        assert_money_eq!(gain.allowable_costs, gbp(67_500), "Allowable costs");
        assert_money_eq!(gain.gain(), gbp(92_500), "Gain 30 days");

        let btc_pool = report.pools.get(&BTC).expect("BTC should have a Pool");

        assert_money_eq!(btc_pool.total, btc(10_500), "Remaining in pool");
        assert_money_eq!(btc_pool.costs, gbp(150_000), "Remaining allowable costs");
    }

    #[test]
    fn multiple_acquisitions_within_30_days() {
        let buy1 = trade(
            "2018-01-01",
            TradeKind::Buy,
            gbp(200_000),
            btc(14_000),
            14.285714286,
        );
        let sell = trade("2018-08-30", TradeKind::Sell, btc(4000), gbp(160_000), 40.);
        let buy2 = trade("2018-09-11", TradeKind::Buy, gbp(8_750), btc(250), 35.);
        let buy3 = trade("2018-09-12", TradeKind::Buy, gbp(8_750), btc(250), 35.);

        let trades = vec![buy1, sell, buy2, buy3];
        let report = calculate(trades, &Prices::default()).unwrap();

        let gains_2019 = report.gains(Some(2019));
        let gain = gains_2019.gains.get(0).unwrap();

        assert_money_eq!(gain.proceeds(), gbp(160_000), "Consideration");
        assert_money_eq!(gain.allowable_costs, gbp(67_500), "Allowable costs");
        assert_money_eq!(gain.gain(), gbp(92_500), "Gain 30 days");

        let btc_pool = report.pools.get(&BTC).expect("BTC should have a Pool");

        assert_money_eq!(btc_pool.total, btc(10_500), "Remaining in pool");
        assert_money_eq!(btc_pool.costs, gbp(150_000), "Remaining allowable costs");
    }

    #[test]
    fn multiple_sells_with_same_buy_within_30_days() {
        let buy1 = trade("2018-01-01", TradeKind::Buy, gbp(100_000), btc(100), 1000.);
        let sell1 = trade("2018-08-30", TradeKind::Sell, btc(20), gbp(40_000), 2000.);
        let sell2 = trade("2018-09-01", TradeKind::Sell, btc(20), gbp(40_000), 2000.);
        let buy2 = trade("2018-09-11", TradeKind::Buy, gbp(15_000), btc(10), 1500.);

        let trades = vec![buy1, sell1, sell2, buy2];
        let report = calculate(trades, &Prices::default()).unwrap();

        let gains_2019 = report.gains(Some(2019));
        let gain1 = gains_2019.gains.get(0).unwrap();

        assert_money_eq!(gain1.proceeds(), gbp(40_000), "Consideration");
        assert_money_eq!(gain1.allowable_costs, gbp(25_000), "Allowable costs");
        assert_money_eq!(gain1.gain(), gbp(15_000), "Gain 30 days");

        let btc_pool = report.pools.get(&BTC).expect("BTC should have a Pool");

        assert_money_eq!(btc_pool.total, btc(70), "Remaining in pool");
        assert_money_eq!(btc_pool.costs, gbp(70_000), "Remaining allowable costs");
    }

    #[test]
    fn acquisition_within_30_days_greater_than_disposal_returned_to_pool() {
        let buy1 = trade(
            "2018-01-01",
            TradeKind::Buy,
            gbp(200_000),
            btc(14_000),
            14.285714286,
        );
        let sell = trade("2018-08-30", TradeKind::Sell, btc(4000), gbp(160_000), 40.);
        let buy2 = trade("2018-09-11", TradeKind::Buy, gbp(175_000), btc(5000), 35.);

        let trades = vec![buy1, sell, buy2];
        let report = calculate(trades, &Prices::default()).unwrap();

        let gains_2019 = report.gains(Some(2019));
        println!(
            "GAINS {}",
            gains_2019.gains
                .iter()
                .map(|g| g.gain().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let tax_event = gains_2019.gains.get(0).unwrap();

        assert_money_eq!(tax_event.proceeds(), gbp(160_000), "Consideration");
        assert_money_eq!(tax_event.allowable_costs, gbp(140_000), "Allowable costs");
        assert_money_eq!(tax_event.gain(), gbp(20_000), "Gain 30 days");

        let btc_pool = report.pools.get(&BTC).expect("BTC should have a Pool");

        assert_money_eq!(btc_pool.total, btc(15_000), "Remaining in pool");
        assert_money_eq!(btc_pool.costs, gbp(235_000), "Remaining allowable costs");
    }

    #[test]
    fn disposal_with_not_enough_funds_in_pool_should_use_partial_allowable_costs() {
        let acq1 = trade("2016-01-01", TradeKind::Buy, gbp(1000), btc(1), 1000.);
        let disp = trade("2018-01-01", TradeKind::Sell, btc(2), gbp(2000), 1000.);

        let trades = vec![acq1, disp];
        let report = calculate(trades, &Prices::default()).unwrap();

        let gains_2018 = report.gains(Some(2018));

        assert_money_eq!(gains_2018.total_proceeds(), gbp(2000));
        assert_money_eq!(gains_2018.total_allowable_costs(), gbp(1000));
        assert_money_eq!(gains_2018.total_gain(), gbp(1000));
    }

    // todo: test crypto -> crypto trade, should be both a sale and a purchase and require a price

    // todo: test 30 days with multiple buys
}
