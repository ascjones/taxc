//! Pools command - pool balances over time

use crate::cmd::events::read_events;
use crate::events::EventType;
use crate::tax::cgt::{calculate_cgt, PoolHistoryEntry, PoolState, YearEndSnapshot};
use crate::tax::TaxYear;
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
    /// Events file (CSV or JSON). Reads from stdin if not specified.
    #[arg(default_value = "-")]
    file: PathBuf,

    /// Tax year to filter (e.g., 2025 for 2024/25)
    #[arg(short, long)]
    year: Option<i32>,

    /// Filter by asset (e.g., BTC, ETH)
    #[arg(short, long)]
    asset: Option<String>,

    /// Show daily time-series instead of year-end snapshots
    #[arg(long)]
    daily: bool,

    /// Output as JSON instead of formatted table
    #[arg(long)]
    json: bool,
}

impl PoolsCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let events = read_events(&self.file)?;
        let cgt_report = calculate_cgt(events);
        let tax_year = self.year.map(TaxYear);
        let asset_filter = self.asset.as_deref();

        if self.daily {
            let entries: Vec<_> =
                filter_daily_entries(&cgt_report.pool_history.entries, tax_year, asset_filter)
                    .collect();
            if self.json {
                self.print_json_daily(&entries)?;
            } else {
                self.print_daily(&entries);
            }
        } else {
            let snapshots = filter_year_end_snapshots(
                &cgt_report.pool_history.year_end_snapshots,
                tax_year,
                asset_filter,
            );
            if self.json {
                self.print_json_year_end(&snapshots)?;
            } else {
                self.print_year_end(&snapshots, tax_year);
            }
        }

        Ok(())
    }

    fn print_year_end(&self, snapshots: &[YearEndSnapshotView], year: Option<TaxYear>) {
        let year_str = year.map_or("All Years".to_string(), |y| y.display());
        if snapshots.is_empty() {
            println!("No pool balances found matching filters ({})", year_str);
            return;
        }

        println!();
        println!("POOL BALANCES ({})", year_str);
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
                event: event_type_name(e.event_type),
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

    fn print_json_year_end(&self, snapshots: &[YearEndSnapshotView]) -> color_eyre::Result<()> {
        let output = YearEndOutput {
            year_end_snapshots: snapshots.to_vec(),
        };

        println!("{}", serde_json::to_string_pretty(&output)?);
        Ok(())
    }

    fn print_json_daily(&self, entries: &[&PoolHistoryEntry]) -> color_eyre::Result<()> {
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
    year: Option<TaxYear>,
    asset_filter: Option<&str>,
) -> Vec<YearEndSnapshotView> {
    snapshots
        .iter()
        .filter(|snapshot| year.is_none_or(|y| snapshot.tax_year == y))
        .map(|snapshot| YearEndSnapshotView {
            tax_year: snapshot.tax_year.display(),
            pools: snapshot
                .pools
                .iter()
                .filter(|p| asset_filter.is_none_or(|a| p.asset.eq_ignore_ascii_case(a)))
                .cloned()
                .collect(),
        })
        .filter(|snapshot| !snapshot.pools.is_empty())
        .collect()
}

fn filter_daily_entries<'a>(
    entries: &'a [PoolHistoryEntry],
    year: Option<TaxYear>,
    asset_filter: Option<&'a str>,
) -> impl Iterator<Item = &'a PoolHistoryEntry> {
    entries.iter().filter(move |entry| {
        year.is_none_or(|y| TaxYear::from_date(entry.date) == y)
            && asset_filter.is_none_or(|a| entry.asset.eq_ignore_ascii_case(a))
    })
}

fn event_type_name(event_type: EventType) -> String {
    match event_type {
        EventType::Acquisition => "Acquisition",
        EventType::Disposal => "Disposal",
        EventType::StakingReward => "StakingReward",
        EventType::Dividend => "Dividend",
        EventType::UnclassifiedIn => "UnclassifiedIn",
        EventType::UnclassifiedOut => "UnclassifiedOut",
    }
    .to_string()
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
