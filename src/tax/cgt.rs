use crate::events::{EventType, TaxableEvent};
use crate::tax::uk::TaxYear;
use chrono::{Duration, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;

/// Snapshot of pool state at a point in time
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct PoolSnapshot {
    pub quantity: Decimal,
    pub cost_gbp: Decimal,
}

impl From<&Pool> for PoolSnapshot {
    fn from(pool: &Pool) -> Self {
        PoolSnapshot {
            quantity: pool.quantity,
            cost_gbp: pool.cost_gbp,
        }
    }
}

/// Which HMRC rule was used for matching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchingRule {
    SameDay,
    BedAndBreakfast,
    Pool,
}

impl MatchingRule {
    pub fn display(&self) -> &'static str {
        match self {
            MatchingRule::SameDay => "Same-Day",
            MatchingRule::BedAndBreakfast => "B&B",
            MatchingRule::Pool => "Pool",
        }
    }
}

impl std::fmt::Display for MatchingRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

/// A single matching component for detailed reporting
#[derive(Debug, Clone)]
pub struct MatchingComponent {
    pub rule: MatchingRule,
    pub quantity: Decimal,
    pub cost: Decimal,
    pub matched_date: Option<NaiveDate>, // For B&B: the acquisition date
}

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

    #[cfg(test)]
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
    #[allow(dead_code)]
    pub matching: DisposalMatching,
    #[allow(dead_code)]
    pub description: Option<String>,
    /// Pool state after this disposal
    #[allow(dead_code)]
    pub pool_after: PoolSnapshot,
    /// Breakdown by matching rule for detailed reporting
    pub matching_components: Vec<MatchingComponent>,
    /// Whether this disposal came from an UnclassifiedOut event
    pub is_unclassified: bool,
}

/// How the disposal was matched for cost basis
#[allow(dead_code)]
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

/// CSV record for disposal output
#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
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

/// CSV record for detailed disposal output with per-rule breakdown
#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct DetailedDisposalCsvRecord {
    pub date: String,
    pub tax_year: String,
    pub asset: String,
    pub rule: String,
    pub matched_date: String,
    pub quantity: String,
    pub proceeds_gbp: String,
    pub cost_gbp: String,
    pub gain_gbp: String,
    pub pool_quantity: String,
    pub pool_cost_gbp: String,
    pub running_gain_gbp: String,
}

/// CGT report containing all disposals
#[derive(Debug)]
pub struct CgtReport {
    pub disposals: Vec<DisposalRecord>,
    #[allow(dead_code)]
    pub pools: HashMap<String, Pool>,
}

impl CgtReport {
    /// Total proceeds for a tax year (classified events only)
    pub fn total_proceeds(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, true).map(|d| d.proceeds_gbp).sum()
    }

    /// Total proceeds including unclassified events
    pub fn total_proceeds_with_unclassified(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, false).map(|d| d.proceeds_gbp).sum()
    }

    /// Total allowable costs for a tax year (classified events only)
    pub fn total_allowable_costs(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, true)
            .map(|d| d.allowable_cost_gbp + d.fees_gbp)
            .sum()
    }

    /// Total allowable costs including unclassified events
    pub fn total_allowable_costs_with_unclassified(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, false)
            .map(|d| d.allowable_cost_gbp + d.fees_gbp)
            .sum()
    }

    /// Total gain/loss for a tax year (classified events only)
    pub fn total_gain(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, true).map(|d| d.gain_gbp).sum()
    }

    /// Total gain/loss including unclassified events
    pub fn total_gain_with_unclassified(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, false).map(|d| d.gain_gbp).sum()
    }

    /// Count of unclassified disposal events
    pub fn unclassified_count(&self, year: Option<TaxYear>) -> usize {
        self.disposals
            .iter()
            .filter(|d| d.is_unclassified && year.is_none_or(|y| d.tax_year == y))
            .count()
    }

    #[cfg(test)]
    pub fn disposal_count(&self, year: Option<TaxYear>) -> usize {
        self.filter_disposals(year, false).count()
    }

    fn filter_disposals(&self, year: Option<TaxYear>, classified_only: bool) -> impl Iterator<Item = &DisposalRecord> {
        self.disposals
            .iter()
            .filter(move |d| year.is_none_or(|y| d.tax_year == y))
            .filter(move |d| !classified_only || !d.is_unclassified)
    }

    /// Write disposals to CSV
    #[allow(dead_code)]
    pub fn write_csv<W: Write>(&self, writer: W, year: Option<TaxYear>) -> color_eyre::Result<()> {
        let mut wtr = csv::Writer::from_writer(writer);
        for disposal in self.filter_disposals(year, false) {
            let record: DisposalCsvRecord = disposal.into();
            wtr.serialize(record)?;
        }
        wtr.flush()?;
        Ok(())
    }

    /// Write detailed disposals to CSV with per-rule breakdown
    #[allow(dead_code)]
    pub fn write_detailed_csv<W: Write>(
        &self,
        writer: W,
        year: Option<TaxYear>,
    ) -> color_eyre::Result<()> {
        let mut wtr = csv::Writer::from_writer(writer);
        let mut running_gain = Decimal::ZERO;

        for disposal in self.filter_disposals(year, false) {
            let total_qty = disposal.quantity;

            for component in &disposal.matching_components {
                let proportion = if total_qty.is_zero() {
                    Decimal::ZERO
                } else {
                    component.quantity / total_qty
                };

                let proceeds = (disposal.proceeds_gbp * proportion).round_dp(2);
                let gain = (proceeds - component.cost).round_dp(2);
                running_gain += gain;

                let record = DetailedDisposalCsvRecord {
                    date: disposal.date.format("%Y-%m-%d").to_string(),
                    tax_year: disposal.tax_year.display(),
                    asset: disposal.asset.clone(),
                    rule: component.rule.display().to_string(),
                    matched_date: component
                        .matched_date
                        .map(|d| d.format("%Y-%m-%d").to_string())
                        .unwrap_or_default(),
                    quantity: component.quantity.to_string(),
                    proceeds_gbp: proceeds.to_string(),
                    cost_gbp: component.cost.round_dp(2).to_string(),
                    gain_gbp: gain.to_string(),
                    pool_quantity: disposal.pool_after.quantity.to_string(),
                    pool_cost_gbp: disposal.pool_after.cost_gbp.round_dp(2).to_string(),
                    running_gain_gbp: running_gain.round_dp(2).to_string(),
                };
                wtr.serialize(record)?;
            }
        }
        wtr.flush()?;
        Ok(())
    }
}

/// Calculate CGT from taxable events with optional opening pool balances
/// Implements HMRC share identification rules:
/// 1. Same-day rule: Match with acquisitions on the same day
/// 2. Bed & breakfast rule: Match with acquisitions within 30 days after disposal
/// 3. Section 104 pool: Match with pooled cost basis
pub fn calculate_cgt(
    events: Vec<TaxableEvent>,
    opening_pools: Option<&crate::events::OpeningPools>,
) -> CgtReport {
    let mut pools: HashMap<String, Pool> = HashMap::new();
    let mut disposals: Vec<DisposalRecord> = Vec::new();

    // Initialize pools from opening balances if provided
    if let Some(op) = opening_pools {
        for (asset, balance) in &op.pools {
            let mut pool = Pool::new(asset.clone());
            pool.quantity = balance.quantity;
            pool.cost_gbp = balance.cost_gbp;
            log::debug!(
                "Initialized pool {} from opening balance: qty={}, cost={}",
                asset,
                balance.quantity,
                balance.cost_gbp
            );
            pools.insert(asset.clone(), pool);
        }
    }

    // Sort events by date, with disposals before acquisitions on the same day
    // This ensures same-day disposals can match with same-day acquisitions
    // before those acquisitions are added to the pool
    let mut events = events;
    events.sort_by(|a, b| {
        match a.date().cmp(&b.date()) {
            std::cmp::Ordering::Equal => {
                // Disposals come before acquisitions on same day
                let a_is_disposal = a.event_type.is_disposal_like();
                let b_is_disposal = b.event_type.is_disposal_like();
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
    // Include Acquisition, StakingReward, and UnclassifiedIn events
    // UnclassifiedIn is treated as acquisition for conservative estimates
    for event in &events {
        if event.event_type.is_acquisition_like() {
            let key = (event.date(), event.asset.clone());
            *acquisition_remaining
                .entry(key.clone())
                .or_insert(Decimal::ZERO) += event.quantity;
            *acquisition_total_qty
                .entry(key.clone())
                .or_insert(Decimal::ZERO) += event.quantity;
            *acquisition_total_cost.entry(key).or_insert(Decimal::ZERO) += event.total_cost_gbp();
        }
    }

    // Track how much has been added to pool per (date, asset)
    let mut pool_added: HashMap<(NaiveDate, String), Decimal> = HashMap::new();

    // Second pass: process all events
    for event in &events {
        match event.event_type {
            // Acquisition-like events add to the pool (after matching)
            EventType::Acquisition | EventType::StakingReward | EventType::UnclassifiedIn => {
                let key = (event.date(), event.asset.clone());

                // Calculate how much of this day's acquisitions should go to pool
                let total_qty = acquisition_total_qty
                    .get(&key)
                    .copied()
                    .unwrap_or(Decimal::ZERO);
                let remaining = acquisition_remaining
                    .get(&key)
                    .copied()
                    .unwrap_or(Decimal::ZERO);
                let total_cost = acquisition_total_cost
                    .get(&key)
                    .copied()
                    .unwrap_or(Decimal::ZERO);
                let _already_added = pool_added.get(&key).copied().unwrap_or(Decimal::ZERO);

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
            EventType::Disposal | EventType::UnclassifiedOut => {
                let fees = event.fees_gbp.unwrap_or(Decimal::ZERO);
                let tax_year = TaxYear::from_date(event.date());
                let is_unclassified = event.event_type == EventType::UnclassifiedOut;

                let mut remaining_to_match = event.quantity;
                let mut total_allowable_cost = Decimal::ZERO;
                let mut same_day_match: Option<(Decimal, Decimal)> = None;
                let mut bnb_matches: Vec<(NaiveDate, Decimal, Decimal)> = Vec::new();
                let mut pool_match: Option<(Decimal, Decimal)> = None;

                // 1. Same-day rule: match with same-day acquisitions
                let same_day_key = (event.date(), event.asset.clone());
                if let Some(available) = acquisition_remaining.get_mut(&same_day_key) {
                    if *available > Decimal::ZERO {
                        let match_qty = remaining_to_match.min(*available);
                        // Find the acquisition to get its cost
                        let same_day_cost =
                            find_acquisition_cost(&events, &event.asset, event.date(), match_qty);
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
                        let future_date = event.date() + Duration::days(days_ahead);
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
                let matching = if let (Some((qty, cost)), true, true) =
                    (same_day_match, bnb_matches.is_empty(), pool_match.is_none())
                {
                    DisposalMatching::SameDay {
                        quantity: qty,
                        cost,
                    }
                } else if same_day_match.is_none() && bnb_matches.len() == 1 && pool_match.is_none()
                {
                    let (date, qty, cost) = bnb_matches[0];
                    DisposalMatching::BedAndBreakfast {
                        date,
                        quantity: qty,
                        cost,
                    }
                } else if same_day_match.is_some() || !bnb_matches.is_empty() {
                    DisposalMatching::Mixed {
                        same_day: same_day_match,
                        bed_and_breakfast: bnb_matches.clone(),
                        pool: pool_match,
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

                // Capture pool state after disposal
                let pool_after = pools
                    .get(&event.asset)
                    .map(PoolSnapshot::from)
                    .unwrap_or_default();

                // Build matching components for detailed reporting
                let mut matching_components = Vec::new();
                if let Some((qty, cost)) = same_day_match {
                    matching_components.push(MatchingComponent {
                        rule: MatchingRule::SameDay,
                        quantity: qty,
                        cost,
                        matched_date: Some(event.date()),
                    });
                }
                for (date, qty, cost) in &bnb_matches {
                    matching_components.push(MatchingComponent {
                        rule: MatchingRule::BedAndBreakfast,
                        quantity: *qty,
                        cost: *cost,
                        matched_date: Some(*date),
                    });
                }
                if let Some((qty, cost)) = pool_match {
                    matching_components.push(MatchingComponent {
                        rule: MatchingRule::Pool,
                        quantity: qty,
                        cost,
                        matched_date: None,
                    });
                }

                disposals.push(DisposalRecord {
                    date: event.date(),
                    tax_year,
                    asset: event.asset.clone(),
                    quantity: event.quantity,
                    proceeds_gbp: event.value_gbp,
                    allowable_cost_gbp: total_allowable_cost,
                    fees_gbp: fees,
                    gain_gbp: gain,
                    matching,
                    description: event.description.clone(),
                    pool_after,
                    matching_components,
                    is_unclassified,
                });
            }
            // Dividends don't affect CGT
            EventType::Dividend => {}
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
    // Include Acquisition, StakingReward, and UnclassifiedIn events
    let day_acquisitions: Vec<&TaxableEvent> = events
        .iter()
        .filter(|e| e.event_type.is_acquisition_like() && e.asset == asset && e.date() == date)
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
            datetime: NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
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
            datetime: NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
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
            datetime: NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            event_type: EventType::Disposal,
            asset: asset.to_string(),
            asset_class: AssetClass::Crypto,
            quantity: qty,
            value_gbp: value,
            fees_gbp: None,
            description: None,
        }
    }

    fn staking(date: &str, asset: &str, qty: Decimal, value: Decimal) -> TaxableEvent {
        TaxableEvent {
            datetime: NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            event_type: EventType::StakingReward,
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
            datetime: NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
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

        let report = calculate_cgt(events, None);

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

        let report = calculate_cgt(events, None);

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

        let report = calculate_cgt(events, None);

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should use same-day cost of £40,000
        assert_eq!(disposal.allowable_cost_gbp, dec!(40000));
        assert_eq!(disposal.gain_gbp, dec!(5000));

        assert!(matches!(
            disposal.matching,
            DisposalMatching::SameDay { .. }
        ));
    }

    #[test]
    fn same_day_rule_partial() {
        // Buy 2 BTC, sell 1 BTC on same day
        let events = vec![
            acq("2024-01-15", "BTC", dec!(2), dec!(80000)),
            disp("2024-01-15", "BTC", dec!(1), dec!(45000)),
        ];

        let report = calculate_cgt(events, None);

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

        let report = calculate_cgt(events, None);

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

        let report = calculate_cgt(events, None);

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

        let report = calculate_cgt(events, None);

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

        let report = calculate_cgt(events, None);

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

        let report = calculate_cgt(events, None);

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

        let report = calculate_cgt(events, None);

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

        let report = calculate_cgt(events, None);

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

        let report = calculate_cgt(events, None);

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

        let report = calculate_cgt(events, None);

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

        let report = calculate_cgt(events, None);

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

    // Tests for new detailed reporting functionality

    #[test]
    fn pool_snapshot_accuracy_after_disposal() {
        // Test that pool_after accurately reflects pool state after each disposal
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            disp("2024-06-15", "BTC", dec!(3), dec!(45000)),
            disp("2024-07-15", "BTC", dec!(2), dec!(30000)),
        ];

        let report = calculate_cgt(events, None);

        assert_eq!(report.disposals.len(), 2);

        // After first disposal: 10 - 3 = 7 BTC remaining
        let d1 = &report.disposals[0];
        assert_eq!(d1.pool_after.quantity, dec!(7));
        // Cost: 100000 * (7/10) = 70000
        assert_eq!(d1.pool_after.cost_gbp, dec!(70000));

        // After second disposal: 7 - 2 = 5 BTC remaining
        let d2 = &report.disposals[1];
        assert_eq!(d2.pool_after.quantity, dec!(5));
        // Cost: 70000 * (5/7) = 50000
        assert_eq!(d2.pool_after.cost_gbp, dec!(50000));
    }

    #[test]
    fn matching_components_sum_to_total_cost() {
        // Test that matching components sum to total allowable cost
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
            acq("2024-06-20", "BTC", dec!(3), dec!(36000)), // B&B - 3 matched
        ];

        let report = calculate_cgt(events, None);
        let disposal = &report.disposals[0];

        // Components should be: 3 B&B + 2 Pool
        assert_eq!(disposal.matching_components.len(), 2);

        // Sum of component costs should equal allowable cost
        let total_component_cost: Decimal =
            disposal.matching_components.iter().map(|c| c.cost).sum();
        assert_eq!(total_component_cost, disposal.allowable_cost_gbp);
    }

    #[test]
    fn matching_components_same_day_and_pool() {
        // Test same-day + pool matching creates correct components
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            acq("2024-06-15", "BTC", dec!(2), dec!(30000)), // Same-day
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
        ];

        let report = calculate_cgt(events, None);
        let disposal = &report.disposals[0];

        // Should have 2 components: same-day (2) + pool (3)
        assert_eq!(disposal.matching_components.len(), 2);

        // Find same-day component
        let same_day = disposal
            .matching_components
            .iter()
            .find(|c| c.rule == MatchingRule::SameDay)
            .unwrap();
        assert_eq!(same_day.quantity, dec!(2));
        assert_eq!(same_day.cost, dec!(30000));
        assert_eq!(same_day.matched_date, Some(disposal.date));

        // Find pool component
        let pool = disposal
            .matching_components
            .iter()
            .find(|c| c.rule == MatchingRule::Pool)
            .unwrap();
        assert_eq!(pool.quantity, dec!(3));
        // Pool cost: 3/10 * 100000 = 30000
        assert_eq!(pool.cost, dec!(30000));
        assert!(pool.matched_date.is_none());
    }

    #[test]
    fn matching_components_bnb_has_matched_date() {
        // Test B&B component has the correct matched acquisition date
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
            acq("2024-06-20", "BTC", dec!(5), dec!(60000)), // B&B
        ];

        let report = calculate_cgt(events, None);
        let disposal = &report.disposals[0];

        assert_eq!(disposal.matching_components.len(), 1);
        let bnb = &disposal.matching_components[0];

        assert_eq!(bnb.rule, MatchingRule::BedAndBreakfast);
        assert_eq!(
            bnb.matched_date,
            Some(NaiveDate::parse_from_str("2024-06-20", "%Y-%m-%d").unwrap())
        );
    }

    #[test]
    fn staking_rewards_matched_same_day() {
        // Staking rewards are acquisitions at FMV and should be matchable
        // via same-day rule when there's a disposal on the same day
        let events = vec![
            staking("2024-03-08", "DOT", dec!(100), dec!(800)), // Staking reward
            disp("2024-03-08", "DOT", dec!(10), dec!(85)),      // Fee disposal same day
        ];

        let report = calculate_cgt(events, None);
        assert_eq!(report.disposals.len(), 1);

        let disposal = &report.disposals[0];

        // Should have same-day matching component
        assert!(
            !disposal.matching_components.is_empty(),
            "Expected matching components but got none"
        );

        let same_day = disposal
            .matching_components
            .iter()
            .find(|c| c.rule == MatchingRule::SameDay);
        assert!(
            same_day.is_some(),
            "Expected Same-Day matching but got: {:?}",
            disposal.matching_components
        );

        let same_day = same_day.unwrap();
        assert_eq!(same_day.quantity, dec!(10));
        // Cost should be proportional: 10/100 * 800 = 80
        assert_eq!(same_day.cost, dec!(80));
    }

    #[test]
    fn staking_rewards_matched_bnb() {
        // Staking rewards should also be matchable via B&B rule
        let events = vec![
            disp("2024-03-08", "DOT", dec!(10), dec!(85)),       // Disposal
            staking("2024-03-15", "DOT", dec!(100), dec!(800)),  // Staking reward within 30 days
        ];

        let report = calculate_cgt(events, None);
        assert_eq!(report.disposals.len(), 1);

        let disposal = &report.disposals[0];

        let bnb = disposal
            .matching_components
            .iter()
            .find(|c| c.rule == MatchingRule::BedAndBreakfast);
        assert!(
            bnb.is_some(),
            "Expected B&B matching but got: {:?}",
            disposal.matching_components
        );

        let bnb = bnb.unwrap();
        assert_eq!(bnb.quantity, dec!(10));
        // Cost should be proportional: 10/100 * 800 = 80
        assert_eq!(bnb.cost, dec!(80));
        assert_eq!(
            bnb.matched_date,
            Some(NaiveDate::parse_from_str("2024-03-15", "%Y-%m-%d").unwrap())
        );
    }

    #[test]
    fn multi_asset_pool_isolation() {
        // Test that pool snapshots are per-asset
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            acq("2024-01-01", "ETH", dec!(100), dec!(50000)),
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
        ];

        let report = calculate_cgt(events, None);
        let btc_disposal = &report.disposals[0];

        // BTC pool after should show BTC state only
        assert_eq!(btc_disposal.pool_after.quantity, dec!(5));
        assert_eq!(btc_disposal.pool_after.cost_gbp, dec!(50000));

        // ETH pool should be unaffected
        let eth_pool = report.pools.get("ETH").unwrap();
        assert_eq!(eth_pool.quantity, dec!(100));
        assert_eq!(eth_pool.cost_gbp, dec!(50000));
    }

    #[test]
    fn detailed_csv_output() {
        // Test that detailed CSV includes all components
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            acq("2024-06-15", "BTC", dec!(2), dec!(30000)),
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
        ];

        let report = calculate_cgt(events, None);
        let mut output = Vec::new();
        report.write_detailed_csv(&mut output, None).unwrap();

        let csv_str = String::from_utf8(output).unwrap();
        // Should have header + 2 data rows (same-day + pool)
        let lines: Vec<_> = csv_str.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 rows

        // Check header contains expected columns
        assert!(csv_str.contains("rule"));
        assert!(csv_str.contains("pool_quantity"));
        assert!(csv_str.contains("running_gain_gbp"));
    }

    // Tests for opening pool functionality

    #[test]
    fn opening_pool_balance() {
        use crate::events::{OpeningPool, OpeningPools};
        use std::collections::HashMap;

        let opening = OpeningPools {
            as_of_date: Some("2024-03-06".to_string()),
            pools: HashMap::from([(
                "BTC".to_string(),
                OpeningPool {
                    quantity: dec!(10),
                    cost_gbp: dec!(100000),
                },
            )]),
        };

        let events = vec![disp("2024-04-15", "BTC", dec!(5), dec!(75000))];

        let report = calculate_cgt(events, Some(&opening));

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should use opening pool cost: 5/10 * £100,000 = £50,000
        assert_eq!(disposal.allowable_cost_gbp, dec!(50000));
        assert_eq!(disposal.gain_gbp, dec!(25000));
    }

    #[test]
    fn opening_pool_with_new_acquisitions() {
        use crate::events::{OpeningPool, OpeningPools};
        use std::collections::HashMap;

        let opening = OpeningPools {
            as_of_date: Some("2024-03-06".to_string()),
            pools: HashMap::from([(
                "BTC".to_string(),
                OpeningPool {
                    quantity: dec!(10),
                    cost_gbp: dec!(100000),
                },
            )]),
        };

        let events = vec![
            acq("2024-04-10", "BTC", dec!(5), dec!(75000)), // New acquisition
            disp("2024-04-15", "BTC", dec!(10), dec!(200000)),
        ];

        let report = calculate_cgt(events, Some(&opening));

        // Pool after acquisition: 15 BTC at £175,000 (avg £11,666.67)
        // Disposal of 10: 10/15 * 175000 = £116,666.67
        let disposal = &report.disposals[0];
        assert_eq!(disposal.allowable_cost_gbp.round_dp(2), dec!(116666.67));
    }

    #[test]
    fn opening_pool_multiple_assets() {
        use crate::events::{OpeningPool, OpeningPools};
        use std::collections::HashMap;

        let opening = OpeningPools {
            as_of_date: Some("2024-03-06".to_string()),
            pools: HashMap::from([
                (
                    "BTC".to_string(),
                    OpeningPool {
                        quantity: dec!(10),
                        cost_gbp: dec!(100000),
                    },
                ),
                (
                    "ETH".to_string(),
                    OpeningPool {
                        quantity: dec!(50),
                        cost_gbp: dec!(50000),
                    },
                ),
            ]),
        };

        let events = vec![
            disp("2024-04-15", "BTC", dec!(5), dec!(75000)),
            disp("2024-04-15", "ETH", dec!(25), dec!(30000)),
        ];

        let report = calculate_cgt(events, Some(&opening));

        assert_eq!(report.disposals.len(), 2);

        // BTC: 5/10 * 100000 = 50000 cost
        let btc = report.disposals.iter().find(|d| d.asset == "BTC").unwrap();
        assert_eq!(btc.allowable_cost_gbp, dec!(50000));

        // ETH: 25/50 * 50000 = 25000 cost
        let eth = report.disposals.iter().find(|d| d.asset == "ETH").unwrap();
        assert_eq!(eth.allowable_cost_gbp, dec!(25000));
    }
}
