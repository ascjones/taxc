const DATA = __JSON_DATA__;

const eventById = new Map((DATA.events || []).map(ev => [ev.id, ev]));

let currentExpandedRow = null;
let currentExpandedTxRow = null;
let activeTab = 'transactions';
let eventsStale = false;
let transactionsStale = false;
let initialized = false;
let lastFilters = null;

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
    return `<span class="${className}">${label}</span>`;
}

function formatEventType(eventKind, warnings) {
    const isDisposal = eventKind === 'disposal';
    const cls = isDisposal ? 'arrow-out' : 'arrow-in';
    const arrow = isDisposal ? '&#x2197;' : '&#x2198;';
    const label = isDisposal ? 'Disp.' : 'Acq.';
    const warn = hasWarningType(warnings, 'UnclassifiedEvent') ? ' arrow-warn' : '';
    return `<span class="event-arrow ${cls}${warn}">${arrow}<small>${label}</small></span>`;
}

function warningTypeName(warning) {
    if (!warning) return '';
    if (typeof warning === 'string') return warning;
    return Object.keys(warning)[0] || '';
}

function hasWarningType(warnings, warningType) {
    return !!warnings && warnings.some(w => warningTypeName(w) === warningType);
}

function formatWarningDisplay(w) {
    if (typeof w === 'string') return w;
    const type = Object.keys(w)[0] || '';
    if (type === 'UnclassifiedEvent') return 'Unclassified';
    if (type === 'InsufficientCostBasis') {
        const detail = w[type];
        if (detail && detail.available === '0') return 'No Cost Basis';
        if (detail) return `Insufficient Cost Basis (${detail.available}/${detail.required})`;
        return 'Insufficient Cost Basis';
    }
    return type;
}

function formatWarnings(warnings) {
    if (!warnings || warnings.length === 0) return '';
    return warnings.map(w => `<span class="warning-badge">${formatWarningDisplay(w)}</span>`).join(' ');
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
    if (!e.cgt) return '<td>\u2014</td>';
    const val = parseFloat(e.cgt.gain_gbp);
    const cls = val >= 0 ? 'gain-value' : 'loss-value';
    return `<td class="${cls}">${formatCurrency(e.cgt.gain_gbp)}</td>`;
}

function navigateToRow(tab, tbodyId, dataAttr, id) {
    switchTab(tab);
    setTimeout(() => {
        const row = document.querySelector(`#${tbodyId} tr[data-${dataAttr}="${id}"]`);
        if (row) {
            row.scrollIntoView({ behavior: 'smooth', block: 'center' });
            row.classList.remove('row-highlight');
            void row.offsetWidth;
            row.classList.add('row-highlight');
        }
    }, 50);
}

function navigateToEvent(eventId) {
    navigateToRow('events', 'events-body', 'event-id', eventId);
}

function navigateToTransaction(txId) {
    navigateToRow('transactions', 'transactions-body', 'tx-id', txId);
}

function toggleExpandableRow(row, detailsClass, getCurrentExpanded, setCurrentExpanded) {
    const detailsRow = row.nextElementSibling;
    if (!detailsRow || !detailsRow.classList.contains(detailsClass)) return;

    const current = getCurrentExpanded();
    if (current && current !== detailsRow) {
        current.style.display = 'none';
        const prevBtn = current.previousElementSibling.querySelector('.expand-chevron');
        if (prevBtn) prevBtn.classList.remove('expanded');
    }

    if (detailsRow.style.display === 'none') {
        detailsRow.style.display = 'table-row';
        setCurrentExpanded(detailsRow);
        const btn = row.querySelector('.expand-chevron');
        if (btn) btn.classList.add('expanded');
    } else {
        detailsRow.style.display = 'none';
        setCurrentExpanded(null);
        const btn = row.querySelector('.expand-chevron');
        if (btn) btn.classList.remove('expanded');
    }
}

function switchTab(tab) {
    activeTab = tab;
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.classList.toggle('active', btn.dataset.tab === tab);
    });
    document.getElementById('events-section').style.display = tab === 'events' ? '' : 'none';
    document.getElementById('transactions-section').style.display = tab === 'transactions' ? '' : 'none';

    if (tab === 'events' && eventsStale && lastFilters) {
        renderEventsTable(filterEvents(DATA.events, lastFilters));
        eventsStale = false;
    }
    if (tab === 'transactions' && transactionsStale && lastFilters) {
        const filtered = filterTransactions(DATA.transactions || [], lastFilters);
        renderTransactionsTable(filtered);
        document.getElementById('transactions-count').textContent = `(${filtered.length})`;
        transactionsStale = false;
    }
}

function formatTransactionType(type) {
    const cls = `tx-type-${type.toLowerCase()}`;
    return `<span class="tx-type-badge ${cls}">${type}</span>`;
}

function formatAmounts(amounts) {
    if (amounts.length === 2) {
        const sold = amounts.find(a => a.label === 'Sold') || amounts[0];
        const bought = amounts.find(a => a.label === 'Bought') || amounts[1];
        return `<div class="tx-amount-flow">`
            + `<span class="tx-amount-out">${formatQuantity(sold.quantity)} ${sold.asset}</span>`
            + `<span class="tx-flow-arrow">&#x2192;</span>`
            + `<span class="tx-amount-in">${formatQuantity(bought.quantity)} ${bought.asset}</span>`
            + `</div>`;
    }
    const a = amounts[0];
    const prefix = a.label === 'Bought' || a.label === 'In' ? '+' : a.label === 'Sold' || a.label === 'Out' ? '−' : '';
    const cls = prefix === '+' ? 'tx-amount-in' : prefix === '−' ? 'tx-amount-out' : '';
    return `<div class="tx-amount-line"><span class="${cls}">${prefix}${formatQuantity(a.quantity)} ${a.asset}</span></div>`;
}

function formatFee(fee) {
    if (!fee) return '\u2014';
    return `${formatQuantity(fee.amount)} ${fee.asset}`;
}

function renderTransactionsTable(transactions) {
    const tbody = document.getElementById('transactions-body');
    tbody.innerHTML = '';
    currentExpandedTxRow = null;

    transactions.forEach(tx => {
        const row = document.createElement('tr');
        row.className = 'tx-row';
        row.dataset.txId = tx.id;

        const hasEvents = tx.event_ids && tx.event_ids.length > 0;
        const expandButton = hasEvents ? '<span class="expand-chevron"></span>' : '';

        row.innerHTML = `
            <td>${expandButton}</td>
            <td>${formatDateTime(tx.datetime)}</td>
            <td>${formatTransactionType(tx.transaction_type)}</td>
            <td>${formatTag(tx.tag)}</td>
            <td class="tx-amounts">${formatAmounts(tx.amounts)}</td>
            <td class="tx-fee">${formatFee(tx.fee)}</td>
            <td>${tx.account || ''}</td>
            <td>${tx.description || ''}</td>
        `;

        if (hasEvents) {
            row.addEventListener('click', () => toggleExpandableRow(
                row, 'expandable-row',
                () => currentExpandedTxRow,
                v => { currentExpandedTxRow = v; }
            ));
        }

        tbody.appendChild(row);

        if (hasEvents) {
            const detailsRow = document.createElement('tr');
            detailsRow.className = 'expandable-row';
            detailsRow.style.display = 'none';

            const eventRows = tx.event_ids.map(id => {
                const e = eventById.get(id);
                if (!e) return '';
                return `
                    <tr class="tx-event-detail-row" onclick="event.stopPropagation(); navigateToEvent(${id})">
                        <td>${formatEventType(e.event_kind, e.warnings)}</td>
                        <td>${formatTag(e.tag)}</td>
                        <td>${formatQuantity(e.quantity)}</td>
                        <td>${e.asset}</td>
                        <td>${formatValueCell(e)}</td>
                        ${formatGainCell(e)}
                        <td>${e.description || ''} ${formatWarnings(e.warnings)}</td>
                    </tr>
                `;
            }).join('');

            detailsRow.innerHTML = `
                <td colspan="8">
                    <div class="tx-events-content">
                        <div class="expandable-title">Generated Events</div>
                        <table class="detail-subtable">
                            <thead>
                                <tr>
                                    <th></th>
                                    <th>Tag</th>
                                    <th>Qty</th>
                                    <th>Asset</th>
                                    <th>Value</th>
                                    <th>Gain/Loss</th>
                                    <th>Description</th>
                                </tr>
                            </thead>
                            <tbody>${eventRows}</tbody>
                        </table>
                    </div>
                </td>
            `;
            tbody.appendChild(detailsRow);
        }
    });
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

        return true;
    });
}

function renderEventsTable(events) {
    const tbody = document.getElementById('events-body');
    tbody.innerHTML = '';
    currentExpandedRow = null;

    events.forEach((e, idx) => {
        const isDisposal = !!e.cgt;
        const row = document.createElement('tr');
        row.className = isDisposal ? 'disposal-row' : '';
        row.dataset.eventId = String(e.id);

        let expandButton = '';
        if (isDisposal && e.cgt.matching_components.length > 0) {
            expandButton = `<span class="expand-chevron"></span>`;
        }

        const txIcon = e.source_transaction_id
            ? `<span class="source-tx-icon" onclick="event.stopPropagation(); navigateToTransaction('${escapeHtml(e.source_transaction_id)}')" title="${escapeHtml(e.source_transaction_id)}">&#x21c4;</span>`
            : '';

        row.innerHTML = `
            <td>${expandButton}</td>
            <td>${txIcon}</td>
            <td>${formatDateTime(e.datetime)}</td>
            <td>${formatEventType(e.event_kind, e.warnings)}</td>
            <td>${formatTag(e.tag)}</td>
            <td>${formatQuantity(e.quantity)}</td>
            <td>${e.asset}</td>
            <td>${formatValueCell(e)}</td>
            ${formatGainCell(e)}
            <td>${e.account || ''}</td>
            <td>${e.description || ''} ${formatWarnings(e.warnings)}</td>
        `;

        if (isDisposal) {
            row.addEventListener('click', () => toggleExpandableRow(
                row, 'expandable-row',
                () => currentExpandedRow,
                v => { currentExpandedRow = v; }
            ));
        }

        tbody.appendChild(row);

        if (isDisposal && e.cgt.matching_components.length > 0) {
            const detailsRow = document.createElement('tr');
            detailsRow.className = 'expandable-row';
            detailsRow.style.display = 'none';
            detailsRow.innerHTML = `
                <td colspan="11">
                    <div class="details-content">
                        <div class="details-header">
                            <div class="expandable-title">Matching Details</div>
                            <div>${formatWarnings(e.cgt.warnings)}</div>
                        </div>
                        <table class="detail-subtable">
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
    if (!mc.matched_date) return '\u2014';
    const date = new Date(mc.matched_date);
    const dateStr = date.toLocaleDateString('en-GB', { day: '2-digit', month: 'short' });

    let details = `${dateStr}`;
    if (mc.matched_event_type) {
        details += ` \u00b7 ${mc.matched_event_type}`;
    }
    if (mc.matched_original_qty) {
        details += ` \u00b7 ${formatQuantity(mc.matched_original_qty)}`;
    }
    if (mc.matched_original_value) {
        details += ` \u00b7 ${formatCurrency(mc.matched_original_value)}`;
    }

    if (mc.matched_row_id != null) {
        return `<a href="#row-${mc.matched_row_id}" class="acquisition-link">${details}</a>`;
    }
    return details;
}

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

function selectPreset(preset, skipApply) {
    const today = new Date();
    const toISO = (d) => d.toISOString().slice(0, 10);
    let from, to, taxYear = '';

    if (preset === 'all') {
        from = dataDateRange.from || '';
        to = dataDateRange.to || '';
    } else if (LAST_N_DAYS[preset] != null) {
        to = toISO(today);
        const d = new Date(today); d.setDate(d.getDate() - LAST_N_DAYS[preset]);
        from = toISO(d);
    } else if (preset === 'mtd') {
        from = toISO(new Date(today.getFullYear(), today.getMonth(), 1));
        to = toISO(today);
    } else if (preset === 'last-month') {
        const d = new Date(today.getFullYear(), today.getMonth() - 1, 1);
        from = toISO(d);
        to = toISO(new Date(today.getFullYear(), today.getMonth(), 0));
    } else if (preset === 'ytd') {
        from = `${today.getFullYear()}-01-01`;
        to = toISO(today);
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

function applyFilters() {
    const filters = {
        dateFrom: document.getElementById('date-from').value,
        dateTo: document.getElementById('date-to').value,
        taxYear: document.getElementById('tax-year').value,
        assetSearch: document.getElementById('asset-search').value.toLowerCase(),
        types: {
            acquisition: document.getElementById('type-acquisition').checked,
            disposal: document.getElementById('type-disposal').checked
        },
        tags: {
            trade: document.getElementById('tag-trade').checked,
            stakingreward: document.getElementById('tag-stakingreward').checked,
            salary: document.getElementById('tag-salary').checked,
            otherincome: document.getElementById('tag-otherincome').checked,
            airdrop: document.getElementById('tag-airdrop').checked,
            airdropincome: document.getElementById('tag-airdropincome').checked,
            dividend: document.getElementById('tag-dividend').checked,
            interest: document.getElementById('tag-interest').checked,
            gift: document.getElementById('tag-gift').checked,
            nogainnoloss: document.getElementById('tag-nogainnoloss').checked,
            unclassified: document.getElementById('tag-unclassified').checked
        },
        classes: {
            crypto: document.getElementById('class-crypto').checked,
            stock: document.getElementById('class-stock').checked,
            fiat: document.getElementById('class-fiat').checked
        }
    };

    lastFilters = filters;

    const filteredEvents = filterEvents(DATA.events, filters);
    updateSummary(filteredEvents);

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

    document.getElementById('events-count').textContent = `(${filteredEvents.length})`;
    document.getElementById('transactions-count').textContent = `(${filteredTransactions.length})`;
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
            const costs = parseFloat(e.cgt.cost_gbp) || 0;
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
        const isIncome = ['stakingreward', 'salary', 'otherincome', 'airdropincome', 'dividend', 'interest'].includes(tag);

        if (isIncome && e.event_kind === 'acquisition') {
            totalIncome += valueGbp;
            incomeCount++;
        }
        if (tag === 'dividend' && e.event_kind === 'acquisition') {
            totalDividendIncome += valueGbp;
        }
        if (tag === 'interest' && e.event_kind === 'acquisition') {
            totalInterestIncome += valueGbp;
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
        'Dividend: ' + formatCurrency(summary.totalDividendIncome);
    document.getElementById('summary-income-interest').textContent =
        'Interest: ' + formatCurrency(summary.totalInterestIncome);

    const gainCard = document.querySelector('.metric-gain');
    if (summary.totalGain < 0) {
        gainCard.classList.add('negative');
    } else {
        gainCard.classList.remove('negative');
    }

    // Asset class breakdown
    const breakdownEl = document.getElementById('summary-class-breakdown');
    if (summary.disposalCount > 0) {
        const ct = summary.classTotals;
        const parts = [];
        if (ct.crypto.g !== 0 || ct.crypto.p !== 0) parts.push(`Crypto: ${formatCurrency(ct.crypto.g)}`);
        if (ct.stock.g !== 0 || ct.stock.p !== 0) parts.push(`Stocks: ${formatCurrency(ct.stock.g)}`);
        if (ct.fiat.g !== 0 || ct.fiat.p !== 0) parts.push(`Fiat: ${formatCurrency(ct.fiat.g)}`);
        breakdownEl.textContent = parts.join(' \u00b7 ');
    } else {
        breakdownEl.textContent = '';
    }

    // Counts
    document.getElementById('summary-counts').textContent =
        `${events.length} events \u00b7 ${summary.disposalCount} disposals \u00b7 ${summary.incomeCount} income`;

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

    // Show "inc. unclassified" sub-values only if there are unclassified events
    if (summary.unclassifiedCount > 0) {
        document.getElementById('summary-proceeds-inc').textContent =
            `inc. unclassified: ${formatCurrency(summary.totalProceedsWithUnclassified)}`;
        document.getElementById('summary-costs-inc').textContent =
            `inc. unclassified: ${formatCurrency(summary.totalCostsWithUnclassified)}`;
        document.getElementById('summary-gain-inc').textContent =
            `inc. unclassified: ${formatCurrency(summary.totalGainWithUnclassified)}`;
    } else {
        document.getElementById('summary-proceeds-inc').textContent = '';
        document.getElementById('summary-costs-inc').textContent = '';
        document.getElementById('summary-gain-inc').textContent = '';
    }

    document.getElementById('events-count').textContent = `(${events.length})`;
}

function resetFilters() {
    if (DATA.summary.tax_years.length === 1) {
        selectPreset('ty:' + DATA.summary.tax_years[0], true);
    } else {
        selectPreset('all', true);
    }
    document.getElementById('asset-search').value = '';
    document.getElementById('type-acquisition').checked = true;
    document.getElementById('type-disposal').checked = true;
    document.getElementById('tag-trade').checked = true;
    document.getElementById('tag-stakingreward').checked = true;
    document.getElementById('tag-salary').checked = true;
    document.getElementById('tag-otherincome').checked = true;
    document.getElementById('tag-airdrop').checked = true;
    document.getElementById('tag-airdropincome').checked = true;
    document.getElementById('tag-dividend').checked = true;
    document.getElementById('tag-interest').checked = true;
    document.getElementById('tag-gift').checked = true;
    document.getElementById('tag-nogainnoloss').checked = true;
    document.getElementById('tag-unclassified').checked = true;
    document.getElementById('class-crypto').checked = true;
    document.getElementById('class-stock').checked = true;
    document.getElementById('class-fiat').checked = true;
    applyFilters();
}

function init() {
    populateFilters();
}

init();
