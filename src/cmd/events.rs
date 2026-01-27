//! Events command - transaction-level view showing all events with filtering

use crate::events::{self, EventType, OpeningPools, TaxableEvent};
use crate::tax::cgt::{calculate_cgt, CgtReport, DisposalRecord, MatchingRule};
use crate::tax::income::{calculate_income_tax, IncomeReport};
use crate::tax::TaxYear;
use clap::{Args, ValueEnum};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::io::BufReader;
use std::path::Path;
use std::{fs::File, io, path::PathBuf};
use tabled::{
    settings::{object::Rows, Alignment, Modify, Style},
    Table, Tabled,
};

#[derive(Args, Debug)]
pub struct EventsCommand {
    /// CSV or JSON file containing taxable events
    #[arg(short, long)]
    events: PathBuf,

    /// Tax year to filter (e.g., 2025 for 2024/25)
    #[arg(short, long)]
    year: Option<i32>,

    /// Filter by event type
    #[arg(short = 't', long, value_enum)]
    event_type: Option<EventTypeFilter>,

    /// Filter by asset (e.g., BTC, ETH)
    #[arg(short, long)]
    asset: Option<String>,

    /// Output as CSV instead of formatted table
    #[arg(long)]
    csv: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum EventTypeFilter {
    Acquisition,
    Disposal,
    Staking,
    Dividend,
}

impl EventsCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let tax_year = self.year.map(TaxYear);
        let (all_events, opening_pools) = read_events(&self.events)?;

        // Build the events view
        let cgt_report = calculate_cgt(all_events.clone(), opening_pools.as_ref());
        let income_report = calculate_income_tax(all_events.clone());

        let rows = build_event_rows(
            &all_events,
            &cgt_report,
            &income_report,
            tax_year,
            self.event_type,
            self.asset.as_deref(),
        );

        if self.csv {
            self.write_csv(&rows)
        } else {
            self.print_table(&rows);
            Ok(())
        }
    }

    fn print_table(&self, rows: &[EventRow]) {
        if rows.is_empty() {
            println!("No events found matching filters");
            return;
        }

        let table = Table::new(rows)
            .with(Style::rounded())
            .with(Modify::new(Rows::new(1..)).with(Alignment::right()))
            .to_string();
        println!("{}", table);
    }

    fn write_csv(&self, rows: &[EventRow]) -> color_eyre::Result<()> {
        let mut wtr = csv::Writer::from_writer(io::stdout());
        for row in rows {
            wtr.serialize(row)?;
        }
        wtr.flush()?;
        Ok(())
    }
}

/// Row for the events table output
#[derive(Debug, Clone, Tabled, serde::Serialize)]
pub struct EventRow {
    #[tabled(rename = "#")]
    #[serde(rename = "row_num")]
    pub row_num: String,

    #[tabled(rename = "Date")]
    pub date: String,

    #[tabled(rename = "Tax Year")]
    pub tax_year: String,

    #[tabled(rename = "Type")]
    pub event_type: String,

    #[tabled(rename = "Asset")]
    pub asset: String,

    #[tabled(rename = "Quantity")]
    pub quantity: String,

    #[tabled(rename = "Acq. Cost")]
    pub acquisition_cost: String,

    #[tabled(rename = "Proceeds")]
    pub proceeds: String,

    #[tabled(rename = "Gain/Loss")]
    pub gain_loss: String,

    #[tabled(rename = "Matched")]
    pub matched_ref: String,

    #[tabled(rename = "Income")]
    pub income_value: String,

    #[tabled(rename = "Description")]
    pub description: String,
}

/// Build event rows from the various data sources
fn build_event_rows(
    events: &[TaxableEvent],
    cgt_report: &CgtReport,
    _income_report: &IncomeReport,
    year: Option<TaxYear>,
    event_type_filter: Option<EventTypeFilter>,
    asset_filter: Option<&str>,
) -> Vec<EventRow> {
    let mut rows = Vec::new();

    // Build a lookup from (date, asset) to disposal record
    let disposal_map: HashMap<_, _> = cgt_report
        .disposals
        .iter()
        .map(|d| ((d.date, d.asset.clone()), d))
        .collect();

    // Build acquisition row number lookup for cross-referencing
    // We need to assign row numbers first, then disposals can reference them
    let mut row_num = 1usize;
    let mut acquisition_row_nums: HashMap<(chrono::NaiveDate, String), usize> = HashMap::new();

    // First pass: assign row numbers to acquisitions
    for event in events {
        let event_year = TaxYear::from_date(event.date());
        if year.is_some_and(|y| event_year != y) {
            continue;
        }
        if let Some(asset) = asset_filter {
            if !event.asset.eq_ignore_ascii_case(asset) {
                continue;
            }
        }
        if let Some(filter) = event_type_filter {
            if !matches_filter(&event.event_type, filter) {
                continue;
            }
        }

        if event.event_type == EventType::Acquisition {
            acquisition_row_nums.insert((event.date(), event.asset.clone()), row_num);
        }
        row_num += 1;
    }

    // Second pass: build the actual rows
    row_num = 1;
    for event in events {
        let event_year = TaxYear::from_date(event.date());

        // Apply filters
        if year.is_some_and(|y| event_year != y) {
            continue;
        }
        if let Some(asset) = asset_filter {
            if !event.asset.eq_ignore_ascii_case(asset) {
                continue;
            }
        }
        if let Some(filter) = event_type_filter {
            if !matches_filter(&event.event_type, filter) {
                continue;
            }
        }

        match event.event_type {
            EventType::Acquisition => {
                rows.push(EventRow {
                    row_num: format!("#{}", row_num),
                    date: event.date().format("%Y-%m-%d").to_string(),
                    tax_year: event_year.display(),
                    event_type: "Acquisition".to_string(),
                    asset: event.asset.clone(),
                    quantity: format_quantity(event.quantity),
                    acquisition_cost: format_gbp(event.total_cost_gbp()),
                    proceeds: "-".to_string(),
                    gain_loss: "-".to_string(),
                    matched_ref: String::new(),
                    income_value: "-".to_string(),
                    description: event.description.clone().unwrap_or_default(),
                });
                row_num += 1;
            }
            EventType::Disposal => {
                // Find the disposal record for detailed info
                if let Some(disposal) = disposal_map.get(&(event.date(), event.asset.clone())) {
                    // Check if this is a multi-rule disposal
                    if disposal.matching_components.len() <= 1 {
                        // Single rule - show inline
                        let (rule_name, matched_ref) =
                            format_single_rule(disposal, &acquisition_row_nums);
                        rows.push(EventRow {
                            row_num: format!("#{}", row_num),
                            date: event.date().format("%Y-%m-%d").to_string(),
                            tax_year: event_year.display(),
                            event_type: rule_name,
                            asset: event.asset.clone(),
                            quantity: format_quantity(event.quantity),
                            acquisition_cost: format_gbp(disposal.allowable_cost_gbp),
                            proceeds: format_gbp(disposal.proceeds_gbp),
                            gain_loss: format_gbp_signed(disposal.gain_gbp),
                            matched_ref,
                            income_value: "-".to_string(),
                            description: event.description.clone().unwrap_or_default(),
                        });
                        row_num += 1;
                    } else {
                        // Multi-rule - show disposal row plus sub-rows
                        rows.push(EventRow {
                            row_num: format!("#{}", row_num),
                            date: event.date().format("%Y-%m-%d").to_string(),
                            tax_year: event_year.display(),
                            event_type: "Disposal".to_string(),
                            asset: event.asset.clone(),
                            quantity: format_quantity(event.quantity),
                            acquisition_cost: format_gbp(disposal.allowable_cost_gbp),
                            proceeds: format_gbp(disposal.proceeds_gbp),
                            gain_loss: format_gbp_signed(disposal.gain_gbp),
                            matched_ref: String::new(),
                            income_value: "-".to_string(),
                            description: event.description.clone().unwrap_or_default(),
                        });
                        row_num += 1;

                        // Add sub-rows for each matching component
                        let total_qty = disposal.quantity;
                        for component in &disposal.matching_components {
                            let proportion = if total_qty.is_zero() {
                                Decimal::ZERO
                            } else {
                                component.quantity / total_qty
                            };
                            let proceeds_portion =
                                (disposal.proceeds_gbp * proportion).round_dp(2);
                            let gain_portion = (proceeds_portion - component.cost).round_dp(2);

                            let matched_ref = format_component_ref(
                                &component.rule,
                                component.matched_date,
                                &acquisition_row_nums,
                                &disposal.asset,
                            );

                            rows.push(EventRow {
                                row_num: "  └─".to_string(),
                                date: String::new(),
                                tax_year: String::new(),
                                event_type: format!("  {}", component.rule.display()),
                                asset: String::new(),
                                quantity: format_quantity(component.quantity),
                                acquisition_cost: format_gbp(component.cost),
                                proceeds: String::new(),
                                gain_loss: format_gbp_signed(gain_portion),
                                matched_ref,
                                income_value: String::new(),
                                description: String::new(),
                            });
                        }
                    }
                } else {
                    // No disposal record found (shouldn't happen)
                    rows.push(EventRow {
                        row_num: format!("#{}", row_num),
                        date: event.date().format("%Y-%m-%d").to_string(),
                        tax_year: event_year.display(),
                        event_type: "Disposal".to_string(),
                        asset: event.asset.clone(),
                        quantity: format_quantity(event.quantity),
                        acquisition_cost: "-".to_string(),
                        proceeds: format_gbp(event.value_gbp),
                        gain_loss: "-".to_string(),
                        matched_ref: String::new(),
                        income_value: "-".to_string(),
                        description: event.description.clone().unwrap_or_default(),
                    });
                    row_num += 1;
                }
            }
            EventType::StakingReward => {
                rows.push(EventRow {
                    row_num: format!("#{}", row_num),
                    date: event.date().format("%Y-%m-%d").to_string(),
                    tax_year: event_year.display(),
                    event_type: "Staking".to_string(),
                    asset: event.asset.clone(),
                    quantity: format_quantity(event.quantity),
                    acquisition_cost: "-".to_string(),
                    proceeds: "-".to_string(),
                    gain_loss: "-".to_string(),
                    matched_ref: String::new(),
                    income_value: format_gbp(event.value_gbp),
                    description: event.description.clone().unwrap_or_default(),
                });
                row_num += 1;
            }
            EventType::Dividend => {
                rows.push(EventRow {
                    row_num: format!("#{}", row_num),
                    date: event.date().format("%Y-%m-%d").to_string(),
                    tax_year: event_year.display(),
                    event_type: "Dividend".to_string(),
                    asset: event.asset.clone(),
                    quantity: format_quantity(event.quantity),
                    acquisition_cost: "-".to_string(),
                    proceeds: "-".to_string(),
                    gain_loss: "-".to_string(),
                    matched_ref: String::new(),
                    income_value: format_gbp(event.value_gbp),
                    description: event.description.clone().unwrap_or_default(),
                });
                row_num += 1;
            }
        }
    }

    rows
}

fn matches_filter(event_type: &EventType, filter: EventTypeFilter) -> bool {
    matches!(
        (event_type, filter),
        (EventType::Acquisition, EventTypeFilter::Acquisition)
            | (EventType::Disposal, EventTypeFilter::Disposal)
            | (EventType::StakingReward, EventTypeFilter::Staking)
            | (EventType::Dividend, EventTypeFilter::Dividend)
    )
}

fn format_single_rule(
    disposal: &DisposalRecord,
    acquisition_row_nums: &HashMap<(chrono::NaiveDate, String), usize>,
) -> (String, String) {
    if disposal.matching_components.is_empty() {
        return ("Disposal (Pool)".to_string(), String::new());
    }

    let component = &disposal.matching_components[0];
    let rule_name = match component.rule {
        MatchingRule::SameDay => "Disposal (Same-Day)".to_string(),
        MatchingRule::BedAndBreakfast => "Disposal (B&B)".to_string(),
        MatchingRule::Pool => "Disposal (Pool)".to_string(),
    };

    let matched_ref =
        format_component_ref(&component.rule, component.matched_date, acquisition_row_nums, &disposal.asset);

    (rule_name, matched_ref)
}

fn format_component_ref(
    rule: &MatchingRule,
    matched_date: Option<chrono::NaiveDate>,
    acquisition_row_nums: &HashMap<(chrono::NaiveDate, String), usize>,
    asset: &str,
) -> String {
    match rule {
        MatchingRule::SameDay => {
            if let Some(date) = matched_date {
                if let Some(&row_num) = acquisition_row_nums.get(&(date, asset.to_string())) {
                    return format!("→ #{}", row_num);
                }
            }
            String::new()
        }
        MatchingRule::BedAndBreakfast => {
            if let Some(date) = matched_date {
                if let Some(&row_num) = acquisition_row_nums.get(&(date, asset.to_string())) {
                    return format!("→ #{} ({})", row_num, date.format("%m-%d"));
                }
                // If we don't have a row number (e.g., filtered out), just show date
                return format!("→ ({})", date.format("%m-%d"));
            }
            String::new()
        }
        MatchingRule::Pool => String::new(),
    }
}

fn format_gbp(amount: Decimal) -> String {
    format!("£{:.2}", amount)
}

fn format_gbp_signed(amount: Decimal) -> String {
    if amount < Decimal::ZERO {
        format!("-£{:.2}", amount.abs())
    } else {
        format!("£{:.2}", amount)
    }
}

fn format_quantity(qty: Decimal) -> String {
    // Use reasonable precision, removing trailing zeros
    let s = format!("{:.8}", qty);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

/// Read events from CSV or JSON file based on extension
pub fn read_events(
    path: &Path,
) -> color_eyre::Result<(Vec<TaxableEvent>, Option<OpeningPools>)> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    match path.extension().and_then(|s| s.to_str()) {
        Some("json") => events::read_json(reader),
        _ => {
            // Default to CSV for .csv files and any other extension
            let events = events::read_csv(reader)?;
            Ok((events, None))
        }
    }
}
