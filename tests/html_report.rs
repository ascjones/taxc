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

    // Verify tax year filter is populated with options
    let option_count = tab
        .evaluate(
            "document.querySelectorAll('#tax-year option').length",
            false,
        )
        .expect("Failed to count tax year options");
    let options = option_count
        .value
        .as_ref()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        options > 1,
        "Tax year dropdown should have options beyond 'All Years', got {}",
        options
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
