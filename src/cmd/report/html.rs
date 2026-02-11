//! HTML report generation.

use super::{build_report_data, EventTypeFilter};
use crate::core::{CgtReport, IncomeReport, TaxYear, TaxableEvent};

/// Generate HTML report content
pub fn generate_html(
    events: &[TaxableEvent],
    cgt_report: &CgtReport,
    income_report: &IncomeReport,
    year: Option<TaxYear>,
    asset_filter: Option<&str>,
    event_type_filter: Option<EventTypeFilter>,
) -> anyhow::Result<String> {
    let data = build_report_data(
        events,
        cgt_report,
        income_report,
        year,
        asset_filter,
        event_type_filter,
    )?;
    let json_data = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
    let js = JS.replace("__JSON_DATA__", &json_data);

    Ok(format!(
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
                <p class="sub-value" id="summary-proceeds-inc">-</p>
            </div>
            <div class="card">
                <h3>Total Costs</h3>
                <p class="value" id="summary-costs">-</p>
                <p class="sub-value" id="summary-costs-inc">-</p>
            </div>
            <div class="card gain">
                <h3>Total Gain/Loss</h3>
                <p class="value" id="summary-gain">-</p>
                <p class="sub-value" id="summary-gain-inc">-</p>
            </div>
            <div class="card">
                <h3>Staking Income</h3>
                <p class="value" id="summary-staking">-</p>
            </div>
        </section>
        <div class="warnings-banner" id="warnings-banner" style="display: none;">
            <span class="warning-icon">⚠</span>
            <span id="warning-summary"></span>
        </div>

        <section class="data-section">
            <h2>Transactions <span class="count" id="events-count"></span></h2>
            <div class="table-container">
                <table id="events-table">
                    <thead>
                        <tr>
                            <th></th>
                            <th>Date/Time</th>
                            <th>Type</th>
                            <th>Qty</th>
                            <th>Asset</th>
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
{js}
    </script>
</body>
</html>"##,
        css = CSS,
        js = js
    ))
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
    /* Matching rule colors */
    --rule-sameday: #2563eb;
    --rule-sameday-bg: #dbeafe;
    --rule-bnb: #d97706;
    --rule-bnb-bg: #fef3c7;
    --rule-pool: #6b7280;
    --rule-pool-bg: #f3f4f6;
    --rule-mixed: #7c3aed;
    --rule-mixed-bg: #ede9fe;
    /* Unclassified colors */
    --type-unclassified: #b45309;
    --type-unclassified-bg: #fef3c7;
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
}

input[type="date"],
select,
input[type="text"] {
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--gray-300);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    background: white;
}

input[type="text"] {
    min-width: 200px;
}

.checkbox-group {
    display: flex;
    gap: 1rem;
    flex-wrap: wrap;
}

.checkbox-group label {
    font-size: 0.875rem;
    color: var(--gray-700);
    display: flex;
    align-items: center;
    gap: 0.375rem;
}

.reset-btn {
    padding: 0.5rem 1rem;
    background: var(--gray-100);
    border: 1px solid var(--gray-300);
    border-radius: 0.375rem;
    color: var(--gray-700);
    cursor: pointer;
    font-size: 0.875rem;
}

.reset-btn:hover {
    background: var(--gray-200);
}

main {
    padding: 2rem;
    max-width: 1400px;
    margin: 0 auto;
}

.summary-cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 1.5rem;
    margin-bottom: 2rem;
}

.card {
    background: white;
    padding: 1.5rem;
    border-radius: 0.5rem;
    border: 1px solid var(--gray-200);
}

.card h3 {
    font-size: 0.875rem;
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

.card .sub-value {
    font-size: 0.875rem;
    color: var(--gray-500);
    margin-top: 0.25rem;
}

.card.gain .value {
    color: var(--success);
}

.card.gain.negative .value {
    color: var(--danger);
}

.warnings-banner {
    background: #fef3c7;
    color: #92400e;
    padding: 0.75rem 1rem;
    border-radius: 0.375rem;
    margin-bottom: 1.5rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
}

.data-section {
    background: white;
    border-radius: 0.5rem;
    border: 1px solid var(--gray-200);
    overflow: hidden;
}

.data-section h2 {
    font-size: 1.125rem;
    font-weight: 600;
    padding: 1rem 1.5rem;
    border-bottom: 1px solid var(--gray-200);
    display: flex;
    justify-content: space-between;
    align-items: center;
}

.count {
    font-size: 0.875rem;
    color: var(--gray-500);
    font-weight: 400;
}

.table-container {
    overflow-x: auto;
}

#events-table {
    width: 100%;
    border-collapse: collapse;
}

#events-table th,
#events-table td {
    padding: 0.75rem 1rem;
    text-align: left;
    border-bottom: 1px solid var(--gray-200);
}

#events-table th {
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--gray-500);
    background: var(--gray-50);
}

.event-type {
    display: inline-flex;
    align-items: center;
    padding: 0.25rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.75rem;
    font-weight: 500;
}

.type-acquisition {
    background: var(--type-acquisition-bg);
    color: var(--type-acquisition);
}

.type-disposal {
    background: var(--type-disposal-bg);
    color: var(--type-disposal);
}

.type-staking {
    background: var(--type-staking-bg);
    color: var(--type-staking);
}

.type-unclassified {
    background: var(--type-unclassified-bg);
    color: var(--type-unclassified);
}

.rule-badge {
    display: inline-flex;
    align-items: center;
    padding: 0.125rem 0.375rem;
    border-radius: 0.25rem;
    font-size: 0.6875rem;
    font-weight: 500;
}

.rule-sameday {
    background: var(--rule-sameday-bg);
    color: var(--rule-sameday);
}

.rule-bnb {
    background: var(--rule-bnb-bg);
    color: var(--rule-bnb);
}

.rule-pool {
    background: var(--rule-pool-bg);
    color: var(--rule-pool);
}

.rule-mixed {
    background: var(--rule-mixed-bg);
    color: var(--rule-mixed);
}

.warning-icon {
    color: #f59e0b;
}

.expand-btn {
    cursor: pointer;
    color: var(--primary);
    font-weight: 600;
    margin-right: 0.5rem;
}

.expand-btn:hover {
    color: var(--primary-dark);
}

.details-row {
    background: var(--gray-50);
}

.details-row td {
    padding: 0;
}

.details-content {
    padding: 1rem 2rem 1.5rem;
}

.details-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
}

.details-title {
    font-weight: 600;
    color: var(--gray-900);
}

.warning-badge {
    background: #fef3c7;
    color: #92400e;
    padding: 0.25rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.75rem;
    font-weight: 500;
}

.matching-table {
    width: 100%;
    border-collapse: collapse;
    margin-top: 0.5rem;
}

.matching-table th,
.matching-table td {
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid var(--gray-200);
    text-align: left;
    font-size: 0.875rem;
}

.matching-table th {
    font-size: 0.75rem;
    color: var(--gray-500);
    text-transform: uppercase;
    letter-spacing: 0.05em;
}

.matching-table tr:last-child td {
    border-bottom: none;
}

.cost-negative {
    color: var(--danger);
}

.gain-positive {
    color: var(--success);
}

.gain-negative {
    color: var(--danger);
}

.disposal-row {
    cursor: pointer;
}

.disposal-row:hover {
    background: var(--gray-50);
}

.acquisition-link {
    color: var(--primary);
    text-decoration: none;
    font-weight: 500;
}

.acquisition-link:hover {
    text-decoration: underline;
}

@media (max-width: 768px) {
    header {
        padding: 1rem;
    }

    main {
        padding: 1rem;
    }

    .summary-cards {
        grid-template-columns: 1fr;
        gap: 1rem;
    }

    .filter-row {
        flex-direction: column;
        align-items: stretch;
    }

    .date-range {
        flex-direction: column;
        align-items: stretch;
    }
}
"#;

const JS: &str = r###"
const DATA = __JSON_DATA__;

let currentExpandedRow = null;

function formatCurrency(value) {
    const num = parseFloat(value) || 0;
    return new Intl.NumberFormat('en-GB', { style: 'currency', currency: 'GBP' }).format(num);
}

function formatDateTime(datetime) {
    const date = new Date(datetime);
    return date.toLocaleString('en-GB', {
        year: 'numeric',
        month: 'short',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit'
    });
}

function formatQuantity(qty) {
    const num = parseFloat(qty) || 0;
    return num.toLocaleString('en-GB', { maximumFractionDigits: 8 });
}

function formatRuleBadge(rule) {
    if (!rule) return '';
    const className = rule.toLowerCase().replace(/[^a-z]/g, '');
    return `<span class="rule-badge rule-${className}">${rule}</span>`;
}

function formatEventType(type, warnings) {
    let className = `type-${type.toLowerCase().replace(/\s+/g, '-')}`;
    if (hasWarningType(warnings, 'UnclassifiedEvent')) {
        className = 'type-unclassified';
    }
    return `<span class="event-type ${className}">${type}</span>`;
}

function warningTypeName(warning) {
    if (!warning) return '';
    if (typeof warning === 'string') return warning;
    return warning.type || '';
}

function hasWarningType(warnings, warningType) {
    return !!warnings && warnings.some(w => warningTypeName(w) === warningType);
}

function formatWarnings(warnings) {
    if (!warnings || warnings.length === 0) return '';
    return warnings.map(w => `<span class="warning-badge">${w}</span>`).join(' ');
}

function renderEventsTable(events) {
    const tbody = document.getElementById('events-body');
    tbody.innerHTML = '';

    events.forEach((e, idx) => {
        const isDisposal = e.cgt !== null;
        const row = document.createElement('tr');
        row.className = isDisposal ? 'disposal-row' : '';

        let expandButton = '';
        if (isDisposal && e.cgt.matching_components.length > 0) {
            expandButton = `<span class="expand-btn">+</span>`;
        }

        row.innerHTML = `
            <td>${expandButton}</td>
            <td>${formatDateTime(e.datetime)}</td>
            <td>${formatEventType(e.event_type, e.warnings)}</td>
            <td>${formatQuantity(e.quantity)}</td>
            <td>${e.asset}</td>
            <td>${formatCurrency(e.value_gbp)}</td>
            <td>${isDisposal ? formatCurrency(e.cgt.gain_gbp) : '-'}</td>
            <td>${e.description || ''}</td>
        `;

        if (isDisposal) {
            row.addEventListener('click', () => toggleDetails(row, e, idx));
        }

        tbody.appendChild(row);

        if (isDisposal && e.cgt.matching_components.length > 0) {
            const detailsRow = document.createElement('tr');
            detailsRow.className = 'details-row';
            detailsRow.style.display = 'none';
            detailsRow.innerHTML = `
                <td colspan="8">
                    <div class="details-content">
                        <div class="details-header">
                            <div class="details-title">Matching Details</div>
                            <div>${formatWarnings(e.cgt.warnings)}</div>
                        </div>
                        <table class="matching-table">
                            <thead>
                                <tr>
                                    <th>Rule</th>
                                    <th>Quantity</th>
                                    <th>Cost</th>
                                    <th>Matched Acquisition</th>
                                </tr>
                            </thead>
                            <tbody>
                                ${e.cgt.matching_components.map(mc => `
                                    <tr>
                                        <td>${formatRuleBadge(mc.rule)}</td>
                                        <td>${formatQuantity(mc.quantity)}</td>
                                        <td>${formatCurrency(mc.cost_gbp)}</td>
                                        <td>${formatMatchedAcquisition(mc)}</td>
                                    </tr>
                                `).join('')}
                            </tbody>
                        </table>
                    </div>
                </td>
            `;
            tbody.appendChild(detailsRow);
        }
    });
}

function formatMatchedAcquisition(mc) {
    if (!mc.matched_date) return '-';
    const date = new Date(mc.matched_date);
    const dateStr = date.toLocaleDateString('en-GB', { day: '2-digit', month: 'short' });

    let details = `${dateStr}`;
    if (mc.matched_event_type) {
        details += ` · ${mc.matched_event_type}`;
    }
    if (mc.matched_original_qty) {
        details += ` · ${formatQuantity(mc.matched_original_qty)}`;
    }
    if (mc.matched_original_value) {
        details += ` · ${formatCurrency(mc.matched_original_value)}`;
    }

    if (mc.matched_row_id !== null) {
        return `<a href="#row-${mc.matched_row_id}" class="acquisition-link">${details}</a>`;
    }
    return details;
}

function toggleDetails(row, event, idx) {
    const detailsRow = row.nextElementSibling;
    if (!detailsRow || !detailsRow.classList.contains('details-row')) return;

    if (currentExpandedRow && currentExpandedRow !== detailsRow) {
        currentExpandedRow.style.display = 'none';
        const prevBtn = currentExpandedRow.previousElementSibling.querySelector('.expand-btn');
        if (prevBtn) prevBtn.textContent = '+';
    }

    if (detailsRow.style.display === 'none') {
        detailsRow.style.display = 'table-row';
        currentExpandedRow = detailsRow;
        const btn = row.querySelector('.expand-btn');
        if (btn) btn.textContent = '-';
    } else {
        detailsRow.style.display = 'none';
        currentExpandedRow = null;
        const btn = row.querySelector('.expand-btn');
        if (btn) btn.textContent = '+';
    }
}

function populateFilters() {
    const taxYearSelect = document.getElementById('tax-year');
    DATA.summary.tax_years.forEach(year => {
        const option = document.createElement('option');
        option.value = year;
        option.textContent = year;
        taxYearSelect.appendChild(option);
    });
}

function applyFilters() {
    const filters = {
        dateFrom: document.getElementById('date-from').value,
        dateTo: document.getElementById('date-to').value,
        taxYear: document.getElementById('tax-year').value,
        assetSearch: document.getElementById('asset-search').value.toLowerCase(),
        types: {
            acquisition: document.getElementById('type-acquisition').checked,
            disposal: document.getElementById('type-disposal').checked,
            staking: document.getElementById('type-staking').checked
        },
        classes: {
            crypto: document.getElementById('class-crypto').checked,
            stock: document.getElementById('class-stock').checked
        }
    };

    const filteredEvents = filterEvents(DATA.events, filters);
    renderEventsTable(filteredEvents);
    updateSummary(filteredEvents);
}

function filterEvents(events, filters) {
    return events.filter(e => {
        if (filters.dateFrom && e.datetime < filters.dateFrom) return false;
        if (filters.dateTo && e.datetime > filters.dateTo + 'T23:59:59') return false;
        if (filters.taxYear && e.tax_year !== filters.taxYear) return false;
        if (filters.assetSearch && !e.asset.toLowerCase().includes(filters.assetSearch)) return false;

        const type = e.event_type.toLowerCase();
        if (type.includes('acquisition') && !filters.types.acquisition) return false;
        if (type.includes('disposal') && !filters.types.disposal) return false;
        if (type.includes('staking') && !filters.types.staking) return false;

        const assetClass = e.asset_class.toLowerCase();
        if (assetClass === 'crypto' && !filters.classes.crypto) return false;
        if (assetClass === 'stock' && !filters.classes.stock) return false;

        return true;
    });
}

function calculateFilteredSummary(events) {
    let totalProceeds = 0;
    let totalCosts = 0;
    let totalGain = 0;
    let totalStaking = 0;
    let warningCount = 0;
    let unclassifiedCount = 0;
    let costBasisWarningCount = 0;

    events.forEach(e => {
        if (e.cgt) {
            totalProceeds += parseFloat(e.cgt.proceeds_gbp) || 0;
            totalCosts += parseFloat(e.cgt.cost_gbp) || 0;
            totalGain += parseFloat(e.cgt.gain_gbp) || 0;
        }

        if (e.warnings && e.warnings.length > 0) {
            warningCount++;
            if (hasWarningType(e.warnings, 'UnclassifiedEvent')) unclassifiedCount++;
            if (hasWarningType(e.warnings, 'InsufficientCostBasis'))
                costBasisWarningCount++;
        }
        if (e.event_type.toLowerCase().includes('staking')) {
            totalStaking += parseFloat(e.value_gbp) || 0;
        }
    });

    return {
        totalProceeds,
        totalCosts,
        totalGain,
        totalStaking,
        warningCount,
        unclassifiedCount,
        costBasisWarningCount
    };
}

function updateSummary(events) {
    const summary = calculateFilteredSummary(events);

    document.getElementById('summary-proceeds').textContent = formatCurrency(summary.totalProceeds);
    document.getElementById('summary-costs').textContent = formatCurrency(summary.totalCosts);
    document.getElementById('summary-gain').textContent = formatCurrency(summary.totalGain);
    document.getElementById('summary-staking').textContent = formatCurrency(summary.totalStaking);

    const gainCard = document.querySelector('.card.gain');
    if (summary.totalGain < 0) {
        gainCard.classList.add('negative');
    } else {
        gainCard.classList.remove('negative');
    }

    const warningsBanner = document.getElementById('warnings-banner');
    if (summary.warningCount > 0) {
        warningsBanner.style.display = 'flex';
        let warningText = `${summary.warningCount} events with warnings`;
        if (summary.unclassifiedCount > 0) warningText += `, ${summary.unclassifiedCount} unclassified`;
        if (summary.costBasisWarningCount > 0)
            warningText += `, ${summary.costBasisWarningCount} cost basis issues`;
        document.getElementById('warning-summary').textContent = warningText;
    } else {
        warningsBanner.style.display = 'none';
    }

    // Show "inc. unclassified" sub-values only if there are unclassified events
    if (summary.unclassifiedCount > 0) {
        document.getElementById('summary-proceeds-inc').textContent =
            `inc. unclassified: ${formatCurrency(summary.totalProceeds)}`;
        document.getElementById('summary-costs-inc').textContent =
            `inc. unclassified: ${formatCurrency(summary.totalCosts)}`;
        document.getElementById('summary-gain-inc').textContent =
            `inc. unclassified: ${formatCurrency(summary.totalGain)}`;
    } else {
        document.getElementById('summary-proceeds-inc').textContent = '';
        document.getElementById('summary-costs-inc').textContent = '';
        document.getElementById('summary-gain-inc').textContent = '';
    }

    document.getElementById('events-count').textContent = `(${events.length})`;
}

function resetFilters() {
    document.getElementById('date-from').value = '';
    document.getElementById('date-to').value = '';
    document.getElementById('tax-year').value = '';
    document.getElementById('asset-search').value = '';
    document.getElementById('type-acquisition').checked = true;
    document.getElementById('type-disposal').checked = true;
    document.getElementById('type-staking').checked = true;
    document.getElementById('class-crypto').checked = true;
    document.getElementById('class-stock').checked = true;
    applyFilters();
}

function init() {
    populateFilters();
    applyFilters();
}

init();
"###;
