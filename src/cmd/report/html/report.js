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

function formatEventType(type, tag, warnings, eventKind) {
    let className = `tag-${(tag || '').toLowerCase()}`;
    if (eventKind === 'disposal') {
        className = 'tag-disposal';
    }
    if (hasWarningType(warnings, 'UnclassifiedEvent')) {
        className = 'tag-unclassified';
    }
    return `<span class="event-type ${className}">${type}</span>`;
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

function renderEventsTable(events) {
    const tbody = document.getElementById('events-body');
    tbody.innerHTML = '';
    currentExpandedRow = null;

    events.forEach((e, idx) => {
        const isDisposal = !!e.cgt;
        const row = document.createElement('tr');
        row.className = isDisposal ? 'disposal-row' : '';

        let expandButton = '';
        if (isDisposal && e.cgt.matching_components.length > 0) {
            expandButton = `<span class="expand-chevron"></span>`;
        }

        const gainCell = isDisposal
            ? (() => {
                const val = parseFloat(e.cgt.gain_gbp);
                const cls = val >= 0 ? 'gain-value' : 'loss-value';
                return `<td class="${cls}">${formatCurrency(e.cgt.gain_gbp)}</td>`;
            })()
            : '<td>—</td>';

        row.innerHTML = `
            <td>${expandButton}</td>
            <td>${formatDateTime(e.datetime)}</td>
            <td>${formatEventType(e.event_type, e.tag, e.warnings, e.event_kind)}</td>
            <td>${formatQuantity(e.quantity)}</td>
            <td>${e.asset}</td>
            <td>${formatCurrency(e.value_gbp)}</td>
            ${gainCell}
            <td>${e.description || ''} ${formatWarnings(e.warnings)}</td>
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
    if (!mc.matched_date) return '—';
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

    if (mc.matched_row_id != null) {
        return `<a href="#row-${mc.matched_row_id}" class="acquisition-link">${details}</a>`;
    }
    return details;
}

function toggleDetails(row, event, idx) {
    const detailsRow = row.nextElementSibling;
    if (!detailsRow || !detailsRow.classList.contains('details-row')) return;

    if (currentExpandedRow && currentExpandedRow !== detailsRow) {
        currentExpandedRow.style.display = 'none';
        const prevBtn = currentExpandedRow.previousElementSibling.querySelector('.expand-chevron');
        if (prevBtn) prevBtn.classList.remove('expanded');
    }

    if (detailsRow.style.display === 'none') {
        detailsRow.style.display = 'table-row';
        currentExpandedRow = detailsRow;
        const btn = row.querySelector('.expand-chevron');
        if (btn) btn.classList.add('expanded');
    } else {
        detailsRow.style.display = 'none';
        currentExpandedRow = null;
        const btn = row.querySelector('.expand-chevron');
        if (btn) btn.classList.remove('expanded');
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
            unclassified: document.getElementById('tag-unclassified').checked
        },
        classes: {
            crypto: document.getElementById('class-crypto').checked,
            stock: document.getElementById('class-stock').checked,
            fiat: document.getElementById('class-fiat').checked
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
        breakdownEl.textContent = parts.join(' · ');
    } else {
        breakdownEl.textContent = '';
    }

    // Counts
    document.getElementById('summary-counts').textContent =
        `${events.length} events · ${summary.disposalCount} disposals · ${summary.incomeCount} income`;

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
    document.getElementById('date-from').value = '';
    document.getElementById('date-to').value = '';
    document.getElementById('tax-year').value = '';
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
    document.getElementById('tag-unclassified').checked = true;
    document.getElementById('class-crypto').checked = true;
    document.getElementById('class-stock').checked = true;
    document.getElementById('class-fiat').checked = true;
    applyFilters();
}

function init() {
    populateFilters();
    applyFilters();
}

init();
