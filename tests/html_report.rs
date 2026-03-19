//! E2E tests for the HTML report output

use std::{fs, path::PathBuf, process::Command, time::SystemTime};

fn unique_tmp_file(name: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("taxc-{name}-{nanos}.{ext}"))
}

#[test]
fn report_html_respects_from_to_and_event_kind() {
    let out = unique_tmp_file("report-filter", "html");
    let out_str = out.to_string_lossy().to_string();
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/mixed_rules.json",
            "--output",
            &out_str,
            "--from",
            "2030-01-01",
            "--event-kind",
            "acquisition",
        ])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Command failed: {:?}", output);
    let html = fs::read_to_string(&out).expect("failed reading generated HTML");

    assert!(
        html.contains("\"events\":[]"),
        "expected no events in embedded data"
    );
    assert!(
        html.contains("\"event_count\":0"),
        "expected filtered summary event_count=0"
    );

    let _ = fs::remove_file(out);
}

#[test]
fn report_html_embeds_ngnl_value_note() {
    let out = unique_tmp_file("report-ngnl-note", "html");
    let out_str = out.to_string_lossy().to_string();
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/ngnl_spouse.json",
            "--output",
            &out_str,
        ])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Command failed: {:?}", output);
    let html = fs::read_to_string(&out).expect("failed reading generated HTML");

    assert!(
        html.contains("\"value_gbp\":\"25000.00\""),
        "expected NGNL report value to use transferred cost basis"
    );
    assert!(
        html.contains("\"value_gbp_note\":\"No gain/no loss transfer: value shows transferred allowable cost basis."),
        "expected NGNL value note to be embedded in HTML data"
    );

    let _ = fs::remove_file(out);
}

/// Test that the HTML report renders correctly in a headless browser:
/// JS executes without errors, summary metrics are populated, and the events table has rows.
#[test]
fn report_html_renders_in_browser() {
    use headless_chrome::{Browser, LaunchOptions};
    use std::time::Duration;

    let out = unique_tmp_file("report-html-browser", "html");
    let out_str = out.to_string_lossy().to_string();
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/mixed_rules.json",
            "--output",
            &out_str,
        ])
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success(), "Command failed: {:?}", output);

    let browser = Browser::new(
        LaunchOptions::default_builder()
            .headless(true)
            .sandbox(false)
            .idle_browser_timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build launch options"),
    )
    .expect("Failed to launch browser");

    let tab = browser.new_tab().expect("Failed to create tab");
    let canonical = out.canonicalize().expect("Failed to canonicalize path");
    let url = format!("file://{}", canonical.display());
    tab.navigate_to(&url).expect("Failed to navigate");
    tab.wait_until_navigated()
        .expect("Failed to wait for navigation");

    // Verify the global DATA object exists and has expected structure
    let errors = tab
        .evaluate(
            r#"
            (function() {
                try {
                    if (typeof DATA === 'undefined') return 'DATA is undefined';
                    if (!DATA.events) return 'DATA.events is missing';
                    if (!DATA.summary) return 'DATA.summary is missing';
                    return '';
                } catch(e) {
                    return e.toString();
                }
            })()
            "#,
            false,
        )
        .expect("Failed to evaluate JS");
    let error_str = errors
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("no value");
    assert!(error_str.is_empty(), "JS data error: {}", error_str);

    // Verify summary metrics are populated (not still showing placeholder "—")
    let proceeds = tab
        .wait_for_element("#summary-proceeds")
        .expect("Missing #summary-proceeds")
        .get_inner_text()
        .expect("Failed to get proceeds text");
    assert_ne!(proceeds.trim(), "—", "Proceeds metric not populated");
    assert!(
        proceeds.contains("£"),
        "Proceeds should contain £ symbol, got: {}",
        proceeds
    );

    let gain = tab
        .wait_for_element("#summary-gain")
        .expect("Missing #summary-gain")
        .get_inner_text()
        .expect("Failed to get gain text");
    assert_ne!(gain.trim(), "—", "Gain metric not populated");

    // Verify events table has rows
    let row_count = tab
        .evaluate("document.querySelectorAll('#events-body tr').length", false)
        .expect("Failed to count rows");
    let count = row_count
        .value
        .as_ref()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(count > 0, "Events table should have rows, got {}", count);

    // Verify tax year preset buttons are populated in the date panel
    let preset_count = tab
        .evaluate(
            "document.querySelectorAll('#date-preset-tax-years .date-preset').length",
            false,
        )
        .expect("Failed to count tax year presets");
    let presets = preset_count
        .value
        .as_ref()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        presets > 0,
        "Date panel should have tax year preset buttons, got {}",
        presets
    );

    // Verify disposal rows have expandable CGT details
    let disposal_details = tab
        .evaluate("document.querySelectorAll('.expandable-row').length", false)
        .expect("Failed to count disposal details");
    let details_count = disposal_details
        .value
        .as_ref()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        details_count > 0,
        "Should have expandable disposal detail rows"
    );

    let _ = fs::remove_file(out);
}

/// Test that the transactions tab renders, expands to show events, and links back to the events tab
#[test]
fn report_html_transactions_tab_and_navigation() {
    use headless_chrome::{Browser, LaunchOptions};
    use std::time::Duration;

    let out = unique_tmp_file("report-html-tx-tab", "html");
    let out_str = out.to_string_lossy().to_string();
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/mixed_rules.json",
            "--output",
            &out_str,
        ])
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success(), "Command failed: {:?}", output);

    let browser = Browser::new(
        LaunchOptions::default_builder()
            .headless(true)
            .sandbox(false)
            .idle_browser_timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build launch options"),
    )
    .expect("Failed to launch browser");

    let tab = browser.new_tab().expect("Failed to create tab");
    let canonical = out.canonicalize().expect("Failed to canonicalize path");
    tab.navigate_to(&format!("file://{}", canonical.display()))
        .expect("Failed to navigate");
    tab.wait_until_navigated()
        .expect("Failed to wait for navigation");

    // 1. Transactions tab is active by default and has rows
    let result = tab
        .evaluate(
            r#"
            (function() {
                var txSection = document.getElementById('transactions-section');
                if (txSection.style.display === 'none') return 'transactions section is hidden';
                var evSection = document.getElementById('events-section');
                if (evSection.style.display !== 'none') return 'events section should be hidden by default';
                var txRows = document.querySelectorAll('#transactions-body .tx-row');
                if (txRows.length === 0) return 'no transaction rows found';
                return '';
            })()
            "#,
            false,
        )
        .expect("Failed to check transactions tab");
    let msg = result
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("no value");
    assert!(msg.is_empty(), "Transactions tab check failed: {}", msg);

    // 2. Expand a transaction row and verify events detail table appears
    let result = tab
        .evaluate(
            r#"
            (function() {
                var row = document.querySelector('#transactions-body .tx-row');
                if (!row) return 'no tx row found';
                row.click();
                var details = row.nextElementSibling;
                if (!details || !details.classList.contains('expandable-row'))
                    return 'no expandable detail row after tx row';
                if (details.style.display === 'none')
                    return 'detail row not visible after click';
                var eventTable = details.querySelector('.detail-subtable');
                if (!eventTable) return 'no detail-subtable in expanded row';
                var eventRows = eventTable.querySelectorAll('.tx-event-detail-row');
                if (eventRows.length === 0) return 'no event detail rows in expanded tx';
                return '';
            })()
            "#,
            false,
        )
        .expect("Failed to test tx expand");
    let msg = result
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("no value");
    assert!(msg.is_empty(), "Tx expand check failed: {}", msg);

    // 3. Click a generated event link to navigate to the events tab
    let result = tab
        .evaluate(
            r#"
            (function() {
                var eventLink = document.querySelector('.tx-event-detail-row');
                if (!eventLink) return 'no event detail row to click';
                eventLink.click();
                // navigateToEvent switches tab after a setTimeout,
                // so we check synchronously that switchTab was called
                var evSection = document.getElementById('events-section');
                if (evSection.style.display === 'none') return 'events section still hidden after navigation';
                var txSection = document.getElementById('transactions-section');
                if (txSection.style.display !== 'none') return 'transactions section should be hidden after navigation';
                return '';
            })()
            "#,
            false,
        )
        .expect("Failed to test event navigation");
    let msg = result
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("no value");
    assert!(msg.is_empty(), "Event navigation check failed: {}", msg);

    // 4. Events tab has source tx icons that navigate back
    let result = tab
        .evaluate(
            r#"
            (function() {
                var icons = document.querySelectorAll('.source-tx-icon');
                if (icons.length === 0) return 'no source-tx-icon found on event rows';
                icons[0].click();
                var txSection = document.getElementById('transactions-section');
                if (txSection.style.display === 'none') return 'transactions section hidden after tx icon click';
                return '';
            })()
            "#,
            false,
        )
        .expect("Failed to test source tx icon");
    let msg = result
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("no value");
    assert!(msg.is_empty(), "Source tx icon check failed: {}", msg);

    // 5. Transactions count badge is populated
    let count_text = tab
        .wait_for_element("#transactions-count")
        .expect("Missing #transactions-count")
        .get_inner_text()
        .expect("Failed to get tx count text");
    assert!(
        count_text.contains('(') && count_text.contains(')'),
        "Transactions count should be in parens, got: {}",
        count_text
    );

    let _ = fs::remove_file(out);
}

/// Test that date range filters are prepopulated with the min/max dates from the data
#[test]
fn report_html_date_range_prepopulated() {
    use headless_chrome::{Browser, LaunchOptions};
    use std::time::Duration;

    let out = unique_tmp_file("report-html-dates", "html");
    let out_str = out.to_string_lossy().to_string();
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/mixed_rules.json",
            "--output",
            &out_str,
        ])
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success(), "Command failed: {:?}", output);

    let browser = Browser::new(
        LaunchOptions::default_builder()
            .headless(true)
            .sandbox(false)
            .idle_browser_timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build launch options"),
    )
    .expect("Failed to launch browser");

    let tab = browser.new_tab().expect("Failed to create tab");
    let canonical = out.canonicalize().expect("Failed to canonicalize path");
    tab.navigate_to(&format!("file://{}", canonical.display()))
        .expect("Failed to navigate");
    tab.wait_until_navigated()
        .expect("Failed to wait for navigation");

    // Verify date-from and date-to are populated with valid dates
    let result = tab
        .evaluate(
            r#"
            (function() {
                var from = document.getElementById('date-from').value;
                var to = document.getElementById('date-to').value;
                if (!from) return 'date-from is empty';
                if (!to) return 'date-to is empty';
                if (!/^\d{4}-\d{2}-\d{2}$/.test(from)) return 'date-from not a valid date: ' + from;
                if (!/^\d{4}-\d{2}-\d{2}$/.test(to)) return 'date-to not a valid date: ' + to;
                if (from > to) return 'date-from (' + from + ') is after date-to (' + to + ')';
                return 'ok:' + from + ':' + to;
            })()
            "#,
            false,
        )
        .expect("Failed to check date filters");
    let msg = result
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("no value");
    assert!(
        msg.starts_with("ok:"),
        "Date range prepopulation failed: {}",
        msg
    );

    // Multi-year report: dates should match actual data range, tax year unselected
    let data_check = tab
        .evaluate(
            r#"
            (function() {
                var from = document.getElementById('date-from').value;
                var to = document.getElementById('date-to').value;
                var allDates = DATA.events.map(function(e) { return e.datetime.slice(0, 10); })
                    .concat((DATA.transactions || []).map(function(t) { return t.datetime.slice(0, 10); }));
                allDates.sort();
                var expectedMin = allDates[0];
                var expectedMax = allDates[allDates.length - 1];
                if (from !== expectedMin) return 'date-from ' + from + ' != expected min ' + expectedMin;
                if (to !== expectedMax) return 'date-to ' + to + ' != expected max ' + expectedMax;
                var taxYear = document.getElementById('tax-year').value;
                if (taxYear !== '') return 'tax year should be unselected for multi-year, got: ' + taxYear;
                return '';
            })()
            "#,
            false,
        )
        .expect("Failed to verify date range matches data");
    let msg = data_check
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("no value");
    assert!(msg.is_empty(), "Date range mismatch: {}", msg);

    // Verify reset restores the data range (not empty)
    let reset_check = tab
        .evaluate(
            r#"
            (function() {
                resetFilters();
                var from = document.getElementById('date-from').value;
                var to = document.getElementById('date-to').value;
                if (!from) return 'date-from empty after reset';
                if (!to) return 'date-to empty after reset';
                return '';
            })()
            "#,
            false,
        )
        .expect("Failed to check reset");
    let msg = reset_check
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("no value");
    assert!(msg.is_empty(), "Reset date range check failed: {}", msg);

    let _ = fs::remove_file(out);
}

/// Test that single tax year report prepopulates year boundaries and selects the tax year
#[test]
fn report_html_single_year_prepopulated() {
    use headless_chrome::{Browser, LaunchOptions};
    use std::time::Duration;

    let out = unique_tmp_file("report-html-single-year", "html");
    let out_str = out.to_string_lossy().to_string();
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/mixed_rules.json",
            "--year",
            "2025",
            "--output",
            &out_str,
        ])
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success(), "Command failed: {:?}", output);

    let browser = Browser::new(
        LaunchOptions::default_builder()
            .headless(true)
            .sandbox(false)
            .idle_browser_timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build launch options"),
    )
    .expect("Failed to launch browser");

    let tab = browser.new_tab().expect("Failed to create tab");
    let canonical = out.canonicalize().expect("Failed to canonicalize path");
    tab.navigate_to(&format!("file://{}", canonical.display()))
        .expect("Failed to navigate");
    tab.wait_until_navigated()
        .expect("Failed to wait for navigation");

    let result = tab
        .evaluate(
            r#"
            (function() {
                var from = document.getElementById('date-from').value;
                var to = document.getElementById('date-to').value;
                if (from !== '2024-04-06') return 'date-from should be 2024-04-06, got: ' + from;
                if (to !== '2025-04-05') return 'date-to should be 2025-04-05, got: ' + to;
                var taxYear = document.getElementById('tax-year').value;
                if (taxYear !== '2024/25') return 'tax year should be 2024/25, got: ' + taxYear;
                return '';
            })()
            "#,
            false,
        )
        .expect("Failed to check single year filters");
    let msg = result
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("no value");
    assert!(msg.is_empty(), "Single year prepopulation failed: {}", msg);

    let _ = fs::remove_file(out);
}

/// Test that changing the tax year dropdown updates date pickers and filters displayed data
#[test]
fn report_html_tax_year_changes_date_range() {
    use headless_chrome::{Browser, LaunchOptions};
    use std::time::Duration;

    let out = unique_tmp_file("report-html-year-change", "html");
    let out_str = out.to_string_lossy().to_string();
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/mixed_rules.json",
            "--output",
            &out_str,
        ])
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success(), "Command failed: {:?}", output);

    let browser = Browser::new(
        LaunchOptions::default_builder()
            .headless(true)
            .sandbox(false)
            .idle_browser_timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build launch options"),
    )
    .expect("Failed to launch browser");

    let tab = browser.new_tab().expect("Failed to create tab");
    let canonical = out.canonicalize().expect("Failed to canonicalize path");
    tab.navigate_to(&format!("file://{}", canonical.display()))
        .expect("Failed to navigate");
    tab.wait_until_navigated()
        .expect("Failed to wait for navigation");

    // Get initial counts for comparison
    let initial = tab
        .evaluate(
            r#"
            (function() {
                var txRows = document.querySelectorAll('#transactions-body .tx-row').length;
                return JSON.stringify({ txRows: txRows });
            })()
            "#,
            false,
        )
        .expect("Failed to get initial counts");
    let initial_str = initial
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("{}");
    let initial_tx: usize = serde_json::from_str::<serde_json::Value>(initial_str)
        .ok()
        .and_then(|v| v["txRows"].as_u64())
        .unwrap_or(0) as usize;
    assert!(initial_tx > 0, "Should have transaction rows initially");

    // Select a specific tax year preset and verify date pickers update
    let result = tab
        .evaluate(
            r#"
            (function() {
                var years = DATA.summary.tax_years;
                if (years.length < 2) return 'need at least 2 tax years for this test, got: ' + years.length;

                // Select the first tax year via preset
                selectPreset('ty:' + years[0]);

                var from = document.getElementById('date-from').value;
                var to = document.getElementById('date-to').value;

                // Parse expected bounds from the tax year string (e.g. "2023/24")
                var parts = years[0].split('/');
                var startYear = parseInt(parts[0], 10);
                var expectedFrom = startYear + '-04-06';
                var expectedTo = (startYear + 1) + '-04-05';

                if (from !== expectedFrom) return 'date-from should be ' + expectedFrom + ', got: ' + from;
                if (to !== expectedTo) return 'date-to should be ' + expectedTo + ', got: ' + to;

                // Check that filtering actually changed the displayed rows
                var txRows = document.querySelectorAll('#transactions-body .tx-row').length;

                // Switch to second year
                selectPreset('ty:' + years[1]);

                var from2 = document.getElementById('date-from').value;
                var to2 = document.getElementById('date-to').value;
                var parts2 = years[1].split('/');
                var startYear2 = parseInt(parts2[0], 10);
                var expectedFrom2 = startYear2 + '-04-06';
                var expectedTo2 = (startYear2 + 1) + '-04-05';

                if (from2 !== expectedFrom2) return 'date-from should be ' + expectedFrom2 + ', got: ' + from2;
                if (to2 !== expectedTo2) return 'date-to should be ' + expectedTo2 + ', got: ' + to2;

                var txRows2 = document.querySelectorAll('#transactions-body .tx-row').length;

                // Select "All data" preset and verify dates revert
                selectPreset('all');

                var fromAll = document.getElementById('date-from').value;
                var toAll = document.getElementById('date-to').value;
                var txRowsAll = document.querySelectorAll('#transactions-body .tx-row').length;

                if (!fromAll) return 'date-from empty after selecting All data';
                if (!toAll) return 'date-to empty after selecting All data';

                return JSON.stringify({
                    year1Rows: txRows,
                    year2Rows: txRows2,
                    allRows: txRowsAll
                });
            })()
            "#,
            false,
        )
        .expect("Failed to test tax year change");
    let msg = result
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("no value");

    // If it starts with '{', it's our success JSON; otherwise it's an error message
    assert!(msg.starts_with('{'), "Tax year change test failed: {}", msg);

    let counts: serde_json::Value = serde_json::from_str(msg).expect("Failed to parse counts");
    let all_rows = counts["allRows"].as_u64().unwrap_or(0) as usize;

    // "All Years" should show all rows (same as initial)
    assert_eq!(
        all_rows, initial_tx,
        "All Years should restore full row count"
    );

    // At least one year should have fewer rows than the total (proving filtering works)
    let year1_rows = counts["year1Rows"].as_u64().unwrap_or(0) as usize;
    let year2_rows = counts["year2Rows"].as_u64().unwrap_or(0) as usize;
    assert!(
        year1_rows < initial_tx || year2_rows < initial_tx,
        "At least one tax year should filter to fewer rows than total ({initial_tx}), got year1={year1_rows} year2={year2_rows}"
    );

    let _ = fs::remove_file(out);
}

/// Test that toggling the disposal filter off and on doesn't break expand/collapse
#[test]
fn report_html_expand_works_after_filter_toggle() {
    use headless_chrome::{Browser, LaunchOptions};
    use std::time::Duration;

    let out = unique_tmp_file("report-html-toggle", "html");
    let out_str = out.to_string_lossy().to_string();
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/mixed_rules.json",
            "--output",
            &out_str,
        ])
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success(), "Command failed: {:?}", output);

    let browser = Browser::new(
        LaunchOptions::default_builder()
            .headless(true)
            .sandbox(false)
            .idle_browser_timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build launch options"),
    )
    .expect("Failed to launch browser");

    let tab = browser.new_tab().expect("Failed to create tab");
    let canonical = out.canonicalize().expect("Failed to canonicalize path");
    tab.navigate_to(&format!("file://{}", canonical.display()))
        .expect("Failed to navigate");
    tab.wait_until_navigated()
        .expect("Failed to wait for navigation");

    // Toggle disposal filter off then on to trigger re-render
    tab.evaluate(
        r#"
        document.getElementById('type-disposal').click();
        document.getElementById('type-disposal').click();
        "#,
        false,
    )
    .expect("Failed to toggle filter");

    // Click on a disposal row to expand it
    let expanded = tab
        .evaluate(
            r#"
            (function() {
                var row = document.querySelector('.disposal-row');
                if (!row) return 'no disposal row found';
                row.click();
                var details = row.nextElementSibling;
                if (!details || !details.classList.contains('expandable-row'))
                    return 'no details row after disposal row';
                if (details.style.display === 'none')
                    return 'details row not visible after click';
                return '';
            })()
            "#,
            false,
        )
        .expect("Failed to test expand");
    let result = expanded
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("no value");
    assert!(
        result.is_empty(),
        "Expand failed after filter toggle: {}",
        result
    );

    let _ = fs::remove_file(out);
}
