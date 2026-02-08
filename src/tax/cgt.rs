use crate::events::{EventType, Label, TaxableEvent};
use crate::tax::uk::TaxYear;
use chrono::{DateTime, Duration, FixedOffset, NaiveDate};
use rust_decimal::Decimal;
use serde::{Serialize, Serializer};
use std::collections::HashMap;

fn serialize_date<S: Serializer>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&date.format("%Y-%m-%d").to_string())
}

fn serialize_quantity<S: Serializer>(qty: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
    let s = format!("{:.8}", qty);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    serializer.serialize_str(trimmed)
}

fn serialize_decimal_2dp<S: Serializer>(d: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("{:.2}", d))
}

/// Snapshot of pool state at a point in time (used in tests via pool_after)
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

/// Snapshot of a single pool at a point in time (for daily history)
#[derive(Debug, Clone, Serialize)]
pub struct PoolHistoryEntry {
    #[serde(serialize_with = "serialize_date")]
    pub date: NaiveDate,
    pub asset: String,
    pub event_type: EventType,
    pub label: Label,
    #[serde(serialize_with = "serialize_quantity")]
    pub quantity: Decimal,
    #[serde(serialize_with = "serialize_decimal_2dp")]
    pub cost_gbp: Decimal,
}

/// Year-end pool snapshot
#[derive(Debug, Clone, Serialize)]
pub struct YearEndSnapshot {
    pub tax_year: TaxYear,
    pub pools: Vec<PoolState>,
}

/// State of a single pool
#[derive(Debug, Clone, Serialize)]
pub struct PoolState {
    pub asset: String,
    #[serde(serialize_with = "serialize_quantity")]
    pub quantity: Decimal,
    #[serde(serialize_with = "serialize_decimal_2dp")]
    pub cost_gbp: Decimal,
}

/// Pool history tracking
#[derive(Debug, Clone, Default)]
pub struct PoolHistory {
    pub entries: Vec<PoolHistoryEntry>,
    pub year_end_snapshots: Vec<YearEndSnapshot>,
}

/// Which HMRC rule was used for matching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchingRule {
    SameDay,
    BedAndBreakfast,
    Pool,
}

/// Warning types for disposal records
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type")]
pub enum DisposalWarning {
    /// Event was Unclassified - may need review
    Unclassified,
    /// Pool had insufficient quantity to cover the disposal
    /// When available = 0, this means no cost basis at all
    /// When available > 0, this means partial cost basis
    InsufficientCostBasis {
        available: Decimal,
        required: Decimal,
    },
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
    /// Optional identifier from source data
    pub id: Option<String>,
    pub datetime: DateTime<FixedOffset>,
    pub date: NaiveDate,
    pub tax_year: TaxYear,
    pub asset: String,
    pub quantity: Decimal,
    pub proceeds_gbp: Decimal,
    pub allowable_cost_gbp: Decimal,
    pub fees_gbp: Decimal,
    pub gain_gbp: Decimal,
    /// Pool state after this disposal (used in tests)
    #[allow(dead_code)]
    pub pool_after: PoolSnapshot,
    /// Breakdown by matching rule for detailed reporting
    pub matching_components: Vec<MatchingComponent>,
    /// Warnings for this disposal (unclassified, no cost basis, insufficient pool, etc.)
    pub warnings: Vec<DisposalWarning>,
}

impl DisposalRecord {
    /// Check if this disposal came from an unclassified event
    pub fn is_unclassified(&self) -> bool {
        self.warnings.contains(&DisposalWarning::Unclassified)
    }

    /// Check if this disposal has any warnings
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

/// CGT report containing all disposals
#[derive(Debug)]
pub struct CgtReport {
    pub disposals: Vec<DisposalRecord>,
    /// Final pool states (used in tests)
    #[allow(dead_code)]
    pub pools: HashMap<String, Pool>,
    pub pool_history: PoolHistory,
}

impl CgtReport {
    /// Total proceeds for a tax year (classified events only)
    pub fn total_proceeds(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, true)
            .map(|d| d.proceeds_gbp)
            .sum()
    }

    /// Total proceeds including unclassified events
    pub fn total_proceeds_with_unclassified(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, false)
            .map(|d| d.proceeds_gbp)
            .sum()
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
            .filter(|d| d.is_unclassified() && year.is_none_or(|y| d.tax_year == y))
            .count()
    }

    /// Count of disposals with any warning
    pub fn warning_count(&self, year: Option<TaxYear>) -> usize {
        self.disposals
            .iter()
            .filter(|d| d.has_warnings() && year.is_none_or(|y| d.tax_year == y))
            .count()
    }

    /// Count of disposals with cost basis warnings (InsufficientCostBasis)
    pub fn cost_basis_warning_count(&self, year: Option<TaxYear>) -> usize {
        self.disposals
            .iter()
            .filter(|d| {
                year.is_none_or(|y| d.tax_year == y)
                    && d.warnings
                        .iter()
                        .any(|w| matches!(w, DisposalWarning::InsufficientCostBasis { .. }))
            })
            .count()
    }

    #[cfg(test)]
    pub fn disposal_count(&self, year: Option<TaxYear>) -> usize {
        self.filter_disposals(year, false).count()
    }

    fn filter_disposals(
        &self,
        year: Option<TaxYear>,
        classified_only: bool,
    ) -> impl Iterator<Item = &DisposalRecord> {
        self.disposals
            .iter()
            .filter(move |d| year.is_none_or(|y| d.tax_year == y))
            .filter(move |d| !classified_only || !d.is_unclassified())
    }
}

/// Tracks acquisition quantities available for matching
#[derive(Debug, Default)]
struct AcquisitionTracker {
    total_qty: Decimal,
    total_cost: Decimal,
    same_day_reserved: Decimal,
    same_day_remaining: Decimal,
    bnb_remaining: Decimal,
}

impl AcquisitionTracker {
    fn cost_for_qty(&self, qty: Decimal) -> Decimal {
        if self.total_qty.is_zero() {
            Decimal::ZERO
        } else {
            (self.total_cost * qty / self.total_qty).round_dp(2)
        }
    }

    fn remaining_for_pool(&self) -> Decimal {
        let same_day_used = self.same_day_reserved - self.same_day_remaining;
        let bnb_originally = self.total_qty - self.same_day_reserved;
        let bnb_used = bnb_originally - self.bnb_remaining;
        self.total_qty - same_day_used - bnb_used
    }
}

type AcqKey = (NaiveDate, String);

/// Calculate CGT from taxable events
/// Implements HMRC share identification rules:
/// 1. Same-day rule: Match with acquisitions on the same day
/// 2. Bed & breakfast rule: Match with acquisitions within 30 days after disposal
/// 3. Section 104 pool: Match with pooled cost basis
pub fn calculate_cgt(events: Vec<TaxableEvent>) -> anyhow::Result<CgtReport> {
    let mut pools: HashMap<String, Pool> = HashMap::new();
    let mut disposals: Vec<DisposalRecord> = Vec::new();
    let mut pool_history = PoolHistory::default();
    let mut current_year: Option<TaxYear> = None;

    // Sort events by date, with disposals before acquisitions on the same day
    let mut events = events;
    events.sort_by(|a, b| {
        match a.date().cmp(&b.date()) {
            std::cmp::Ordering::Equal => {
                // Disposals come before acquisitions on same day
                let a_is_disposal = a.event_type == EventType::Disposal;
                let b_is_disposal = b.event_type == EventType::Disposal;
                b_is_disposal.cmp(&a_is_disposal)
            }
            other => other,
        }
    });

    // Build acquisition tracker: first pass records totals
    let mut acquisitions: HashMap<AcqKey, AcquisitionTracker> = HashMap::new();
    for event in &events {
        if event.event_type == EventType::Acquisition {
            let key = (event.date(), event.asset.clone());
            let tracker = acquisitions.entry(key).or_default();
            tracker.total_qty += event.quantity;
            tracker.total_cost += event.total_cost_gbp();
        }
    }

    // Second pass: reserve acquisitions for same-day matching (priority over B&B).
    // See HMRC CG51560 for the matching order: same-day, then 30-day (B&B), then Section 104.
    // https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
    for event in &events {
        if event.event_type == EventType::Disposal {
            let key = (event.date(), event.asset.clone());
            if let Some(tracker) = acquisitions.get_mut(&key) {
                let available = tracker.total_qty - tracker.same_day_reserved;
                if available > Decimal::ZERO {
                    tracker.same_day_reserved += event.quantity.min(available);
                }
            }
        }
    }

    // Initialize remaining amounts for matching
    for tracker in acquisitions.values_mut() {
        tracker.same_day_remaining = tracker.same_day_reserved;
        tracker.bnb_remaining = tracker.total_qty - tracker.same_day_reserved;
    }

    // Third pass: process all events
    for event in &events {
        let event_year = TaxYear::from_date(event.date());

        // Snapshot at year boundary (before processing new year's first event)
        if let Some(prev_year) = current_year {
            if event_year > prev_year {
                pool_history
                    .year_end_snapshots
                    .push(snapshot_pools(prev_year, &pools));
            }
        }
        current_year = Some(event_year);

        match event.event_type {
            // Acquisition events add to the pool (after matching)
            EventType::Acquisition => {
                let key = (event.date(), event.asset.clone());
                if let Some(tracker) = acquisitions.get(&key) {
                    let remaining = tracker.remaining_for_pool();
                    if tracker.total_qty > Decimal::ZERO && remaining > Decimal::ZERO {
                        // This acquisition's proportional share of what goes to pool
                        let proportion = event.quantity / tracker.total_qty;
                        let to_add = (remaining * proportion).round_dp(8);
                        if to_add > Decimal::ZERO {
                            let pool = pools
                                .entry(event.asset.clone())
                                .or_insert_with(|| Pool::new(event.asset.clone()));
                            let cost = tracker.cost_for_qty(to_add);
                            pool.add(to_add, cost);
                        }
                    }
                }
            }
            EventType::Disposal => {
                let fees = event.fee_gbp.unwrap_or(Decimal::ZERO);
                let tax_year = TaxYear::from_date(event.date());

                let mut remaining_to_match = event.quantity;
                let mut total_allowable_cost = Decimal::ZERO;
                let mut same_day_match: Option<(Decimal, Decimal)> = None;
                let mut bnb_matches: Vec<(NaiveDate, Decimal, Decimal)> = Vec::new();
                let mut pool_match: Option<(Decimal, Decimal)> = None;

                // 1. Same-day rule: match with same-day acquisitions.
                // See HMRC CG51560 for the ordering of identification rules.
                // https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
                let key = (event.date(), event.asset.clone());
                if let Some(tracker) = acquisitions.get_mut(&key) {
                    if tracker.same_day_remaining > Decimal::ZERO {
                        let match_qty = remaining_to_match.min(tracker.same_day_remaining);
                        let cost = tracker.cost_for_qty(match_qty);
                        total_allowable_cost += cost;
                        same_day_match = Some((match_qty, cost));
                        remaining_to_match -= match_qty;
                        tracker.same_day_remaining -= match_qty;
                        log::debug!(
                            "Same-day match: {} {} at cost {}",
                            match_qty,
                            event.asset,
                            cost
                        );
                    }
                }

                // 2. Bed & breakfast rule: match with acquisitions in next 30 days.
                // See HMRC CG51560 for the 30-day rule and worked examples.
                // https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
                if remaining_to_match > Decimal::ZERO {
                    for days_ahead in 1..=30 {
                        if remaining_to_match <= Decimal::ZERO {
                            break;
                        }
                        let future_date = event.date() + Duration::days(days_ahead);
                        let future_key = (future_date, event.asset.clone());
                        if let Some(tracker) = acquisitions.get_mut(&future_key) {
                            if tracker.bnb_remaining > Decimal::ZERO {
                                let match_qty = remaining_to_match.min(tracker.bnb_remaining);
                                let cost = tracker.cost_for_qty(match_qty);
                                total_allowable_cost += cost;
                                bnb_matches.push((future_date, match_qty, cost));
                                remaining_to_match -= match_qty;
                                tracker.bnb_remaining -= match_qty;
                                log::debug!(
                                    "B&B match: {} {} on {} at cost {}",
                                    match_qty,
                                    event.asset,
                                    future_date,
                                    cost
                                );
                            }
                        }
                    }
                }

                // 3. Section 104 pool: match remaining from pool.
                // See HMRC CG51560 for the final matching step.
                // https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
                // Track pool state before removal for insufficient pool warning
                let pool_qty_before = pools
                    .get(&event.asset)
                    .map(|p| p.quantity)
                    .unwrap_or(Decimal::ZERO);
                let insufficient_pool =
                    remaining_to_match > Decimal::ZERO && remaining_to_match > pool_qty_before;

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

                // Build warnings
                let mut warnings = Vec::new();
                if event.label == Label::Unclassified {
                    warnings.push(DisposalWarning::Unclassified);
                }
                // Insufficient cost basis: pool didn't have enough to cover the disposal
                // This includes the "no cost basis" case when available = 0
                if insufficient_pool {
                    warnings.push(DisposalWarning::InsufficientCostBasis {
                        available: pool_qty_before,
                        required: remaining_to_match,
                    });
                }

                disposals.push(DisposalRecord {
                    id: event.id.clone(),
                    datetime: event.datetime,
                    date: event.date(),
                    tax_year,
                    asset: event.asset.clone(),
                    quantity: event.quantity,
                    proceeds_gbp: event.value_gbp,
                    allowable_cost_gbp: total_allowable_cost,
                    fees_gbp: fees,
                    gain_gbp: gain,
                    pool_after,
                    matching_components,
                    warnings,
                });
            }
        }

        // Record pool state after event (for daily history)
        if let Some(pool) = pools.get(&event.asset) {
            pool_history.entries.push(PoolHistoryEntry {
                date: event.date(),
                asset: event.asset.clone(),
                event_type: event.event_type,
                label: event.label,
                quantity: pool.quantity,
                cost_gbp: pool.cost_gbp,
            });
        }
    }

    // Final snapshot for last tax year
    if let Some(year) = current_year {
        pool_history
            .year_end_snapshots
            .push(snapshot_pools(year, &pools));
    }

    Ok(CgtReport {
        disposals,
        pools,
        pool_history,
    })
}

fn snapshot_pools(year: TaxYear, pools: &HashMap<String, Pool>) -> YearEndSnapshot {
    let mut pool_states: Vec<PoolState> = pools
        .values()
        .filter(|p| p.quantity > Decimal::ZERO)
        .map(|p| PoolState {
            asset: p.asset.clone(),
            quantity: p.quantity,
            cost_gbp: p.cost_gbp,
        })
        .collect();
    pool_states.sort_by(|a, b| a.asset.cmp(&b.asset));
    YearEndSnapshot {
        tax_year: year,
        pools: pool_states,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{AssetClass, Label};
    use chrono::DateTime;
    use rust_decimal_macros::dec;

    fn dt(date: &str) -> chrono::DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339(&format!("{date}T00:00:00+00:00")).unwrap()
    }

    fn event(
        event_type: EventType,
        label: Label,
        date: &str,
        asset: &str,
        qty: Decimal,
        value: Decimal,
        fee: Option<Decimal>,
    ) -> TaxableEvent {
        TaxableEvent {
            id: None,
            datetime: dt(date),
            event_type,
            label,
            asset: asset.to_string(),
            asset_class: AssetClass::Crypto,
            quantity: qty,
            value_gbp: value,
            fee_gbp: fee,
            description: None,
        }
    }

    fn acq(date: &str, asset: &str, qty: Decimal, value: Decimal) -> TaxableEvent {
        event(
            EventType::Acquisition,
            Label::Trade,
            date,
            asset,
            qty,
            value,
            None,
        )
    }

    fn acq_with_fee(
        date: &str,
        asset: &str,
        qty: Decimal,
        value: Decimal,
        fee: Decimal,
    ) -> TaxableEvent {
        event(
            EventType::Acquisition,
            Label::Trade,
            date,
            asset,
            qty,
            value,
            Some(fee),
        )
    }

    fn disp(date: &str, asset: &str, qty: Decimal, value: Decimal) -> TaxableEvent {
        event(
            EventType::Disposal,
            Label::Trade,
            date,
            asset,
            qty,
            value,
            None,
        )
    }

    fn disp_with_fee(
        date: &str,
        asset: &str,
        qty: Decimal,
        value: Decimal,
        fee: Decimal,
    ) -> TaxableEvent {
        event(
            EventType::Disposal,
            Label::Trade,
            date,
            asset,
            qty,
            value,
            Some(fee),
        )
    }

    fn staking(date: &str, asset: &str, qty: Decimal, value: Decimal) -> TaxableEvent {
        event(
            EventType::Acquisition,
            Label::StakingReward,
            date,
            asset,
            qty,
            value,
            None,
        )
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

        let report = calculate_cgt(events).unwrap();

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

        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        assert_eq!(disposal.proceeds_gbp, dec!(300000));
        assert_eq!(disposal.allowable_cost_gbp, dec!(42000));
        assert_eq!(disposal.gain_gbp, dec!(258000));
    }

    #[test]
    fn hmrc_bnb_example_1() {
        // HMRC example 1: https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
        // Section 104 holding 1,000 shares, disposal of all 1,000,
        // then buy 1,000 within 30 days.
        let events = vec![
            acq("2011-01-01", "X", dec!(1000), dec!(10000)),
            disp("2011-07-01", "X", dec!(1000), dec!(15000)),
            acq("2011-07-31", "X", dec!(1000), dec!(12000)),
        ];

        let report = calculate_cgt(events).unwrap();
        let disposal = &report.disposals[0];

        assert_eq!(disposal.matching_components.len(), 1);
        assert_eq!(
            disposal.matching_components[0].rule,
            MatchingRule::BedAndBreakfast
        );
        assert_eq!(disposal.matching_components[0].quantity, dec!(1000));
        assert_eq!(
            disposal.matching_components[0].matched_date,
            Some(chrono::NaiveDate::from_ymd_opt(2011, 7, 31).unwrap())
        );
        assert_eq!(disposal.allowable_cost_gbp, dec!(12000));

        // Pool should remain as the original holding (B&B acquisition not added).
        let pool = report.pools.get("X").unwrap();
        assert_eq!(pool.quantity, dec!(1000));
        assert_eq!(pool.cost_gbp, dec!(10000));
    }

    #[test]
    fn hmrc_bnb_example_2() {
        // HMRC example 2: https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
        // Section 104 holding 2,500 shares. Dispose 1,700, then buy 500 within 30 days.
        let events = vec![
            acq("2012-01-01", "Y", dec!(2500), dec!(2500)),
            disp("2012-03-27", "Y", dec!(1700), dec!(1700)),
            acq("2012-03-30", "Y", dec!(500), dec!(1000)),
        ];

        let report = calculate_cgt(events).unwrap();
        let disposal = &report.disposals[0];

        assert_eq!(disposal.matching_components.len(), 2);
        let bnb = disposal
            .matching_components
            .iter()
            .find(|c| c.rule == MatchingRule::BedAndBreakfast)
            .expect("expected B&B match");
        assert_eq!(bnb.quantity, dec!(500));
        assert_eq!(
            bnb.matched_date,
            Some(chrono::NaiveDate::from_ymd_opt(2012, 3, 30).unwrap())
        );
        assert!(disposal
            .matching_components
            .iter()
            .any(|c| c.rule == MatchingRule::Pool && c.quantity == dec!(1200)));
        assert_eq!(disposal.allowable_cost_gbp, dec!(2200));

        let pool = report.pools.get("Y").unwrap();
        assert_eq!(pool.quantity, dec!(1300));
        assert_eq!(pool.cost_gbp, dec!(1300));
    }

    #[test]
    fn hmrc_bnb_example_3() {
        // HMRC example 3: https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
        // Disposal on 28 Feb, acquisition on 31 Mar (outside 30 days).
        let events = vec![
            acq("2008-01-01", "Z", dec!(10000), dec!(10000)),
            disp("2009-02-28", "Z", dec!(2000), dec!(2000)),
            acq("2009-03-31", "Z", dec!(3000), dec!(6000)),
        ];

        let report = calculate_cgt(events).unwrap();
        let disposal = &report.disposals[0];

        assert_eq!(disposal.matching_components.len(), 1);
        assert_eq!(disposal.matching_components[0].rule, MatchingRule::Pool);
        assert_eq!(disposal.matching_components[0].quantity, dec!(2000));
        assert_eq!(disposal.matching_components[0].matched_date, None);
        assert_eq!(disposal.allowable_cost_gbp, dec!(2000));

        // Pool after later acquisition should include remaining + new shares.
        let pool = report.pools.get("Z").unwrap();
        assert_eq!(pool.quantity, dec!(11000));
        assert_eq!(pool.cost_gbp, dec!(14000));
    }

    #[test]
    fn same_day_rule() {
        // Buy and sell on same day - should match same-day acquisition
        let events = vec![
            acq("2024-01-15", "BTC", dec!(1), dec!(40000)),
            disp("2024-01-15", "BTC", dec!(1), dec!(45000)),
        ];

        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should use same-day cost of £40,000
        assert_eq!(disposal.allowable_cost_gbp, dec!(40000));
        assert_eq!(disposal.gain_gbp, dec!(5000));

        // Should be pure same-day match
        assert_eq!(disposal.matching_components.len(), 1);
        assert_eq!(disposal.matching_components[0].rule, MatchingRule::SameDay);
    }

    #[test]
    fn same_day_rule_partial() {
        // Buy 2 BTC, sell 1 BTC on same day
        let events = vec![
            acq("2024-01-15", "BTC", dec!(2), dec!(80000)),
            disp("2024-01-15", "BTC", dec!(1), dec!(45000)),
        ];

        let report = calculate_cgt(events).unwrap();

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

        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should match with B&B acquisition at £60,000
        assert_eq!(disposal.allowable_cost_gbp, dec!(60000));
        assert_eq!(disposal.gain_gbp, dec!(15000));

        // Should be pure B&B match
        assert_eq!(disposal.matching_components.len(), 1);
        assert_eq!(
            disposal.matching_components[0].rule,
            MatchingRule::BedAndBreakfast
        );

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

        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // 3 BTC from B&B at £36,000
        // 2 BTC from pool at £20,000 (2/10 * £100,000)
        // Total allowable cost: £56,000
        assert_eq!(disposal.allowable_cost_gbp, dec!(56000));
        assert_eq!(disposal.gain_gbp, dec!(19000));

        // Should be mixed: B&B + Pool
        assert_eq!(disposal.matching_components.len(), 2);
    }

    #[test]
    fn bed_and_breakfast_outside_30_days() {
        // Buy back after 30 days - should use pool instead
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
            acq("2024-07-16", "BTC", dec!(5), dec!(60000)), // 31 days later
        ];

        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should use pool cost: 5/10 * £100,000 = £50,000
        assert_eq!(disposal.allowable_cost_gbp, dec!(50000));
        assert_eq!(disposal.gain_gbp, dec!(25000));

        // Should be pure pool match
        assert_eq!(disposal.matching_components.len(), 1);
        assert_eq!(disposal.matching_components[0].rule, MatchingRule::Pool);
    }

    #[test]
    fn same_day_takes_priority_over_bed_and_breakfast() {
        // Same-day rule should apply before B&B rule
        let events = vec![
            acq("2024-06-15", "BTC", dec!(3), dec!(45000)), // Same day
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
            acq("2024-06-20", "BTC", dec!(5), dec!(60000)), // B&B
        ];

        let report = calculate_cgt(events).unwrap();

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

        let report = calculate_cgt(events).unwrap();

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

        let report = calculate_cgt(events).unwrap();

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

        let report = calculate_cgt(events).unwrap();

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

        let report = calculate_cgt(events).unwrap();

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

        let report = calculate_cgt(events).unwrap();

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

        let report = calculate_cgt(events).unwrap();

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

        let report = calculate_cgt(events).unwrap();

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

        let report = calculate_cgt(events).unwrap();
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

        let report = calculate_cgt(events).unwrap();
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

        let report = calculate_cgt(events).unwrap();
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

        let report = calculate_cgt(events).unwrap();
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
            disp("2024-03-08", "DOT", dec!(10), dec!(85)), // Disposal
            staking("2024-03-15", "DOT", dec!(100), dec!(800)), // Staking reward within 30 days
        ];

        let report = calculate_cgt(events).unwrap();
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

        let report = calculate_cgt(events).unwrap();
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
    fn same_day_has_priority_over_bnb() {
        // Scenario: Same-day rule should have priority over B&B
        // - April 8: Disposal of 100 BTC (will try to B&B with April 11 acquisition)
        // - April 11: Acquisition of 80 BTC at £40000
        // - April 11: Disposal of 50 BTC at £30000
        //
        // Expected: April 11 disposal should get same-day match FIRST (50 BTC at £25000 cost)
        // Then April 8 disposal can B&B with remaining 30 BTC from April 11
        //
        // Bug: Without the fix, April 8 disposal consumes all 80 BTC via B&B,
        // leaving nothing for April 11's same-day match.

        // Need some initial pool for the April 8 disposal that can't fully B&B match
        let events = vec![
            // Initial acquisition to seed the pool
            acq("2024-01-01", "BTC", dec!(100), dec!(50000)), // 100 BTC at £500 each
            // April 8: Disposal - should use B&B with leftover from April 11, plus pool
            disp("2024-04-08", "BTC", dec!(100), dec!(60000)), // Sell 100 BTC at £600 each
            // April 11: Acquisition - should be reserved for same-day first
            acq("2024-04-11", "BTC", dec!(80), dec!(40000)), // 80 BTC at £500 each
            // April 11: Disposal - MUST get same-day match with April 11 acquisition
            disp("2024-04-11", "BTC", dec!(50), dec!(30000)), // Sell 50 BTC at £600 each
        ];

        let report = calculate_cgt(events).unwrap();
        assert_eq!(report.disposals.len(), 2);

        // Find the April 11 disposal
        let apr11_disposal = report
            .disposals
            .iter()
            .find(|d| d.date == NaiveDate::from_ymd_opt(2024, 4, 11).unwrap())
            .unwrap();

        // The April 11 disposal should use same-day matching
        // 50 BTC at £500 each = £25000 cost
        assert_eq!(
            apr11_disposal.allowable_cost_gbp,
            dec!(25000),
            "April 11 disposal should use same-day matching at £500/BTC"
        );

        // Check matching components - should be Same-Day
        assert!(
            apr11_disposal
                .matching_components
                .iter()
                .any(|mc| mc.rule == MatchingRule::SameDay),
            "April 11 disposal should have Same-Day matching component"
        );

        // Find the April 8 disposal
        let apr8_disposal = report
            .disposals
            .iter()
            .find(|d| d.date == NaiveDate::from_ymd_opt(2024, 4, 8).unwrap())
            .unwrap();

        // April 8 disposal (100 BTC) should:
        // - B&B match with remaining 30 BTC from April 11 (80 - 50 used for same-day) at £500 each = £15000
        // - Pool match with 70 BTC from Jan 1 at £500 each = £35000
        // Total cost: £50000
        assert_eq!(
            apr8_disposal.allowable_cost_gbp,
            dec!(50000),
            "April 8 disposal should use B&B (30 BTC) + Pool (70 BTC)"
        );

        // Check that April 8 has B&B component
        assert!(
            apr8_disposal
                .matching_components
                .iter()
                .any(|mc| mc.rule == MatchingRule::BedAndBreakfast),
            "April 8 disposal should have B&B matching component"
        );
    }

    // Tests for disposal warnings

    #[test]
    fn warning_no_cost_basis() {
        // Disposal with no prior acquisitions should have InsufficientCostBasis warning with available=0
        let events = vec![disp("2024-06-15", "BTC", dec!(5), dec!(75000))];

        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should have zero allowable cost
        assert_eq!(disposal.allowable_cost_gbp, dec!(0));

        // Should have InsufficientCostBasis warning with available=0
        let warning = disposal
            .warnings
            .iter()
            .find(|w| matches!(w, DisposalWarning::InsufficientCostBasis { .. }));
        assert!(
            warning.is_some(),
            "Expected InsufficientCostBasis warning, got: {:?}",
            disposal.warnings
        );

        if let Some(DisposalWarning::InsufficientCostBasis {
            available,
            required,
        }) = warning
        {
            assert_eq!(*available, dec!(0));
            assert_eq!(*required, dec!(5));
        }
    }

    #[test]
    fn warning_insufficient_pool() {
        // Disposal exceeding pool quantity should have InsufficientCostBasis warning
        let events = vec![
            acq("2024-01-01", "BTC", dec!(5), dec!(50000)),
            disp("2024-06-15", "BTC", dec!(10), dec!(150000)),
        ];

        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should have InsufficientCostBasis warning
        let has_insufficient = disposal
            .warnings
            .iter()
            .any(|w| matches!(w, DisposalWarning::InsufficientCostBasis { .. }));
        assert!(
            has_insufficient,
            "Expected InsufficientCostBasis warning, got: {:?}",
            disposal.warnings
        );

        // Check the values in the warning
        if let Some(DisposalWarning::InsufficientCostBasis {
            available,
            required,
        }) = disposal
            .warnings
            .iter()
            .find(|w| matches!(w, DisposalWarning::InsufficientCostBasis { .. }))
        {
            assert_eq!(*available, dec!(5));
            assert_eq!(*required, dec!(10));
        }
    }

    #[test]
    fn warning_unclassified_out() {
        // Unclassified event should have Unclassified warning
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            event(
                EventType::Disposal,
                Label::Unclassified,
                "2024-06-15",
                "BTC",
                dec!(5),
                dec!(75000),
                None,
            ),
        ];

        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should have Unclassified warning
        assert!(
            disposal.warnings.contains(&DisposalWarning::Unclassified),
            "Expected Unclassified warning, got: {:?}",
            disposal.warnings
        );

        // Should also be detected by is_unclassified helper
        assert!(disposal.is_unclassified());
    }

    #[test]
    fn warning_count_methods() {
        // Test the warning count methods on CgtReport
        let events = vec![
            acq("2024-01-01", "BTC", dec!(5), dec!(50000)),
            disp("2024-06-15", "BTC", dec!(10), dec!(150000)), // Insufficient pool
            event(
                EventType::Disposal,
                Label::Unclassified,
                "2024-06-16",
                "ETH",
                dec!(10),
                dec!(20000),
                None,
            ), // Unclassified + InsufficientCostBasis
        ];

        let report = calculate_cgt(events).unwrap();

        // Should have 2 disposals with warnings
        assert_eq!(report.warning_count(None), 2);

        // Should have 1 unclassified
        assert_eq!(report.unclassified_count(None), 1);

        // Should have 2 with cost basis warnings
        assert_eq!(report.cost_basis_warning_count(None), 2);
    }

    #[test]
    fn no_warning_for_normal_disposal() {
        // Normal disposal with sufficient pool should have no warnings
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
        ];

        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // Should have no warnings
        assert!(
            disposal.warnings.is_empty(),
            "Expected no warnings, got: {:?}",
            disposal.warnings
        );
        assert!(!disposal.has_warnings());
    }

    #[test]
    fn year_end_snapshots_omit_zero_balance() {
        let events = vec![
            acq("2024-01-15", "BTC", dec!(5), dec!(50000)),
            disp("2024-06-15", "BTC", dec!(5), dec!(75000)), // Dispose all
        ];
        let report = calculate_cgt(events).unwrap();

        // Final snapshot should have no pools (BTC is zero)
        let final_snapshot = report.pool_history.year_end_snapshots.last().unwrap();
        assert!(
            final_snapshot.pools.is_empty(),
            "Expected no pools after disposing all, got: {:?}",
            final_snapshot.pools
        );
    }

    #[test]
    fn pool_history_multiple_assets() {
        let events = vec![
            acq("2024-01-15", "BTC", dec!(10), dec!(100000)),
            acq("2024-01-20", "ETH", dec!(50), dec!(25000)),
            disp("2024-06-15", "BTC", dec!(3), dec!(45000)),
        ];
        let report = calculate_cgt(events).unwrap();

        // Should have 3 entries (2 acquisitions + 1 disposal)
        assert_eq!(report.pool_history.entries.len(), 3);

        // Final snapshot should have both assets
        let final_snapshot = report.pool_history.year_end_snapshots.last().unwrap();
        assert_eq!(final_snapshot.pools.len(), 2);

        // Verify BTC state after disposal (10 - 3 = 7)
        let btc_pool = final_snapshot
            .pools
            .iter()
            .find(|p| p.asset == "BTC")
            .unwrap();
        assert_eq!(btc_pool.quantity, dec!(7));

        // Verify ETH state unchanged
        let eth_pool = final_snapshot
            .pools
            .iter()
            .find(|p| p.asset == "ETH")
            .unwrap();
        assert_eq!(eth_pool.quantity, dec!(50));
    }

    #[test]
    fn id_propagates_to_disposal_record() {
        // Create events with explicit ids
        let events = vec![
            TaxableEvent {
                id: Some("acq-001".to_string()),
                datetime: dt("2024-01-01"),
                event_type: EventType::Acquisition,
                label: Label::Trade,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(10),
                value_gbp: dec!(100000),
                fee_gbp: None,
                description: None,
            },
            TaxableEvent {
                id: Some("disp-001".to_string()),
                datetime: dt("2024-06-15"),
                event_type: EventType::Disposal,
                label: Label::Trade,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(5),
                value_gbp: dec!(75000),
                fee_gbp: None,
                description: None,
            },
        ];

        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.disposals.len(), 1);
        let disposal = &report.disposals[0];

        // The disposal record should have the id from the source event
        assert_eq!(disposal.id, Some("disp-001".to_string()));
    }

    #[test]
    fn pool_history_tracks_acquisitions() {
        let events = vec![
            acq("2024-01-15", "BTC", dec!(5), dec!(50000)),
            acq("2024-03-20", "ETH", dec!(10), dec!(5000)),
        ];
        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.pool_history.entries.len(), 2);
        assert_eq!(report.pool_history.entries[0].asset, "BTC");
        assert_eq!(
            report.pool_history.entries[0].event_type,
            EventType::Acquisition
        );
        assert_eq!(report.pool_history.entries[0].quantity, dec!(5));
    }

    #[test]
    fn pool_history_tracks_disposals() {
        let events = vec![
            acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
            disp("2024-06-15", "BTC", dec!(3), dec!(45000)),
        ];
        let report = calculate_cgt(events).unwrap();

        let btc_entries: Vec<_> = report
            .pool_history
            .entries
            .iter()
            .filter(|e| e.asset == "BTC")
            .collect();
        assert_eq!(btc_entries.len(), 2);
        assert_eq!(btc_entries[1].quantity, dec!(7));
        assert_eq!(btc_entries[1].event_type, EventType::Disposal);
    }

    #[test]
    fn year_end_snapshots_at_boundaries() {
        let events = vec![
            acq("2024-01-15", "BTC", dec!(10), dec!(100000)), // 2023/24
            disp("2024-04-10", "BTC", dec!(3), dec!(45000)),  // 2024/25
        ];
        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.pool_history.year_end_snapshots.len(), 2);

        let snapshot_2324 = &report.pool_history.year_end_snapshots[0];
        assert_eq!(snapshot_2324.tax_year, TaxYear(2024));
        assert_eq!(snapshot_2324.pools.len(), 1);
        assert_eq!(snapshot_2324.pools[0].quantity, dec!(10));

        let snapshot_2425 = &report.pool_history.year_end_snapshots[1];
        assert_eq!(snapshot_2425.tax_year, TaxYear(2025));
        assert_eq!(snapshot_2425.pools[0].quantity, dec!(7));
    }

    // Edge case tests for pool history

    #[test]
    fn pool_history_empty_events() {
        let events: Vec<TaxableEvent> = vec![];
        let report = calculate_cgt(events).unwrap();

        assert!(report.pool_history.entries.is_empty());
        assert!(report.pool_history.year_end_snapshots.is_empty());
        assert!(report.disposals.is_empty());
    }

    #[test]
    fn pool_history_single_tax_year() {
        // All events in same tax year (2024/25: April 6, 2024 - April 5, 2025)
        let events = vec![
            acq("2024-04-10", "BTC", dec!(10), dec!(100000)),
            acq("2024-06-15", "BTC", dec!(5), dec!(60000)),
            disp("2024-12-01", "BTC", dec!(3), dec!(45000)),
        ];
        let report = calculate_cgt(events).unwrap();

        // Should have only 1 year-end snapshot
        assert_eq!(report.pool_history.year_end_snapshots.len(), 1);
        assert_eq!(
            report.pool_history.year_end_snapshots[0].tax_year,
            TaxYear(2025)
        );

        // Should have 3 daily entries
        assert_eq!(report.pool_history.entries.len(), 3);
    }

    #[test]
    fn pool_history_old_events() {
        // Test events from before 2020
        let events = vec![
            acq("2017-01-15", "BTC", dec!(100), dec!(1000)), // Very old
            acq("2018-06-20", "BTC", dec!(50), dec!(200000)), // 2018/19
            disp("2019-01-10", "BTC", dec!(30), dec!(150000)), // 2018/19
            disp("2024-06-15", "BTC", dec!(50), dec!(500000)), // 2024/25
        ];
        let report = calculate_cgt(events).unwrap();

        // Should have snapshots spanning multiple years
        assert!(report.pool_history.year_end_snapshots.len() >= 2);

        // First snapshot should be from 2016/17
        assert_eq!(
            report.pool_history.year_end_snapshots[0].tax_year,
            TaxYear(2017)
        );
    }

    #[test]
    fn pool_history_staking_rewards_tracked() {
        // Staking rewards should appear in pool history
        let events = vec![
            staking("2024-01-15", "DOT", dec!(100), dec!(500)),
            staking("2024-02-15", "DOT", dec!(50), dec!(280)),
        ];
        let report = calculate_cgt(events).unwrap();

        assert_eq!(report.pool_history.entries.len(), 2);
        assert_eq!(
            report.pool_history.entries[0].event_type,
            EventType::Acquisition
        );
        assert_eq!(report.pool_history.entries[0].label, Label::StakingReward);
        assert_eq!(report.pool_history.entries[1].quantity, dec!(150)); // Accumulated
    }
}
