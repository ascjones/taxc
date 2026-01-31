//! Validate command - surface data quality issues without generating full reports

use crate::cmd::events::read_events;
use crate::tax::cgt::{calculate_cgt, DisposalWarning};
use crate::tax::TaxYear;
use clap::Args;
use rust_decimal::Decimal;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ValidateCommand {
    /// CSV or JSON file containing taxable events
    #[arg(short, long)]
    events: PathBuf,

    /// Tax year to filter (e.g., 2025 for 2024/25)
    #[arg(short, long)]
    year: Option<i32>,

    /// Output as JSON instead of formatted text
    #[arg(long)]
    json: bool,
}

/// A validation issue for output
#[derive(Debug, Serialize)]
struct ValidationIssue {
    #[serde(rename = "type")]
    issue_type: String,
    date: String,
    asset: String,
    quantity: String,
    proceeds_gbp: String,
    message: String,
}

/// JSON output structure
#[derive(Debug, Serialize)]
struct ValidationOutput {
    tax_year: String,
    issue_count: usize,
    issues: Vec<ValidationIssue>,
}

impl ValidateCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let events = read_events(&self.events)?;
        let cgt_report = calculate_cgt(events);
        let tax_year = self.year.map(TaxYear);

        // Collect issues from disposals with warnings
        let issues: Vec<ValidationIssue> = cgt_report
            .disposals
            .iter()
            .filter(|d| tax_year.is_none_or(|y| d.tax_year == y))
            .filter(|d| d.has_warnings())
            .flat_map(|d| {
                d.warnings.iter().map(|w| ValidationIssue {
                    issue_type: warning_type_name(w),
                    date: d.date.format("%Y-%m-%d").to_string(),
                    asset: d.asset.clone(),
                    quantity: format_quantity(d.quantity),
                    proceeds_gbp: format!("{:.2}", d.proceeds_gbp),
                    message: warning_message(w, d.allowable_cost_gbp),
                })
            })
            .collect();

        if self.json {
            self.print_json(&issues, tax_year)?;
        } else {
            self.print_text(&issues, tax_year);
        }

        // Exit with code 1 if issues found
        if !issues.is_empty() {
            std::process::exit(1);
        }
        Ok(())
    }

    fn print_text(&self, issues: &[ValidationIssue], year: Option<TaxYear>) {
        let year_str = year.map_or("All Years".to_string(), |y| y.display());

        println!();
        println!("VALIDATION RESULTS ({})", year_str);
        println!();

        if issues.is_empty() {
            println!("\u{2713} No issues found.");
        } else {
            println!("\u{26A0} {} issue(s) found:", issues.len());
            println!();

            for (i, issue) in issues.iter().enumerate() {
                println!(
                    "  {}. [{}] {} Disposal of {} {} for \u{00A3}{}",
                    i + 1,
                    issue.issue_type,
                    issue.date,
                    issue.quantity,
                    issue.asset,
                    issue.proceeds_gbp
                );
                println!("     {}", issue.message);
                println!();
            }
        }
    }

    fn print_json(&self, issues: &[ValidationIssue], year: Option<TaxYear>) -> color_eyre::Result<()> {
        let year_str = year.map_or("All Years".to_string(), |y| y.display());

        let output = ValidationOutput {
            tax_year: year_str,
            issue_count: issues.len(),
            issues: issues.to_vec(),
        };

        println!("{}", serde_json::to_string_pretty(&output)?);
        Ok(())
    }
}

fn warning_type_name(warning: &DisposalWarning) -> String {
    match warning {
        DisposalWarning::Unclassified => "Unclassified".to_string(),
        DisposalWarning::InsufficientCostBasis { available, .. } => {
            if available.is_zero() {
                "NoCostBasis".to_string()
            } else {
                "InsufficientCostBasis".to_string()
            }
        }
    }
}

fn warning_message(warning: &DisposalWarning, allowable_cost: Decimal) -> String {
    match warning {
        DisposalWarning::Unclassified => {
            "Event type needs review - may be a disposal".to_string()
        }
        DisposalWarning::InsufficientCostBasis { available, required } => {
            if available.is_zero() {
                format!(
                    "No matching acquisitions found - cost basis is \u{00A3}{:.2}",
                    allowable_cost
                )
            } else {
                format!(
                    "Pool only had {} available (required {}) - partial cost basis \u{00A3}{:.2}",
                    format_quantity(*available),
                    format_quantity(*required),
                    allowable_cost
                )
            }
        }
    }
}

fn format_quantity(qty: Decimal) -> String {
    let s = format!("{:.8}", qty);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

// Allow cloning for JSON serialization
impl Clone for ValidationIssue {
    fn clone(&self) -> Self {
        ValidationIssue {
            issue_type: self.issue_type.clone(),
            date: self.date.clone(),
            asset: self.asset.clone(),
            quantity: self.quantity.clone(),
            proceeds_gbp: self.proceeds_gbp.clone(),
            message: self.message.clone(),
        }
    }
}
