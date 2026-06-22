//! Pools command - pool balances over time

use super::filter::{EventFilter, FilterArgs};
use super::format::{format_gbp, format_quantity};
use super::read_events;
use crate::core::{
    calculate_cgt, display_event_type, PoolHistoryEntry, PoolState, YearEndSnapshot,
};
use clap::Args;
use rust_decimal::Decimal;
use serde::Serialize;
use std::path::PathBuf;
use tabled::{
    builder::Builder,
    settings::{object::Rows, Alignment, Modify, Style},
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
        let scope = filter.scope_label();
        if snapshots.is_empty() {
            println!("No pool balances found matching filters ({})", scope);
            return;
        }

        println!();
        println!("POOL BALANCES ({})", scope);
        println!();

        for snapshot in snapshots {
            println!("Tax Year {}", snapshot.tax_year);
            if snapshot.pools.is_empty() {
                println!("  (no pools)");
                println!();
                continue;
            }

            let mut builder = Builder::with_capacity(snapshot.pools.len() + 1, 4);
            builder.push_record(["Asset", "Quantity", "Cost (GBP)", "Cost Basis"]);
            for pool in &snapshot.pools {
                builder.push_record([
                    pool.asset.clone(),
                    format_quantity(pool.quantity),
                    format_gbp(pool.cost_gbp),
                    format_gbp(cost_basis(pool.quantity, pool.cost_gbp)),
                ]);
            }

            let table = builder
                .build()
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

        println!();
        println!("POOL HISTORY");
        println!();

        let mut builder = Builder::with_capacity(entries.len() + 1, 6);
        builder.push_record([
            "Date",
            "Asset",
            "Event",
            "Quantity",
            "Cost (GBP)",
            "Cost Basis",
        ]);
        for entry in entries {
            builder.push_record([
                entry.date.format("%Y-%m-%d").to_string(),
                entry.asset.clone(),
                display_event_type(entry.event_type, entry.tag).to_string(),
                format_quantity(entry.quantity),
                format_gbp(entry.cost_gbp),
                format_gbp(cost_basis(entry.quantity, entry.cost_gbp)),
            ]);
        }

        let table = builder
            .build()
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
        .filter(|snapshot| filter.matches_date(snapshot.tax_year.end_date()))
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

fn cost_basis(quantity: Decimal, cost_gbp: Decimal) -> Decimal {
    if quantity.is_zero() {
        Decimal::ZERO
    } else {
        (cost_gbp / quantity).round_dp(2)
    }
}
