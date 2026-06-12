const DATA = __JSON_DATA__;

const eventById = new Map((DATA.events || []).map(ev => [ev.id, ev]));

// Asset -> asset class (lowercase), derived from events; used to filter transactions by class.
const assetClassByAsset = new Map(
    (DATA.events || []).map(e => [e.asset, (e.asset_class || '').toLowerCase()])
);

const INCOME_TAGS = new Set(['stakingreward', 'salary', 'otherincome', 'airdropincome', 'dividend', 'interest']);

let currentExpandedRow = null;
let currentExpandedTxRow = null;
let activeTab = 'transactions';
let eventsStale = false;
let transactionsStale = false;
let initialized = false;
let lastFilters = null;

const sortState = {
    events: { key: null, dir: 1 },
    transactions: { key: null, dir: 1 },
};

// Cached formatter instances: constructing Intl formatters per call is the
// dominant cost when rendering tens of thousands of rows.
const GBP_FORMAT = new Intl.NumberFormat('en-GB', { style: 'currency', currency: 'GBP' });
const QTY_FORMAT = new Intl.NumberFormat('en-GB', { maximumFractionDigits: 8 });
const COUNT_FORMAT = new Intl.NumberFormat('en-GB');
const DATETIME_FORMAT = new Intl.DateTimeFormat('en-GB', {
    year: 'numeric',
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit'
});
const DATE_FORMAT = new Intl.DateTimeFormat('en-GB', { day: '2-digit', month: 'short' });

function formatCurrency(value) {
    const num = parseFloat(value) || 0;
    return GBP_FORMAT.format(num);
}

function formatCount(value) {
    return COUNT_FORMAT.format(value);
}

function formatCompactGbp(value) {
    const sign = value < 0 ? '−' : '';
    const abs = Math.abs(value);
    if (abs >= 1e6) return `${sign}£${(abs / 1e6).toFixed(1)}M`;
    if (abs >= 1e3) return `${sign}£${(abs / 1e3).toFixed(0)}K`;
    return `${sign}£${abs.toFixed(0)}`;
}

function formatDateTime(datetime) {
    return DATETIME_FORMAT.format(new Date(datetime));
}

function formatQuantity(qty) {
    const num = parseFloat(qty) || 0;
    return QTY_FORMAT.format(num);
}

function formatRuleBadge(rule) {
    if (!rule) return '';
    const className = rule.toLowerCase().replace(/[^a-z]/g, '');
    return `<span class="rule-badge rule-${className}">${escapeHtml(rule)}</span>`;
}

function formatTag(tag) {
    if (!tag) return '';
    const labels = {
        StakingReward: 'Staking',
        OtherIncome: 'Other',
        AirdropIncome: 'Airdrop Inc',
        NoGainNoLoss: 'NGNL',
    };
    const label = labels[tag] || tag;
    const className = `tag-pill tag-${tag.toLowerCase()}`;
    return `<span class="${className}">${escapeHtml(label)}</span>`;
}

function formatEventType(eventKind, warnings) {
    const isDisposal = eventKind === 'disposal';
    const cls = isDisposal ? 'arrow-out' : 'arrow-in';
    const arrow = isDisposal ? '&#x2197;' : '&#x2198;';
    const label = isDisposal ? 'Disp.' : 'Acq.';
    const warn = warnings && warnings.length > 0 ? ' arrow-warn' : '';
    return `<span class="event-arrow ${cls}${warn}">${arrow}<small>${label}</small></span>`;
}

// Warnings are serialized internally tagged: {"type": "UnclassifiedEvent", ...fields}.
function warningTypeName(warning) {
    if (!warning) return '';
    if (typeof warning === 'string') return warning;
    return warning.type || '';
}

function hasWarningType(warnings, warningType) {
    return !!warnings && warnings.some(w => warningTypeName(w) === warningType);
}

function formatWarningDisplay(w) {
    if (typeof w === 'string') return w;
    const type = warningTypeName(w);
    if (type === 'UnclassifiedEvent') return 'Unclassified';
    if (type === 'InsufficientCostBasis') {
        if (w.available == null) return 'Insufficient Cost Basis';
        if (parseFloat(w.available) === 0) return 'No Cost Basis';
        return `Insufficient Cost Basis (${w.available}/${w.required})`;
    }
    return type;
}

function formatWarnings(warnings) {
    if (!warnings || warnings.length === 0) return '';
    return warnings.map(w => `<span class="warning-badge">${escapeHtml(formatWarningDisplay(w))}</span>`).join(' ');
}

function escapeHtml(value) {
    return String(value)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function formatValueCell(event) {
    const value = formatCurrency(event.value_gbp);
    if (!event.value_gbp_note) return value;

    const note = escapeHtml(event.value_gbp_note);
    return `
        <span class="value-with-note">
            <span>${value}</span>
            <span class="value-note" title="${note}" aria-label="${note}">i</span>
        </span>
    `;
}

function formatGainCell(e) {
    if (!e.cgt) return '<td>—</td>';
    const val = parseFloat(e.cgt.gain_gbp);
    const cls = val >= 0 ? 'gain-value' : 'loss-value';
    return `<td class="${cls}">${formatCurrency(e.cgt.gain_gbp)}</td>`;
}

function navigateToRow(tab, tbodyId, dataAttr, id, attempt = 0) {
    switchTab(tab);
    ensureRowRendered(tab, id);
    setTimeout(() => {
        const row = document.querySelector(`#${tbodyId} tr[data-${dataAttr}="${CSS.escape(String(id))}"]`);
        if (!row) {
            // The target may not exist in the current filtered view.
            if (attempt < 3) navigateToRow(tab, tbodyId, dataAttr, id, attempt + 1);
            return;
        }
        row.scrollIntoView({ behavior: 'smooth', block: 'center' });
        row.classList.remove('row-highlight');
        void row.offsetWidth;
        row.classList.add('row-highlight');
    }, 50);
}

function navigateToEvent(eventId) {
    navigateToRow('events', 'events-body', 'event-id', eventId);
}

function navigateToTransaction(txId) {
    navigateToRow('transactions', 'transactions-body', 'tx-id', txId);
}

function toggleExpandableRow(row, getCurrentExpanded, setCurrentExpanded) {
    const detailsRow = row.nextElementSibling;
    if (!detailsRow || !detailsRow.classList.contains('expandable-row')) return;

    const current = getCurrentExpanded();
    if (current && current !== detailsRow) {
        current.style.display = 'none';
        const prevBtn = current.previousElementSibling.querySelector('.expand-chevron');
        if (prevBtn) prevBtn.classList.remove('expanded');
    }

    const expanding = detailsRow.style.display === 'none';
    detailsRow.style.display = expanding ? 'table-row' : 'none';
    setCurrentExpanded(expanding ? detailsRow : null);
    const btn = row.querySelector('.expand-chevron');
    if (btn) btn.classList.toggle('expanded', expanding);
}

const TAB_SECTIONS = {
    transactions: 'transactions-section',
    events: 'events-section',
    taxyears: 'taxyears-section',
};

function switchTab(tab) {
    activeTab = tab;
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.classList.toggle('active', btn.dataset.tab === tab);
    });
    Object.entries(TAB_SECTIONS).forEach(([t, sectionId]) => {
        document.getElementById(sectionId).style.display = t === tab ? '' : 'none';
    });

    if (tab === 'events' && eventsStale && lastFilters) {
        renderEventsTable(filterEvents(DATA.events, lastFilters));
        eventsStale = false;
    }
    if (tab === 'transactions' && transactionsStale && lastFilters) {
        const filtered = filterTransactions(DATA.transactions || [], lastFilters);
        renderTransactionsTable(filtered);
        document.getElementById('transactions-count').textContent = `(${formatCount(filtered.length)})`;
        transactionsStale = false;
    }
}

function formatTransactionType(type) {
    const cls = `tx-type-${type.toLowerCase()}`;
    return `<span class="tx-type-badge ${cls}">${escapeHtml(type)}</span>`;
}

function formatAmounts(amounts) {
    if (amounts.length === 2) {
        const sold = amounts.find(a => a.label === 'Sold') || amounts[0];
        const bought = amounts.find(a => a.label === 'Bought') || amounts[1];
        return `<div class="tx-amount-flow">`
            + `<span class="tx-amount-out">${formatQuantity(sold.quantity)} ${escapeHtml(sold.asset)}</span>`
            + `<span class="tx-flow-arrow">&#x2192;</span>`
            + `<span class="tx-amount-in">${formatQuantity(bought.quantity)} ${escapeHtml(bought.asset)}</span>`
            + `</div>`;
    }
    const a = amounts[0];
    const prefix = a.label === 'Bought' || a.label === 'In' ? '+' : a.label === 'Sold' || a.label === 'Out' ? '−' : '';
    const cls = prefix === '+' ? 'tx-amount-in' : prefix === '−' ? 'tx-amount-out' : '';
    return `<div class="tx-amount-line"><span class="${cls}">${prefix}${formatQuantity(a.quantity)} ${escapeHtml(a.asset)}</span></div>`;
}

function formatFee(fee) {
    if (!fee) return '—';
    return `${formatQuantity(fee.amount)} ${escapeHtml(fee.asset)}`;
}

/* ---- Sorting ---- */

const EVENT_SORT_ACCESSORS = {
    datetime: e => e.datetime,
    tag: e => e.tag || '',
    quantity: e => parseFloat(e.quantity) || 0,
    asset: e => e.asset,
    value: e => parseFloat(e.value_gbp) || 0,
    gain: e => (e.cgt ? parseFloat(e.cgt.gain_gbp) || 0 : null),
    account: e => e.account || '',
};

const TX_SORT_ACCESSORS = {
    datetime: tx => tx.datetime,
    type: tx => tx.transaction_type,
    tag: tx => tx.tag || '',
    account: tx => tx.account || '',
};

function sortRows(rows, sort, accessors) {
    const accessor = sort.key && accessors[sort.key];
    if (!accessor) return rows;
    return rows.slice().sort((a, b) => {
        const va = accessor(a);
        const vb = accessor(b);
        if (va == null && vb == null) return 0;
        if (va == null) return 1; // nulls last regardless of direction
        if (vb == null) return -1;
        if (va < vb) return -sort.dir;
        if (va > vb) return sort.dir;
        return 0;
    });
}

function updateSortIndicators(tableId, sort) {
    document.querySelectorAll(`#${tableId} th[data-sort]`).forEach(th => {
        th.classList.toggle('sorted-asc', th.dataset.sort === sort.key && sort.dir === 1);
        th.classList.toggle('sorted-desc', th.dataset.sort === sort.key && sort.dir === -1);
    });
}

function onSortClick(tableKey, tableId, key) {
    const sort = sortState[tableKey];
    if (sort.key === key) {
        sort.dir = -sort.dir;
    } else {
        sort.key = key;
        sort.dir = 1;
    }
    updateSortIndicators(tableId, sort);
    applyFilters();
}

function initSorting() {
    document.querySelectorAll('#events-table th[data-sort]').forEach(th => {
        th.addEventListener('click', () => onSortClick('events', 'events-table', th.dataset.sort));
    });
    document.querySelectorAll('#transactions-table th[data-sort]').forEach(th => {
        th.addEventListener('click', () => onSortClick('transactions', 'transactions-table', th.dataset.sort));
    });
}

/* ---- Windowed row rendering ----
   Tables can hold tens of thousands of rows; rendering them all up front
   makes every filter change take seconds. Instead render a window and append
   further chunks as the table container scrolls near the bottom. Navigation
   force-renders up to its target row first. */

const RENDER_CHUNK = 2500;

const tableRenderState = {
    events: { rows: [], buildRow: null, rendered: 0, tbodyId: 'events-body', getId: e => String(e.id) },
    transactions: { rows: [], buildRow: null, rendered: 0, tbodyId: 'transactions-body', getId: tx => String(tx.id) },
};

function setTableRows(key, rows, buildRow, colspan, emptyMessage) {
    const st = tableRenderState[key];
    st.rows = rows;
    st.buildRow = buildRow;
    st.rendered = 0;
    const tbody = document.getElementById(st.tbodyId);
    if (rows.length === 0) {
        tbody.innerHTML = emptyStateRow(colspan, emptyMessage);
        return;
    }
    tbody.innerHTML = '';
    appendRowChunk(key, RENDER_CHUNK);
}

function appendRowChunk(key, count) {
    const st = tableRenderState[key];
    if (st.rendered >= st.rows.length) return;
    const end = Math.min(st.rendered + count, st.rows.length);
    let html = '';
    for (let i = st.rendered; i < end; i++) html += st.buildRow(st.rows[i]);
    document.getElementById(st.tbodyId).insertAdjacentHTML('beforeend', html);
    st.rendered = end;
}

function ensureRowRendered(key, id) {
    const st = tableRenderState[key];
    if (st.rendered >= st.rows.length) return;
    const target = String(id);
    const idx = st.rows.findIndex(r => st.getId(r) === target);
    if (idx >= st.rendered) appendRowChunk(key, idx - st.rendered + RENDER_CHUNK);
}

function initInfiniteScroll() {
    Object.keys(tableRenderState).forEach(key => {
        const container = document.getElementById(tableRenderState[key].tbodyId).closest('.table-container');
        container.addEventListener('scroll', () => {
            if (container.scrollTop + container.clientHeight > container.scrollHeight - 2000) {
                appendRowChunk(key, RENDER_CHUNK);
            }
        });
    });
}

/* ---- Transactions table ---- */

function emptyStateRow(colspan, message) {
    return `<tr class="empty-row"><td colspan="${colspan}">`
        + `<div class="empty-state"><span class="empty-state-title">${message}</span>`
        + `<span class="empty-state-hint">Try widening the date range or resetting filters</span></div>`
        + `</td></tr>`;
}

// A label/value block inside an expanded row card; omitted when the value is empty.
function cardField(label, valueHtml) {
    if (!valueHtml) return '';
    return `<div class="card-field">`
        + `<span class="card-field-label">${label}</span>`
        + `<span class="card-field-value">${valueHtml}</span>`
        + `</div>`;
}

function buildTransactionRow(tx) {
    const hasEvents = tx.event_ids && tx.event_ids.length > 0;

    let html = `<tr class="tx-row" data-tx-id="${escapeHtml(tx.id)}" data-expandable="1">`
        + `<td><span class="expand-chevron"></span></td>`
        + `<td class="date-cell">${formatDateTime(tx.datetime)}</td>`
        + `<td>${formatTransactionType(tx.transaction_type)}</td>`
        + `<td>${formatTag(tx.tag)}</td>`
        + `<td class="tx-amounts">${formatAmounts(tx.amounts)}</td>`
        + `<td class="tx-fee">${formatFee(tx.fee)}</td>`
        + `<td>${escapeHtml(tx.account || '')}</td>`
        + `</tr>`;

    const fields = cardField('Description', escapeHtml(tx.description || ''))
        + cardField('Transaction ID', `<span class="mono">${escapeHtml(tx.id)}</span>`)
        + cardField('Tax Year', escapeHtml(tx.tax_year));

    let subtable = '';
    if (hasEvents) {
        const eventRows = tx.event_ids.map(id => {
            const e = eventById.get(id);
            if (!e) return '';
            return `<tr class="tx-event-detail-row" data-nav-event="${id}">`
                + `<td>${formatEventType(e.event_kind, e.warnings)}</td>`
                + `<td>${formatTag(e.tag)}</td>`
                + `<td>${formatQuantity(e.quantity)}</td>`
                + `<td>${escapeHtml(e.asset)}</td>`
                + `<td>${formatValueCell(e)}</td>`
                + formatGainCell(e)
                + `<td>${escapeHtml(e.description || '')} ${formatWarnings(e.warnings)}</td>`
                + `</tr>`;
        }).join('');

        // A single event renders as one compact line: the title and column
        // headers only earn their space when there are several rows.
        const single = tx.event_ids.length === 1;
        subtable = `<div class="card-divider">Events</div>`
            + `<table class="detail-subtable${single ? ' detail-subtable-compact' : ''}">`
            + (single ? '' : `<thead><tr><th></th><th>Tag</th><th>Qty</th><th>Asset</th><th>Value</th><th>Gain/Loss</th><th>Description</th></tr></thead>`)
            + `<tbody>${eventRows}</tbody>`
            + `</table>`;
    }

    html += `<tr class="expandable-row" style="display:none"><td colspan="7">`
        + `<div class="tx-events-content">`
        + `<div class="row-card">${fields}</div>`
        + subtable
        + `</div></td></tr>`;
    return html;
}

function renderTransactionsTable(transactions) {
    currentExpandedTxRow = null;
    const rows = sortRows(transactions, sortState.transactions, TX_SORT_ACCESSORS);
    setTableRows('transactions', rows, buildTransactionRow, 7, 'No matching transactions');
}

function filterTransactions(transactions, filters) {
    return transactions.filter(tx => {
        if (filters.dateFrom && tx.datetime < filters.dateFrom) return false;
        if (filters.dateTo && tx.datetime > filters.dateTo + 'T23:59:59') return false;
        if (filters.taxYear && tx.tax_year !== filters.taxYear) return false;

        if (filters.assetSearch) {
            const matchesAsset = tx.amounts.some(a => a.asset.toLowerCase().includes(filters.assetSearch));
            if (!matchesAsset) return false;
        }

        const tag = (tx.tag || '').toLowerCase();
        if (tag && Object.prototype.hasOwnProperty.call(filters.tags, tag) && !filters.tags[tag]) {
            return false;
        }

        const classes = filters.classes;
        const allClasses = Object.values(classes).every(Boolean);
        if (!allClasses) {
            // Keep the transaction if any involved asset belongs to an enabled
            // class; assets with no known class always pass.
            const anyEnabled = tx.amounts.some(a => {
                const cls = assetClassByAsset.get(a.asset);
                return cls && Object.prototype.hasOwnProperty.call(classes, cls) ? classes[cls] : true;
            });
            if (!anyEnabled) return false;
        }

        return true;
    });
}

/* ---- Events table ---- */

function buildEventRow(e) {
    const isDisposal = !!e.cgt;
    const hasComponents = isDisposal && e.cgt.matching_components.length > 0;
    const rowClass = isDisposal ? 'disposal-row' : '';

    const txIcon = e.source_transaction_id
        ? `<span class="source-tx-icon" data-nav-tx="${escapeHtml(e.source_transaction_id)}" title="${escapeHtml(e.source_transaction_id)}">&#x21c4;</span>`
        : '';

    let html = `<tr class="${rowClass}" data-event-id="${e.id}" data-expandable="1">`
        + `<td><span class="expand-chevron"></span></td>`
        + `<td>${txIcon}</td>`
        + `<td class="date-cell">${formatDateTime(e.datetime)}</td>`
        + `<td>${formatEventType(e.event_kind, e.warnings)}</td>`
        + `<td>${formatTag(e.tag)}</td>`
        + `<td>${formatQuantity(e.quantity)}</td>`
        + `<td>${escapeHtml(e.asset)}</td>`
        + `<td>${formatValueCell(e)}</td>`
        + formatGainCell(e)
        + `<td>${escapeHtml(e.account || '')}</td>`
        + `</tr>`;

    const fees = parseFloat(e.fees_gbp) || 0;
    const fields = cardField('Description', escapeHtml(e.description || ''))
        + cardField('Note', escapeHtml(e.value_gbp_note || ''))
        + cardField('Event Type', escapeHtml(e.event_type))
        + cardField('Tax Year', escapeHtml(e.tax_year))
        + cardField('Fees', fees ? formatCurrency(fees) : '')
        + cardField('Source Tx', e.source_transaction_id
            ? `<button class="card-link mono" data-nav-tx="${escapeHtml(e.source_transaction_id)}">${escapeHtml(e.source_transaction_id)}</button>`
            : '')
        + cardField('Warnings', formatWarnings(e.warnings));

    let subtable = '';
    if (hasComponents) {
        const componentRows = e.cgt.matching_components.map(mc => `<tr>`
            + `<td>${formatRuleBadge(mc.rule)}</td>`
            + `<td>${formatQuantity(mc.quantity)}</td>`
            + `<td>${formatCurrency(mc.cost_gbp)}</td>`
            + `<td>${formatMatchedAcquisition(mc)}</td>`
            + `</tr>`).join('');

        subtable = `<div class="card-divider">Matching details</div>`
            + `<table class="detail-subtable">`
            + `<thead><tr><th>Rule</th><th>Quantity</th><th>Cost</th><th>Matched Acquisition</th></tr></thead>`
            + `<tbody>${componentRows}</tbody>`
            + `</table>`;
    }

    html += `<tr class="expandable-row" style="display:none"><td colspan="10">`
        + `<div class="details-content">`
        + `<div class="row-card">${fields}</div>`
        + subtable
        + `</div></td></tr>`;
    return html;
}

function renderEventsTable(events) {
    currentExpandedRow = null;
    const rows = sortRows(events, sortState.events, EVENT_SORT_ACCESSORS);
    setTableRows('events', rows, buildEventRow, 10, 'No matching events');
}

function formatMatchedAcquisition(mc) {
    if (!mc.matched_date) return '—';
    const dateStr = DATE_FORMAT.format(new Date(mc.matched_date));

    let details = `${dateStr}`;
    if (mc.matched_event_type) {
        details += ` · ${escapeHtml(mc.matched_event_type)}`;
    }
    if (mc.matched_original_qty) {
        details += ` · ${formatQuantity(mc.matched_original_qty)}`;
    }
    if (mc.matched_original_value) {
        details += ` · ${formatCurrency(mc.matched_original_value)}`;
    }

    if (mc.matched_event_id != null) {
        return `<button class="acquisition-link" data-nav-event="${mc.matched_event_id}">${details}</button>`;
    }
    return details;
}

/* ---- Row click delegation ---- */

function onTableBodyClick(event, getCurrentExpanded, setCurrentExpanded) {
    const nav = event.target.closest('[data-nav-event], [data-nav-tx]');
    if (nav) {
        event.stopPropagation();
        if (nav.dataset.navEvent != null) {
            navigateToEvent(Number(nav.dataset.navEvent));
        } else {
            navigateToTransaction(nav.dataset.navTx);
        }
        return;
    }
    const row = event.target.closest('tr[data-expandable]');
    if (row) toggleExpandableRow(row, getCurrentExpanded, setCurrentExpanded);
}

function initTableDelegation() {
    document.getElementById('transactions-body').addEventListener('click', e =>
        onTableBodyClick(e, () => currentExpandedTxRow, v => { currentExpandedTxRow = v; }));
    document.getElementById('events-body').addEventListener('click', e =>
        onTableBodyClick(e, () => currentExpandedRow, v => { currentExpandedRow = v; }));
}

/* ---- Date range ---- */

function taxYearBounds(taxYearStr) {
    const endYear = parseInt(taxYearStr.split('/')[0], 10) + 1;
    const from = `${endYear - 1}-04-06`;
    const to = `${endYear}-04-05`;
    return { from, to };
}

function computeDateRange() {
    const taxYears = DATA.summary.tax_years || [];
    if (taxYears.length === 1) {
        return taxYearBounds(taxYears[0]);
    }

    let min = null;
    let max = null;
    const consider = (dt) => {
        if (!dt) return;
        const d = dt.slice(0, 10);
        if (!min || d < min) min = d;
        if (!max || d > max) max = d;
    };
    (DATA.events || []).forEach(e => consider(e.datetime));
    (DATA.transactions || []).forEach(t => consider(t.datetime));
    return { from: min, to: max };
}

const dataDateRange = computeDateRange();
let activePreset = null;

const LAST_N_DAYS = { 'last-7d': 6, 'last-30d': 29, 'last-90d': 89 };

function toggleDatePanel() {
    document.getElementById('date-panel').classList.contains('open')
        ? closeDatePanel() : openDatePanel();
}

function openDatePanel() {
    document.getElementById('date-panel').classList.add('open');
    document.getElementById('date-range-trigger').classList.add('active');
    setTimeout(() => document.addEventListener('click', closeDatePanelOutside), 0);
}

function closeDatePanel() {
    document.getElementById('date-panel').classList.remove('open');
    document.getElementById('date-range-trigger').classList.remove('active');
    document.removeEventListener('click', closeDatePanelOutside);
}

function closeDatePanelOutside(e) {
    const wrapper = document.querySelector('.date-range-wrapper');
    if (!wrapper.contains(e.target)) closeDatePanel();
}

function updateDateRangeLabel() {
    const from = document.getElementById('date-from').value;
    const to = document.getElementById('date-to').value;
    const label = document.getElementById('date-range-label');

    if (!from && !to) { label.textContent = 'All Data'; return; }

    const fmt = (d) => {
        const dt = new Date(d + 'T00:00:00');
        return dt.toLocaleDateString('en-GB', { day: '2-digit', month: 'short', year: '2-digit' });
    };
    label.textContent = `${from ? fmt(from) : '…'} — ${to ? fmt(to) : '…'}`;
}

function updatePresetHighlight() {
    document.querySelectorAll('.date-preset').forEach(btn => {
        btn.classList.toggle('selected', btn.dataset.preset === activePreset);
    });
}

// Local-date ISO string (toISOString would shift across midnight in non-UTC timezones).
function toLocalISO(d) {
    const m = String(d.getMonth() + 1).padStart(2, '0');
    const day = String(d.getDate()).padStart(2, '0');
    return `${d.getFullYear()}-${m}-${day}`;
}

function selectPreset(preset, skipApply) {
    const today = new Date();
    let from, to, taxYear = '';

    if (preset === 'all') {
        from = dataDateRange.from || '';
        to = dataDateRange.to || '';
    } else if (LAST_N_DAYS[preset] != null) {
        to = toLocalISO(today);
        const d = new Date(today); d.setDate(d.getDate() - LAST_N_DAYS[preset]);
        from = toLocalISO(d);
    } else if (preset === 'mtd') {
        from = toLocalISO(new Date(today.getFullYear(), today.getMonth(), 1));
        to = toLocalISO(today);
    } else if (preset === 'last-month') {
        const d = new Date(today.getFullYear(), today.getMonth() - 1, 1);
        from = toLocalISO(d);
        to = toLocalISO(new Date(today.getFullYear(), today.getMonth(), 0));
    } else if (preset === 'ytd') {
        from = `${today.getFullYear()}-01-01`;
        to = toLocalISO(today);
    } else if (preset === 'last-year') {
        from = `${today.getFullYear() - 1}-01-01`;
        to = `${today.getFullYear() - 1}-12-31`;
    } else if (preset.startsWith('ty:')) {
        const yearStr = preset.slice(3);
        const bounds = taxYearBounds(yearStr);
        from = bounds.from;
        to = bounds.to;
        taxYear = yearStr;
    } else {
        return;
    }

    activePreset = preset;
    document.getElementById('tax-year').value = taxYear;
    document.getElementById('date-from').value = from;
    document.getElementById('date-to').value = to;
    updateDateRangeLabel();
    updatePresetHighlight();
    if (!skipApply) applyFilters();
    closeDatePanel();
}

function onCustomDateChange() {
    activePreset = 'custom';
    document.getElementById('tax-year').value = '';
    updateDateRangeLabel();
    updatePresetHighlight();
    applyFilters();
}

function populateFilters() {
    // Build tax year preset buttons
    const taxYearContainer = document.getElementById('date-preset-tax-years');
    DATA.summary.tax_years.forEach(year => {
        const btn = document.createElement('button');
        btn.className = 'date-preset';
        btn.dataset.preset = 'ty:' + year;
        btn.textContent = year;
        btn.addEventListener('click', () => selectPreset('ty:' + year));
        taxYearContainer.appendChild(btn);
    });

    // Wire up non-tax-year preset buttons
    document.querySelectorAll('.date-preset[data-preset]').forEach(btn => {
        if (!btn.dataset.preset.startsWith('ty:')) {
            btn.addEventListener('click', () => selectPreset(btn.dataset.preset));
        }
    });

    // Set initial state
    if (DATA.summary.tax_years.length === 1) {
        selectPreset('ty:' + DATA.summary.tax_years[0]);
    } else {
        selectPreset('all');
    }
}

/* ---- Filter dropdowns ---- */

function toggleFilterDd(id) {
    const dd = document.getElementById(id);
    const panel = dd.querySelector('.filter-dd-panel');
    const trigger = dd.querySelector('.filter-dd-trigger');
    const isOpen = panel.classList.contains('open');

    // Close all open dropdowns
    document.querySelectorAll('.filter-dd-panel.open').forEach(p => {
        p.classList.remove('open');
        p.closest('.filter-dd').querySelector('.filter-dd-trigger').classList.remove('active');
    });
    document.removeEventListener('click', closeFilterDdOutside);

    if (!isOpen) {
        panel.classList.add('open');
        trigger.classList.add('active');
        setTimeout(() => document.addEventListener('click', closeFilterDdOutside), 0);
    }
}

function closeFilterDdOutside(e) {
    if (!e.target.closest('.filter-dd')) {
        document.querySelectorAll('.filter-dd-panel.open').forEach(p => {
            p.classList.remove('open');
            p.closest('.filter-dd').querySelector('.filter-dd-trigger').classList.remove('active');
        });
        document.removeEventListener('click', closeFilterDdOutside);
    }
}

function updateFilterDdStates() {
    document.querySelectorAll('.filter-dd').forEach(dd => {
        const boxes = dd.querySelectorAll('input[type="checkbox"]');
        const allChecked = Array.from(boxes).every(b => b.checked);
        dd.querySelector('.filter-dd-trigger').classList.toggle('has-filter', !allChecked);
    });
}

// Collect checkbox states from a dropdown panel, keyed by id minus its prefix
// (e.g. "tag-trade" -> tags.trade).
function collectCheckboxes(ddId, prefix) {
    const result = {};
    document.querySelectorAll(`#${ddId} input[type="checkbox"]`).forEach(cb => {
        result[cb.id.slice(prefix.length)] = cb.checked;
    });
    return result;
}

let searchDebounce = null;
function onAssetSearchInput() {
    clearTimeout(searchDebounce);
    searchDebounce = setTimeout(applyFilters, 150);
}

function applyFilters() {
    const filters = {
        dateFrom: document.getElementById('date-from').value,
        dateTo: document.getElementById('date-to').value,
        taxYear: document.getElementById('tax-year').value,
        assetSearch: document.getElementById('asset-search').value.toLowerCase(),
        types: collectCheckboxes('dd-type', 'type-'),
        tags: collectCheckboxes('dd-tag', 'tag-'),
        classes: collectCheckboxes('dd-class', 'class-'),
    };

    lastFilters = filters;

    const filteredEvents = filterEvents(DATA.events, filters);
    updateSummary(filteredEvents);
    renderTaxYearChart();

    const filteredTransactions = filterTransactions(DATA.transactions || [], filters);

    if (!initialized || activeTab === 'events') {
        renderEventsTable(filteredEvents);
        eventsStale = false;
    } else {
        eventsStale = true;
    }

    if (!initialized || activeTab === 'transactions') {
        renderTransactionsTable(filteredTransactions);
        transactionsStale = false;
    } else {
        transactionsStale = true;
    }

    initialized = true;

    document.getElementById('events-count').textContent = `(${formatCount(filteredEvents.length)})`;
    document.getElementById('transactions-count').textContent = `(${formatCount(filteredTransactions.length)})`;
    updateFilterDdStates();
}

function filterEvents(events, filters) {
    return events.filter(e => {
        if (filters.dateFrom && e.datetime < filters.dateFrom) return false;
        if (filters.dateTo && e.datetime > filters.dateTo + 'T23:59:59') return false;
        if (filters.taxYear && e.tax_year !== filters.taxYear) return false;
        if (filters.assetSearch && !e.asset.toLowerCase().includes(filters.assetSearch)) return false;

        const eventKind = (e.event_kind || '').toLowerCase();
        if (eventKind === 'acquisition' && !filters.types.acquisition) return false;
        if (eventKind === 'disposal' && !filters.types.disposal) return false;

        const tag = (e.tag || '').toLowerCase();
        if (
            tag &&
            Object.prototype.hasOwnProperty.call(filters.tags, tag) &&
            !filters.tags[tag]
        ) {
            return false;
        }

        const assetClass = e.asset_class.toLowerCase();
        if (assetClass === 'crypto' && !filters.classes.crypto) return false;
        if (assetClass === 'stock' && !filters.classes.stock) return false;
        if (assetClass === 'fiat' && !filters.classes.fiat) return false;

        return true;
    });
}

function calculateFilteredSummary(events) {
    let totalProceeds = 0;
    let totalCosts = 0;
    let totalGain = 0;
    let totalProceedsWithUnclassified = 0;
    let totalCostsWithUnclassified = 0;
    let totalGainWithUnclassified = 0;
    let totalIncome = 0;
    let totalDividendIncome = 0;
    let totalInterestIncome = 0;
    let warningCount = 0;
    let unclassifiedCount = 0;
    let costBasisWarningCount = 0;
    let disposalCount = 0;
    let incomeCount = 0;
    const classTotals = { crypto: {p:0,c:0,g:0}, stock: {p:0,c:0,g:0}, fiat: {p:0,c:0,g:0} };

    events.forEach(e => {
        if (e.cgt) {
            const proceeds = parseFloat(e.cgt.proceeds_gbp) || 0;
            // Costs include disposal fees so Proceeds − Costs = Gain,
            // matching the Rust-side Summary totals.
            const costs = (parseFloat(e.cgt.cost_gbp) || 0) + (parseFloat(e.fees_gbp) || 0);
            const gain = parseFloat(e.cgt.gain_gbp) || 0;
            const isUnclassified = hasWarningType(e.warnings, 'UnclassifiedEvent');

            totalProceedsWithUnclassified += proceeds;
            totalCostsWithUnclassified += costs;
            totalGainWithUnclassified += gain;

            if (!isUnclassified) {
                totalProceeds += proceeds;
                totalCosts += costs;
                totalGain += gain;

                const cls = (e.asset_class || '').toLowerCase();
                if (classTotals[cls]) {
                    classTotals[cls].p += proceeds;
                    classTotals[cls].c += costs;
                    classTotals[cls].g += gain;
                }
            }

            disposalCount++;
        }

        if (e.warnings && e.warnings.length > 0) {
            warningCount++;
            if (hasWarningType(e.warnings, 'UnclassifiedEvent')) unclassifiedCount++;
            if (hasWarningType(e.warnings, 'InsufficientCostBasis'))
                costBasisWarningCount++;
        }

        const tag = (e.tag || '').toLowerCase();
        const valueGbp = parseFloat(e.value_gbp) || 0;

        if (INCOME_TAGS.has(tag) && e.event_kind === 'acquisition') {
            totalIncome += valueGbp;
            incomeCount++;
            if (tag === 'dividend') totalDividendIncome += valueGbp;
            if (tag === 'interest') totalInterestIncome += valueGbp;
        }
    });

    return {
        totalProceeds,
        totalCosts,
        totalGain,
        totalProceedsWithUnclassified,
        totalCostsWithUnclassified,
        totalGainWithUnclassified,
        totalIncome,
        totalDividendIncome,
        totalInterestIncome,
        warningCount,
        unclassifiedCount,
        costBasisWarningCount,
        disposalCount,
        incomeCount,
        classTotals
    };
}

function updateSummary(events) {
    const summary = calculateFilteredSummary(events);

    document.getElementById('summary-proceeds').textContent = formatCurrency(summary.totalProceeds);
    document.getElementById('summary-costs').textContent = formatCurrency(summary.totalCosts);
    document.getElementById('summary-gain').textContent = formatCurrency(summary.totalGain);
    document.getElementById('summary-income').textContent = formatCurrency(summary.totalIncome);
    document.getElementById('summary-income-dividend').textContent =
        summary.totalDividendIncome ? 'Dividend: ' + formatCurrency(summary.totalDividendIncome) : '';
    document.getElementById('summary-income-interest').textContent =
        summary.totalInterestIncome ? 'Interest: ' + formatCurrency(summary.totalInterestIncome) : '';

    document.querySelector('.metric-gain').classList.toggle('negative', summary.totalGain < 0);

    // Asset class breakdown
    const breakdownEl = document.getElementById('summary-class-breakdown');
    if (summary.disposalCount > 0) {
        const ct = summary.classTotals;
        const parts = [];
        if (ct.crypto.g !== 0 || ct.crypto.p !== 0) parts.push(`Crypto: ${formatCurrency(ct.crypto.g)}`);
        if (ct.stock.g !== 0 || ct.stock.p !== 0) parts.push(`Stocks: ${formatCurrency(ct.stock.g)}`);
        if (ct.fiat.g !== 0 || ct.fiat.p !== 0) parts.push(`Fiat: ${formatCurrency(ct.fiat.g)}`);
        breakdownEl.textContent = parts.join(' · ');
    } else {
        breakdownEl.textContent = '';
    }

    // Counts
    document.getElementById('summary-counts').textContent =
        `${formatCount(events.length)} events · ${formatCount(summary.disposalCount)} disposals · ${formatCount(summary.incomeCount)} income`;

    // Warnings banner
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

    // Show "inc. unclassified" sub-values only when unclassified events exist
    // AND actually move the figure; an identical repeat is just noise.
    const incLine = (withUnclassified, base) =>
        summary.unclassifiedCount > 0 && Math.abs(withUnclassified - base) >= 0.005
            ? `inc. unclassified: ${formatCurrency(withUnclassified)}`
            : '';
    document.getElementById('summary-proceeds-inc').textContent =
        incLine(summary.totalProceedsWithUnclassified, summary.totalProceeds);
    document.getElementById('summary-costs-inc').textContent =
        incLine(summary.totalCostsWithUnclassified, summary.totalCosts);
    document.getElementById('summary-gain-inc').textContent =
        incLine(summary.totalGainWithUnclassified, summary.totalGain);
}

/* ---- Tax year chart ---- */

function renderTaxYearChart() {
    const chartEl = document.getElementById('tax-year-chart');
    const tbody = document.getElementById('taxyears-body');
    if (!chartEl || !tbody) return;

    const years = DATA.summary.tax_years || [];

    // Apply the non-date filters so the view works as a year navigator even
    // when a single year is selected.
    const f = Object.assign({}, lastFilters, { dateFrom: '', dateTo: '', taxYear: '' });
    const byYear = new Map(years.map(y => [y, { gain: 0, proceeds: 0, costs: 0, income: 0, disposals: 0 }]));
    filterEvents(DATA.events, f).forEach(e => {
        const b = byYear.get(e.tax_year);
        if (!b) return;
        if (e.cgt && !hasWarningType(e.warnings, 'UnclassifiedEvent')) {
            b.gain += parseFloat(e.cgt.gain_gbp) || 0;
            b.proceeds += parseFloat(e.cgt.proceeds_gbp) || 0;
            b.costs += (parseFloat(e.cgt.cost_gbp) || 0) + (parseFloat(e.fees_gbp) || 0);
            b.disposals++;
        }
        if (e.event_kind === 'acquisition' && INCOME_TAGS.has((e.tag || '').toLowerCase())) {
            b.income += parseFloat(e.value_gbp) || 0;
        }
    });

    const gains = [...byYear.values()].map(b => b.gain);
    const maxPos = Math.max(0, ...gains);
    const maxNeg = Math.max(0, ...gains.map(g => -g));
    const scale = 100 / ((maxPos + maxNeg) || 1);
    const baseline = maxNeg * scale;
    const selected = activePreset && activePreset.startsWith('ty:') ? activePreset.slice(3) : '';

    chartEl.innerHTML = years.map(y => {
        const b = byYear.get(y);
        const height = Math.abs(b.gain) * scale;
        const neg = b.gain < 0;
        const barStyle = neg
            ? `bottom:${baseline - height}%;height:${height}%`
            : `bottom:${baseline}%;height:${height}%`;
        const cls = neg ? 'neg' : 'pos';
        const sel = y === selected ? ' selected' : '';
        const title = `${y}\nGain: ${formatCurrency(b.gain)}\nDisposals: ${b.disposals}`
            + (b.income ? `\nIncome: ${formatCurrency(b.income)}` : '');
        return `<button class="ty-col${sel}" data-year="${escapeHtml(y)}" title="${escapeHtml(title)}">`
            + `<span class="ty-value ${cls}">${formatCompactGbp(b.gain)}</span>`
            + `<span class="ty-bar-area">`
            + `<span class="ty-baseline" style="bottom:${baseline}%"></span>`
            + `<span class="ty-bar ${cls}" style="${barStyle}"></span>`
            + `</span>`
            + `<span class="ty-label">${escapeHtml(y.slice(2))}</span>`
            + `</button>`;
    }).join('');

    tbody.innerHTML = years.map(y => {
        const b = byYear.get(y);
        const gainCls = b.disposals === 0 ? 'dim-zero' : b.gain < 0 ? 'loss-value' : 'gain-value';
        const sel = y === selected ? ' selected' : '';
        const dim = (v) => v ? '' : ' class="dim-zero"';
        return `<tr class="ty-row${sel}" data-year="${escapeHtml(y)}">`
            + `<td>${escapeHtml(y)}</td>`
            + `<td${dim(b.disposals)}>${formatCount(b.disposals)}</td>`
            + `<td${dim(b.proceeds)}>${formatCurrency(b.proceeds)}</td>`
            + `<td${dim(b.costs)}>${formatCurrency(b.costs)}</td>`
            + `<td class="${gainCls}">${formatCurrency(b.gain)}</td>`
            + `<td${dim(b.income)}>${formatCurrency(b.income)}</td>`
            + `</tr>`;
    }).join('');
}

function initTaxYearChart() {
    const section = document.getElementById('taxyears-section');
    if (!section) return;
    section.addEventListener('click', e => {
        const el = e.target.closest('[data-year]');
        if (!el) return;
        const preset = 'ty:' + el.dataset.year;
        selectPreset(activePreset === preset ? 'all' : preset);
    });
}

/* ---- Reset / init ---- */

function resetFilters() {
    if (DATA.summary.tax_years.length === 1) {
        selectPreset('ty:' + DATA.summary.tax_years[0], true);
    } else {
        selectPreset('all', true);
    }
    document.getElementById('asset-search').value = '';
    document.querySelectorAll('.filter-dd-panel input[type="checkbox"]').forEach(cb => { cb.checked = true; });
    applyFilters();
}

function init() {
    initTableDelegation();
    initInfiniteScroll();
    initSorting();
    initTaxYearChart();
    populateFilters();
}

init();
