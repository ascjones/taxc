//! E2E tests for report, pools, summary, and validate command functionality

mod common;
use common::run_taxc;

/// Test that the report JSON output includes mixed matching rules
#[test]
fn report_mixed_rules() {
    let output = run_taxc(&["report", "tests/data/mixed_rules.json", "--json"]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Command failed: {:?}", output);

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Invalid JSON report output");
    let events = json
        .get("events")
        .and_then(|v| v.as_array())
        .expect("Missing events array");

    let has_acquisition = events
        .iter()
        .any(|e| e.get("event_type").and_then(|v| v.as_str()) == Some("Acquisition"));
    let has_disposal = events
        .iter()
        .any(|e| e.get("event_type").and_then(|v| v.as_str()) == Some("Disposal"));
    let has_btc = events
        .iter()
        .any(|e| e.get("asset").and_then(|v| v.as_str()) == Some("BTC"));

    let mut has_same_day = false;
    let mut has_bnb = false;
    for e in events {
        if let Some(cgt) = e.get("cgt") {
            if let Some(components) = cgt.get("matching_components").and_then(|v| v.as_array()) {
                for component in components {
                    match component.get("rule").and_then(|v| v.as_str()) {
                        Some("Same-Day") => has_same_day = true,
                        Some("B&B") => has_bnb = true,
                        _ => {}
                    }
                }
            }
        }
    }

    assert!(
        has_acquisition,
        "Expected acquisition events in report output"
    );
    assert!(has_disposal, "Expected disposal events in report output");
    assert!(
        has_same_day,
        "Expected Same-Day matching rule in report output"
    );
    assert!(has_bnb, "Expected B&B matching rule in report output");
    assert!(has_btc, "Expected BTC asset in report output");
}

/// Test filtering by asset
#[test]
fn report_filter_by_asset() {
    let output = run_taxc(&[
        "report",
        "tests/data/two_assets.json",
        "--json",
        "--asset",
        "btc",
    ]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Command failed: {:?}", output);

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Invalid JSON report output");
    let events = json
        .get("events")
        .and_then(|v| v.as_array())
        .expect("Missing events array");

    // two_assets.json holds BTC and ETH, so the filter has something to drop.
    assert!(!events.is_empty(), "Expected filtered events");
    for e in events {
        assert_eq!(
            e.get("asset").and_then(|v| v.as_str()),
            Some("BTC"),
            "Expected only BTC events"
        );
    }
    assert_eq!(json["summary"]["assets"], serde_json::json!(["BTC"]));
}

/// Test JSON input format using summary command
#[test]
fn json_input_format() {
    let output = run_taxc(&["summary", "tests/data/basic_json.json"]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify summary report is generated
    assert!(stdout.contains("TAX SUMMARY"));
    assert!(stdout.contains("CAPITAL GAINS"));
    assert!(stdout.contains("Dividend:"));
    assert!(stdout.contains("Interest:"));

    // Verify the disposal count
    assert!(stdout.contains("Disposals: 1"));
}

/// Test summary command with JSON output
#[test]
fn summary_json_output() {
    let output = run_taxc(&["summary", "tests/data/mixed_rules.json", "--json"]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Invalid JSON summary output");

    // Verify stable numeric schema
    assert!(json.get("tax_year").is_some());
    assert!(json.get("filters").is_some());
    assert!(json.get("tax_band").is_some());
    assert!(json
        .get("disposal_count")
        .and_then(|v| v.as_u64())
        .is_some());
    assert!(json.get("gross_gains").and_then(|v| v.as_str()).is_some());
    assert!(json
        .get("in_year_losses")
        .and_then(|v| v.as_str())
        .is_some());
    assert!(json
        .get("net_gain_before_aea")
        .and_then(|v| v.as_str())
        .is_some());
    assert!(json.get("aea").and_then(|v| v.as_str()).is_some());
    assert!(json.get("taxable_gain").and_then(|v| v.as_str()).is_some());
    assert!(json.get("estimated_cgt").and_then(|v| v.as_str()).is_some());
    assert!(json.get("income").and_then(|v| v.as_str()).is_some());
    assert!(json
        .get("dividend_income")
        .and_then(|v| v.as_str())
        .is_some());
    assert!(json
        .get("interest_income")
        .and_then(|v| v.as_str())
        .is_some());
    assert!(json
        .get("estimated_income_tax")
        .and_then(|v| v.as_str())
        .is_some());
    assert!(json
        .get("estimated_total_tax")
        .and_then(|v| v.as_str())
        .is_some());
    assert_eq!(json.get("currency").and_then(|v| v.as_str()), Some("GBP"));
}

#[test]
fn report_json_summary_respects_event_kind_filter() {
    let output = run_taxc(&[
        "report",
        "tests/data/mixed_rules.json",
        "--json",
        "--event-kind",
        "acquisition",
    ]);

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON output");
    let summary = json.get("summary").expect("Missing summary");

    assert_eq!(summary["disposal_count"], 0);
    assert_eq!(summary["total_proceeds"], "0.00");
    assert_eq!(summary["total_costs"], "0.00");
    assert_eq!(summary["total_gain"], "0.00");
}

#[test]
fn report_json_summary_respects_from_to_filter() {
    let output = run_taxc(&[
        "report",
        "tests/data/mixed_rules.json",
        "--json",
        "--from",
        "2030-01-01",
    ]);

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON output");
    let summary = json.get("summary").expect("Missing summary");

    assert_eq!(summary["event_count"], 0);
    assert_eq!(summary["disposal_count"], 0);
    assert_eq!(summary["total_proceeds"], "0.00");
    assert_eq!(summary["total_costs"], "0.00");
    assert_eq!(summary["total_gain"], "0.00");
}

#[test]
fn report_json_summary_respects_asset_filter() {
    let output = run_taxc(&[
        "report",
        "tests/data/two_assets.json",
        "--json",
        "--asset",
        "BTC",
    ]);

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON output");
    let summary = json.get("summary").expect("Missing summary");

    assert_eq!(summary["total_proceeds"], "12000.00");
    assert_eq!(summary["total_costs"], "10000.00");
    assert_eq!(summary["total_gain"], "2000.00");
}

/// Test no gain/no loss disposal produces zero gain and correct pool reduction
#[test]
fn report_no_gain_no_loss_spouse_transfer() {
    let output = run_taxc(&["report", "tests/data/ngnl_spouse.json", "--json"]);

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON output");

    let events = json["events"].as_array().expect("Missing events array");

    // Find the NGNL disposal
    let ngnl = events
        .iter()
        .find(|e| e["tag"] == "NoGainNoLoss")
        .expect("Missing NoGainNoLoss event");

    assert_eq!(ngnl["event_kind"], "disposal");
    let cgt = ngnl
        .get("cgt")
        .expect("NGNL disposal should have CGT details");
    assert_eq!(ngnl["value_gbp"], "25000.00");
    assert_eq!(
        ngnl["value_gbp_note"],
        "No gain/no loss transfer: value shows transferred allowable cost basis. CGT proceeds are deemed from cost basis and disposal fees; see disposal details for tax values."
    );
    // Gain must be zero
    assert_eq!(cgt["gain_gbp"], "0.00");
    // Proceeds should equal cost (no gain no loss)
    assert_eq!(cgt["proceeds_gbp"], cgt["cost_gbp"]);

    // Find the normal sale
    let sale = events
        .iter()
        .find(|e| e["event_type"] == "Disposal" && e["tag"] == "Trade")
        .expect("Missing normal disposal");

    let sale_cgt = sale.get("cgt").expect("Sale should have CGT details");
    // Bought 2 BTC for £50,000. Transferred 1 at cost £25,000. Sold 1 for £40,000.
    // Gain = £40,000 - £25,000 = £15,000
    assert_eq!(sale_cgt["proceeds_gbp"], "40000.00");
    assert_eq!(sale_cgt["cost_gbp"], "25000.00");
    assert_eq!(sale_cgt["gain_gbp"], "15000.00");

    // Summary should include both disposals but NGNL contributes zero gain
    let summary = json.get("summary").expect("Missing summary");
    assert_eq!(summary["disposal_count"], 2);
    assert_eq!(summary["total_gain"], "15000.00");
}

/// Test report command with year filter
#[test]
fn report_filter_by_year() {
    let output = run_taxc(&[
        "report",
        "tests/data/mixed_rules.json",
        "--year",
        "2025",
        "--json",
    ]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Command failed: {:?}", output);

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Invalid JSON report output");
    let events = json
        .get("events")
        .and_then(|v| v.as_array())
        .expect("Missing events array");

    assert!(!events.is_empty(), "Expected filtered events");
    for e in events {
        assert_eq!(
            e.get("tax_year").and_then(|v| v.as_str()),
            Some("2024/25"),
            "Expected all events in tax year 2024/25"
        );
    }
}

/// Run `report --json` on a fixture and assert the distinct disposal proceeds.
fn assert_disposal_proceeds(fixture: &str, expected: &[&str]) {
    let output = run_taxc(&["report", fixture, "--json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command failed: {:?}", output);

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Invalid JSON report output");
    let events = json
        .get("events")
        .and_then(|v| v.as_array())
        .expect("Missing events array");

    let mut proceeds: Vec<String> = events
        .iter()
        .filter(|e| {
            e.get("event_type")
                .and_then(|v| v.as_str())
                .is_some_and(|t| t.contains("Disposal"))
        })
        .filter_map(|e| {
            e.get("cgt")
                .and_then(|c| c.get("proceeds_gbp"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();
    proceeds.sort();
    proceeds.dedup();

    for want in expected {
        assert!(
            proceeds.contains(&want.to_string()),
            "Expected disposal proceeds {want} not found. Got: {proceeds:?}"
        );
    }
}

/// Ensure multiple disposals on the same date/asset map to the correct CGT record
#[test]
fn report_multiple_disposals_same_day() {
    assert_disposal_proceeds(
        "tests/data/duplicate_disposals.json",
        &["12000.00", "9000.00"],
    );
}

/// Ensure the report maps disposals correctly when descriptions are duplicated
#[test]
fn report_duplicate_descriptions() {
    assert_disposal_proceeds(
        "tests/data/duplicate_descriptions.json",
        &["12000.00", "9000.00"],
    );
}

// Integration tests for pools command

/// Test pools command basic output
#[test]
fn pools_basic_output() {
    let output = run_taxc(&["pools", "tests/data/mixed_rules.json"]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify key elements are present
    assert!(stdout.contains("POOL BALANCES"));
    assert!(stdout.contains("BTC"));
    assert!(stdout.contains("Cost"));
}

/// Test pools command JSON output parses correctly
#[test]
fn pools_json_output() {
    let output = run_taxc(&["pools", "tests/data/mixed_rules.json", "--json"]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify JSON structure parses
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Failed to parse pools JSON output");

    // Verify expected fields exist
    assert!(json.get("year_end_snapshots").is_some());

    let snapshots = json["year_end_snapshots"].as_array().unwrap();
    assert!(!snapshots.is_empty());

    // Verify snapshot structure
    let first_snapshot = &snapshots[0];
    assert!(first_snapshot.get("tax_year").is_some());
    assert!(first_snapshot.get("pools").is_some());
}

/// A quantity carrying more than 8 decimal places is rendered rounded, half
/// away from zero -- the same rule money uses. Nothing below 8dp is silently
/// truncated away.
#[test]
fn pools_json_quantity_rounds_beyond_eight_places() {
    let output = run_taxc(&["pools", "tests/data/sub_satoshi_quantity.json", "--json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command failed: {:?}", output);

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Failed to parse pools JSON output");
    let pools = json["year_end_snapshots"][0]["pools"].as_array().unwrap();
    let btc = pools
        .iter()
        .find(|p| p["asset"] == "BTC")
        .expect("BTC pool present");

    assert_eq!(btc["quantity"].as_str(), Some("1.12345679"));
}

/// Test pools command with --daily flag
#[test]
fn pools_daily_output() {
    let output = run_taxc(&["pools", "tests/data/mixed_rules.json", "--daily"]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify daily-specific elements
    assert!(stdout.contains("POOL HISTORY"));
    assert!(stdout.contains("Date"));
    assert!(stdout.contains("Event"));
}

/// Test pools command with --daily --json
#[test]
fn pools_daily_json_output() {
    let output = run_taxc(&["pools", "tests/data/mixed_rules.json", "--daily", "--json"]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify JSON structure parses
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Failed to parse pools daily JSON output");

    // Verify expected fields exist
    assert!(json.get("entries").is_some());

    let entries = json["entries"].as_array().unwrap();
    assert!(!entries.is_empty());

    // Verify entry structure
    let first_entry = &entries[0];
    assert!(first_entry.get("date").is_some());
    assert!(first_entry.get("asset").is_some());
    assert!(first_entry.get("event_type").is_some());
    assert!(first_entry.get("quantity").is_some());
    assert!(first_entry.get("cost_gbp").is_some());
}

/// Test pools command with asset filter
#[test]
fn pools_filter_by_asset() {
    let output = run_taxc(&["pools", "tests/data/two_assets.json", "-a", "BTC"]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // two_assets.json holds BTC and ETH; only BTC may remain.
    assert!(stdout.contains("BTC"), "{stdout}");
    assert!(!stdout.contains("ETH"), "{stdout}");
}

/// Test pools command with year filter
#[test]
fn pools_filter_by_year() {
    let output = run_taxc(&["pools", "tests/data/mixed_rules.json", "-y", "2025"]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Should show 2024/25 tax year
    assert!(stdout.contains("2024/25"));
}

/// Test pools command with combined filters
#[test]
fn pools_combined_filters() {
    let output = run_taxc(&[
        "pools",
        "tests/data/mixed_rules.json",
        "-y",
        "2025",
        "-a",
        "BTC",
        "--json",
    ]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify JSON parses and has expected structure
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Failed to parse filtered pools JSON");

    let snapshots = json["year_end_snapshots"].as_array().unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0]["tax_year"], "2024/25");
    let pools = snapshots[0]["pools"].as_array().unwrap();
    assert!(pools.iter().all(|p| p["asset"] == "BTC"), "{pools:?}");
}

#[test]
fn pools_non_daily_date_filter_behavior_is_explicit() {
    let output = run_taxc(&[
        "pools",
        "tests/data/mixed_rules.json",
        "--json",
        "--from",
        "2030-01-01",
    ]);

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Failed to parse filtered pools JSON");
    let snapshots = json["year_end_snapshots"]
        .as_array()
        .expect("Missing year_end_snapshots array");
    assert!(
        snapshots.is_empty(),
        "expected empty snapshots for future range"
    );
}

/// Defense-in-depth: the grouped warnings JSON shape, exercised end-to-end
/// through the compiled binary. Disposal-level cgt objects must not carry a
/// warnings key (removed in favour of event-level and top-level warnings).
#[test]
fn report_json_warnings_grouped_with_no_cgt_warnings_key() {
    let output = run_taxc(&[
        "report",
        "tests/data/insufficient_cost_basis.json",
        "--json",
    ]);

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON output");

    let warnings = json["warnings"].as_array().expect("Missing warnings array");
    assert_eq!(warnings.len(), 1, "expected one grouped warning record");
    assert_eq!(
        warnings[0]["warning"]["type"], "InsufficientCostBasis",
        "warning must be a type-tagged object"
    );
    assert_eq!(warnings[0]["related_event_ids"][0], 1);
    assert_eq!(warnings[0]["source_transaction_ids"][0], "tx-001");

    let events = json["events"].as_array().expect("Missing events array");
    let disposal = events
        .iter()
        .find(|e| e["event_type"] == "Disposal")
        .expect("expected a disposal event");
    assert!(
        disposal["cgt"].get("warnings").is_none(),
        "cgt objects must not contain a warnings key"
    );
    assert!(
        disposal["warnings"].is_array(),
        "event-level structured warnings must be present"
    );
}

/// Salary is always PAYE-settled; cashback never counts as income
#[test]
fn summary_salary_paye_cashback_not_income() {
    let output = run_taxc(&["summary", "tests/data/salary_cashback.json", "--json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command failed: {:?}", output);

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Invalid JSON summary output");

    // Only the £200 dividend is in the estimate; salary stays visible and
    // Cashback £50 must not be counted. The dividend is inside the 2024/25
    // £500 dividend allowance, so no income tax is due.
    assert_eq!(json["income"].as_str(), Some("200.00"));
    assert_eq!(json["salary_income"].as_str(), Some("1000.00"));
    assert_eq!(json["dividend_allowance"].as_str(), Some("500.00"));
    assert_eq!(json["estimated_income_tax"].as_str(), Some("0.00"));
}

/// Default text output shows PAYE salary as its own auditable line
#[test]
fn summary_default_text_output_shows_paye_salary_line() {
    let output = run_taxc(&["summary", "tests/data/salary_cashback.json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command failed: {:?}", output);

    assert!(
        stdout.contains("Salary (PAYE): £1000.00"),
        "Expected PAYE salary line, got:\n{}",
        stdout
    );
}

/// Cashback events appear in the report with their own tag
#[test]
fn report_cashback_event_tagged() {
    let output = run_taxc(&["report", "tests/data/salary_cashback.json", "--json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command failed: {:?}", output);

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Invalid JSON report output");
    let events = json["events"].as_array().expect("Missing events array");
    let cashback = events
        .iter()
        .find(|e| e["tag"] == "Cashback")
        .expect("Missing Cashback event");
    assert_eq!(cashback["event_kind"], "acquisition");
}

/// Without --year, a range spanning several tax years is summarised per year
/// -- each with its own AEA and rates -- and the totals are the sums.
#[test]
fn summary_json_spanning_tax_years_sums_per_year_figures() {
    let output = run_taxc(&["summary", "tests/data/two_tax_years.json", "--json"]);
    assert!(output.status.success(), "Command failed: {:?}", output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let years = json["years"].as_array().expect("years array");
    assert_eq!(years.len(), 2);
    assert_eq!(years[0]["tax_year"], "2022/23");
    assert_eq!(years[0]["aea"], "12300.00");
    assert_eq!(years[0]["estimated_cgt"], "0.00");
    assert_eq!(years[1]["tax_year"], "2024/25");
    assert_eq!(years[1]["aea"], "3000.00");
    assert_eq!(years[1]["estimated_cgt"], "900.00");

    assert_eq!(json["tax_year"], "2022/23 to 2024/25");
    assert_eq!(json["gross_gains"], "18000.00");
    assert_eq!(json["estimated_cgt"], "900.00");
    assert_eq!(json["estimated_total_tax"], "900.00");
    // Rates differ between the two years, so there is no single rate.
    assert!(json["cgt_rate_pct"].is_null());
}

#[test]
fn summary_json_single_tax_year_keeps_scalar_rate() {
    let output = run_taxc(&[
        "summary",
        "tests/data/two_tax_years.json",
        "--json",
        "-y",
        "2025",
    ]);
    assert!(output.status.success(), "Command failed: {:?}", output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["tax_year"], "2024/25");
    assert_eq!(json["cgt_rate_pct"], 18);
    assert_eq!(json["estimated_cgt"], "900.00");
    assert_eq!(json["years"].as_array().unwrap().len(), 1);
}

#[test]
fn summary_text_spanning_tax_years_shows_each_year() {
    let output = run_taxc(&["summary", "tests/data/two_tax_years.json"]);
    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("TAX YEAR 2022/23"), "{stdout}");
    assert!(stdout.contains("TAX YEAR 2024/25"), "{stdout}");
    assert!(stdout.contains("TOTAL TAX LIABILITY: £900.00"), "{stdout}");
}

/// The output schema must describe what `report --json` actually emits:
/// warning amounts are decimal strings, not numbers.
#[test]
fn output_schema_types_warning_amounts_as_strings() {
    let output = run_taxc(&["schema", "output"]);
    assert!(output.status.success(), "Command failed: {:?}", output);
    let schema: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let variants = schema["$defs"]["Warning"]["oneOf"]
        .as_array()
        .expect("Warning variants");
    let insufficient = variants
        .iter()
        .find(|v| v["properties"]["type"]["const"] == "InsufficientCostBasis")
        .expect("InsufficientCostBasis variant");
    assert_eq!(insufficient["properties"]["available"]["type"], "string");
    assert_eq!(insufficient["properties"]["required"]["type"], "string");

    let report = run_taxc(&[
        "report",
        "--json",
        "tests/data/insufficient_cost_basis.json",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&report.stdout).unwrap();
    let warning = &json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["warning"]["type"] == "InsufficientCostBasis")
        .unwrap()["warning"];
    assert!(warning["available"].is_string());
}

#[test]
fn missing_input_file_error_names_the_path() {
    let output = run_taxc(&["summary", "tests/data/does-not-exist.json"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("tests/data/does-not-exist.json"),
        "{stderr}"
    );
}

/// Rate fields are JSON numbers; unclassified disposals left out of the
/// figures are counted rather than silently dropped.
#[test]
fn summary_json_rates_are_numbers_and_unclassified_disposals_are_counted() {
    let output = run_taxc(&["summary", "tests/data/salary_cashback.json", "--json"]);
    assert!(output.status.success(), "Command failed: {:?}", output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["dividend_rate_pct"].as_f64(), Some(8.75));
    assert_eq!(json["unclassified_disposal_count"], 0);
}

/// An unclassified disposal is left out of the tax figures, and the summary
/// says so instead of printing a silent zero.
#[test]
fn summary_flags_unclassified_disposals_it_excludes() {
    let output = run_taxc(&["summary", "tests/data/unlinked_withdrawal.json", "--json"]);
    assert!(output.status.success(), "Command failed: {:?}", output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["unclassified_disposal_count"], 1);
    assert_eq!(json["disposal_count"], 0);

    let output = run_taxc(&["summary", "tests/data/unlinked_withdrawal.json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("1 unclassified disposal(s) are excluded"),
        "{stdout}"
    );
}

#[test]
fn pools_daily_lists_reorganisations_by_type() {
    // AE5: the demerger shows in `pools --daily`, in the table and JSON.
    let output = run_taxc(&["pools", "tests/data/reorganisations.json", "--daily"]);
    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for label in ["Demerger", "RightsIssue", "SmallCapitalDistribution"] {
        assert!(stdout.contains(label), "missing {label} in:\n{stdout}");
    }

    let output = run_taxc(&[
        "pools",
        "tests/data/reorganisations.json",
        "--daily",
        "--json",
    ]);
    assert!(output.status.success(), "Command failed: {:?}", output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let demerger: Vec<(&str, &str, &str)> = json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["event_type"] == "Demerger")
        .map(|e| {
            (
                e["asset"].as_str().unwrap(),
                e["quantity"].as_str().unwrap(),
                e["cost_gbp"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        demerger,
        [("ULVR", "887", "35102.90"), ("MICC", "177", "1892.34")]
    );
}

#[test]
fn event_kind_adjustment_selects_only_pool_adjustments() {
    let output = run_taxc(&[
        "pools",
        "tests/data/reorganisations.json",
        "--daily",
        "--json",
        "--event-kind",
        "adjustment",
    ]);
    assert!(output.status.success(), "Command failed: {:?}", output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let entries = json["entries"].as_array().unwrap();
    assert!(!entries.is_empty());
    for entry in entries {
        assert!(
            ["Demerger", "RightsIssue", "SmallCapitalDistribution"]
                .contains(&entry["event_type"].as_str().unwrap()),
            "{entry}"
        );
    }

    let output = run_taxc(&[
        "report",
        "tests/data/reorganisations.json",
        "--json",
        "--event-kind",
        "adjustment",
    ]);
    assert!(output.status.success(), "Command failed: {:?}", output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let events = json["events"].as_array().unwrap();
    assert_eq!(events.len(), 4);
    assert!(events.iter().all(|e| e["event_kind"] == "adjustment"));
}

#[test]
fn summary_counts_the_distribution_excess_and_fee_disposal_but_no_adjustment() {
    // 2025/26: the CSN and MICC sales, the DOT fee disposal and the X
    // distribution's £20 excess. The demerger, rights issue and ULVR cash
    // are not disposals.
    let output = run_taxc(&[
        "summary",
        "tests/data/reorganisations.json",
        "--year",
        "2026",
        "--json",
    ]);
    assert!(output.status.success(), "Command failed: {:?}", output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["disposal_count"], 4);
    // CSN: 2,500 - 2,806.58 = -306.58; MICC: 264.33; DOT: 0.10 - 0.10 = 0;
    // X: 20.00.
    assert_eq!(json["gross_gains"], "284.33");
    assert_eq!(json["in_year_losses"], "306.58");
    assert_eq!(json["income"], "0.00");
}
