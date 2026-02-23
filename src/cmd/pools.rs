//! Pools command - pool balances over time

use super::filter::{EventFilter, FilterArgs};
use super::read_events;
use crate::core::{
    calculate_cgt, display_event_type, PoolHistoryEntry, PoolState, TaxYear, YearEndSnapshot,
};
use chrono::NaiveDate;
use clap::Args;
use rust_decimal::Decimal;
use serde::Serialize;
use std::path::PathBuf;
use tabled::{
    settings::{object::Rows, Alignment, Modify, Style},
    Table, Tabled,
};

#[derive(Args, Debug)]
pub struct PoolsCommand {
    /// Transactions file (JSON). Reads from stdin if not specified.
    #[arg(default_value = "-")]
    file: PathBuf,

    /// Filter by asset (e.g., BTC, ETH)
    #[arg(short, long)]
    asset: Option<String>,

    /// Show daily time-series instead of year-end snapshots
    #[arg(long)]
    daily: bool,

    /// Output as JSON instead of formatted table
    #[arg(long)]
    json: bool,

    /// Don't include unlinked deposits/withdrawals in calculations
    #[arg(long)]
    exclude_unlinked: bool,

    #[command(flatten)]
    filter: FilterArgs,
}

impl PoolsCommand {
    pub fn exec(&self) -> anyhow::Result<()> {
        let event_filter = self.filter.build(self.asset.clone())?;

        if !self.daily && event_filter.event_kind.is_some() {
            anyhow::bail!(
                "--event-kind is not supported for year-end snapshots; use --daily to filter by event kind"
            );
        }

        let events = read_events(&self.file, self.exclude_unlinked)?;
        let cgt_report = calculate_cgt(events)?;

        if self.daily {
            let entries: Vec<_> = cgt_report
                .pool_history
                .entries
                .iter()
                .filter(|entry| event_filter.matches_date(entry.date))
                .filter(|entry| {
                    event_filter
                        .asset
                        .as_ref()
                        .is_none_or(|a| entry.asset.eq_ignore_ascii_case(a))
                })
                .filter(|entry| {
                    event_filter
                        .event_kind
                        .is_none_or(|kind| kind.matches(entry.event_type))
                })
                .collect();
            if self.json {
                self.print_json_daily(&entries)?;
            } else {
                self.print_daily(&entries);
            }
        } else {
            let snapshots = filter_year_end_snapshots(
                &cgt_report.pool_history.year_end_snapshots,
                &event_filter,
            );
            if self.json {
                self.print_json_year_end(&snapshots)?;
            } else {
                self.print_year_end(&snapshots, &event_filter);
            }
        }

        Ok(())
    }

    fn print_year_end(&self, snapshots: &[YearEndSnapshotView], filter: &EventFilter) {
        let scope = pool_scope_label(filter);
        if snapshots.is_empty() {
            println!("No pool balances found matching filters ({})", scope);
            return;
        }

        println!();
        println!("POOL BALANCES ({})", scope);
        println!();

        for snapshot in snapshots {
            println!("Tax Year {}", snapshot.tax_year);
            let rows: Vec<YearEndRow> = snapshot
                .pools
                .iter()
                .map(|p| YearEndRow {
                    asset: p.asset.clone(),
                    quantity: format_quantity(p.quantity),
                    cost_gbp: format_gbp(p.cost_gbp),
                    cost_basis: format_gbp(cost_basis(p.quantity, p.cost_gbp)),
                })
                .collect();

            if rows.is_empty() {
                println!("  (no pools)");
                println!();
                continue;
            }

            let table = Table::new(rows)
                .with(Style::rounded())
                .with(Modify::new(Rows::new(1..)).with(Alignment::right()))
                .to_string();
            println!("{}", table);
            println!();
        }
    }

    fn print_daily(&self, entries: &[&PoolHistoryEntry]) {
        if entries.is_empty() {
            println!("No pool history found matching filters");
            return;
        }

        let rows: Vec<DailyRow> = entries
            .iter()
            .map(|e| DailyRow {
                date: e.date.format("%Y-%m-%d").to_string(),
                asset: e.asset.clone(),
                event: display_event_type(e.event_type, e.tag).to_string(),
                quantity: format_quantity(e.quantity),
                cost_gbp: format_gbp(e.cost_gbp),
                cost_basis: format_gbp(cost_basis(e.quantity, e.cost_gbp)),
            })
            .collect();

        println!();
        println!("POOL HISTORY");
        println!();

        let table = Table::new(rows)
            .with(Style::rounded())
            .with(Modify::new(Rows::new(1..)).with(Alignment::right()))
            .to_string();
        println!("{}", table);
    }

    fn print_json_year_end(&self, snapshots: &[YearEndSnapshotView]) -> anyhow::Result<()> {
        let output = YearEndOutput {
            year_end_snapshots: snapshots.to_vec(),
        };

        println!("{}", serde_json::to_string_pretty(&output)?);
        Ok(())
    }

    fn print_json_daily(&self, entries: &[&PoolHistoryEntry]) -> anyhow::Result<()> {
        let output = DailyOutput {
            entries: entries.iter().cloned().cloned().collect(),
        };

        println!("{}", serde_json::to_string_pretty(&output)?);
        Ok(())
    }
}

#[derive(Debug, Clone, Tabled)]
struct YearEndRow {
    #[tabled(rename = "Asset")]
    asset: String,
    #[tabled(rename = "Quantity")]
    quantity: String,
    #[tabled(rename = "Cost (GBP)")]
    cost_gbp: String,
    #[tabled(rename = "Cost Basis")]
    cost_basis: String,
}

#[derive(Debug, Clone, Tabled)]
struct DailyRow {
    #[tabled(rename = "Date")]
    date: String,
    #[tabled(rename = "Asset")]
    asset: String,
    #[tabled(rename = "Event")]
    event: String,
    #[tabled(rename = "Quantity")]
    quantity: String,
    #[tabled(rename = "Cost (GBP)")]
    cost_gbp: String,
    #[tabled(rename = "Cost Basis")]
    cost_basis: String,
}

#[derive(Debug, Clone, Serialize)]
struct YearEndSnapshotView {
    tax_year: String,
    pools: Vec<PoolState>,
}

#[derive(Debug, Serialize)]
struct YearEndOutput {
    year_end_snapshots: Vec<YearEndSnapshotView>,
}

#[derive(Debug, Serialize)]
struct DailyOutput {
    entries: Vec<PoolHistoryEntry>,
}

fn filter_year_end_snapshots(
    snapshots: &[YearEndSnapshot],
    filter: &EventFilter,
) -> Vec<YearEndSnapshotView> {
    snapshots
        .iter()
        .filter(|snapshot| {
            let snapshot_date = NaiveDate::from_ymd_opt(snapshot.tax_year.0, 4, 5).unwrap();
            filter.matches_date(snapshot_date)
        })
        .map(|snapshot| YearEndSnapshotView {
            tax_year: snapshot.tax_year.display(),
            pools: snapshot
                .pools
                .iter()
                .filter(|p| {
                    filter
                        .asset
                        .as_ref()
                        .is_none_or(|a| p.asset.eq_ignore_ascii_case(a))
                })
                .cloned()
                .collect(),
        })
        .filter(|snapshot| !snapshot.pools.is_empty())
        .collect()
}

fn pool_scope_label(filter: &EventFilter) -> String {
    match (filter.from, filter.to) {
        (None, None) => "All Years".to_string(),
        (Some(from), Some(to)) => {
            let tax_year = TaxYear::from_date(from);
            let start = NaiveDate::from_ymd_opt(tax_year.0 - 1, 4, 6).unwrap();
            let end = NaiveDate::from_ymd_opt(tax_year.0, 4, 5).unwrap();
            if from == start && to == end {
                tax_year.display()
            } else {
                format!("{} to {}", from.format("%Y-%m-%d"), to.format("%Y-%m-%d"))
            }
        }
        (Some(from), None) => format!("From {}", from.format("%Y-%m-%d")),
        (None, Some(to)) => format!("Up to {}", to.format("%Y-%m-%d")),
    }
}

fn cost_basis(quantity: Decimal, cost_gbp: Decimal) -> Decimal {
    if quantity.is_zero() {
        Decimal::ZERO
    } else {
        (cost_gbp / quantity).round_dp(2)
    }
}

fn format_gbp(amount: Decimal) -> String {
    format!("£{:.2}", amount)
}

fn format_quantity(qty: Decimal) -> String {
    let s = format!("{:.8}", qty);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}
