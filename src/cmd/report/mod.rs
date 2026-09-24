//! Report command and JSON/HTML data model.

pub mod html;

pub const NGNL_VALUE_NOTE: &str = "No gain/no loss transfer: value shows transferred allowable cost basis. CGT proceeds are deemed from cost basis and disposal fees; see disposal details for tax values.";

use super::filter::{EventFilter, FilterArgs};
use super::read_transactions_and_events;
use crate::core::fmt::{iso_date, pence_string, quantity_string};
use crate::core::transactions::{Transaction, TransactionType};
use crate::core::{
    calculate_cgt, display_event_type, event_warnings, AssetClass, CgtReport, DisposalIndex,
    DisposalRecord, EventType, Tag, TaxYear, TaxableEvent, Warning,
};
use crate::core::{uk_date, uk_rfc3339};
use anyhow::Context;
use chrono::NaiveDate;
use clap::Args;
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ReportCommand {
    /// Transactions file (JSON). Reads from stdin if not specified.
    #[arg(default_value = "-")]
    file: PathBuf,

    /// Output file path (default: opens in browser)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Output as JSON instead of HTML
    #[arg(long)]
    json: bool,

    /// Filter by asset (e.g., BTC, ETH)
    #[arg(short, long)]
    asset: Option<String>,

    /// Don't include unlinked deposits/withdrawals in calculations
    #[arg(long)]
    exclude_unlinked: bool,

    #[command(flatten)]
    filter: FilterArgs,
}

impl ReportCommand {
    pub fn exec(&self) -> anyhow::Result<()> {
        let event_filter = self.filter.build(self.asset.clone())?;
        let (transactions, events) =
            read_transactions_and_events(&self.file, self.exclude_unlinked)?;

        let cgt_report = calculate_cgt(events.clone());

        if self.json {
            let data = build_report_data(&[], &events, &cgt_report, &event_filter);
            let json = serde_json::to_string_pretty(&data)?;

            if let Some(ref output_path) = self.output {
                write_output(output_path, &json)?;
                eprintln!("JSON report written to: {}", output_path.display());
            } else {
                println!("{}", json);
            }
        } else {
            let html = html::generate_html(&transactions, &events, &cgt_report, &event_filter)?;

            if let Some(ref output_path) = self.output {
                write_output(output_path, &html)?;
                println!("HTML report written to: {}", output_path.display());
            } else {
                let temp_path = write_private_temp(&html)?;
                opener::open(&temp_path)?;
                println!("Opened HTML report in browser: {}", temp_path.display());
            }
        }

        Ok(())
    }
}

fn write_output(path: &std::path::Path, contents: &str) -> anyhow::Result<()> {
    std::fs::write(path, contents).with_context(|| format!("cannot write {}", path.display()))
}

/// Write the report to a new, uniquely named file only the user can read.
///
/// The report holds personal financial data and the temp directory may be
/// shared (e.g. /tmp on Linux), so a fixed name would be readable by others
/// and could be pre-created as a symlink by another user.
fn write_private_temp(contents: &str) -> anyhow::Result<PathBuf> {
    use std::io::Write;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let path =
        std::env::temp_dir().join(format!("taxc-report-{}-{}.html", std::process::id(), nanos));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(&path)
        .with_context(|| format!("cannot create {}", path.display()))?;
    file.write_all(contents.as_bytes())?;
    Ok(path)
}

/// Data structure for embedding in HTML as JSON
#[derive(Serialize, JsonSchema)]
pub struct ReportData {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub transactions: Vec<TransactionRow>,
    pub events: Vec<EventRow>,
    pub warnings: Vec<WarningRecord>,
    pub summary: Summary,
}

/// A serialized input transaction for the transactions view
#[derive(Serialize, JsonSchema)]
pub struct TransactionRow {
    pub id: String,
    pub datetime: String,
    pub tax_year: String,
    pub account: String,
    pub transaction_type: String,
    pub tag: Tag,
    pub description: String,
    /// Assets involved (e.g. for Trade: sold + bought; for Deposit/Withdrawal: single amount)
    pub amounts: Vec<TransactionAmount>,
    /// Fee if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<TransactionFee>,
    /// Event IDs generated from this transaction
    pub event_ids: Vec<usize>,
}

#[derive(Serialize, JsonSchema)]
pub struct TransactionAmount {
    pub label: String,
    pub asset: String,
    pub quantity: String,
}

#[derive(Serialize, JsonSchema)]
pub struct TransactionFee {
    pub asset: String,
    pub amount: String,
}

#[derive(Serialize, JsonSchema)]
pub struct EventRow {
    /// Sequential event identifier
    pub id: usize,
    /// Source transaction identifier from input
    pub source_transaction_id: String,
    pub account: String,
    pub datetime: String,
    pub tax_year: String,
    pub event_kind: String,
    pub tag: Tag,
    pub event_type: String,
    pub asset: String,
    pub asset_class: String,
    pub quantity: String,
    pub value_gbp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_gbp_note: Option<String>,
    pub fees_gbp: String,
    pub description: String,
    /// Warnings attached to this event.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
    /// CGT details for disposal events (None for other event types)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgt: Option<CgtDetails>,
}

#[derive(Serialize, JsonSchema)]
pub struct WarningRecord {
    pub warning: Warning,
    /// Input transaction IDs related to this warning.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_transaction_ids: Vec<String>,
    /// Output event IDs related to this warning.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_event_ids: Vec<usize>,
}

/// CGT details for disposal events
#[derive(Serialize, JsonSchema)]
pub struct CgtDetails {
    pub proceeds_gbp: String,
    pub cost_gbp: String,
    pub gain_gbp: String,
    pub rule: String,
    pub matching_components: Vec<MatchingComponentRow>,
}

#[derive(Serialize, JsonSchema)]
pub struct MatchingComponentRow {
    pub rule: String,
    pub quantity: String,
    pub cost_gbp: String,
    /// For Same-Day/B&B: the linked acquisition date
    pub matched_date: Option<String>,
    /// Event ID of the matched acquisition (for navigation)
    pub matched_event_id: Option<usize>,
    /// Details of the matched acquisition for display
    pub matched_event_type: Option<String>,
    pub matched_tax_year: Option<String>,
    pub matched_asset: Option<String>,
    pub matched_original_qty: Option<String>,
    pub matched_original_value: Option<String>,
    pub matched_description: Option<String>,
}

/// Aggregated acquisition details for a (date, asset) key
#[derive(Default)]
struct AcquisitionDetail {
    /// The first acquisition's event id, for navigation.
    event_id: usize,
    event_type: String,
    tax_year: String,
    quantity: Decimal,
    value_gbp: Decimal,
    description: String,
}

#[derive(Serialize, JsonSchema)]
pub struct AssetClassTotals {
    pub proceeds: String,
    pub costs: String,
    pub gain: String,
}

#[derive(Serialize, JsonSchema)]
pub struct Summary {
    pub total_proceeds: String,
    pub total_costs: String,
    pub total_gain: String,
    /// Totals including unclassified events (for conservative estimates)
    pub total_proceeds_with_unclassified: String,
    pub total_costs_with_unclassified: String,
    pub total_gain_with_unclassified: String,
    /// Per-asset-class CGT totals (classified only)
    pub crypto: AssetClassTotals,
    pub stocks: AssetClassTotals,
    pub fiat: AssetClassTotals,
    pub total_income: String,
    pub total_dividend_income: String,
    pub total_interest_income: String,
    pub event_count: usize,
    pub disposal_count: usize,
    pub income_count: usize,
    /// Count of events with any warning
    pub warning_count: usize,
    /// Count of unclassified events
    pub unclassified_count: usize,
    /// Count of events with cost basis issues
    pub cost_basis_warning_count: usize,
    pub tax_years: Vec<String>,
    pub assets: Vec<String>,
    pub min_date: Option<String>,
    pub max_date: Option<String>,
}

pub(super) fn build_report_data(
    transactions: &[Transaction],
    events: &[TaxableEvent],
    cgt_report: &CgtReport,
    filter: &EventFilter,
) -> ReportData {
    let filtered_events: Vec<_> = filter.apply(events);
    // Built from every event, so a match to an acquisition the filter hides
    // (e.g. a B&B repurchase in the next tax year) still links to it.
    let acquisitions = acquisition_lookup(events);
    let event_rows = build_event_rows(&filtered_events, cgt_report, &acquisitions);

    ReportData {
        transactions: build_transaction_rows(transactions, events, &filtered_events, filter),
        warnings: group_warnings(&event_rows),
        summary: build_summary(&filtered_events, &event_rows, cgt_report, filter),
        events: event_rows,
    }
}

/// Acquisitions keyed by (date, asset), so a disposal's Same-Day and B&B
/// matches can be linked back to the acquisition row they came from.
/// Multiple acquisitions of one asset on one day are aggregated, and take
/// the first event's id for navigation.
type AcquisitionLookup = HashMap<(NaiveDate, String), AcquisitionDetail>;

fn acquisition_lookup(events: &[TaxableEvent]) -> AcquisitionLookup {
    let mut lookup = AcquisitionLookup::new();
    for e in events
        .iter()
        .filter(|e| e.event_type == EventType::Acquisition)
    {
        let detail = lookup
            .entry((e.date(), e.asset.clone()))
            .or_insert_with(|| AcquisitionDetail {
                event_id: e.id,
                event_type: display_event_type(e.event_type, e.tag).to_string(),
                tax_year: TaxYear::from_date(e.date()).display(),
                description: e.description.clone().unwrap_or_default(),
                quantity: Decimal::ZERO,
                value_gbp: Decimal::ZERO,
            });
        detail.quantity += e.quantity;
        detail.value_gbp += e.value_gbp;
    }
    lookup
}

/// The CGT detail block for one disposal row.
fn cgt_details(d: &DisposalRecord, acquisitions: &AcquisitionLookup) -> CgtDetails {
    let rule = match d.matching_components.as_slice() {
        [only] => only.rule.display().to_string(),
        _ => "Mixed".to_string(),
    };
    let matching_components = d
        .matching_components
        .iter()
        .map(|mc| {
            let acq = mc
                .matched_date
                .and_then(|date| acquisitions.get(&(date, d.asset.clone())));
            MatchingComponentRow {
                rule: mc.rule.display().to_string(),
                quantity: quantity_string(mc.quantity),
                cost_gbp: pence_string(mc.cost),
                matched_date: mc.matched_date.map(iso_date),
                matched_event_id: acq.map(|a| a.event_id),
                matched_event_type: acq.map(|a| a.event_type.clone()),
                matched_tax_year: acq.map(|a| a.tax_year.clone()),
                matched_asset: acq.map(|_| d.asset.clone()),
                matched_original_qty: acq.map(|a| quantity_string(a.quantity)),
                matched_original_value: acq.map(|a| pence_string(a.value_gbp)),
                matched_description: acq.map(|a| a.description.clone()),
            }
        })
        .collect();
    CgtDetails {
        proceeds_gbp: pence_string(d.proceeds_gbp),
        cost_gbp: pence_string(d.allowable_cost_gbp),
        gain_gbp: pence_string(d.gain_gbp),
        rule,
        matching_components,
    }
}

fn build_event_rows(
    filtered_events: &[&TaxableEvent],
    cgt_report: &CgtReport,
    acquisitions: &AcquisitionLookup,
) -> Vec<EventRow> {
    // Build CGT lookup: prefer id, fallback to a composite key
    let mut disposal_index = DisposalIndex::new(cgt_report);

    // Build events list with CGT details for disposals
    filtered_events
        .iter()
        .map(|e| {
            // Look up CGT details for disposal events
            let disposal = if e.event_type == EventType::Disposal {
                disposal_index.find(e)
            } else {
                None
            };
            let event_warnings = event_warnings(e, disposal);

            let cgt = disposal.map(|d| cgt_details(d, acquisitions));

            let fees_gbp = e.fee_gbp.map(pence_string).unwrap_or_default();

            let (value_gbp, value_gbp_note) = if e.tag == Tag::NoGainNoLoss {
                (
                    cgt.as_ref()
                        .map(|details| details.cost_gbp.clone())
                        .unwrap_or_else(|| pence_string(e.value_gbp)),
                    Some(NGNL_VALUE_NOTE.to_string()),
                )
            } else {
                (pence_string(e.value_gbp), None)
            };

            EventRow {
                id: e.id,
                source_transaction_id: e.source_transaction_id.clone(),
                account: e.account.clone(),
                datetime: uk_rfc3339(e.datetime),
                tax_year: TaxYear::from_date(e.date()).display(),
                event_kind: match e.event_type {
                    EventType::Acquisition => "acquisition".to_string(),
                    EventType::Disposal => "disposal".to_string(),
                },
                tag: e.tag,
                event_type: format_event_type(e.event_type, e.tag),
                asset: e.asset.clone(),
                asset_class: format_asset_class(&e.asset_class),
                quantity: quantity_string(e.quantity),
                value_gbp,
                value_gbp_note,
                fees_gbp,
                description: e.description.clone().unwrap_or_default(),
                warnings: event_warnings,
                cgt,
            }
        })
        .collect()
}

fn build_summary(
    filtered_events: &[&TaxableEvent],
    event_rows: &[EventRow],
    cgt_report: &CgtReport,
    filter: &EventFilter,
) -> Summary {
    // Build asset -> asset_class mapping from events
    let asset_class_map: HashMap<String, AssetClass> = filtered_events
        .iter()
        .map(|e| (e.asset.clone(), e.asset_class.clone()))
        .collect();

    // Calculate summary from disposals that match the active filter.
    let filtered_disposals: Vec<_> = cgt_report
        .disposals
        .iter()
        .filter(|d| filter.matches_disposal(d))
        .collect();

    // Classified-only totals
    let classified_disposals: Vec<_> = filtered_disposals
        .iter()
        .copied()
        .filter(|d| !d.is_unclassified())
        .collect();

    let total_proceeds: Decimal = classified_disposals.iter().map(|d| d.proceeds_gbp).sum();
    let total_costs: Decimal = classified_disposals
        .iter()
        .map(|d| d.allowable_cost_gbp + d.fees_gbp)
        .sum();
    let total_gain: Decimal = classified_disposals.iter().map(|d| d.gain_gbp).sum();

    // Per-asset-class totals (classified only)
    let crypto =
        sum_disposals_by_class(&classified_disposals, &asset_class_map, AssetClass::Crypto);
    let stocks = sum_disposals_by_class(&classified_disposals, &asset_class_map, AssetClass::Stock);
    let fiat = sum_disposals_by_class(&classified_disposals, &asset_class_map, AssetClass::Fiat);

    // Totals including unclassified events
    let total_proceeds_with_unclassified: Decimal =
        filtered_disposals.iter().map(|d| d.proceeds_gbp).sum();
    let total_costs_with_unclassified: Decimal = filtered_disposals
        .iter()
        .map(|d| d.allowable_cost_gbp + d.fees_gbp)
        .sum();
    let total_gain_with_unclassified: Decimal = filtered_disposals.iter().map(|d| d.gain_gbp).sum();

    // Warning counts
    let warning_count = event_rows.iter().filter(|e| !e.warnings.is_empty()).count();
    let unclassified_count = event_rows
        .iter()
        .filter(|e| {
            e.warnings
                .iter()
                .any(|w| matches!(w, Warning::UnclassifiedEvent))
        })
        .count();
    let cost_basis_warning_count = event_rows
        .iter()
        .filter(|e| {
            e.warnings
                .iter()
                .any(|w| matches!(w, Warning::InsufficientCostBasis { .. }))
        })
        .count();

    let (total_income, total_dividend_income, total_interest_income) = filtered_events
        .iter()
        .filter(|e| e.event_type == EventType::Acquisition && e.tag.is_income())
        .fold(
            (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO),
            |(income_total, dividend_total, interest_total), e| {
                let value = e.value_gbp;
                match e.tag {
                    Tag::Dividend => (income_total + value, dividend_total + value, interest_total),
                    Tag::Interest => (income_total + value, dividend_total, interest_total + value),
                    _ => (income_total + value, dividend_total, interest_total),
                }
            },
        );

    // Collect unique tax years
    let mut tax_years: Vec<String> = filtered_events
        .iter()
        .map(|e| TaxYear::from_date(e.date()).display())
        .collect();
    tax_years.sort();
    tax_years.dedup();

    // Collect unique assets
    let mut assets: Vec<String> = filtered_events.iter().map(|e| e.asset.clone()).collect();
    assets.sort();
    assets.dedup();

    // Calculate date range from filtered events
    let min_date = filtered_events.iter().map(|e| e.date()).min();
    let max_date = filtered_events.iter().map(|e| e.date()).max();

    let disposal_count = classified_disposals.len();
    let income_count = filtered_events
        .iter()
        .filter(|e| e.event_type == EventType::Acquisition && e.tag.is_income())
        .count();

    Summary {
        total_proceeds: pence_string(total_proceeds),
        total_costs: pence_string(total_costs),
        total_gain: pence_string(total_gain),
        total_proceeds_with_unclassified: pence_string(total_proceeds_with_unclassified),
        total_costs_with_unclassified: pence_string(total_costs_with_unclassified),
        total_gain_with_unclassified: pence_string(total_gain_with_unclassified),
        crypto,
        stocks,
        fiat,
        total_income: pence_string(total_income),
        total_dividend_income: pence_string(total_dividend_income),
        total_interest_income: pence_string(total_interest_income),
        event_count: filtered_events.len(),
        disposal_count,
        income_count,
        warning_count,
        unclassified_count,
        cost_basis_warning_count,
        tax_years,
        assets,
        min_date: min_date.map(iso_date),
        max_date: max_date.map(iso_date),
    }
}

fn build_transaction_rows(
    transactions: &[Transaction],
    events: &[TaxableEvent],
    filtered_events: &[&TaxableEvent],
    filter: &EventFilter,
) -> Vec<TransactionRow> {
    // Build transaction_id -> event_ids mapping
    let mut tx_event_map: HashMap<String, Vec<usize>> = HashMap::new();
    for e in filtered_events {
        tx_event_map
            .entry(e.source_transaction_id.clone())
            .or_default()
            .push(e.id);
    }
    let has_events: HashSet<&str> = events
        .iter()
        .map(|e| e.source_transaction_id.as_str())
        .collect();

    // A transaction is shown when the filter keeps one of its events, or --
    // for transactions with no taxable events (sterling moves, linked
    // transfers) -- when it falls in the date range and involves the asset.
    let shown = |tx: &&Transaction| {
        if has_events.contains(tx.id.as_str()) {
            return tx_event_map.contains_key(&tx.id);
        }
        filter.event_kind.is_none()
            && filter.matches_date(uk_date(tx.datetime))
            && transaction_assets(tx).any(|a| filter.matches_asset(a))
    };

    // Build transaction rows
    transactions
        .iter()
        .filter(shown)
        .map(|tx| {
            let (transaction_type, amounts) = match &tx.details {
                TransactionType::Trade { sold, bought } => (
                    "Trade".to_string(),
                    vec![
                        TransactionAmount {
                            label: "Sold".to_string(),
                            asset: sold.asset.clone(),
                            quantity: quantity_string(sold.quantity),
                        },
                        TransactionAmount {
                            label: "Bought".to_string(),
                            asset: bought.asset.clone(),
                            quantity: quantity_string(bought.quantity),
                        },
                    ],
                ),
                TransactionType::Deposit { amount, .. } => (
                    "Deposit".to_string(),
                    vec![TransactionAmount {
                        label: "Amount".to_string(),
                        asset: amount.asset.clone(),
                        quantity: quantity_string(amount.quantity),
                    }],
                ),
                TransactionType::Withdrawal { amount, .. } => (
                    "Withdrawal".to_string(),
                    vec![TransactionAmount {
                        label: "Amount".to_string(),
                        asset: amount.asset.clone(),
                        quantity: quantity_string(amount.quantity),
                    }],
                ),
            };

            let fee = tx.fee.as_ref().map(|f| TransactionFee {
                asset: f.asset.clone(),
                amount: quantity_string(f.amount),
            });

            let event_ids = tx_event_map.get(&tx.id).cloned().unwrap_or_default();

            TransactionRow {
                id: tx.id.clone(),
                datetime: uk_rfc3339(tx.datetime),
                tax_year: TaxYear::from_date(uk_date(tx.datetime)).display(),
                account: tx.account.clone(),
                transaction_type,
                tag: tx.tag,
                description: tx.description.clone().unwrap_or_default(),
                amounts,
                fee,
                event_ids,
            }
        })
        .collect()
}

/// Every asset a transaction moves, including its fee asset.
fn transaction_assets(tx: &Transaction) -> impl Iterator<Item = &str> {
    let moved = match &tx.details {
        TransactionType::Trade { sold, bought } => vec![sold.asset.as_str(), bought.asset.as_str()],
        TransactionType::Deposit { amount, .. } | TransactionType::Withdrawal { amount, .. } => {
            vec![amount.asset.as_str()]
        }
    };
    moved
        .into_iter()
        .chain(tx.fee.as_ref().map(|f| f.asset.as_str()))
}

fn sum_disposals_by_class(
    disposals: &[&DisposalRecord],
    asset_class_map: &HashMap<String, AssetClass>,
    class: AssetClass,
) -> AssetClassTotals {
    let (proceeds, costs, gain) = disposals
        .iter()
        .filter(|d| asset_class_map.get(&d.asset) == Some(&class))
        .fold(
            (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO),
            |(proceeds, costs, gain), d| {
                (
                    proceeds + d.proceeds_gbp,
                    costs + d.allowable_cost_gbp + d.fees_gbp,
                    gain + d.gain_gbp,
                )
            },
        );
    AssetClassTotals {
        proceeds: pence_string(proceeds),
        costs: pence_string(costs),
        gain: pence_string(gain),
    }
}

fn format_event_type(event_type: EventType, tag: Tag) -> String {
    display_event_type(event_type, tag).to_string()
}

fn format_asset_class(ac: &AssetClass) -> String {
    match ac {
        AssetClass::Crypto => "Crypto",
        AssetClass::Stock => "Stock",
        AssetClass::Fiat => "Fiat",
    }
    .to_string()
}

/// Group event warnings into one record per distinct warning value, ordered
/// by first affected event with the warning value as tie-breaker (one event
/// can carry both an Unclassified and an InsufficientCostBasis warning).
/// Transaction and event ids are unique per warning value upstream, so the
/// accumulated id lists need no dedup.
fn group_warnings(event_rows: &[EventRow]) -> Vec<WarningRecord> {
    let mut groups: HashMap<&Warning, (Vec<String>, Vec<usize>)> = HashMap::new();
    for event in event_rows {
        for warning in &event.warnings {
            let entry = groups.entry(warning).or_default();
            entry.0.push(event.source_transaction_id.clone());
            entry.1.push(event.id);
        }
    }
    let mut warnings: Vec<WarningRecord> = groups
        .into_iter()
        .map(
            |(warning, (source_transaction_ids, related_event_ids))| WarningRecord {
                warning: warning.clone(),
                source_transaction_ids,
                related_event_ids,
            },
        )
        .collect();
    warnings.sort_by(|a, b| {
        (a.related_event_ids.first().copied(), &a.warning)
            .cmp(&(b.related_event_ids.first().copied(), &b.warning))
    });
    warnings
}

#[cfg(test)]
mod tests;
