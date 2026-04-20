use super::events::{EventType, Tag, TaxableEvent};
use super::uk::TaxYear;
use super::warnings::Warning;
use chrono::{DateTime, Duration, FixedOffset, NaiveDate};
use rust_decimal::Decimal;
use serde::{Serialize, Serializer};
use std::collections::{HashMap, VecDeque};

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

/// Snapshot of pool state at a point in time (test-only, surfaced via `pool_after`)
#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct PoolSnapshot {
    pub quantity: Decimal,
    pub cost_gbp: Decimal,
}

#[cfg(test)]
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
    pub tag: Tag,
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
    /// Event identifier from source data
    pub id: usize,
    pub datetime: DateTime<FixedOffset>,
    pub date: NaiveDate,
    #[allow(dead_code)]
    pub tax_year: TaxYear,
    pub asset: String,
    pub quantity: Decimal,
    pub proceeds_gbp: Decimal,
    pub allowable_cost_gbp: Decimal,
    pub fees_gbp: Decimal,
    pub gain_gbp: Decimal,
    /// Pool state after this disposal (test-only)
    #[cfg(test)]
    pub pool_after: PoolSnapshot,
    /// Breakdown by matching rule for detailed reporting
    pub matching_components: Vec<MatchingComponent>,
    /// Warnings for this disposal (unclassified, no cost basis, insufficient pool, etc.)
    pub warnings: Vec<Warning>,
}

impl DisposalRecord {
    /// Check if this disposal came from an unclassified event
    pub fn is_unclassified(&self) -> bool {
        self.warnings.contains(&Warning::UnclassifiedEvent)
    }
}

/// CGT report containing all disposals
#[derive(Debug)]
pub struct CgtReport {
    pub disposals: Vec<DisposalRecord>,
    /// Final pool states (test-only)
    #[cfg(test)]
    pub pools: HashMap<String, Pool>,
    pub pool_history: PoolHistory,
}

impl CgtReport {
    /// Total proceeds for a tax year (classified events only)
    #[allow(dead_code)]
    pub fn total_proceeds(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, true)
            .map(|d| d.proceeds_gbp)
            .sum()
    }

    /// Total proceeds including unclassified events
    #[allow(dead_code)]
    pub fn total_proceeds_with_unclassified(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, false)
            .map(|d| d.proceeds_gbp)
            .sum()
    }

    /// Total allowable costs for a tax year (classified events only)
    #[allow(dead_code)]
    pub fn total_allowable_costs(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, true)
            .map(|d| d.allowable_cost_gbp + d.fees_gbp)
            .sum()
    }

    /// Total allowable costs including unclassified events
    #[allow(dead_code)]
    pub fn total_allowable_costs_with_unclassified(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, false)
            .map(|d| d.allowable_cost_gbp + d.fees_gbp)
            .sum()
    }

    /// Total gain/loss for a tax year (classified events only)
    #[allow(dead_code)]
    pub fn total_gain(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, true).map(|d| d.gain_gbp).sum()
    }

    /// Total gain/loss including unclassified events
    #[allow(dead_code)]
    pub fn total_gain_with_unclassified(&self, year: Option<TaxYear>) -> Decimal {
        self.filter_disposals(year, false).map(|d| d.gain_gbp).sum()
    }

    #[cfg(test)]
    pub fn disposal_count(&self, year: Option<TaxYear>) -> usize {
        self.filter_disposals(year, false).count()
    }

    #[allow(dead_code)]
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

                // No gain/no loss: deemed proceeds = allowable cost + fees
                let (proceeds, gain) = if event.tag == Tag::NoGainNoLoss {
                    (total_allowable_cost + fees, Decimal::ZERO)
                } else {
                    (
                        event.value_gbp,
                        event.value_gbp - total_allowable_cost - fees,
                    )
                };

                // Capture pool state after disposal (test-only)
                #[cfg(test)]
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
                if event.tag == Tag::Unclassified {
                    warnings.push(Warning::UnclassifiedEvent);
                }
                // Insufficient cost basis: pool didn't have enough to cover the disposal
                // This includes the "no cost basis" case when available = 0
                if insufficient_pool {
                    warnings.push(Warning::InsufficientCostBasis {
                        available: pool_qty_before,
                        required: remaining_to_match,
                    });
                }

                disposals.push(DisposalRecord {
                    id: event.id,
                    datetime: event.datetime,
                    date: event.date(),
                    tax_year,
                    asset: event.asset.clone(),
                    quantity: event.quantity,
                    proceeds_gbp: proceeds,
                    allowable_cost_gbp: total_allowable_cost,
                    fees_gbp: fees,
                    gain_gbp: gain,
                    #[cfg(test)]
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
                tag: event.tag,
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
        #[cfg(test)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DisposalKey {
    date: NaiveDate,
    datetime: String,
    asset: String,
    quantity: String,
}

impl DisposalKey {
    fn from_disposal(disposal: &DisposalRecord) -> Self {
        DisposalKey {
            date: disposal.date,
            datetime: disposal.datetime.to_rfc3339(),
            asset: disposal.asset.clone(),
            quantity: format_decimal_key(disposal.quantity, 8),
        }
    }

    fn from_event(event: &TaxableEvent) -> Self {
        DisposalKey {
            date: event.date(),
            datetime: event.datetime.to_rfc3339(),
            asset: event.asset.clone(),
            quantity: format_decimal_key(event.quantity, 8),
        }
    }
}

fn format_decimal_key(value: Decimal, dp: u32) -> String {
    value.round_dp(dp).normalize().to_string()
}

pub struct DisposalIndex<'a> {
    report: &'a CgtReport,
    by_id: HashMap<usize, usize>,
    by_key: HashMap<DisposalKey, VecDeque<usize>>,
}

impl<'a> DisposalIndex<'a> {
    pub fn new(report: &'a CgtReport) -> Self {
        let mut by_id = HashMap::new();
        let mut by_key: HashMap<DisposalKey, VecDeque<usize>> = HashMap::new();
        for (idx, d) in report.disposals.iter().enumerate() {
            by_id.insert(d.id, idx);
            let key = DisposalKey::from_disposal(d);
            by_key.entry(key).or_default().push_back(idx);
        }

        DisposalIndex {
            report,
            by_id,
            by_key,
        }
    }

    pub fn find(&mut self, event: &TaxableEvent) -> Option<&'a DisposalRecord> {
        if let Some(&idx) = self.by_id.get(&event.id) {
            return self.report.disposals.get(idx);
        }

        let key = DisposalKey::from_event(event);
        self.by_key
            .get_mut(&key)
            .and_then(|queue| queue.pop_front())
            .and_then(|idx| self.report.disposals.get(idx))
    }
}

#[cfg(test)]
mod tests;
