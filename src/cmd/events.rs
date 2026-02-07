//! Events command - transaction-level view showing all events with filtering

use crate::events::{EventType, Label, TaxableEvent};
use crate::tax::cgt::{calculate_cgt, CgtReport, DisposalRecord, MatchingRule};
use crate::tax::income::{calculate_income_tax, IncomeReport};
use crate::tax::TaxYear;
use crate::transaction::{self, ConversionOptions};
use clap::{Args, ValueEnum};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use tabled::{
    settings::{object::Rows, Alignment, Modify, Style},
    Table, Tabled,
};

#[derive(Args, Debug)]
pub struct EventsCommand {
    /// Transactions file (JSON). Reads from stdin if not specified.
    #[arg(default_value = "-")]
    file: PathBuf,

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

impl EventsCommand {
    pub fn exec(&self) -> anyhow::Result<()> {
        let tax_year = self.year.map(TaxYear);
        let all_events = read_events(&self.file, self.exclude_unlinked)?;

        // Build the events view
        let cgt_report = calculate_cgt(all_events.clone())?;
        let income_report = calculate_income_tax(all_events.clone())?;

        let rows = build_event_rows(
            &all_events,
            &cgt_report,
            &income_report,
            tax_year,
            self.event_type,
            self.asset.as_deref(),
        )?;

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

    fn write_csv(&self, rows: &[EventRow]) -> anyhow::Result<()> {
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

    /// Source data identifier (hidden in table, shown in CSV)
    #[tabled(skip)]
    pub id: Option<String>,

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

/// Create a base EventRow with common fields from an event
fn base_row(event: &TaxableEvent, row_num: usize, event_year: TaxYear) -> EventRow {
    EventRow {
        row_num: format!("#{}", row_num),
        id: event.id.clone(),
        date: event.date().format("%Y-%m-%d").to_string(),
        tax_year: event_year.display(),
        event_type: String::new(),
        asset: event.asset.clone(),
        quantity: format_quantity(event.quantity),
        acquisition_cost: "-".to_string(),
        proceeds: "-".to_string(),
        gain_loss: "-".to_string(),
        matched_ref: String::new(),
        income_value: "-".to_string(),
        description: event.description.clone().unwrap_or_default(),
    }
}

/// Create an EventRow for a disposal with CGT record
fn disposal_row(
    event: &TaxableEvent,
    row_num: usize,
    event_year: TaxYear,
    disposal: &DisposalRecord,
    event_type: String,
) -> EventRow {
    EventRow {
        event_type,
        acquisition_cost: format_gbp(disposal.allowable_cost_gbp),
        proceeds: format_gbp(disposal.proceeds_gbp),
        gain_loss: format_gbp_signed(disposal.gain_gbp),
        ..base_row(event, row_num, event_year)
    }
}

/// Create an EventRow for a disposal without CGT record (fallback)
fn disposal_row_fallback(
    event: &TaxableEvent,
    row_num: usize,
    event_year: TaxYear,
    event_type: &str,
) -> EventRow {
    EventRow {
        event_type: event_type.to_string(),
        proceeds: format_gbp(event.value_gbp),
        ..base_row(event, row_num, event_year)
    }
}

/// Build event rows from the various data sources
fn build_event_rows(
    events: &[TaxableEvent],
    cgt_report: &CgtReport,
    _income_report: &IncomeReport,
    year: Option<TaxYear>,
    event_type_filter: Option<EventTypeFilter>,
    asset_filter: Option<&str>,
) -> anyhow::Result<Vec<EventRow>> {
    let mut rows = Vec::new();

    // Build a lookup from (date, asset) to disposal record
    let disposal_map: HashMap<_, _> = cgt_report
        .disposals
        .iter()
        .map(|d| ((d.date, d.asset.clone()), d))
        .collect();

    // Build acquisition row number lookup for cross-referencing
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
            if !matches_filter(event, filter) {
                continue;
            }
        }

        if event.event_type == EventType::Acquisition && event.label == Label::Trade {
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
            if !matches_filter(event, filter) {
                continue;
            }
        }

        match (event.event_type, event.label) {
            (EventType::Acquisition, Label::Trade) => {
                rows.push(EventRow {
                    event_type: "Acquisition".to_string(),
                    acquisition_cost: format_gbp(event.total_cost_gbp()),
                    ..base_row(event, row_num, event_year)
                });
                row_num += 1;
            }
            (EventType::Disposal, Label::Trade) => {
                if let Some(disposal) = disposal_map.get(&(event.date(), event.asset.clone())) {
                    if disposal.matching_components.len() <= 1 {
                        // Single rule - show inline
                        let (rule_name, matched_ref) =
                            format_single_rule(disposal, &acquisition_row_nums);
                        rows.push(EventRow {
                            matched_ref,
                            ..disposal_row(event, row_num, event_year, disposal, rule_name)
                        });
                        row_num += 1;
                    } else {
                        // Multi-rule - show disposal row plus sub-rows
                        let warning_prefix = if disposal.has_warnings() { "⚠ " } else { "" };
                        let event_type = format!("{}Disposal", warning_prefix);
                        rows.push(disposal_row(
                            event, row_num, event_year, disposal, event_type,
                        ));
                        row_num += 1;

                        // Add sub-rows for each matching component
                        push_component_rows(&mut rows, disposal, &acquisition_row_nums);
                    }
                } else {
                    rows.push(disposal_row_fallback(
                        event, row_num, event_year, "Disposal",
                    ));
                    row_num += 1;
                }
            }
            (EventType::Acquisition, Label::StakingReward) => {
                rows.push(EventRow {
                    event_type: "Staking".to_string(),
                    income_value: format_gbp(event.value_gbp),
                    ..base_row(event, row_num, event_year)
                });
                row_num += 1;
            }
            (EventType::Acquisition, Label::Gift) => {
                rows.push(EventRow {
                    event_type: "Gift In".to_string(),
                    acquisition_cost: format_gbp(event.total_cost_gbp()),
                    ..base_row(event, row_num, event_year)
                });
                row_num += 1;
            }
            (EventType::Disposal, Label::Gift) => {
                if let Some(disposal) = disposal_map.get(&(event.date(), event.asset.clone())) {
                    let warning_prefix = if disposal.has_warnings() { "⚠ " } else { "" };
                    let event_type = format!("{}Gift Out", warning_prefix);
                    rows.push(disposal_row(
                        event, row_num, event_year, disposal, event_type,
                    ));
                } else {
                    rows.push(disposal_row_fallback(
                        event, row_num, event_year, "Gift Out",
                    ));
                }
                row_num += 1;
            }
            (EventType::Acquisition, Label::Unclassified) => {
                rows.push(EventRow {
                    event_type: "Unclassified In".to_string(),
                    acquisition_cost: format_gbp(event.total_cost_gbp()),
                    ..base_row(event, row_num, event_year)
                });
                row_num += 1;
            }
            (EventType::Disposal, Label::Unclassified) => {
                if let Some(disposal) = disposal_map.get(&(event.date(), event.asset.clone())) {
                    let warning_prefix = if disposal.has_warnings() { "⚠ " } else { "" };
                    let event_type = format!("{}Unclassified Out", warning_prefix);
                    rows.push(disposal_row(
                        event, row_num, event_year, disposal, event_type,
                    ));
                } else {
                    rows.push(disposal_row_fallback(
                        event,
                        row_num,
                        event_year,
                        "⚠ Unclassified Out",
                    ));
                }
                row_num += 1;
            }
            _ => continue,
        }
    }

    Ok(rows)
}

/// Add sub-rows for matching components of a multi-rule disposal
fn push_component_rows(
    rows: &mut Vec<EventRow>,
    disposal: &DisposalRecord,
    acquisition_row_nums: &HashMap<(chrono::NaiveDate, String), usize>,
) {
    let total_qty = disposal.quantity;
    for component in &disposal.matching_components {
        let proportion = if total_qty.is_zero() {
            Decimal::ZERO
        } else {
            component.quantity / total_qty
        };
        let proceeds_portion = (disposal.proceeds_gbp * proportion).round_dp(2);
        let gain_portion = (proceeds_portion - component.cost).round_dp(2);

        let matched_ref = format_component_ref(
            &component.rule,
            component.matched_date,
            acquisition_row_nums,
            &disposal.asset,
        );

        rows.push(EventRow {
            row_num: "  └─".to_string(),
            id: None,
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

fn format_single_rule(
    disposal: &DisposalRecord,
    acquisition_row_nums: &HashMap<(chrono::NaiveDate, String), usize>,
) -> (String, String) {
    let warning_prefix = if disposal.has_warnings() { "⚠ " } else { "" };

    if disposal.matching_components.is_empty() {
        return (format!("{}Disposal (Pool)", warning_prefix), String::new());
    }

    let component = &disposal.matching_components[0];
    let rule_name = match component.rule {
        MatchingRule::SameDay => format!("{}Disposal (Same-Day)", warning_prefix),
        MatchingRule::BedAndBreakfast => format!("{}Disposal (B&B)", warning_prefix),
        MatchingRule::Pool => format!("{}Disposal (Pool)", warning_prefix),
    };

    let matched_ref = format_component_ref(
        &component.rule,
        component.matched_date,
        acquisition_row_nums,
        &disposal.asset,
    );

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

/// Read transactions (JSON) and convert to events (or stdin with "-")
pub fn read_events(path: &Path, exclude_unlinked: bool) -> anyhow::Result<Vec<TaxableEvent>> {
    let options = ConversionOptions { exclude_unlinked };
    if path.as_os_str() == "-" {
        read_from_stdin(options)
    } else {
        read_from_file(path, options)
    }
}

fn read_from_file(path: &Path, options: ConversionOptions) -> anyhow::Result<Vec<TaxableEvent>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let transactions = transaction::read_transactions_json(reader)?;
    let events = transaction::transactions_to_events(&transactions, options)?;
    Ok(events)
}

fn read_from_stdin(options: ConversionOptions) -> anyhow::Result<Vec<TaxableEvent>> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());

    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;

    if buffer.is_empty() {
        anyhow::bail!("No input received. Provide a file or pipe data to stdin.");
    }

    let cursor = io::Cursor::new(buffer);
    let transactions = transaction::read_transactions_json(cursor)?;
    let events = transaction::transactions_to_events(&transactions, options)?;
    Ok(events)
}
