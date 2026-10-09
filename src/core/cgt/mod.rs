use super::events::{AdjustmentKind, EventType, Tag, TaxableEvent};
use super::fmt::round_pence;
use super::fmt::{iso_date, pence_string, quantity_string};
use super::uk::TaxYear;
use super::warnings::Warning;
use chrono::{DateTime, Duration, FixedOffset, NaiveDate};
use rust_decimal::Decimal;
use serde::{Serialize, Serializer};
use std::collections::{BTreeMap, HashMap};

fn serialize_date<S: Serializer>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&iso_date(*date))
}

fn serialize_quantity<S: Serializer>(qty: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&quantity_string(*qty))
}

fn serialize_decimal_2dp<S: Serializer>(d: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&pence_string(*d))
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
            let cost = round_pence(self.cost_gbp * proportion);
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
}

/// Record of a disposal for CGT purposes
#[derive(Debug, Clone)]
pub struct DisposalRecord {
    /// Event identifier from source data
    pub id: usize,
    pub datetime: DateTime<FixedOffset>,
    pub date: NaiveDate,
    pub asset: String,
    pub quantity: Decimal,
    pub proceeds_gbp: Decimal,
    pub allowable_cost_gbp: Decimal,
    pub fees_gbp: Decimal,
    pub gain_gbp: Decimal,
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

/// Proceeds, allowable costs (including disposal fees) and gain summed over
/// a set of disposals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisposalTotals {
    pub proceeds: Decimal,
    pub costs: Decimal,
    pub gain: Decimal,
}

impl<'a> FromIterator<&'a DisposalRecord> for DisposalTotals {
    fn from_iter<I: IntoIterator<Item = &'a DisposalRecord>>(disposals: I) -> Self {
        disposals
            .into_iter()
            .fold(Self::default(), |t, d| DisposalTotals {
                proceeds: t.proceeds + d.proceeds_gbp,
                costs: t.costs + d.allowable_cost_gbp + d.fees_gbp,
                gain: t.gain + d.gain_gbp,
            })
    }
}

/// CGT report containing all disposals
#[derive(Debug)]
pub struct CgtReport {
    /// Disposals, plus the gain on any small capital distribution that
    /// exceeded its pool's cost (keyed by the distribution's event id).
    pub disposals: Vec<DisposalRecord>,
    pub pool_history: PoolHistory,
    /// Warnings raised applying pool adjustments, by event id.
    pub adjustment_warnings: BTreeMap<usize, Vec<Warning>>,
}

impl CgtReport {
    /// Warnings raised applying a pool-adjustment event; empty for any other
    /// event.
    pub fn warnings_for_adjustment(&self, event: &TaxableEvent) -> &[Warning] {
        if !matches!(event.event_type, EventType::PoolAdjustment(_)) {
            return &[];
        }
        self.adjustment_warnings
            .get(&event.id)
            .map_or(&[], Vec::as_slice)
    }
}

/// Tracks acquisition quantities available for matching
#[derive(Debug, Default)]
struct AcquisitionTracker {
    total_qty: Decimal,
    total_cost: Decimal,
    same_day_remaining: Decimal,
    bnb_remaining: Decimal,
    /// Whether the day's unmatched remainder has been added to the pool.
    pooled: bool,
}

impl AcquisitionTracker {
    fn cost_for_qty(&self, qty: Decimal) -> Decimal {
        if self.total_qty.is_zero() {
            Decimal::ZERO
        } else {
            round_pence(self.total_cost * qty / self.total_qty)
        }
    }

    /// What neither a same-day nor a B&B disposal claimed.
    fn remaining_for_pool(&self) -> Decimal {
        self.same_day_remaining + self.bnb_remaining
    }
}

type AcqKey = (NaiveDate, String);

/// Calculate CGT from taxable events
/// Implements HMRC share identification rules:
/// 1. Same-day rule: Match with acquisitions on the same day
/// 2. Bed & breakfast rule: Match with acquisitions within 30 days after disposal
/// 3. Section 104 pool: Match with pooled cost basis
pub fn calculate_cgt(events: Vec<TaxableEvent>) -> CgtReport {
    let mut pools: HashMap<String, Pool> = HashMap::new();
    let mut disposals: Vec<DisposalRecord> = Vec::new();
    let mut adjustment_warnings: BTreeMap<usize, Vec<Warning>> = BTreeMap::new();
    let mut pool_history = PoolHistory::default();
    let mut current_year: Option<TaxYear> = None;

    // Sterling is not a chargeable asset, so GBP income never enters a pool.
    let mut events: Vec<TaxableEvent> = events
        .into_iter()
        .filter(|e| !e.asset.eq_ignore_ascii_case("GBP"))
        .collect();
    // By UK date: pool adjustments, then disposals, then acquisitions (stable).
    // Adjustments apply in time order, ties broken by kind (rights issue,
    // demerger, small capital distribution), so a same-day disposal sees the
    // reorganised pools.
    events.sort_by_key(|e| {
        let (rank, adjustment) = match e.event_type {
            EventType::PoolAdjustment(kind) => (0, Some((e.datetime, kind))),
            EventType::Disposal => (1, None),
            EventType::Acquisition => (2, None),
        };
        (e.date(), rank, adjustment)
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
                let available = tracker.total_qty - tracker.same_day_remaining;
                if available > Decimal::ZERO {
                    tracker.same_day_remaining += event.quantity.min(available);
                }
            }
        }
    }
    // Whatever same-day disposals did not reserve is open to B&B matching.
    for tracker in acquisitions.values_mut() {
        tracker.bnb_remaining = tracker.total_qty - tracker.same_day_remaining;
    }

    // Third pass: process all events
    for event in &events {
        let event_year = TaxYear::from_date(event.date());

        // Snapshot every year-end passed since the last event, including
        // idle years, which carry their holdings forward unchanged.
        if let Some(prev_year) = current_year {
            for year in prev_year.0..event_year.0 {
                pool_history
                    .year_end_snapshots
                    .push(snapshot_pools(TaxYear(year), &pools));
            }
        }
        current_year = Some(event_year);

        match event.event_type {
            // Acquisition events add to the pool (after matching)
            // Same-day acquisitions are one acquisition (TCGA 1992 s105), so the
            // day's unmatched remainder is pooled once, exactly -- splitting it
            // per event and rounding each share left dust or shortfalls.
            // Every same-day and earlier B&B claim is settled by now.
            EventType::Acquisition => {
                let key = (event.date(), event.asset.clone());
                if let Some(tracker) = acquisitions.get_mut(&key).filter(|t| !t.pooled) {
                    tracker.pooled = true;
                    let remaining = tracker.remaining_for_pool();
                    if remaining > Decimal::ZERO {
                        let cost = tracker.cost_for_qty(remaining);
                        pool_for(&mut pools, &event.asset).add(remaining, cost);
                    }
                }
            }
            EventType::Disposal => {
                disposals.push(process_disposal(event, &mut acquisitions, &mut pools));
            }
            // Adjustments change the pools directly, bypassing the
            // acquisition tracker, so they are never matched.
            EventType::PoolAdjustment(kind) => {
                let outcome = apply_adjustment(event, kind, &mut pools);
                if !outcome.warnings.is_empty() {
                    adjustment_warnings.insert(event.id, outcome.warnings);
                }
                disposals.extend(outcome.excess);
            }
        }

        // Record pool state after event (for daily history). A demerger
        // changes the original holding's pool as well as its own.
        let demerged_from = event.demerged_from.as_ref().map(|d| d.asset.as_str());
        for asset in demerged_from.into_iter().chain([event.asset.as_str()]) {
            if let Some(pool) = pools.get(asset) {
                pool_history.entries.push(PoolHistoryEntry {
                    date: event.date(),
                    asset: asset.to_string(),
                    event_type: event.event_type,
                    tag: event.tag,
                    quantity: pool.quantity,
                    cost_gbp: pool.cost_gbp,
                });
            }
        }
    }

    // Final snapshot for last tax year
    if let Some(year) = current_year {
        pool_history
            .year_end_snapshots
            .push(snapshot_pools(year, &pools));
    }

    CgtReport {
        disposals,
        pool_history,
        adjustment_warnings,
    }
}

/// Apply the HMRC identification rules (same-day, then B&B, then Section 104
/// pool) to a single disposal, consuming matched acquisition quantities and
/// pool cost, and produce its disposal record.
fn process_disposal(
    event: &TaxableEvent,
    acquisitions: &mut HashMap<AcqKey, AcquisitionTracker>,
    pools: &mut HashMap<String, Pool>,
) -> DisposalRecord {
    let date = event.date();
    let mut remaining = event.quantity;
    let mut components = Vec::new();

    // Identification order (HMRC CG51560): same day, then the next 30 days
    // (bed and breakfast), then the Section 104 pool.
    // https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
    let same_day = std::iter::once((MatchingRule::SameDay, date));
    let bnb = (1..=30).map(|days| (MatchingRule::BedAndBreakfast, date + Duration::days(days)));
    for (rule, acq_date) in same_day.chain(bnb) {
        if remaining <= Decimal::ZERO {
            break;
        }
        let Some(tracker) = acquisitions.get_mut(&(acq_date, event.asset.clone())) else {
            continue;
        };
        let available = match rule {
            MatchingRule::SameDay => &mut tracker.same_day_remaining,
            _ => &mut tracker.bnb_remaining,
        };
        let quantity = remaining.min(*available);
        if quantity <= Decimal::ZERO {
            continue;
        }
        *available -= quantity;
        remaining -= quantity;
        let cost = tracker.cost_for_qty(quantity);
        log::debug!(
            "{rule} match: {quantity} {} on {acq_date} at cost {cost}",
            event.asset
        );
        components.push(MatchingComponent {
            rule,
            quantity,
            cost,
            matched_date: Some(acq_date),
        });
    }

    let mut warnings = Vec::new();
    if event.tag == Tag::Unclassified {
        warnings.push(Warning::UnclassifiedEvent);
    }
    if remaining > Decimal::ZERO {
        let pool = pool_for(pools, &event.asset);
        // Short of the pool (including an empty one: no cost basis at all).
        if remaining > pool.quantity {
            warnings.push(Warning::InsufficientCostBasis {
                available: pool.quantity,
                required: remaining,
            });
        }
        let cost = pool.remove(remaining);
        log::debug!("Pool match: {remaining} {} at cost {cost}", event.asset);
        components.push(MatchingComponent {
            rule: MatchingRule::Pool,
            quantity: remaining,
            cost,
            matched_date: None,
        });
    }

    let allowable_cost: Decimal = components.iter().map(|c| c.cost).sum();
    let fees = event.fee_gbp.unwrap_or(Decimal::ZERO);
    // No gain/no loss: deemed proceeds = allowable cost + fees.
    let (proceeds, gain) = if event.tag == Tag::NoGainNoLoss {
        (allowable_cost + fees, Decimal::ZERO)
    } else {
        (event.value_gbp, event.value_gbp - allowable_cost - fees)
    };

    DisposalRecord {
        id: event.id,
        datetime: event.datetime,
        date,
        asset: event.asset.clone(),
        quantity: event.quantity,
        proceeds_gbp: proceeds,
        allowable_cost_gbp: allowable_cost,
        fees_gbp: fees,
        gain_gbp: gain,
        matching_components: components,
        warnings,
    }
}

fn pool_for<'a>(pools: &'a mut HashMap<String, Pool>, asset: &str) -> &'a mut Pool {
    pools
        .entry(asset.to_string())
        .or_insert_with(|| Pool::new(asset.to_string()))
}

/// What applying one pool adjustment produced besides the pool changes.
struct AdjustmentOutcome {
    warnings: Vec<Warning>,
    /// The chargeable excess of a small capital distribution over its
    /// pool's cost.
    excess: Option<DisposalRecord>,
}

/// Apply a share reorganisation to the pools as one step, with no disposal.
fn apply_adjustment(
    event: &TaxableEvent,
    kind: AdjustmentKind,
    pools: &mut HashMap<String, Pool>,
) -> AdjustmentOutcome {
    // A demerger or distribution with nothing in its pool has no cost basis.
    let no_pool_warning = || Warning::InsufficientCostBasis {
        available: Decimal::ZERO,
        required: Decimal::ZERO,
    };
    let mut warnings = Vec::new();
    let mut excess = None;

    match kind {
        // TCGA 1992 s.127/s.128: the rights shares and their cost join the
        // original holding.
        AdjustmentKind::RightsIssue => {
            pool_for(pools, &event.asset).add(event.quantity, event.total_cost_gbp());
        }
        // s.130: the stated fraction of the original cost moves to the new
        // holding.
        AdjustmentKind::Demerger => {
            let from = event
                .demerged_from
                .as_ref()
                .expect("demerger events carry their original holding");
            let original = pool_for(pools, &from.asset);
            if original.quantity.is_zero() {
                warnings.push(no_pool_warning());
            }
            let moved = round_pence(original.cost_gbp * from.cost_fraction);
            original.cost_gbp -= moved;
            pool_for(pools, &event.asset).add(event.quantity, moved);
        }
        // s.122(2): the distribution reduces allowable cost. Beyond the
        // cost, the excess is a gain (s.122(4) election, HMRC CG57847).
        AdjustmentKind::SmallCapitalDistribution => {
            let pool = pool_for(pools, &event.asset);
            if pool.quantity.is_zero() {
                warnings.push(no_pool_warning());
            }
            let amount = -event.total_cost_gbp();
            let deducted = amount.min(pool.cost_gbp);
            pool.cost_gbp -= deducted;
            let gain = amount - deducted;
            if gain > Decimal::ZERO {
                excess = Some(DisposalRecord {
                    id: event.id,
                    datetime: event.datetime,
                    date: event.date(),
                    asset: event.asset.clone(),
                    quantity: Decimal::ZERO,
                    proceeds_gbp: gain,
                    allowable_cost_gbp: Decimal::ZERO,
                    fees_gbp: Decimal::ZERO,
                    gain_gbp: gain,
                    matching_components: Vec::new(),
                    warnings: vec![Warning::CapitalDistributionExceedsCost],
                });
            }
        }
    }

    AdjustmentOutcome { warnings, excess }
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

/// Disposal records by the id of the event they came from.
///
/// Event ids are unique -- `transactions_to_events` numbers them
/// sequentially -- and every caller looks up events from the same list that
/// went into `calculate_cgt`, so the id is always enough.
pub struct DisposalIndex<'a>(HashMap<usize, &'a DisposalRecord>);

impl<'a> DisposalIndex<'a> {
    pub fn new(report: &'a CgtReport) -> Self {
        DisposalIndex(report.disposals.iter().map(|d| (d.id, d)).collect())
    }

    /// The disposal record for a disposal event, or the excess gain of a
    /// small capital distribution; `None` for acquisitions.
    pub fn find(&self, event: &TaxableEvent) -> Option<&'a DisposalRecord> {
        if event.event_type == EventType::Acquisition {
            return None;
        }
        self.0.get(&event.id).copied()
    }
}

mod summary;
pub use summary::CgtSummary;

#[cfg(test)]
mod tests;
