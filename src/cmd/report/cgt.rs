use std::{collections::HashMap, fmt, io::Write};

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};

use serde::{Deserialize, Serialize};

use crate::{
    cmd::prices::{CurrencyPair, Price, Prices},
    currencies::{Currency, BTC, ETH, GBP},
    money::display_amount,
    trades::{Trade, TradeKey, TradeKind, TradeRecord},
    Money,
};
use rust_decimal::Decimal;

pub type Year = i32;

pub struct TaxYear<'a> {
    pub year: Year,
    pub events: Vec<TaxEvent<'a>>,
}
impl<'a> TaxYear<'a> {
    fn new(year: Year) -> Self {
        TaxYear {
            year,
            events: Vec::new(),
        }
    }
}

pub struct TaxReport<'a> {
    pub trades: Vec<Trade<'a>>,
    pub years: HashMap<Year, TaxYear<'a>>,
    pub pools: HashMap<String, Pool<'a>>,
}

impl<'a> TaxReport<'a> {
    fn new(
        trades: Vec<Trade<'a>>,
        gains: Vec<TaxEvent<'a>>,
        pools: HashMap<String, Pool<'a>>,
    ) -> Self {
        let mut tax_years = HashMap::new();
        for gain in gains.iter() {
            let year = gain.tax_year;
            let ty = tax_years.entry(year).or_insert(TaxYear::new(year));
            ty.events.push(gain.clone())
        }
        Self {
            trades: trades.to_vec(),
            years: tax_years,
            pools,
        }
    }

    pub(crate) fn gains(&self, year: Option<Year>) -> Gains {
        let mut gains = year
            .and_then(|y| self.years.get(&y).map(|ty| ty.events.clone()))
            .unwrap_or(
                self.years
                    .iter()
                    .flat_map(|(_, y)| y.events.clone())
                    .collect::<Vec<_>>(),
            );
        gains.sort_by(|g1, g2| g1.trade.date_time.cmp(&g2.trade.date_time));
        Gains { year, gains }
    }
}

pub struct Gains<'a> {
    pub year: Option<Year>,
    pub gains: Vec<TaxEvent<'a>>,
}

impl<'a> IntoIterator for Gains<'a> {
    type Item = TaxEvent<'a>;
    type IntoIter = std::vec::IntoIter<TaxEvent<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.gains.into_iter()
    }
}

impl<'a> Gains<'a> {
    pub(crate) fn len(&self) -> usize {
        self.gains.len()
    }

    pub(crate) fn total_proceeds(&self) -> Money<'a> {
        self.gains.iter().fold(Money::from_major(0, GBP), |acc, g| {
            acc + g.proceeds().clone()
        })
    }

    pub(crate) fn total_allowable_costs(&self) -> Money<'a> {
        self.gains.iter().fold(Money::from_major(0, GBP), |acc, g| {
            acc + g.allowable_costs().clone()
        })
    }

    pub(crate) fn total_gain(&self) -> Money<'a> {
        self.gains
            .iter()
            .fold(Money::from_major(0, GBP), |acc, g| acc + g.gain())
    }
}

#[derive(Clone)]
pub struct TaxEvent<'a> {
    trade: Trade<'a>,
    tax_year: Year,
    buy_value: Money<'a>,
    sell_value: Money<'a>,
    fee_value: Money<'a>,
    price: Price<'a>,
    allowable_costs: Money<'a>,
    buy_pool: Option<Pool<'a>>,
    sell_pool: Option<Pool<'a>>,
}
impl<'a> TaxEvent<'a> {
    pub fn proceeds(&self) -> &Money<'a> {
        &self.sell_value // todo: fees
    }

    pub fn allowable_costs(&self) -> &Money<'a> {
        &self.allowable_costs
    }

    pub fn fee(&self) -> &Money<'a> {
        &self.fee_value
    }

    pub fn gain(&self) -> Money<'a> {
        self.sell_value.clone() - self.allowable_costs.clone() - self.fee().clone()
    }

    pub fn write_csv<E, W>(tax_events: E, writer: W) -> color_eyre::Result<()>
    where
        E: IntoIterator<Item = TaxEvent<'a>>,
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
impl<'a> From<TaxEvent<'a>> for TaxEventRecord {
    fn from(tax_event: TaxEvent) -> Self {
        TaxEventRecord {
            date_time: tax_event.trade.date_time.date().to_string(),
            tax_year: tax_event.tax_year,
            exchange: tax_event.trade.exchange.clone().unwrap_or(String::new()),
            buy_asset: tax_event.trade.buy.currency().code.to_string(),
            buy_amt: display_amount(&tax_event.trade.buy),
            sell_asset: tax_event.trade.sell.currency().code.to_string(),
            sell_amt: display_amount(&tax_event.trade.sell),
            price: tax_event.price.pair.to_string(),
            rate: tax_event.price.rate.to_string(),
            buy_gbp: display_amount(&tax_event.buy_value),
            sell_gbp: display_amount(&tax_event.sell_value),
            fee: display_amount(tax_event.fee()),
            allowable_cost: display_amount(tax_event.allowable_costs()),
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
pub struct Pool<'a> {
    currency: &'a Currency,
    total: Money<'a>,
    costs: Money<'a>,
}
impl<'a> Pool<'a> {
    fn new(currency: &'a Currency) -> Self {
        Pool {
            currency,
            total: Money::from_major(0, currency),
            costs: Money::from_major(0, GBP),
        }
    }

    fn buy(&mut self, buy: &Money<'a>, costs: &Money<'a>) {
        self.total = self.total.clone() + buy.clone();
        self.costs = self.costs.clone() + costs.clone();
        log::debug!(
            "Pool BUY {}, costs: {}",
            display_amount(&buy),
            display_amount(&costs)
        );
        log::debug!("Pool: {:?}", self);
    }

    fn sell(&mut self, sell: Money<'a>) -> Money<'a> {
        let (costs, new_total, new_costs) = if sell > self.total {
            // selling more than is in the pool
            (
                self.costs.clone(),
                Money::from_major(0, &self.currency),
                Money::from_major(0, GBP),
            )
        } else {
            let perc = sell.amount() / self.total.amount();
            let costs = self.costs.clone() * perc;
            let new_total = self.total.clone() - sell.clone();
            let new_costs = self.costs.clone() - costs.clone();
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

    fn cost_basis(&self) -> Decimal {
        use rust_decimal::prelude::Zero;
        self.costs
            .amount()
            .checked_div(*self.total.amount())
            .unwrap_or(Decimal::zero())
    }
}

impl<'a> fmt::Debug for Pool<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "currency: {}, total: {}, costs: {}",
            self.currency.code,
            display_amount(&self.total),
            display_amount(&self.costs)
        )
    }
}

pub fn calculate<'a>(
    trades: Vec<Trade<'a>>,
    prices: &'a Prices<'a>,
) -> color_eyre::Result<TaxReport<'a>> {
    let mut pools = HashMap::new();

    // todo: sort trades (test)
    let trades_with_prices = trades
        .iter()
        .map(|trade| {
            let price = get_price(trade, &prices).expect(&format!(
                "Should have price for buy: {} sell: {} at {}",
                trade.buy, trade.sell, trade.date_time
            ));
            (trade, price)
        })
        .collect::<Vec<_>>();

    let mut special_buys: HashMap<TradeKey, Money> = HashMap::new();

    let gains = trades_with_prices
        .iter()
        .cloned()
        .map(|(trade, price)| {
            let trade_record: TradeRecord = trade.into();
            log::debug!("Trade: {:?}", trade_record);
            let mut buy_pool: Option<Pool> = None;
            let mut sell_pool: Option<Pool> = None;
            let mut allowable_costs = Money::from_major(0, GBP);

            if trade.buy.currency() != GBP {
                let _zero = Money::from_major(0, trade.buy.currency());
                let buy_amount = special_buys.get(&trade.key()).unwrap_or(&trade.buy);
                let costs = convert_to_gbp(buy_amount.clone(), &price, trade.rate)?;
                let pool = pools
                    .entry(trade.buy.currency().code.to_string())
                    .or_insert(Pool::new(trade.buy.currency()));
                pool.buy(buy_amount, &costs);
                buy_pool = Some(pool.clone());
            }

            if trade.sell.currency() != GBP {
                // find any buys of this asset within the next 30 days
                let special_rules_buy = trades_with_prices
                    .iter()
                    .filter(|(t, _)| {
                        t.buy.currency() == trade.sell.currency()
                            && t.date_time.date() >= trade.date_time.date()
                            && t.date_time < trade.date_time + Duration::days(30)
                    })
                    .cloned()
                    .collect::<Vec<_>>();

                let mut main_pool_sell = trade.sell.clone();
                let mut special_allowable_costs = Money::from_major(0, GBP);

                for (future_buy, buy_price) in special_rules_buy {
                    let remaining_buy_amount = special_buys
                        .entry(future_buy.key())
                        .or_insert(future_buy.buy.clone());

                    if *remaining_buy_amount > Money::from_major(0, remaining_buy_amount.currency())
                    {
                        let (sell, special_buy_amt) = if *remaining_buy_amount <= main_pool_sell {
                            (
                                main_pool_sell - remaining_buy_amount.clone(),
                                remaining_buy_amount.clone(),
                            )
                        } else {
                            (Money::from_major(0, trade.sell.currency()), main_pool_sell)
                        };
                        *remaining_buy_amount =
                            remaining_buy_amount.clone() - special_buy_amt.clone();
                        let costs =
                            convert_to_gbp(special_buy_amt.clone(), &buy_price, future_buy.rate)?;
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
                    .entry(trade.sell.currency().code.to_string())
                    .or_insert(Pool::new(trade.sell.currency()));
                let main_pool_costs = pool.sell(main_pool_sell);
                allowable_costs = main_pool_costs + special_allowable_costs;
                sell_pool = Some(pool.clone());
            }

            let sell_value = if trade.sell.currency() == GBP {
                trade.sell.clone()
            } else {
                convert_to_gbp(trade.sell.clone(), &price, trade.rate)?
            };

            let buy_value = if trade.buy.currency() == GBP {
                trade.buy.clone()
            } else {
                convert_to_gbp(trade.buy.clone(), &price, trade.rate)?
            };

            let fee_value = if trade.fee.currency() == GBP {
                trade.fee.clone()
            } else {
                convert_to_gbp(trade.fee.clone(), &price, trade.rate)?
            };

            let tax_year = uk_tax_year(trade.date_time);

            Ok(TaxEvent {
                trade: trade.clone(),
                buy_value,
                sell_value,
                fee_value,
                price: price.clone(),
                allowable_costs,
                tax_year,
                sell_pool,
                buy_pool,
            })
        })
        .collect::<color_eyre::Result<Vec<_>>>()?;
    let report = TaxReport::new(trades, gains, pools);
    Ok(report)
}

fn convert_to_gbp<'a>(
    money: Money<'a>,
    price: &Price<'a>,
    trade_rate: Decimal,
) -> color_eyre::Result<Money<'a>> {
    if money.currency() == GBP {
        return Ok(money);
    }
    if money.currency() == price.pair.base {
        let quote_rate =
            rusty_money::ExchangeRate::new(money.currency(), GBP, price.rate).expect(&format!(
                "Creating quote rate exchange pair from {} price",
                price.pair
            ));
        let gbp = quote_rate.convert(money)?;
        Ok(gbp)
    } else {
        let base_rate =
            rusty_money::ExchangeRate::new(money.currency(), price.pair.base, trade_rate).expect(
                &format!("Creating base rate exchange pair from {} price", price.pair),
            );
        let gbp_rate =
            rusty_money::ExchangeRate::new(price.pair.base, GBP, price.rate).expect(&format!(
                "Creating quote rate exchange pair from {} price",
                price.pair
            ));
        let base = base_rate.convert(money)?;
        let gbp = gbp_rate.convert(base)?;
        Ok(gbp)
    }
}

fn get_price<'a>(trade: &Trade<'a>, prices: &'a Prices<'a>) -> Option<Price<'a>> {
    // todo - extract and dedup this logic
    let (quote, base) = match trade.kind {
        TradeKind::Buy => (trade.sell.currency(), trade.buy.currency()),
        TradeKind::Sell => (trade.buy.currency(), trade.sell.currency()),
    };

    if quote == GBP {
        return Some(Price {
            pair: CurrencyPair { base, quote: GBP },
            date_time: trade.date_time,
            rate: trade.rate,
        });
    }

    // prefer BTC price, then ETH price
    let base = if quote == BTC {
        BTC
    } else if quote == ETH {
        ETH
    } else {
        panic!(
            "Expected quote price to be BTC or ETH or GBP for trade at {}. quote {}, base {}",
            trade.date_time, quote.code, base.code
        )
    };

    let pair = CurrencyPair {
        base: &base,
        quote: GBP,
    };
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
    use rust_decimal_macros::dec;

    macro_rules! assert_money_eq {
        ($left:expr, $right:expr, $($arg:tt)+) => {
            assert_eq!(format!("{}", $left), format!("{}", $right), $($arg)+);
        };
        ($left:expr, $right:expr) => {
            assert_eq!(format!("{}", $left), format!("{}", $right));
        };
    }

    macro_rules! gbp {
        ($amount:literal) => {
            Money::from_decimal(dec!($amount), GBP);
        };
    }

    macro_rules! btc {
        ($amount:literal) => {
            Money::from_decimal(dec!($amount), BTC);
        };
    }

    fn trade<'a, D>(
        dt: &'a str,
        kind: TradeKind,
        sell: Money<'a>,
        buy: Money<'a>,
        rate: D,
    ) -> Trade<'a>
    where
        D: Into<Decimal>,
    {
        let date_time = NaiveDate::parse_from_str(dt, "%Y-%m-%d")
            .expect("DateTime string should match pattern")
            .and_hms(23, 59, 59);
        let rate = rate.into();

        Trade {
            date_time,
            kind,
            sell,
            buy,
            rate,
            fee: gbp!(0),
            exchange: None,
        }
    }

    #[test]
    fn hmrc_pooling_example() {
        let acq1 = trade("2016-01-01", TradeKind::Buy, gbp!(1000.00), btc!(100.), 10);
        let acq2 = trade("2017-01-01", TradeKind::Buy, gbp!(125_000), btc!(50.), 2500);
        let disp = trade(
            "2018-01-01",
            TradeKind::Sell,
            btc!(50.00),
            gbp!(300_000),
            6000,
        );

        let trades = vec![acq1, acq2, disp];
        let prices = Prices::default();
        let report = calculate(trades, &prices).unwrap();

        let gains_2018 = report.gains(Some(2018));

        assert_money_eq!(gains_2018.total_proceeds(), gbp!(300_000.00));
        assert_money_eq!(gains_2018.total_allowable_costs(), gbp!(42_000.00));
        assert_money_eq!(gains_2018.total_gain(), gbp!(258_000.00));
    }

    #[test]
    fn hmrc_acquiring_within_30_days_of_selling_example() {
        let buy1 = trade(
            "2018-01-01",
            TradeKind::Buy,
            gbp!(200_000),
            btc!(14_000),
            dec!(14.285714286),
        );
        let sell = trade("2018-08-30", TradeKind::Sell, btc!(4000), gbp!(160_000), 40);
        let buy2 = trade("2018-09-11", TradeKind::Buy, gbp!(17_500), btc!(500), 35);

        let trades = vec![buy1, sell, buy2];
        let prices = Prices::default();
        let report = calculate(trades, &prices).unwrap();

        let gains_2019 = report.gains(Some(2019));
        let gain = gains_2019.gains.get(0).unwrap();

        assert_money_eq!(gain.proceeds(), gbp!(160_000), "Consideration");
        assert_money_eq!(gain.allowable_costs, gbp!(67_500.00), "Allowable costs");
        assert_money_eq!(gain.gain(), gbp!(92_500.00), "Gain 30 days");

        let btc_pool = report.pools.get("BTC").expect("BTC should have a Pool");

        assert_money_eq!(btc_pool.total, btc!(10_500), "Remaining in pool");
        assert_money_eq!(
            btc_pool.costs,
            gbp!(150_000.00),
            "Remaining allowable costs"
        );
    }

    #[test]
    fn multiple_acquisitions_within_30_days() {
        let buy1 = trade(
            "2018-01-01",
            TradeKind::Buy,
            gbp!(200_000),
            btc!(14_000),
            dec!(14.285714286),
        );
        let sell = trade("2018-08-30", TradeKind::Sell, btc!(4000), gbp!(160_000), 40);
        let buy2 = trade("2018-09-11", TradeKind::Buy, gbp!(8_750), btc!(250), 35);
        let buy3 = trade("2018-09-12", TradeKind::Buy, gbp!(8_750), btc!(250), 35);

        let trades = vec![buy1, sell, buy2, buy3];
        let prices = Prices::default();
        let report = calculate(trades, &prices).unwrap();

        let gains_2019 = report.gains(Some(2019));
        let gain = gains_2019.gains.get(0).unwrap();

        assert_money_eq!(gain.proceeds(), gbp!(160_000), "Consideration");
        assert_money_eq!(gain.allowable_costs, gbp!(67_500.00), "Allowable costs");
        assert_money_eq!(gain.gain(), gbp!(92_500.00), "Gain 30 days");

        let btc_pool = report.pools.get("BTC").expect("BTC should have a Pool");

        assert_money_eq!(btc_pool.total, btc!(10_500), "Remaining in pool");
        assert_money_eq!(
            btc_pool.costs,
            gbp!(150_000.00),
            "Remaining allowable costs"
        );
    }

    #[test]
    fn multiple_sells_with_same_buy_within_30_days() {
        let buy1 = trade("2018-01-01", TradeKind::Buy, gbp!(100_000), btc!(100), 1000);
        let sell1 = trade("2018-08-30", TradeKind::Sell, btc!(20), gbp!(40_000), 2000);
        let sell2 = trade("2018-09-01", TradeKind::Sell, btc!(20), gbp!(40_000), 2000);
        let buy2 = trade("2018-09-11", TradeKind::Buy, gbp!(15_000), btc!(10), 1500);

        let trades = vec![buy1, sell1, sell2, buy2];
        let prices = Prices::default();
        let report = calculate(trades, &prices).unwrap();

        let gains_2019 = report.gains(Some(2019));
        let gain1 = gains_2019.gains.get(0).unwrap();

        assert_money_eq!(gain1.proceeds(), gbp!(40_000), "Consideration");
        assert_money_eq!(gain1.allowable_costs, gbp!(25_000.00), "Allowable costs");
        assert_money_eq!(gain1.gain(), gbp!(15_000.00), "Gain 30 days");

        let btc_pool = report.pools.get("BTC").expect("BTC should have a Pool");

        assert_money_eq!(btc_pool.total, btc!(70), "Remaining in pool");
        assert_money_eq!(btc_pool.costs, gbp!(70_000.00), "Remaining allowable costs");
    }

    #[test]
    fn acquisition_within_30_days_greater_than_disposal_returned_to_pool() {
        let buy1 = trade(
            "2018-01-01",
            TradeKind::Buy,
            gbp!(200_000),
            btc!(14_000),
            dec!(14.285714286),
        );
        let sell = trade("2018-08-30", TradeKind::Sell, btc!(4000), gbp!(160_000), 40);
        let buy2 = trade("2018-09-11", TradeKind::Buy, gbp!(175_000), btc!(5000), 35);

        let trades = vec![buy1, sell, buy2];
        let prices = Prices::default();
        let report = calculate(trades, &prices).unwrap();

        let gains_2019 = report.gains(Some(2019));
        println!(
            "GAINS {}",
            gains_2019
                .gains
                .iter()
                .map(|g| g.gain().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let tax_event = gains_2019.gains.get(0).unwrap();

        assert_money_eq!(tax_event.proceeds(), gbp!(160_000), "Consideration");
        assert_money_eq!(tax_event.allowable_costs, gbp!(140_000), "Allowable costs");
        assert_money_eq!(tax_event.gain(), gbp!(20_000), "Gain 30 days");

        let btc_pool = report.pools.get("BTC").expect("BTC should have a Pool");

        assert_money_eq!(btc_pool.total, btc!(15_000), "Remaining in pool");
        assert_money_eq!(
            btc_pool.costs,
            gbp!(235_000.00),
            "Remaining allowable costs"
        );
    }

    #[test]
    fn disposal_with_not_enough_funds_in_pool_should_use_partial_allowable_costs() {
        let acq1 = trade("2016-01-01", TradeKind::Buy, gbp!(1000), btc!(1), 1000);
        let disp = trade("2018-01-01", TradeKind::Sell, btc!(2), gbp!(2000), 1000);

        let trades = vec![acq1, disp];
        let prices = Prices::default();
        let report = calculate(trades, &prices).unwrap();

        let gains_2018 = report.gains(Some(2018));

        assert_money_eq!(gains_2018.total_proceeds(), gbp!(2000));
        assert_money_eq!(gains_2018.total_allowable_costs(), gbp!(1000));
        assert_money_eq!(gains_2018.total_gain(), gbp!(1000));
    }

    // todo: test crypto -> crypto trade, should be both a sale and a purchase and require a price

    // todo: test 30 days with multiple buys
}
