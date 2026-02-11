//! Report command and JSON/HTML data model.

pub mod html;

use super::read_events;
use crate::core::{
    calculate_cgt, calculate_income_tax, display_event_type, AssetClass, CgtReport, DisposalIndex,
    EventType, IncomeReport, Label, MatchingRule, TaxYear, TaxableEvent, Warning,
};
use clap::{Args, ValueEnum};
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ReportCommand {
    /// Transactions file (JSON). Reads from stdin if not specified.
    #[arg(default_value = "-")]
    file: PathBuf,

    /// Tax year to filter (e.g., 2025 for 2024/25)
    #[arg(short, long)]
    year: Option<i32>,

    /// Output file path (default: opens in browser)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Output as JSON instead of HTML
    #[arg(long)]
    json: bool,

    /// Filter by event type
    #[arg(short = 't', long, value_enum)]
    event_type: Option<EventTypeFilter>,

    /// Filter by asset (e.g., BTC, ETH)
    #[arg(short, long)]
    asset: Option<String>,

    /// Don't include unlinked deposits/withdrawals in calculations
    #[arg(long)]
    exclude_unlinked: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum EventTypeFilter {
    Acquisition,
    Disposal,
    Staking,
}

impl ReportCommand {
    pub fn exec(&self) -> anyhow::Result<()> {
        let tax_year = self.year.map(TaxYear);
        let events = read_events(&self.file, self.exclude_unlinked)?;

        let cgt_report = calculate_cgt(events.clone())?;
        let income_report = calculate_income_tax(events.clone())?;

        if self.json {
            let data = build_report_data(
                &events,
                &cgt_report,
                &income_report,
                tax_year,
                self.asset.as_deref(),
                self.event_type,
            )?;
            let json = serde_json::to_string_pretty(&data)?;

            if let Some(ref output_path) = self.output {
                std::fs::write(output_path, &json)?;
                eprintln!("JSON report written to: {}", output_path.display());
            } else {
                println!("{}", json);
            }
        } else {
            let html = html::generate_html(
                &events,
                &cgt_report,
                &income_report,
                tax_year,
                self.asset.as_deref(),
                self.event_type,
            )?;

            if let Some(ref output_path) = self.output {
                std::fs::write(output_path, &html)?;
                println!("HTML report written to: {}", output_path.display());
            } else {
                // Write to temp file and open in browser
                let temp_path = std::env::temp_dir().join("taxc-report.html");
                std::fs::write(&temp_path, &html)?;
                opener::open(&temp_path)?;
                println!("Opened HTML report in browser: {}", temp_path.display());
            }
        }

        Ok(())
    }
}

/// Data structure for embedding in HTML as JSON
#[derive(Serialize, JsonSchema)]
pub struct ReportData {
    pub events: Vec<EventRow>,
    pub warnings: Vec<WarningRecord>,
    pub summary: Summary,
}

#[derive(Serialize, JsonSchema)]
pub struct EventRow {
    /// Sequential event identifier
    pub id: usize,
    /// Source transaction identifier from input
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_transaction_id: Option<String>,
    pub datetime: String,
    pub tax_year: String,
    pub event_type: String,
    pub asset: String,
    pub asset_class: String,
    pub quantity: String,
    pub value_gbp: String,
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
    /// Warnings for this disposal (e.g., "Unclassified", "NoCostBasis", "InsufficientPool")
    pub warnings: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct MatchingComponentRow {
    pub rule: String,
    pub quantity: String,
    pub cost_gbp: String,
    /// For Same-Day/B&B: the linked acquisition date
    pub matched_date: Option<String>,
    /// Row ID of the matched acquisition (for navigation)
    pub matched_row_id: Option<usize>,
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
    event_type: String,
    tax_year: String,
    quantity: Decimal,
    value_gbp: Decimal,
    description: String,
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
    pub total_staking: String,
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
    events: &[TaxableEvent],
    cgt_report: &CgtReport,
    income_report: &IncomeReport,
    year: Option<TaxYear>,
    asset_filter: Option<&str>,
    event_type_filter: Option<EventTypeFilter>,
) -> anyhow::Result<ReportData> {
    use chrono::NaiveDate;
    use std::collections::HashMap;

    // Filter events for the target year
    let filtered_events: Vec<_> = events
        .iter()
        .filter(|e| year.is_none_or(|y| TaxYear::from_date(e.date()) == y))
        .filter(|e| asset_filter.is_none_or(|asset| e.asset.eq_ignore_ascii_case(asset)))
        .filter(|e| event_type_filter.is_none_or(|filter| matches_filter(e, filter)))
        .collect();

    // Build index of acquisitions by (date, asset) -> row index for navigation
    // Multiple acquisitions on the same day for the same asset share a row index (first one)
    let mut acquisition_row_index: HashMap<(NaiveDate, String), usize> = HashMap::new();
    for (idx, e) in filtered_events.iter().enumerate() {
        if e.event_type == EventType::Acquisition && e.label != Label::Unclassified {
            let key = (e.date(), e.asset.clone());
            acquisition_row_index.entry(key).or_insert(idx);
        }
    }

    // Build a map of acquisition details by (date, asset) for lookup
    // Aggregates multiple acquisitions on the same day
    let mut acquisition_details: HashMap<(NaiveDate, String), AcquisitionDetail> = HashMap::new();
    for e in &filtered_events {
        if e.event_type == EventType::Acquisition && e.label != Label::Unclassified {
            let key = (e.date(), e.asset.clone());
            let detail = acquisition_details
                .entry(key)
                .or_insert_with(|| AcquisitionDetail {
                    event_type: format_event_type(e.event_type, e.label),
                    tax_year: TaxYear::from_date(e.date()).display(),
                    description: e.description.clone().unwrap_or_default(),
                    ..Default::default()
                });
            detail.quantity += e.quantity;
            detail.value_gbp += e.value_gbp;
        }
    }

    // Build CGT lookup: prefer id, fallback to a composite key
    let mut disposal_index = DisposalIndex::new(cgt_report);

    // Build events list with CGT details for disposals
    let event_rows: Vec<EventRow> = filtered_events
        .iter()
        .map(|e| {
            // Look up CGT details for disposal events
            let mut event_warnings = if e.label == Label::Unclassified {
                vec![Warning::UnclassifiedEvent]
            } else {
                Vec::new()
            };

            let cgt = if e.event_type == EventType::Disposal {
                disposal_index.find(e).map(|d| {
                    for warning in &d.warnings {
                        if !event_warnings.contains(warning) {
                            event_warnings.push(warning.clone());
                        }
                    }

                    // Determine primary matching rule
                    let rule = if d.matching_components.is_empty() {
                        "Pool".to_string()
                    } else if d.matching_components.len() == 1 {
                        format_matching_rule(&d.matching_components[0].rule)
                    } else {
                        "Mixed".to_string()
                    };

                    // Build matching components with acquisition details
                    let matching_components: Vec<MatchingComponentRow> = d
                        .matching_components
                        .iter()
                        .map(|mc| {
                            // Look up acquisition details for Same-Day and B&B matches
                            let (
                                matched_row_id,
                                matched_event_type,
                                matched_tax_year,
                                matched_asset,
                                matched_original_qty,
                                matched_original_value,
                                matched_description,
                            ) = if let Some(date) = mc.matched_date {
                                let key = (date, d.asset.clone());
                                let row_id = acquisition_row_index.get(&key).copied();
                                if let Some(detail) = acquisition_details.get(&key) {
                                    (
                                        row_id,
                                        Some(detail.event_type.clone()),
                                        Some(detail.tax_year.clone()),
                                        Some(d.asset.clone()),
                                        Some(detail.quantity.to_string()),
                                        Some(format!("{:.2}", detail.value_gbp)),
                                        Some(detail.description.clone()),
                                    )
                                } else {
                                    (None, None, None, None, None, None, None)
                                }
                            } else {
                                (None, None, None, None, None, None, None)
                            };

                            MatchingComponentRow {
                                rule: format_matching_rule(&mc.rule),
                                quantity: mc.quantity.to_string(),
                                cost_gbp: format!("{:.2}", mc.cost),
                                matched_date: mc
                                    .matched_date
                                    .map(|d| d.format("%Y-%m-%d").to_string()),
                                matched_row_id,
                                matched_event_type,
                                matched_tax_year,
                                matched_asset,
                                matched_original_qty,
                                matched_original_value,
                                matched_description,
                            }
                        })
                        .collect();

                    // Convert warnings to display strings
                    let warnings: Vec<String> =
                        d.warnings.iter().map(format_event_warning).collect();

                    CgtDetails {
                        proceeds_gbp: format!("{:.2}", d.proceeds_gbp),
                        cost_gbp: format!("{:.2}", d.allowable_cost_gbp),
                        gain_gbp: format!("{:.2}", d.gain_gbp),
                        rule,
                        matching_components,
                        warnings,
                    }
                })
            } else {
                None
            };

            let fees_gbp = e
                .fee_gbp
                .map(|fee| format!("{:.2}", fee))
                .unwrap_or_default();

            Ok(EventRow {
                id: e.id,
                source_transaction_id: e.source_transaction_id.clone(),
                datetime: e.datetime.to_rfc3339(),
                tax_year: TaxYear::from_date(e.date()).display(),
                event_type: format_event_type(e.event_type, e.label),
                asset: e.asset.clone(),
                asset_class: format_asset_class(&e.asset_class),
                quantity: e.quantity.to_string(),
                value_gbp: format!("{:.2}", e.value_gbp),
                fees_gbp,
                description: e.description.clone().unwrap_or_default(),
                warnings: event_warnings,
                cgt,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let warnings: Vec<WarningRecord> = event_rows
        .iter()
        .flat_map(|event| {
            event.warnings.iter().map(move |warning| {
                let source_transaction_ids = event
                    .source_transaction_id
                    .clone()
                    .map(|id| vec![id])
                    .unwrap_or_default();
                let related_event_ids = vec![event.id];

                WarningRecord {
                    warning: warning.clone(),
                    source_transaction_ids,
                    related_event_ids,
                }
            })
        })
        .collect();

    // Calculate summary (classified events only)
    let total_proceeds = cgt_report.total_proceeds(year);
    let total_costs = cgt_report.total_allowable_costs(year);
    let total_gain = cgt_report.total_gain(year);

    // Calculate totals including unclassified events
    let total_proceeds_with_unclassified = cgt_report.total_proceeds_with_unclassified(year);
    let total_costs_with_unclassified = cgt_report.total_allowable_costs_with_unclassified(year);
    let total_gain_with_unclassified = cgt_report.total_gain_with_unclassified(year);

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

    let total_staking: Decimal = income_report
        .staking_events
        .iter()
        .filter(|e| year.is_none_or(|y| e.tax_year == y))
        .map(|e| e.value_gbp)
        .sum();

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

    let disposal_count = event_rows.iter().filter(|e| e.cgt.is_some()).count();
    let income_count = event_rows
        .iter()
        .filter(|e| e.event_type == "StakingReward")
        .count();

    Ok(ReportData {
        events: event_rows,
        warnings,
        summary: Summary {
            total_proceeds: format!("{:.2}", total_proceeds),
            total_costs: format!("{:.2}", total_costs),
            total_gain: format!("{:.2}", total_gain),
            total_proceeds_with_unclassified: format!("{:.2}", total_proceeds_with_unclassified),
            total_costs_with_unclassified: format!("{:.2}", total_costs_with_unclassified),
            total_gain_with_unclassified: format!("{:.2}", total_gain_with_unclassified),
            total_staking: format!("{:.2}", total_staking),
            event_count: events.len(),
            disposal_count,
            income_count,
            warning_count,
            unclassified_count,
            cost_basis_warning_count,
            tax_years,
            assets,
            min_date: min_date.map(|d| d.format("%Y-%m-%d").to_string()),
            max_date: max_date.map(|d| d.format("%Y-%m-%d").to_string()),
        },
    })
}

fn format_event_type(event_type: EventType, label: Label) -> String {
    display_event_type(event_type, label).to_string()
}

fn matches_filter(event: &TaxableEvent, filter: EventTypeFilter) -> bool {
    match filter {
        EventTypeFilter::Acquisition => {
            event.event_type == EventType::Acquisition && event.label == Label::Trade
        }
        EventTypeFilter::Disposal => {
            event.event_type == EventType::Disposal && event.label == Label::Trade
        }
        EventTypeFilter::Staking => {
            event.event_type == EventType::Acquisition && event.label == Label::StakingReward
        }
    }
}

fn format_asset_class(ac: &AssetClass) -> String {
    match ac {
        AssetClass::Crypto => "Crypto",
        AssetClass::Stock => "Stock",
    }
    .to_string()
}

fn format_matching_rule(rule: &MatchingRule) -> String {
    match rule {
        MatchingRule::SameDay => "Same-Day",
        MatchingRule::BedAndBreakfast => "B&B",
        MatchingRule::Pool => "Pool",
    }
    .to_string()
}

fn format_event_warning(warning: &Warning) -> String {
    match warning {
        Warning::UnclassifiedEvent => "Unclassified".to_string(),
        Warning::InsufficientCostBasis {
            available,
            required,
        } => {
            if available.is_zero() {
                "NoCostBasis".to_string()
            } else {
                format!("InsufficientCostBasis({}/{})", available, required)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{AssetClass, EventType, Label, TaxableEvent};
    use chrono::DateTime;
    use rust_decimal_macros::dec;

    fn dt(date: &str) -> chrono::DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339(&format!("{date}T00:00:00+00:00")).unwrap()
    }

    #[test]
    fn gift_event_types_in_report_data() {
        let events = vec![
            TaxableEvent {
                id: 1,
                source_transaction_id: None,
                datetime: dt("2024-01-01"),
                event_type: EventType::Acquisition,
                label: Label::Gift,
                asset: "ETH".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(2),
                value_gbp: dec!(2000),
                fee_gbp: None,
                description: Some("Gift received".to_string()),
            },
            TaxableEvent {
                id: 2,
                source_transaction_id: None,
                datetime: dt("2024-02-01"),
                event_type: EventType::Disposal,
                label: Label::Gift,
                asset: "ETH".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(1),
                value_gbp: dec!(1500),
                fee_gbp: None,
                description: Some("Gift given".to_string()),
            },
        ];

        let cgt_report = calculate_cgt(events.clone()).unwrap();
        let income_report = calculate_income_tax(events.clone()).unwrap();
        let data =
            build_report_data(&events, &cgt_report, &income_report, None, None, None).unwrap();

        let event_types: Vec<String> = data.events.iter().map(|e| e.event_type.clone()).collect();
        assert!(event_types.iter().any(|t| t == "GiftIn"));
        assert!(event_types.iter().any(|t| t == "GiftOut"));
    }

    #[test]
    fn same_day_duplicate_acquisitions_link_to_first_row() {
        let events = vec![
            TaxableEvent {
                id: 1,
                source_transaction_id: None,
                datetime: dt("2024-06-15"),
                event_type: EventType::Acquisition,
                label: Label::Trade,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(1),
                value_gbp: dec!(30000),
                fee_gbp: None,
                description: None,
            },
            TaxableEvent {
                id: 2,
                source_transaction_id: None,
                datetime: dt("2024-06-15"),
                event_type: EventType::Acquisition,
                label: Label::Trade,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(1),
                value_gbp: dec!(40000),
                fee_gbp: None,
                description: None,
            },
            TaxableEvent {
                id: 3,
                source_transaction_id: None,
                datetime: dt("2024-06-15"),
                event_type: EventType::Disposal,
                label: Label::Trade,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(2),
                value_gbp: dec!(80000),
                fee_gbp: None,
                description: None,
            },
        ];

        let cgt_report = calculate_cgt(events.clone()).unwrap();
        let income_report = calculate_income_tax(events.clone()).unwrap();
        let data =
            build_report_data(&events, &cgt_report, &income_report, None, None, None).unwrap();

        let disposal = data
            .events
            .iter()
            .find(|e| e.event_type == "Disposal")
            .and_then(|e| e.cgt.as_ref())
            .expect("expected disposal with CGT details");

        for component in &disposal.matching_components {
            assert_eq!(
                component.matched_row_id,
                Some(0),
                "expected same-day match to point to first acquisition row"
            );
        }
    }

    #[test]
    fn bnb_duplicate_acquisitions_link_to_first_row() {
        let events = vec![
            TaxableEvent {
                id: 1,
                source_transaction_id: None,
                datetime: dt("2024-01-01"),
                event_type: EventType::Acquisition,
                label: Label::Trade,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(5),
                value_gbp: dec!(100000),
                fee_gbp: None,
                description: None,
            },
            TaxableEvent {
                id: 2,
                source_transaction_id: None,
                datetime: dt("2024-06-01"),
                event_type: EventType::Disposal,
                label: Label::Trade,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(1),
                value_gbp: dec!(25000),
                fee_gbp: None,
                description: None,
            },
            TaxableEvent {
                id: 3,
                source_transaction_id: None,
                datetime: dt("2024-06-10"),
                event_type: EventType::Acquisition,
                label: Label::Trade,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(1),
                value_gbp: dec!(22000),
                fee_gbp: None,
                description: None,
            },
            TaxableEvent {
                id: 4,
                source_transaction_id: None,
                datetime: dt("2024-06-10"),
                event_type: EventType::Acquisition,
                label: Label::Trade,
                asset: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
                quantity: dec!(1),
                value_gbp: dec!(24000),
                fee_gbp: None,
                description: None,
            },
        ];

        let cgt_report = calculate_cgt(events.clone()).unwrap();
        let income_report = calculate_income_tax(events.clone()).unwrap();
        let data =
            build_report_data(&events, &cgt_report, &income_report, None, None, None).unwrap();

        let disposal = data
            .events
            .iter()
            .find(|e| e.event_type == "Disposal")
            .and_then(|e| e.cgt.as_ref())
            .expect("expected disposal with CGT details");

        for component in &disposal.matching_components {
            assert_eq!(
                component.matched_row_id,
                Some(2),
                "expected B&B match to point to first acquisition row for the matched date"
            );
        }
    }

    #[test]
    fn warning_records_link_source_transaction_and_event_ids() {
        let events = vec![TaxableEvent {
            id: 1,
            source_transaction_id: Some("tx-1".to_string()),
            datetime: dt("2024-06-01"),
            event_type: EventType::Disposal,
            label: Label::Trade,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1),
            value_gbp: dec!(25000),
            fee_gbp: None,
            description: None,
        }];

        let cgt_report = calculate_cgt(events.clone()).unwrap();
        let income_report = calculate_income_tax(events.clone()).unwrap();
        let data =
            build_report_data(&events, &cgt_report, &income_report, None, None, None).unwrap();

        assert!(data.warnings.iter().any(|w| matches!(
            w.warning,
            Warning::InsufficientCostBasis { .. }
        ) && w.source_transaction_ids
            == vec!["tx-1".to_string()]
            && w.related_event_ids == vec![1]));
    }
}
