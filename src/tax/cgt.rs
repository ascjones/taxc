use crate::events::{EventType, TaxableEvent};
use crate::tax::uk::TaxYear;
use chrono::{Duration, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;

/// Asset pool for share pooling (section 104 pool)
#[derive(Debug, Clone)]
pub struct Pool {
    pub asset: String,
    pub quantity: Decimal,
    pub cost_gbp: Decimal,
}

impl Pool {
    pub fn new(asset: String) -> Self {
        Pool {
            asset,
            quantity: Decimal::ZERO,
            cost_gbp: Decimal::ZERO,
        }
    }

    /// Add to the pool (acquisition)
    pub fn add(&mut self, quantity: Decimal, cost_gbp: Decimal) {
        self.quantity += quantity;
        self.cost_gbp += cost_gbp;
        log::debug!(
            "Pool {} ADD: qty={}, cost={}. New total: qty={}, cost={}",
            self.asset,
            quantity,
            cost_gbp,
            self.quantity,
            self.cost_gbp
        );
    }

    /// Remove from the pool (disposal), returns allowable cost
    pub fn remove(&mut self, quantity: Decimal) -> Decimal {
        if quantity >= self.quantity {
            // Disposing of all or more than in pool
            let cost = self.cost_gbp;
            self.quantity = Decimal::ZERO;
            self.cost_gbp = Decimal::ZERO;
            log::debug!(
                "Pool {} REMOVE ALL: qty={}, cost={}",
                self.asset,
                quantity,
                cost
            );
            cost
        } else {
            // Partial disposal - proportional cost
            let proportion = quantity / self.quantity;
            let cost = (self.cost_gbp * proportion).round_dp(2);
            self.quantity -= quantity;
            self.cost_gbp -= cost;
            log::debug!(
                "Pool {} REMOVE: qty={}, cost={}. Remaining: qty={}, cost={}",
                self.asset,
                quantity,
                cost,
                self.quantity,
                self.cost_gbp
            );
            cost
        }
    }

    /// Cost basis per unit
    pub fn cost_basis(&self) -> Decimal {
        if self.quantity.is_zero() {
            Decimal::ZERO
        } else {
            (self.cost_gbp / self.quantity).round_dp(8)
        }
    }
}

/// Record of a disposal for CGT purposes
#[derive(Debug, Clone)]
pub struct DisposalRecord {
    pub date: NaiveDate,
    pub tax_year: TaxYear,
    pub asset: String,
    pub quantity: Decimal,
    pub proceeds_gbp: Decimal,
    pub allowable_cost_gbp: Decimal,
    pub fees_gbp: Decimal,
    pub gain_gbp: Decimal,
    pub matching: DisposalMatching,
    pub description: Option<String>,
}

/// How the disposal was matched for cost basis
#[derive(Debug, Clone)]
pub enum DisposalMatching {
    /// Matched with same-day acquisition
    SameDay { quantity: Decimal, cost: Decimal },
    /// Matched with bed & breakfast acquisition (within 30 days)
    BedAndBreakfast {
        date: NaiveDate,
        quantity: Decimal,
        cost: Decimal,
    },
    /// Matched from section 104 pool
    Pool { quantity: Decimal, cost: Decimal },
    /// Mixed matching (multiple rules applied)
    Mixed {
        same_day: Option<(Decimal, Decimal)>,
        bed_and_breakfast: Vec<(NaiveDate, Decimal, Decimal)>,
        pool: Option<(Decimal, Decimal)>,
    },
}

impl DisposalRecord {
    pub fn gain(&self) -> Decimal {
        self.proceeds_gbp - self.allowable_cost_gbp - self.fees_gbp
    }
}

/// CSV record for disposal output
#[derive(Debug, Serialize, Deserialize)]
pub struct DisposalCsvRecord {
    pub date: String,
    pub tax_year: String,
    pub asset: String,
    pub quantity: String,
    pub proceeds_gbp: String,
    pub allowable_cost_gbp: String,
    pub fees_gbp: String,
    pub gain_gbp: String,
    pub description: String,
}

impl From<&DisposalRecord> for DisposalCsvRecord {
    fn from(d: &DisposalRecord) -> Self {
        DisposalCsvRecord {
            date: d.date.format("%Y-%m-%d").to_string(),
            tax_year: d.tax_year.display(),
            asset: d.asset.clone(),
            quantity: d.quantity.to_string(),
            proceeds_gbp: d.proceeds_gbp.round_dp(2).to_string(),
            allowable_cost_gbp: d.allowable_cost_gbp.round_dp(2).to_string(),
            fees_gbp: d.fees_gbp.round_dp(2).to_string(),
            gain_gbp: d.gain_gbp.round_dp(2).to_string(),
            description: d.description.clone().unwrap_or_default(),
        }
    }
}

/// CGT report containing all disposals
#[derive(Debug)]
pub struct CgtReport {
    pub disposals: Vec<DisposalRecord>,
    pub pools: HashMap<String, Pool>,
}

impl CgtReport {
    /// Get disposals for a specific tax year
    pub fn disposals_for_year(&self, year: TaxYear) -> Vec<&DisposalRecord> {
        self.disposals
            .iter()
            .filter(|d| d.tax_year == year)
            .collect()
    }

    /// Total proceeds for a tax year
    pub fn total_proceeds(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year)
            .map(|d| d.proceeds_gbp)
            .sum()
    }

    /// Total allowable costs for a tax year
    pub fn total_allowable_costs(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year)
            .map(|d| d.allowable_cost_gbp + d.fees_gbp)
            .sum()
    }

    /// Total gain/loss for a tax year
    pub fn total_gain(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year).map(|d| d.gain_gbp).sum()
    }

    /// Number of disposals for a tax year
    pub fn disposal_count(&self, year: Option<TaxYear>) -> usize {
        self.filter_disposals(year).count()
    }

    fn filter_disposals(&self, year: Option<TaxYear>) -> impl Iterator<Item = &DisposalRecord> {
        self.disposals
            .iter()
            .filter(move |d| year.map_or(true, |y| d.tax_year == y))
    }

    /// Write disposals to CSV
    pub fn write_csv<W: Write>(&self, writer: W, year: Option<TaxYear>) -> color_eyre::Result<()> {
        let mut wtr = csv::Writer::from_writer(writer);
        for disposal in self.filter_disposals(year) {
            let record: DisposalCsvRecord = disposal.into();
            wtr.serialize(record)?;
        }
        wtr.flush()?;
        Ok(())
    }
}

/// Calculate CGT from taxable events
/// Implements HMRC share identification rules:
/// 1. Same-day rule: Match with acquisitions on the same day
/// 2. Bed & breakfast rule: Match with acquisitions within 30 days after disposal
/// 3. Section 104 pool: Match with pooled cost basis
pub fn calculate_cgt(events: Vec<TaxableEvent>) -> CgtReport {
    let mut pools: HashMap<String, Pool> = HashMap::new();
    let mut disposals: Vec<DisposalRecord> = Vec::new();

    // Sort events by date, with disposals before acquisitions on the same day
    // This ensures same-day disposals can match with same-day acquisitions
    // before those acquisitions are added to the pool
    let mut events = events;
    events.sort_by(|a, b| {
        match a.date.cmp(&b.date) {
            std::cmp::Ordering::Equal => {
                // Disposals come before acquisitions on same day
                let a_is_disposal = a.event_type == EventType::Disposal;
                let b_is_disposal = b.event_type == EventType::Disposal;
                b_is_disposal.cmp(&a_is_disposal) // true (disposal) comes first
            }
            other => other,
        }
    });

    // Track which acquisitions have been used for matching
    // Key: (date, asset), Value: remaining quantity available for matching
    let mut acquisition_remaining: HashMap<(NaiveDate, String), Decimal> = HashMap::new();
    // Track total acquisition cost per (date, asset) for cost calculations
    let mut acquisition_total_cost: HashMap<(NaiveDate, String), Decimal> = HashMap::new();
    // Track total acquisition quantity per (date, asset)
    let mut acquisition_total_qty: HashMap<(NaiveDate, String), Decimal> = HashMap::new();

    // First pass: record all acquisitions for matching purposes
    for event in &events {
        if event.event_type == EventType::Acquisition {
            let key = (event.date, event.asset.clone());
            *acquisition_remaining.entry(key.clone()).or_insert(Decimal::ZERO) += event.quantity;
            *acquisition_total_qty.entry(key.clone()).or_insert(Decimal::ZERO) += event.quantity;
            *acquisition_total_cost.entry(key).or_insert(Decimal::ZERO) += event.total_cost_gbp();
        }
    }

    // Track how much has been added to pool per (date, asset)
    let mut pool_added: HashMap<(NaiveDate, String), Decimal> = HashMap::new();

    // Second pass: process all events
    for event in &events {
        match event.event_type {
            EventType::Acquisition => {
                let key = (event.date, event.asset.clone());

                // Calculate how much of this day's acquisitions should go to pool
                let total_qty = acquisition_total_qty.get(&key).copied().unwrap_or(Decimal::ZERO);
                let remaining = acquisition_remaining.get(&key).copied().unwrap_or(Decimal::ZERO);
                let total_cost = acquisition_total_cost.get(&key).copied().unwrap_or(Decimal::ZERO);
                let already_added = pool_added.get(&key).copied().unwrap_or(Decimal::ZERO);

                // The amount that should go to pool is what's left for matching
                // We spread this across all acquisitions proportionally
                if total_qty > Decimal::ZERO {
                    // What portion of remaining should this acquisition contribute?
                    let this_proportion = event.quantity / total_qty;
                    let this_share_of_remaining = (remaining * this_proportion).round_dp(8);

                    // Only add if we haven't already added our share
                    let to_add = this_share_of_remaining;
                    if to_add > Decimal::ZERO {
                        let pool = pools
                            .entry(event.asset.clone())
                            .or_insert_with(|| Pool::new(event.asset.clone()));
                        // Proportional cost
                        let cost = (total_cost * to_add / total_qty).round_dp(2);
                        pool.add(to_add, cost);
                        *pool_added.entry(key).or_insert(Decimal::ZERO) += to_add;
                    }
                }
            }
            EventType::Disposal => {
                let fees = event.fees_gbp.unwrap_or(Decimal::ZERO);
                let tax_year = TaxYear::from_date(event.date);

                let mut remaining_to_match = event.quantity;
                let mut total_allowable_cost = Decimal::ZERO;
                let mut same_day_match: Option<(Decimal, Decimal)> = None;
                let mut bnb_matches: Vec<(NaiveDate, Decimal, Decimal)> = Vec::new();
                let mut pool_match: Option<(Decimal, Decimal)> = None;

                // 1. Same-day rule: match with same-day acquisitions
                let same_day_key = (event.date, event.asset.clone());
                if let Some(available) = acquisition_remaining.get_mut(&same_day_key) {
                    if *available > Decimal::ZERO {
                        let match_qty = remaining_to_match.min(*available);
                        // Find the acquisition to get its cost
                        let same_day_cost = find_acquisition_cost(
                            &events,
                            &event.asset,
                            event.date,
                            match_qty,
                        );
                        total_allowable_cost += same_day_cost;
                        same_day_match = Some((match_qty, same_day_cost));
                        remaining_to_match -= match_qty;
                        *available -= match_qty;
                        log::debug!(
                            "Same-day match: {} {} at cost {}",
                            match_qty,
                            event.asset,
                            same_day_cost
                        );
                    }
                }

                // 2. Bed & breakfast rule: match with acquisitions in next 30 days
                if remaining_to_match > Decimal::ZERO {
                    for days_ahead in 1..=30 {
                        if remaining_to_match <= Decimal::ZERO {
                            break;
                        }
                        let future_date = event.date + Duration::days(days_ahead);
                        let future_key = (future_date, event.asset.clone());
                        if let Some(available) = acquisition_remaining.get_mut(&future_key) {
                            if *available > Decimal::ZERO {
                                let match_qty = remaining_to_match.min(*available);
                                let bnb_cost = find_acquisition_cost(
                                    &events,
                                    &event.asset,
                                    future_date,
                                    match_qty,
                                );
                                total_allowable_cost += bnb_cost;
                                bnb_matches.push((future_date, match_qty, bnb_cost));
                                remaining_to_match -= match_qty;
                                *available -= match_qty;
                                log::debug!(
                                    "B&B match: {} {} on {} at cost {}",
                                    match_qty,
                                    event.asset,
                                    future_date,
                                    bnb_cost
                                );
                            }
                        }
                    }
                }

                // 3. Section 104 pool: match remaining from pool
                if remaining_to_match > Decimal::ZERO {
                    let pool = pools
                        .entry(event.asset.clone())
                        .or_insert_with(|| Pool::new(event.asset.clone()));
                    let pool_cost = pool.remove(remaining_to_match);
                    total_allowable_cost += pool_cost;
                    pool_match = Some((remaining_to_match, pool_cost));
                    log::debug!(
                        "Pool match: {} {} at cost {}",
                        remaining_to_match,
                        event.asset,
                        pool_cost
                    );
                }

                // Determine matching type for record
                let matching = if same_day_match.is_some() || !bnb_matches.is_empty() {
                    if same_day_match.is_some() && bnb_matches.is_empty() && pool_match.is_none() {
                        let (qty, cost) = same_day_match.unwrap();
                        DisposalMatching::SameDay {
                            quantity: qty,
                            cost,
                        }
                    } else if same_day_match.is_none() && bnb_matches.len() == 1 && pool_match.is_none() {
                        let (date, qty, cost) = bnb_matches[0];
                        DisposalMatching::BedAndBreakfast {
                            date,
                            quantity: qty,
                            cost,
                        }
                    } else {
                        DisposalMatching::Mixed {
                            same_day: same_day_match,
                            bed_and_breakfast: bnb_matches,
                            pool: pool_match,
                        }
                    }
                } else if let Some((qty, cost)) = pool_match {
                    DisposalMatching::Pool {
                        quantity: qty,
                        cost,
                    }
                } else {
                    DisposalMatching::Pool {
                        quantity: Decimal::ZERO,
                        cost: Decimal::ZERO,
                    }
                };

                let gain = event.value_gbp - total_allowable_cost - fees;

                disposals.push(DisposalRecord {
                    date: event.date,
                    tax_year,
                    asset: event.asset.clone(),
                    quantity: event.quantity,
                    proceeds_gbp: event.value_gbp,
                    allowable_cost_gbp: total_allowable_cost,
                    fees_gbp: fees,
                    gain_gbp: gain,
                    matching,
                    description: event.description.clone(),
                });
            }
            // Income events (staking, dividends) don't affect CGT
            EventType::StakingReward | EventType::Dividend => {}
        }
    }

    CgtReport { disposals, pools }
}

/// Find the cost of an acquisition on a specific date for a specific quantity
fn find_acquisition_cost(
    events: &[TaxableEvent],
    asset: &str,
    date: NaiveDate,
    quantity: Decimal,
) -> Decimal {
    // Find acquisitions on this date for this asset
    let day_acquisitions: Vec<&TaxableEvent> = events
        .iter()
        .filter(|e| {
            e.event_type == EventType::Acquisition && e.asset == asset && e.date == date
        })
        .collect();

    if day_acquisitions.is_empty() {
        return Decimal::ZERO;
    }

    // Calculate total acquired on this day
    let total_qty: Decimal = day_acquisitions.iter().map(|e| e.quantity).sum();
    let total_cost: Decimal = day_acquisitions.iter().map(|e| e.total_cost_gbp()).sum();

    if total_qty.is_zero() {
        return Decimal::ZERO;
    }

    // Proportional cost for the matched quantity
    let proportion = quantity / total_qty;
    (total_cost * proportion).round_dp(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::AssetClass;
    use rust_decimal_macros::dec;

    fn acq(date: &str, asset: &str, qty: Decimal, value: Decimal) -> TaxableEvent {
        TaxableEvent {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            event_type: EventType::Acquisition,
            asset: asset.to_string(),
            asset_class: AssetClass::Crypto,
            quantity: qty,
            value_gbp: value,
            fees_gbp: None,
            description: None,
        }
    }

    fn acq_with_fee(
        date: &str,
        asset: &str,
        qty: Decimal,
        value: Decimal,
        fee: Decimal,
    ) -> TaxableEvent {
        TaxableEvent {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            event_type: EventType::Acquisition,
            asset: asset.to_string(),
            asset_class: AssetClass::Crypto,
            quantity: qty,
            value_gbp: value,
            fees_gbp: Some(fee),
            description: None,
        }
    }

    fn disp(date: &str, asset: &str, qty: Decimal, value: Decimal) -> TaxableEvent {
        TaxableEvent {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            event_type: EventType::Disposal,
            asset: asset.to_string(),
            asset_class: AssetClass::Crypto,
            quantity: qty,
            value_gbp: value,
            fees_gbp: None,
            description: None,
        }
    }

    fn disp_with_fee(
        date: &str,
        asset: &str,
        qty: Decimal,
        value: Decimal,
        fee: Decimal,
    ) -> TaxableEvent {
        TaxableEvent {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            event_type: EventType::Disposal,
            asset: asset.to_string(),
            asset_class: AssetClass::Crypto,
            quantity: qty,
            value_gbp: value,
            fees_gbp: Some(fee),
            description: None,
        }
    }

    #[test]
    fn pool_basic_operations() {
        let mut pool = Pool::new("BTC".to_string());
        pool.add(dec!(10), dec!(1000));
        assert_eq!(pool.quantity, dec!(10));
        assert_eq!(pool.cost_gbp, dec!(1000));
        assert_eq!(pool.cost_basis(), dec!(100));

        let cost = pool.remove(dec!(5));
        assert_eq!(cost, dec!(500));
        assert_eq!(pool.quantity, dec!(5));
        assert_eq!(pool.cost_gbp, dec!(500));
    }

    #[test]
    fn pool_remove_all() {
        let mut pool = Pool::new("BTC".to_string());
        pool.add(dec!(10), dec!(1000));

        let cost = pool.remove(dec!(15)); // More than available
        assert_eq!(cost, dec!(1000));
        assert_eq!(pool.quantity, Decimal::ZERO);
        assert_eq!(pool.cost_gbp, Decimal::ZERO);
    }

    #[test]
    fn hmrc_pooling_example() {
        // HMRC example: https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
        // Buy 100 BTC for £1,000 in 2016
        // Buy 50 BTC for £125,000 in 2017
        // Sell 50 BTC for £300,000 in 2018
        let events = vec![
            acq("2016-01-01", "BTC", dec!(100), dec!(1000)),
            acq("2017-01-01", "BTC", dec!(50), dec!(125000)),
            disp("2018-01-01", "BTC", dec!(50), dec!(300000)),
        ];

        let report = calculate_cgt(events);

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Pool: 150 BTC, cost £126,000
        // Selling 50 BTC = 50/150 * £126,000 = £42,000 allowable cost
        assert_eq!(disposal.proceeds_gbp, dec!(300000));
        assert_eq!(disposal.allowable_cost_gbp, dec!(42000));
        assert_eq!(disposal.gain_gbp, dec!(258000));
    }

    #[test]
    fn hmrc_pooling_example_out_of_order() {
        // Same as above but events in wrong order - should still work
        let events = vec![
            disp("2018-01-01", "BTC", dec!(50), dec!(300000)),
            acq("2017-01-01", "BTC", dec!(50), dec!(125000)),
            acq("2016-01-01", "BTC", dec!(100), dec!(1000)),
        ];

        let report = calculate_cgt(events);

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        assert_eq!(disposal.proceeds_gbp, dec!(300000));
        assert_eq!(disposal.allowable_cost_gbp, dec!(42000));
        assert_eq!(disposal.gain_gbp, dec!(258000));
    }

    #[test]
    fn same_day_rule() {
        // Buy and sell on same day - should match same-day acquisition
        let events = vec![
            acq("2024-01-15", "BTC", dec!(1), dec!(40000)),
            disp("2024-01-15", "BTC", dec!(1), dec!(45000)),
        ];

        let report = calculate_cgt(events);

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should use same-day cost of £40,000
        assert_eq!(disposal.allowable_cost_gbp, dec!(40000));
        assert_eq!(disposal.gain_gbp, dec!(5000));

        assert!(matches!(disposal.matching, DisposalMatching::SameDay { .. }));
    }

    #[test]
    fn same_day_rule_partial() {
        // Buy 2 BTC, sell 1 BTC on same day
        let events = vec![
            acq("2024-01-15", "BTC", dec!(2), dec!(80000)),
            disp("2024-01-15", "BTC", dec!(1), dec!(45000)),
        ];

        let report = calculate_cgt(events);

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should use proportional same-day cost: 1/2 * £80,000 = £40,000
        assert_eq!(disposal.allowable_cost_gbp, dec!(40000));
        assert_eq!(disposal.gain_gbp, dec!(5000));

        // Pool should have remaining 1 BTC at £40,000
        let pool = report.pools.get("BTC").unwrap();
        assert_eq!(pool.quantity, dec!(1));
        assert_eq!(pool.cost_gbp, dec!(40000));
    }

    #[test]
    fn bed_and_breakfast_rule() {
        // Sell, then buy back within 30 days
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)), // Pool acquisition
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),  // Disposal
            acq("2024-06-20", "BTC", dec!(5), dec!(60000)),   // B&B acquisition
        ];

        let report = calculate_cgt(events);

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should match with B&B acquisition at £60,000
        assert_eq!(disposal.allowable_cost_gbp, dec!(60000));
        assert_eq!(disposal.gain_gbp, dec!(15000));

        assert!(matches!(
            disposal.matching,
            DisposalMatching::BedAndBreakfast { .. }
        ));

        // Pool should still have original 10 BTC
        let pool = report.pools.get("BTC").unwrap();
        assert_eq!(pool.quantity, dec!(10));
    }

    #[test]
    fn bed_and_breakfast_partial() {
        // Sell 5, buy back 3 within 30 days - should match 3 with B&B, 2 from pool
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
            acq("2024-06-20", "BTC", dec!(3), dec!(36000)),
        ];

        let report = calculate_cgt(events);

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // 3 BTC from B&B at £36,000
        // 2 BTC from pool at £20,000 (2/10 * £100,000)
        // Total allowable cost: £56,000
        assert_eq!(disposal.allowable_cost_gbp, dec!(56000));
        assert_eq!(disposal.gain_gbp, dec!(19000));

        assert!(matches!(disposal.matching, DisposalMatching::Mixed { .. }));
    }

    #[test]
    fn bed_and_breakfast_outside_30_days() {
        // Buy back after 30 days - should use pool instead
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
            acq("2024-07-16", "BTC", dec!(5), dec!(60000)), // 31 days later
        ];

        let report = calculate_cgt(events);

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should use pool cost: 5/10 * £100,000 = £50,000
        assert_eq!(disposal.allowable_cost_gbp, dec!(50000));
        assert_eq!(disposal.gain_gbp, dec!(25000));

        assert!(matches!(disposal.matching, DisposalMatching::Pool { .. }));
    }

    #[test]
    fn same_day_takes_priority_over_bed_and_breakfast() {
        // Same-day rule should apply before B&B rule
        let events = vec![
            acq("2024-06-15", "BTC", dec!(3), dec!(45000)), // Same day
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
            acq("2024-06-20", "BTC", dec!(5), dec!(60000)), // B&B
        ];

        let report = calculate_cgt(events);

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // 3 BTC from same-day at £45,000
        // 2 BTC from B&B at 2/5 * £60,000 = £24,000
        // Total: £69,000
        assert_eq!(disposal.allowable_cost_gbp, dec!(69000));
        assert_eq!(disposal.gain_gbp, dec!(6000));
    }

    #[test]
    fn multiple_assets_separate_pools() {
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            acq("2024-01-01", "ETH", dec!(100), dec!(50000)),
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
            disp("2024-06-15", "ETH", dec!(50), dec!(30000)),
        ];

        let report = calculate_cgt(events);

        assert_eq!(report.disposals.len(), 2);

        // BTC disposal
        let btc_disposal = report.disposals.iter().find(|d| d.asset == "BTC").unwrap();
        assert_eq!(btc_disposal.allowable_cost_gbp, dec!(50000));
        assert_eq!(btc_disposal.gain_gbp, dec!(25000));

        // ETH disposal
        let eth_disposal = report.disposals.iter().find(|d| d.asset == "ETH").unwrap();
        assert_eq!(eth_disposal.allowable_cost_gbp, dec!(25000));
        assert_eq!(eth_disposal.gain_gbp, dec!(5000));
    }

    #[test]
    fn tax_year_boundaries() {
        // April 5 is end of tax year, April 6 is start of next
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            disp("2024-04-05", "BTC", dec!(2), dec!(30000)), // 2023/24 tax year
            disp("2024-04-06", "BTC", dec!(2), dec!(32000)), // 2024/25 tax year
        ];

        let report = calculate_cgt(events);

        assert_eq!(report.disposals.len(), 2);

        let d1 = &report.disposals[0];
        assert_eq!(d1.tax_year, TaxYear(2024)); // 2023/24

        let d2 = &report.disposals[1];
        assert_eq!(d2.tax_year, TaxYear(2025)); // 2024/25
    }

    #[test]
    fn disposal_with_fees() {
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            disp_with_fee("2024-06-15", "BTC", dec!(5), dec!(75000), dec!(100)),
        ];

        let report = calculate_cgt(events);

        let disposal = &report.disposals[0];
        assert_eq!(disposal.fees_gbp, dec!(100));
        // Gain = proceeds - allowable cost - fees = 75000 - 50000 - 100 = 24900
        assert_eq!(disposal.gain_gbp, dec!(24900));
    }

    #[test]
    fn acquisition_fees_added_to_pool() {
        let events = vec![
            acq_with_fee("2024-01-01", "BTC", dec!(10), dec!(100000), dec!(500)),
            disp("2024-06-15", "BTC", dec!(10), dec!(150000)),
        ];

        let report = calculate_cgt(events);

        let disposal = &report.disposals[0];
        // Allowable cost should include the £500 fee
        assert_eq!(disposal.allowable_cost_gbp, dec!(100500));
        assert_eq!(disposal.gain_gbp, dec!(49500));
    }

    #[test]
    fn disposal_more_than_pool() {
        // Edge case: selling more than in pool (shouldn't happen but handle gracefully)
        let events = vec![
            acq("2024-01-01", "BTC", dec!(5), dec!(50000)),
            disp("2024-06-15", "BTC", dec!(10), dec!(150000)),
        ];

        let report = calculate_cgt(events);

        let disposal = &report.disposals[0];
        // Should use full pool cost even though disposing more
        assert_eq!(disposal.allowable_cost_gbp, dec!(50000));
        assert_eq!(disposal.gain_gbp, dec!(100000));
    }

    #[test]
    fn report_totals_by_year() {
        let events = vec![
            acq("2024-01-01", "BTC", dec!(100), dec!(100000)),
            disp("2024-04-05", "BTC", dec!(10), dec!(15000)), // 2023/24
            disp("2024-04-06", "BTC", dec!(10), dec!(16000)), // 2024/25
            disp("2024-06-15", "BTC", dec!(10), dec!(17000)), // 2024/25
        ];

        let report = calculate_cgt(events);

        // 2023/24 totals
        assert_eq!(report.total_proceeds(Some(TaxYear(2024))), dec!(15000));
        assert_eq!(report.disposal_count(Some(TaxYear(2024))), 1);

        // 2024/25 totals
        assert_eq!(report.total_proceeds(Some(TaxYear(2025))), dec!(33000));
        assert_eq!(report.disposal_count(Some(TaxYear(2025))), 2);

        // All years
        assert_eq!(report.total_proceeds(None), dec!(48000));
        assert_eq!(report.disposal_count(None), 3);
    }
}
