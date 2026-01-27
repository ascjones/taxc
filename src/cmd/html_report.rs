//! HTML report generation for tax data
//!
//! Generates a self-contained HTML file with embedded CSS/JS for interactive filtering.

use crate::cmd::events::read_events;
use crate::events::{AssetClass, EventType, TaxableEvent};
use crate::tax::cgt::{calculate_cgt, CgtReport, MatchingRule};
use crate::tax::income::{calculate_income_tax, IncomeReport};
use crate::tax::TaxYear;
use clap::Args;
use rust_decimal::Decimal;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct HtmlCommand {
    /// CSV or JSON file containing taxable events
    #[arg(short, long)]
    events: PathBuf,

    /// Tax year to filter (e.g., 2025 for 2024/25)
    #[arg(short, long)]
    year: Option<i32>,

    /// Output file path (default: opens in browser)
    #[arg(short, long)]
    output: Option<PathBuf>,
}

impl HtmlCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let tax_year = self.year.map(TaxYear);
        let (events, opening_pools) = read_events(&self.events)?;

        let cgt_report = calculate_cgt(events.clone(), opening_pools.as_ref());
        let income_report = calculate_income_tax(events.clone());

        let html = generate(&events, &cgt_report, &income_report, tax_year);

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

        Ok(())
    }
}

/// Data structure for embedding in HTML as JSON
#[derive(Serialize)]
pub struct HtmlReportData {
    pub events: Vec<EventRow>,
    pub summary: Summary,
}

#[derive(Serialize)]
pub struct EventRow {
    pub date: String,
    pub tax_year: String,
    pub event_type: String,
    pub asset: String,
    pub asset_class: String,
    pub quantity: String,
    pub value_gbp: String,
    pub fees_gbp: String,
    pub description: String,
    /// CGT details for disposal events (None for other event types)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgt: Option<CgtDetails>,
}

/// CGT details for disposal events
#[derive(Serialize)]
pub struct CgtDetails {
    pub proceeds_gbp: String,
    pub cost_gbp: String,
    pub gain_gbp: String,
    pub rule: String,
    pub matching_components: Vec<MatchingComponentRow>,
}

#[derive(Serialize)]
pub struct MatchingComponentRow {
    pub rule: String,
    pub quantity: String,
    pub cost_gbp: String,
    /// For B&B: the linked acquisition date
    pub matched_date: Option<String>,
}

#[derive(Serialize)]
pub struct Summary {
    pub total_proceeds: String,
    pub total_costs: String,
    pub total_gain: String,
    pub total_staking: String,
    pub total_dividends: String,
    pub event_count: usize,
    pub disposal_count: usize,
    pub income_count: usize,
    pub tax_years: Vec<String>,
    pub assets: Vec<String>,
    pub min_date: Option<String>,
    pub max_date: Option<String>,
}

/// Generate HTML report content
pub fn generate(
    events: &[TaxableEvent],
    cgt_report: &CgtReport,
    income_report: &IncomeReport,
    year: Option<TaxYear>,
) -> String {
    let data = build_report_data(events, cgt_report, income_report, year);
    let json_data = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>UK Tax Report</title>
    <style>
{css}
    </style>
</head>
<body>
    <header>
        <h1>UK Tax Report</h1>
        <div class="filters">
            <div class="filter-row">
                <div class="filter-group">
                    <label>Date Range</label>
                    <div class="date-range">
                        <input type="date" id="date-from" onchange="applyFilters()">
                        <span>to</span>
                        <input type="date" id="date-to" onchange="applyFilters()">
                    </div>
                </div>
                <div class="filter-group">
                    <label for="tax-year">Tax Year</label>
                    <select id="tax-year" onchange="applyFilters()">
                        <option value="">All Years</option>
                    </select>
                </div>
                <div class="filter-group">
                    <label for="asset-search">Asset</label>
                    <input type="text" id="asset-search" placeholder="Search assets..." oninput="applyFilters()">
                </div>
            </div>
            <div class="filter-row">
                <div class="filter-group">
                    <label>Event Type</label>
                    <div class="checkbox-group">
                        <label><input type="checkbox" id="type-acquisition" checked onchange="applyFilters()"> Acquisition</label>
                        <label><input type="checkbox" id="type-disposal" checked onchange="applyFilters()"> Disposal</label>
                        <label><input type="checkbox" id="type-staking" checked onchange="applyFilters()"> Staking</label>
                        <label><input type="checkbox" id="type-dividend" checked onchange="applyFilters()"> Dividend</label>
                    </div>
                </div>
                <div class="filter-group">
                    <label>Asset Class</label>
                    <div class="checkbox-group">
                        <label><input type="checkbox" id="class-crypto" checked onchange="applyFilters()"> Crypto</label>
                        <label><input type="checkbox" id="class-stock" checked onchange="applyFilters()"> Stock</label>
                    </div>
                </div>
                <button class="reset-btn" onclick="resetFilters()">Reset Filters</button>
            </div>
        </div>
    </header>

    <main>
        <section class="summary-cards">
            <div class="card">
                <h3>Total Proceeds</h3>
                <p class="value" id="summary-proceeds">-</p>
            </div>
            <div class="card">
                <h3>Total Costs</h3>
                <p class="value" id="summary-costs">-</p>
            </div>
            <div class="card gain">
                <h3>Total Gain/Loss</h3>
                <p class="value" id="summary-gain">-</p>
            </div>
            <div class="card">
                <h3>Staking Income</h3>
                <p class="value" id="summary-staking">-</p>
            </div>
            <div class="card">
                <h3>Dividend Income</h3>
                <p class="value" id="summary-dividends">-</p>
            </div>
        </section>

        <section class="data-section">
            <h2>Transactions <span class="count" id="events-count"></span></h2>
            <div class="table-container">
                <table id="events-table">
                    <thead>
                        <tr>
                            <th></th>
                            <th>Date</th>
                            <th>Tax Year</th>
                            <th>Type</th>
                            <th>Asset</th>
                            <th>Quantity</th>
                            <th>Value</th>
                            <th>Gain/Loss</th>
                            <th>Description</th>
                        </tr>
                    </thead>
                    <tbody id="events-body"></tbody>
                </table>
            </div>
        </section>
    </main>

    <script>
const DATA = {json_data};

function init() {{
    // Populate tax year dropdown
    const taxYearSelect = document.getElementById('tax-year');
    DATA.summary.tax_years.forEach(year => {{
        const opt = document.createElement('option');
        opt.value = year;
        opt.textContent = year;
        taxYearSelect.appendChild(opt);
    }});

    // Set date range to match data
    if (DATA.summary.min_date) {{
        document.getElementById('date-from').value = DATA.summary.min_date;
    }}
    if (DATA.summary.max_date) {{
        document.getElementById('date-to').value = DATA.summary.max_date;
    }}

    applyFilters();
}}

function getFilters() {{
    return {{
        dateFrom: document.getElementById('date-from').value,
        dateTo: document.getElementById('date-to').value,
        taxYear: document.getElementById('tax-year').value,
        eventTypes: {{
            Acquisition: document.getElementById('type-acquisition').checked,
            Disposal: document.getElementById('type-disposal').checked,
            StakingReward: document.getElementById('type-staking').checked,
            Dividend: document.getElementById('type-dividend').checked,
        }},
        assetClasses: {{
            Crypto: document.getElementById('class-crypto').checked,
            Stock: document.getElementById('class-stock').checked,
        }},
        assetSearch: document.getElementById('asset-search').value.toLowerCase(),
    }};
}}

function filterEvents(events, filters) {{
    return events.filter(e => {{
        // Date filter
        if (filters.dateFrom && e.date < filters.dateFrom) return false;
        if (filters.dateTo && e.date > filters.dateTo) return false;

        // Tax year filter
        if (filters.taxYear && e.tax_year !== filters.taxYear) return false;

        // Event type filter
        if (e.event_type && !filters.eventTypes[e.event_type]) return false;

        // Asset class filter
        if (e.asset_class && !filters.assetClasses[e.asset_class]) return false;

        // Asset search
        if (filters.assetSearch && !e.asset.toLowerCase().includes(filters.assetSearch)) return false;

        return true;
    }});
}}

function formatGbp(value) {{
    const num = parseFloat(value.replace(/[£,]/g, ''));
    if (isNaN(num)) return value;
    const prefix = num < 0 ? '-£' : '£';
    return prefix + Math.abs(num).toLocaleString('en-GB', {{ minimumFractionDigits: 2, maximumFractionDigits: 2 }});
}}

function getEventTypeBadgeClass(eventType) {{
    switch(eventType) {{
        case 'Acquisition': return 'badge-acquisition';
        case 'Disposal': return 'badge-disposal';
        case 'StakingReward': return 'badge-staking';
        case 'Dividend': return 'badge-dividend';
        default: return '';
    }}
}}

function getRuleBadgeClass(rule) {{
    switch(rule) {{
        case 'Same-Day': return 'badge-sameday';
        case 'B&B': return 'badge-bnb';
        case 'Pool': return 'badge-pool';
        case 'Mixed': return 'badge-mixed';
        default: return '';
    }}
}}

function getEventTypeLabel(eventType) {{
    switch(eventType) {{
        case 'StakingReward': return 'Staking';
        default: return eventType;
    }}
}}

function renderEventsTable(events) {{
    const tbody = document.getElementById('events-body');
    let html = '';

    events.forEach((e, idx) => {{
        const badgeClass = getEventTypeBadgeClass(e.event_type);
        const label = getEventTypeLabel(e.event_type);
        const hasCgt = e.cgt != null;
        const expandIcon = hasCgt ? '<span class="expand-icon">▶</span>' : '';
        const expandableClass = hasCgt ? 'expandable' : '';

        // Gain/Loss column - show CGT gain for disposals, empty for others
        let gainCell = '<td class="number">-</td>';
        if (hasCgt) {{
            const gainNum = parseFloat(e.cgt.gain_gbp.replace(/[£,]/g, ''));
            const gainClass = gainNum >= 0 ? 'gain' : 'loss';
            gainCell = `<td class="number ${{gainClass}}">${{formatGbp(e.cgt.gain_gbp)}}</td>`;
        }}

        html += `
            <tr class="${{expandableClass}}" data-idx="${{idx}}" ${{hasCgt ? `onclick="toggleCgtDetails(${{idx}})"` : ''}}>
                <td>${{expandIcon}}</td>
                <td>${{e.date}}</td>
                <td>${{e.tax_year}}</td>
                <td><span class="badge ${{badgeClass}}">${{label}}</span></td>
                <td>${{e.asset}}</td>
                <td class="number">${{e.quantity}}</td>
                <td class="number">${{formatGbp(e.value_gbp)}}</td>
                ${{gainCell}}
                <td>${{e.description || ''}}</td>
            </tr>
        `;

        // Add hidden CGT detail rows for disposals
        if (hasCgt) {{
            const ruleBadgeClass = getRuleBadgeClass(e.cgt.rule);
            html += `
                <tr class="matching-row cgt-summary" data-parent="${{idx}}" style="display: none;">
                    <td colspan="9">
                        <div class="matching-detail">
                            <span class="badge ${{ruleBadgeClass}}">${{e.cgt.rule}}</span>
                            <span><span class="label">Proceeds:</span> <span class="value">${{formatGbp(e.cgt.proceeds_gbp)}}</span></span>
                            <span><span class="label">Cost:</span> <span class="value">${{formatGbp(e.cgt.cost_gbp)}}</span></span>
                            <span><span class="label">Gain:</span> <span class="value">${{formatGbp(e.cgt.gain_gbp)}}</span></span>
                        </div>
                    </td>
                </tr>
            `;

            // Add matching component rows
            if (e.cgt.matching_components && e.cgt.matching_components.length > 0) {{
                e.cgt.matching_components.forEach(mc => {{
                    const mcBadgeClass = getRuleBadgeClass(mc.rule);
                    const linkedDate = mc.matched_date ? `<span class="linked-date">→ ${{mc.matched_date}}</span>` : '';
                    html += `
                        <tr class="matching-row" data-parent="${{idx}}" style="display: none;">
                            <td colspan="9">
                                <div class="matching-detail">
                                    <span class="badge ${{mcBadgeClass}}">${{mc.rule}}</span>
                                    <span><span class="label">Qty:</span> <span class="value">${{mc.quantity}}</span></span>
                                    <span><span class="label">Cost:</span> <span class="value">${{formatGbp(mc.cost_gbp)}}</span></span>
                                    ${{linkedDate}}
                                </div>
                            </td>
                        </tr>
                    `;
                }});
            }}
        }}
    }});

    tbody.innerHTML = html;
    document.getElementById('events-count').textContent = `(${{events.length}})`;
}}

function toggleCgtDetails(idx) {{
    const parentRow = document.querySelector(`tr[data-idx="${{idx}}"]`);
    const childRows = document.querySelectorAll(`tr[data-parent="${{idx}}"]`);

    if (childRows.length === 0) return;

    const isExpanded = parentRow.classList.contains('expanded');

    if (isExpanded) {{
        parentRow.classList.remove('expanded');
        childRows.forEach(row => row.style.display = 'none');
    }} else {{
        parentRow.classList.add('expanded');
        childRows.forEach(row => row.style.display = '');
    }}
}}

function calculateFilteredSummary(events) {{
    let proceeds = 0, costs = 0, gain = 0, staking = 0, dividends = 0;

    events.forEach(e => {{
        if (e.cgt) {{
            proceeds += parseFloat(e.cgt.proceeds_gbp.replace(/[£,]/g, '')) || 0;
            costs += parseFloat(e.cgt.cost_gbp.replace(/[£,]/g, '')) || 0;
            gain += parseFloat(e.cgt.gain_gbp.replace(/[£,]/g, '')) || 0;
        }}
        if (e.event_type === 'StakingReward') {{
            staking += parseFloat(e.value_gbp.replace(/[£,]/g, '')) || 0;
        }}
        if (e.event_type === 'Dividend') {{
            dividends += parseFloat(e.value_gbp.replace(/[£,]/g, '')) || 0;
        }}
    }});

    return {{ proceeds, costs, gain, staking, dividends }};
}}

function updateSummary(events) {{
    const summary = calculateFilteredSummary(events);

    document.getElementById('summary-proceeds').textContent = formatGbp(summary.proceeds.toString());
    document.getElementById('summary-costs').textContent = formatGbp(summary.costs.toString());

    const gainEl = document.getElementById('summary-gain');
    gainEl.textContent = formatGbp(summary.gain.toString());
    gainEl.className = 'value ' + (summary.gain >= 0 ? 'gain' : 'loss');

    document.getElementById('summary-staking').textContent = formatGbp(summary.staking.toString());
    document.getElementById('summary-dividends').textContent = formatGbp(summary.dividends.toString());
}}

function applyFilters() {{
    const filters = getFilters();
    const filteredEvents = filterEvents(DATA.events, filters);
    renderEventsTable(filteredEvents);
    updateSummary(filteredEvents);
}}

function resetFilters() {{
    document.getElementById('date-from').value = DATA.summary.min_date || '';
    document.getElementById('date-to').value = DATA.summary.max_date || '';
    document.getElementById('tax-year').value = '';
    document.getElementById('asset-search').value = '';
    document.getElementById('type-acquisition').checked = true;
    document.getElementById('type-disposal').checked = true;
    document.getElementById('type-staking').checked = true;
    document.getElementById('type-dividend').checked = true;
    document.getElementById('class-crypto').checked = true;
    document.getElementById('class-stock').checked = true;
    applyFilters();
}}

document.addEventListener('DOMContentLoaded', init);
    </script>
</body>
</html>"##,
        css = CSS,
        json_data = json_data
    )
}

fn build_report_data(
    events: &[TaxableEvent],
    cgt_report: &CgtReport,
    income_report: &IncomeReport,
    year: Option<TaxYear>,
) -> HtmlReportData {
    use std::collections::HashMap;

    // Build a map of CGT data keyed by description for disposal lookup
    let mut cgt_map: HashMap<String, &crate::tax::cgt::DisposalRecord> = HashMap::new();
    for d in &cgt_report.disposals {
        if let Some(ref desc) = d.description {
            cgt_map.insert(desc.clone(), d);
        }
    }

    // Build events list with CGT details for disposals
    let event_rows: Vec<EventRow> = events
        .iter()
        .filter(|e| year.is_none_or(|y| TaxYear::from_date(e.date) == y))
        .map(|e| {
            // Look up CGT details for disposal events
            let cgt = if e.event_type == EventType::Disposal {
                e.description.as_ref().and_then(|desc| cgt_map.get(desc)).map(|d| {
                    // Determine primary matching rule
                    let rule = if d.matching_components.is_empty() {
                        "Pool".to_string()
                    } else if d.matching_components.len() == 1 {
                        format_matching_rule(&d.matching_components[0].rule)
                    } else {
                        "Mixed".to_string()
                    };

                    // Build matching components
                    let matching_components: Vec<MatchingComponentRow> = d
                        .matching_components
                        .iter()
                        .map(|mc| MatchingComponentRow {
                            rule: format_matching_rule(&mc.rule),
                            quantity: mc.quantity.to_string(),
                            cost_gbp: format!("{:.2}", mc.cost),
                            matched_date: mc.matched_date.map(|d| d.format("%Y-%m-%d").to_string()),
                        })
                        .collect();

                    CgtDetails {
                        proceeds_gbp: format!("{:.2}", d.proceeds_gbp),
                        cost_gbp: format!("{:.2}", d.allowable_cost_gbp),
                        gain_gbp: format!("{:.2}", d.gain_gbp),
                        rule,
                        matching_components,
                    }
                })
            } else {
                None
            };

            EventRow {
                date: e.date.format("%Y-%m-%d").to_string(),
                tax_year: TaxYear::from_date(e.date).display(),
                event_type: format_event_type(&e.event_type),
                asset: e.asset.clone(),
                asset_class: format_asset_class(&e.asset_class),
                quantity: e.quantity.to_string(),
                value_gbp: format!("{:.2}", e.value_gbp),
                fees_gbp: e.fees_gbp.map(|f| format!("{:.2}", f)).unwrap_or_default(),
                description: e.description.clone().unwrap_or_default(),
                cgt,
            }
        })
        .collect();

    // Calculate summary
    let total_proceeds = cgt_report.total_proceeds(year);
    let total_costs = cgt_report.total_allowable_costs(year);
    let total_gain = cgt_report.total_gain(year);

    let total_staking: Decimal = income_report
        .staking_events
        .iter()
        .filter(|e| year.is_none_or(|y| e.tax_year == y))
        .map(|e| e.value_gbp)
        .sum();

    let total_dividends: Decimal = income_report
        .dividend_events
        .iter()
        .filter(|e| year.is_none_or(|y| e.tax_year == y))
        .map(|e| e.value_gbp)
        .sum();

    // Collect unique tax years
    let mut tax_years: Vec<String> = events
        .iter()
        .map(|e| TaxYear::from_date(e.date).display())
        .collect();
    tax_years.sort();
    tax_years.dedup();

    // Collect unique assets
    let mut assets: Vec<String> = events.iter().map(|e| e.asset.clone()).collect();
    assets.sort();
    assets.dedup();

    // Calculate date range from filtered events
    let filtered_events: Vec<_> = events
        .iter()
        .filter(|e| year.is_none_or(|y| TaxYear::from_date(e.date) == y))
        .collect();
    let min_date = filtered_events.iter().map(|e| e.date).min();
    let max_date = filtered_events.iter().map(|e| e.date).max();

    let disposal_count = event_rows.iter().filter(|e| e.cgt.is_some()).count();
    let income_count = event_rows
        .iter()
        .filter(|e| e.event_type == "StakingReward" || e.event_type == "Dividend")
        .count();

    HtmlReportData {
        events: event_rows,
        summary: Summary {
            total_proceeds: format!("{:.2}", total_proceeds),
            total_costs: format!("{:.2}", total_costs),
            total_gain: format!("{:.2}", total_gain),
            total_staking: format!("{:.2}", total_staking),
            total_dividends: format!("{:.2}", total_dividends),
            event_count: events.len(),
            disposal_count,
            income_count,
            tax_years,
            assets,
            min_date: min_date.map(|d| d.format("%Y-%m-%d").to_string()),
            max_date: max_date.map(|d| d.format("%Y-%m-%d").to_string()),
        },
    }
}

fn format_event_type(et: &EventType) -> String {
    match et {
        EventType::Acquisition => "Acquisition",
        EventType::Disposal => "Disposal",
        EventType::StakingReward => "StakingReward",
        EventType::Dividend => "Dividend",
    }
    .to_string()
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

const CSS: &str = r#"
:root {
    --primary: #2563eb;
    --primary-dark: #1d4ed8;
    --success: #16a34a;
    --danger: #dc2626;
    --gray-50: #f9fafb;
    --gray-100: #f3f4f6;
    --gray-200: #e5e7eb;
    --gray-300: #d1d5db;
    --gray-500: #6b7280;
    --gray-700: #374151;
    --gray-900: #111827;
    /* Event type colors */
    --type-acquisition: #059669;
    --type-acquisition-bg: #d1fae5;
    --type-disposal: #dc2626;
    --type-disposal-bg: #fee2e2;
    --type-staking: #7c3aed;
    --type-staking-bg: #ede9fe;
    --type-dividend: #0891b2;
    --type-dividend-bg: #cffafe;
    /* Matching rule colors */
    --rule-sameday: #2563eb;
    --rule-sameday-bg: #dbeafe;
    --rule-bnb: #d97706;
    --rule-bnb-bg: #fef3c7;
    --rule-pool: #6b7280;
    --rule-pool-bg: #f3f4f6;
    --rule-mixed: #7c3aed;
    --rule-mixed-bg: #ede9fe;
}

* {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
}

body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
    background: var(--gray-50);
    color: var(--gray-900);
    line-height: 1.5;
}

header {
    background: white;
    border-bottom: 1px solid var(--gray-200);
    padding: 1.5rem 2rem;
    position: sticky;
    top: 0;
    z-index: 100;
}

header h1 {
    font-size: 1.5rem;
    font-weight: 600;
    margin-bottom: 1rem;
    color: var(--gray-900);
}

.filters {
    display: flex;
    flex-direction: column;
    gap: 1rem;
}

.filter-row {
    display: flex;
    flex-wrap: wrap;
    gap: 1.5rem;
    align-items: flex-end;
}

.filter-group {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
}

.filter-group label:first-child {
    font-size: 0.75rem;
    font-weight: 500;
    color: var(--gray-500);
    text-transform: uppercase;
    letter-spacing: 0.05em;
}

.date-range {
    display: flex;
    align-items: center;
    gap: 0.5rem;
}

.date-range span {
    color: var(--gray-500);
    font-size: 0.875rem;
}

input[type="date"],
input[type="text"],
select {
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--gray-300);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    background: white;
    min-width: 140px;
}

input[type="date"]:focus,
input[type="text"]:focus,
select:focus {
    outline: none;
    border-color: var(--primary);
    box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.1);
}

.checkbox-group {
    display: flex;
    gap: 1rem;
}

.checkbox-group label {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    font-size: 0.875rem;
    color: var(--gray-700);
    cursor: pointer;
}

.checkbox-group input[type="checkbox"] {
    width: 1rem;
    height: 1rem;
    cursor: pointer;
}

.reset-btn {
    padding: 0.5rem 1rem;
    background: var(--gray-100);
    border: 1px solid var(--gray-300);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
    color: var(--gray-700);
    transition: all 0.15s;
}

.reset-btn:hover {
    background: var(--gray-200);
}

main {
    padding: 2rem;
    max-width: 1600px;
    margin: 0 auto;
}

.summary-cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 1rem;
    margin-bottom: 2rem;
}

.card {
    background: white;
    border-radius: 0.5rem;
    padding: 1.25rem;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.card h3 {
    font-size: 0.75rem;
    font-weight: 500;
    color: var(--gray-500);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 0.5rem;
}

.card .value {
    font-size: 1.5rem;
    font-weight: 600;
    color: var(--gray-900);
}

.card .value.gain {
    color: var(--success);
}

.card .value.loss {
    color: var(--danger);
}

.data-section {
    background: white;
    border-radius: 0.5rem;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
    margin-bottom: 1.5rem;
    overflow: hidden;
}

.data-section h2 {
    font-size: 1rem;
    font-weight: 600;
    padding: 1rem 1.25rem;
    border-bottom: 1px solid var(--gray-200);
    background: var(--gray-50);
}

.data-section h2 .count {
    font-weight: 400;
    color: var(--gray-500);
}

.table-container {
    overflow-x: auto;
}

table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.875rem;
}

thead {
    background: var(--gray-50);
    position: sticky;
    top: 0;
}

th {
    text-align: left;
    padding: 0.75rem 1rem;
    font-weight: 500;
    color: var(--gray-700);
    border-bottom: 1px solid var(--gray-200);
    white-space: nowrap;
}

td {
    padding: 0.75rem 1rem;
    border-bottom: 1px solid var(--gray-100);
    color: var(--gray-700);
}

tbody tr:hover {
    background: var(--gray-50);
}

tbody tr:nth-child(even) {
    background: var(--gray-50);
}

tbody tr:nth-child(even):hover {
    background: var(--gray-100);
}

.number {
    text-align: right;
    font-variant-numeric: tabular-nums;
}

.gain {
    color: var(--success);
    font-weight: 500;
}

.loss {
    color: var(--danger);
    font-weight: 500;
}

@media (max-width: 768px) {
    header {
        padding: 1rem;
    }

    .filter-row {
        flex-direction: column;
        align-items: stretch;
    }

    main {
        padding: 1rem;
    }

    .summary-cards {
        grid-template-columns: repeat(2, 1fr);
    }
}

/* Badge styles for event types and rules */
.badge {
    display: inline-block;
    padding: 0.125rem 0.5rem;
    border-radius: 9999px;
    font-size: 0.75rem;
    font-weight: 500;
    white-space: nowrap;
}

.badge-acquisition {
    background: var(--type-acquisition-bg);
    color: var(--type-acquisition);
}

.badge-disposal {
    background: var(--type-disposal-bg);
    color: var(--type-disposal);
}

.badge-staking {
    background: var(--type-staking-bg);
    color: var(--type-staking);
}

.badge-dividend {
    background: var(--type-dividend-bg);
    color: var(--type-dividend);
}

.badge-sameday {
    background: var(--rule-sameday-bg);
    color: var(--rule-sameday);
}

.badge-bnb {
    background: var(--rule-bnb-bg);
    color: var(--rule-bnb);
}

.badge-pool {
    background: var(--rule-pool-bg);
    color: var(--rule-pool);
}

.badge-mixed {
    background: var(--rule-mixed-bg);
    color: var(--rule-mixed);
}

/* Expandable row styles */
.expandable {
    cursor: pointer;
}

.expandable:hover {
    background: var(--gray-100) !important;
}

.expand-icon {
    display: inline-block;
    width: 1rem;
    margin-right: 0.25rem;
    text-align: center;
    transition: transform 0.15s;
}

.expanded .expand-icon {
    transform: rotate(90deg);
}

/* Matching component sub-rows */
.matching-row {
    background: var(--gray-50) !important;
}

.matching-row td {
    padding: 0.5rem 1rem;
    font-size: 0.8125rem;
    color: var(--gray-500);
    border-bottom: 1px solid var(--gray-100);
}

.matching-row td:first-child {
    padding-left: 2.5rem;
}

.matching-detail {
    display: flex;
    gap: 1.5rem;
    align-items: center;
}

.matching-detail .label {
    font-weight: 500;
    color: var(--gray-500);
}

.matching-detail .value {
    color: var(--gray-700);
}

.linked-date {
    color: var(--rule-bnb);
    font-weight: 500;
}
"#;
