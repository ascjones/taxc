use super::Year;
use crate::{
    cmd::prices::{CurrencyPair, Price, Prices},
    currencies::{Currency, GBP},
    money::{display_amount, saturating_sub, zero},
    trades::{Trade, TradeKind},
    Money,
};
use chrono::{Datelike, Duration, NaiveDate};
use color_eyre::eyre;
use rust_decimal::Decimal;
use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
    fmt
};
use serde::Serialize;

pub struct TaxYear<'a> {
    pub year: Year,
    pub disposals: Vec<Disposal<'a>>,
}
impl<'a> TaxYear<'a> {
    fn new(year: Year) -> Self {
        TaxYear {
            year,
            disposals: Vec::new(),
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
        gains: Vec<Disposal<'a>>,
        pools: HashMap<String, Pool<'a>>,
    ) -> Self {
        let mut tax_years = HashMap::new();
        for disposal in gains.iter() {
            let year = Self::uk_tax_year(disposal.date);
            let ty = tax_years.entry(year).or_insert(TaxYear::new(year));
            ty.disposals.push(disposal.clone())
        }
        Self {
            trades: trades.to_vec(),
            years: tax_years,
            pools,
        }
    }

    fn uk_tax_year(date: NaiveDate) -> Year {
        // todo check tax year length 6th Apr -> 5th Apr
        let year = date.year();
        if date > NaiveDate::from_ymd(year, 4, 5) && date <= NaiveDate::from_ymd(year, 12, 31) {
            year + 1
        } else {
            year
        }
    }

    pub(crate) fn gains(&self, year: Option<Year>) -> Gains {
        let mut disposals = year
            .and_then(|y| self.years.get(&y).map(|ty| ty.disposals.clone()))
            .unwrap_or(
                self.years
                    .iter()
                    .flat_map(|(_, y)| y.disposals.clone())
                    .collect::<Vec<_>>(),
            );
        disposals.sort_by_key(|d| d.date);
        Gains { year, gains: disposals }
    }
}

pub struct Gains<'a> {
    pub year: Option<Year>,
    pub gains: Vec<Disposal<'a>>,
}

impl<'a> IntoIterator for Gains<'a> {
    type Item = Disposal<'a>;
    type IntoIter = std::vec::IntoIter<Disposal<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.gains.into_iter()
    }
}

impl<'a> Gains<'a> {
    pub(crate) fn len(&self) -> usize {
        self.gains.len()
    }

    pub(crate) fn total_proceeds(&self) -> Money<'a> {
        self.gains
            .iter()
            .fold(zero(GBP), |acc, g| acc + g.proceeds().clone())
    }

    pub(crate) fn total_allowable_costs(&self) -> Money<'a> {
        self.gains
            .iter()
            .fold(zero(GBP), |acc, g| acc + g.allowable_costs().clone())
    }

    pub(crate) fn total_gain(&self) -> Money<'a> {
        self.gains.iter().fold(zero(GBP), |acc, g| acc + g.gain())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Disposal<'a> {
    pub(super) date: NaiveDate,
    #[serde(serialize_with = "serialize_amount")]
    pub(super) quantity: Money<'a>,
    pub(super) match_type: MatchType,
    #[serde(serialize_with = "serialize_amount")]
    pub(super) cost: Money<'a>,
    #[serde(serialize_with = "serialize_amount")]
    pub(super) fees: Money<'a>,
    #[serde(serialize_with = "serialize_amount")]
    pub(super) proceeds: Money<'a>,
}

impl<'a> Disposal<'a> {
    pub fn proceeds(&self) -> &Money<'a> {
        &self.proceeds // todo: fees
    }

    pub fn allowable_costs(&self) -> &Money<'a> {
        &self.cost
    }

    pub fn fee(&self) -> &Money<'a> {
        &self.fees
    }

    pub fn gain(&self) -> Money<'a> {
        self.proceeds.clone() - self.cost.clone() - self.fee().clone()
    }
}

fn serialize_amount<S>(money: &Money, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer
{
    display_amount(money).serialize(serializer)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
pub enum MatchType {
    SameDay,
    BedAndBreakfast(NaiveDate),
    Section104Pool,
    None,
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
            total: zero(currency),
            costs: zero(GBP),
        }
    }

    pub fn total(&self) -> &Money<'a> {
        &self.total
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
            (self.costs.clone(), zero(&self.currency), zero(GBP))
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

    pub fn cost_basis(&self) -> Decimal {
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

pub struct Disposals<'a> {
    disposals: BTreeMap<(NaiveDate, String, MatchType), Disposal<'a>>
}

impl<'a> Disposals<'a> {
    fn new() -> Self {
        Self { disposals: BTreeMap::new() }
    }

    fn add(&mut self, trade: &Trade<'a>, date: NaiveDate, price: &Price<'a>, quantity: Money<'a>, cost: Money<'a>, match_type: MatchType) -> color_eyre::Result<()> {
        if quantity.is_zero() {
            log::debug!(
                "Attempting to add a disposal with 0 quantity for trade on {:?}, igmoring",
                trade.date_time
            );
            return Ok(())
        }

        let proceeds = if quantity.currency() == GBP {
            quantity.clone()
        } else {
            price.convert_to_gbp(quantity.clone(), trade.rate)?
        };

        let fees = if trade.fee.currency() == GBP {
            trade.fee.clone()
        } else {
            price.convert_to_gbp(trade.fee.clone(), trade.rate)?
        };

        let key = (trade.date_time.date(), trade.sell.currency().code.to_string(), match_type.clone());
        let updated = self.disposals
            .entry(key)
            .and_modify(|disposal| {
                disposal.quantity += quantity.clone();
                disposal.cost += cost.clone();
                disposal.fees += fees.clone();
                disposal.proceeds += proceeds.clone();
            })
            .or_insert(Disposal {
                date,
                quantity: quantity.clone(),
                cost,
                fees,
                proceeds,
                match_type
            });
        Ok(())
    }

    fn into_vec(&self) -> Vec<Disposal<'a>> {
        let mut result = self.disposals.values().cloned().collect::<Vec<_>>();
        result.sort_by_key(|d| (d.date, d.match_type));
        result
    }
}

pub fn calculate<'a>(
    mut trades: Vec<Trade<'a>>,
    prices: &'a Prices<'a>,
) -> color_eyre::Result<TaxReport<'a>> {
    let mut pools = HashMap::new();

    trades.sort_by_key(|trade| trade.date_time);

    // acquisitions already matched within 30 days of a disposal
    let mut bandb_matched_acqs: HashMap<(NaiveDate, String), Money> = HashMap::new();
    let mut disposals = Disposals::new();

    for trade in &trades {
        let price = get_price(&trade, &prices).expect(&format!(
            "Should have price for buy: {} sell: {} at {}",
            trade.buy, trade.sell, trade.date_time
        ));

        if trade.buy.currency() != GBP {
            // this trade is an acquisition
            // if this acquisition was already matched with a disposal, only pool the remainder
            let trade_date = trade.date_time.date();
            let key = (trade_date, trade.buy.currency().code.to_string());
            let buy_amount = if let Some(bandb_remaining) = bandb_matched_acqs.get(&key) {
                // this acquisition was already partly accounted for by an earlier disposal less
                // than 30 days ago
                bandb_remaining.clone()
            } else {
                trade.buy.clone()
            };
            let costs = price.convert_to_gbp(buy_amount.clone(), trade.rate)?;
            let pool = pools
                .entry(trade.buy.currency().code.to_string())
                .or_insert(Pool::new(trade.buy.currency()));
            pool.buy(&buy_amount, &costs);
        }

        if trade.sell.currency() != GBP {
            // this trade is a disposal
            // find any buys of this asset within the next 30 days
            let special_rules_buy = trades
                .iter()
                .filter(|t| {
                    t.buy.currency() == trade.sell.currency()
                        && t.date_time.date() >= trade.date_time.date()
                        && t.date_time < trade.date_time + Duration::days(30)
                })
                .cloned()
                .collect::<Vec<_>>();

            let mut unmatched_disposed = trade.sell.clone();

            for future_buy in &special_rules_buy {
                let future_buy_key = (future_buy.date_time.date(), future_buy.buy.currency().code.to_string());
                let remaining_buy_amount = bandb_matched_acqs
                    // todo: update if multiple trades same day
                    .entry(future_buy_key)
                    .or_insert(future_buy.buy.clone());

                if *remaining_buy_amount > zero(remaining_buy_amount.currency()) {
                    let matched_disposed = std::cmp::min(unmatched_disposed.clone(), remaining_buy_amount.clone());
                    unmatched_disposed = saturating_sub(&unmatched_disposed, remaining_buy_amount);

                    *remaining_buy_amount = saturating_sub(remaining_buy_amount, &matched_disposed);

                    let buy_price = get_price(&future_buy, &prices).ok_or(eyre::eyre!(
                        "Failed to find price for B&B trade {}",
                        future_buy.date_time
                    ))?;
                    let costs =
                        buy_price.convert_to_gbp(matched_disposed.clone(), future_buy.rate)?;
                    let match_type = MatchType::BedAndBreakfast(future_buy.date_time.date()); // todo: make SameDay if same day acq

                    disposals.add(future_buy, trade.date_time.date(), &price, matched_disposed, costs, match_type)?;
                }
            }

            let pool = pools
                .entry(trade.sell.currency().code.to_string())
                .or_insert(Pool::new(trade.sell.currency()));
            let main_pool_costs = pool.sell(unmatched_disposed.clone());
            let match_type = MatchType::Section104Pool; // todo: split if part or all of costs not in pool

            disposals.add(trade, trade.date_time.date(), &price, unmatched_disposed, main_pool_costs, match_type)?;
        }
    }
    let report = TaxReport::new(trades, disposals.into_vec(), pools);
    Ok(report)
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

    let pair = CurrencyPair {
        base: &quote,
        quote: GBP,
    };
    prices.get(pair, trade.date_time.date())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{currencies::BTC, trades::Trade};
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
    fn hmrc_pooling_example_out_of_order() {
        let acq1 = trade("2016-01-01", TradeKind::Buy, gbp!(1000.00), btc!(100.), 10);
        let acq2 = trade("2017-01-01", TradeKind::Buy, gbp!(125_000), btc!(50.), 2500);
        let disp = trade(
            "2018-01-01",
            TradeKind::Sell,
            btc!(50.00),
            gbp!(300_000),
            6000,
        );

        let trades = vec![disp, acq2, acq1];
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
        let bandb_gain = gains_2019.gains.get(0).unwrap();
        let pooled_gain = gains_2019.gains.get(1).unwrap();

        assert_eq!(bandb_gain.match_type, MatchType::BedAndBreakfast(NaiveDate::from_ymd(2018, 09, 11)));
        assert_money_eq!(bandb_gain.proceeds(), gbp!(20_000), "Consideration");
        assert_money_eq!(bandb_gain.cost, gbp!(17_500), "Allowable costs");
        assert_money_eq!(bandb_gain.gain(), gbp!(2_500), "Gain B&B");

        assert_eq!(pooled_gain.match_type, MatchType::Section104Pool);
        assert_money_eq!(pooled_gain.proceeds(), gbp!(140_000), "Consideration");
        assert_money_eq!(pooled_gain.cost, gbp!(50_000.00), "Allowable costs");
        assert_money_eq!(pooled_gain.gain(), gbp!(90_000.00), "Gain Pooled");

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

        let bandb_gain1 = gains_2019.gains.get(0).unwrap();
        assert_eq!(bandb_gain1.match_type, MatchType::BedAndBreakfast(NaiveDate::from_ymd(2018, 09, 11)));
        assert_money_eq!(bandb_gain1.proceeds(), gbp!(10_000), "Consideration");
        assert_money_eq!(bandb_gain1.cost, gbp!(8_750), "Allowable costs");
        assert_money_eq!(bandb_gain1.gain(), gbp!(1_250), "Gain B&B");

        let bandb_gain2 = gains_2019.gains.get(1).unwrap();
        assert_eq!(bandb_gain2.match_type, MatchType::BedAndBreakfast(NaiveDate::from_ymd(2018, 09, 12)));
        assert_money_eq!(bandb_gain2.proceeds(), gbp!(10_000), "Consideration");
        assert_money_eq!(bandb_gain2.cost, gbp!(8_750), "Allowable costs");
        assert_money_eq!(bandb_gain2.gain(), gbp!(1_250), "Gain B&B");

        let pooled_gain = gains_2019.gains.get(2).unwrap();
        assert_eq!(pooled_gain.match_type, MatchType::Section104Pool);
        assert_money_eq!(pooled_gain.proceeds(), gbp!(140_000), "Consideration");
        assert_money_eq!(pooled_gain.cost, gbp!(50_000.00), "Allowable costs");
        assert_money_eq!(pooled_gain.gain(), gbp!(90_000.00), "Gain Pooled");

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
        println!(
            "GAINS {}",
            gains_2019
                .gains
                .iter()
                .map(|g| format!(
                    "\nmatch: {:?}, proceeds: {}, cost: {}, gain {}",
                    g.match_type,
                    display_amount(&g.proceeds()),
                    display_amount(&g.cost),
                    display_amount(&g.gain()),
                ))
                // .map(|g| g.gain().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let gain1 = gains_2019.gains.get(0).unwrap();
        assert_eq!(gain1.match_type, MatchType::BedAndBreakfast(NaiveDate::from_ymd(2018, 09, 11)));
        assert_money_eq!(gain1.proceeds(), gbp!(20_000), "Consideration");
        assert_money_eq!(gain1.cost, gbp!(15_000), "Allowable costs");
        assert_money_eq!(gain1.gain(), gbp!(5_000), "Gain 30 days");

        let gain2 = gains_2019.gains.get(1).unwrap();
        assert_eq!(gain2.match_type, MatchType::Section104Pool);
        assert_money_eq!(gain2.proceeds(), gbp!(20_000), "Consideration");
        assert_money_eq!(gain2.cost, gbp!(10_000.00), "Allowable costs");
        assert_money_eq!(gain2.gain(), gbp!(10_000.00), "Gain 30 days");

        let gain3 = gains_2019.gains.get(2).unwrap();
        assert_eq!(gain3.match_type, MatchType::Section104Pool);
        assert_money_eq!(gain3.proceeds(), gbp!(40_000), "Consideration");
        assert_money_eq!(gain3.cost, gbp!(20_000.00), "Allowable costs");
        assert_money_eq!(gain3.gain(), gbp!(20_000.00), "Gain 30 days");

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
        let tax_event = gains_2019.gains.get(0).unwrap();

        assert_money_eq!(tax_event.proceeds(), gbp!(160_000), "Consideration");
        assert_money_eq!(tax_event.cost, gbp!(140_000), "Allowable costs");
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
