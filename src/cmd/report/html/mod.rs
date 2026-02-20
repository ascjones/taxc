//! HTML report generation.

use super::build_report_data;
use crate::core::{CgtReport, IncomeReport, TaxYear, TaxableEvent};

const TEMPLATE: &str = include_str!("report.html");
const CSS: &str = include_str!("report.css");
const JS: &str = include_str!("report.js");

/// Generate HTML report content
pub fn generate_html(
    events: &[TaxableEvent],
    cgt_report: &CgtReport,
    income_report: &IncomeReport,
    year: Option<TaxYear>,
    asset_filter: Option<&str>,
) -> anyhow::Result<String> {
    let data = build_report_data(events, cgt_report, income_report, year, asset_filter)?;
    let json_data = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
    let js = JS.replace("__JSON_DATA__", &json_data);

    Ok(TEMPLATE.replace("__CSS__", CSS).replace("__JS__", &js))
}
