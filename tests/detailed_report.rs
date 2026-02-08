//! E2E tests for events and summary command functionality

use std::process::Command;

/// Test that the events command works with mixed matching rules
#[test]
fn events_mixed_rules() {
    let output = Command::new("cargo")
        .args(["run", "--", "events", "tests/data/mixed_rules.json"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify key elements are present in output
    assert!(stdout.contains("Acquisition"));
    assert!(stdout.contains("Disposal"));
    assert!(stdout.contains("Same-Day"));
    assert!(stdout.contains("B&B"));
    assert!(stdout.contains("BTC"));
}

/// Test events CSV output with mixed matching rules
#[test]
fn events_csv_mixed_rules() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "events",
            "tests/data/mixed_rules.json",
            "--csv",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify CSV header
    assert!(stdout.contains("row_num"));
    assert!(stdout.contains("date"));
    assert!(stdout.contains("event_type"));

    // Verify both rules are present
    assert!(stdout.contains("Same-Day"));
    assert!(stdout.contains("B&B"));
}

/// Test filtering by event type
#[test]
fn events_filter_by_type() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "events",
            "tests/data/mixed_rules.json",
            "--event-type",
            "disposal",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Should only see disposal-related entries
    assert!(stdout.contains("Disposal"));
    // Should not have "Acquisition" as a main row type (only in sub-rows or references)
}

/// Test JSON input format using summary command
#[test]
fn json_input_format() {
    let output = Command::new("cargo")
        .args(["run", "--", "summary", "tests/data/basic_json.json"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify summary report is generated
    assert!(stdout.contains("TAX SUMMARY"));
    assert!(stdout.contains("CAPITAL GAINS"));

    // Verify the disposal count
    assert!(stdout.contains("Disposals: 1"));
}

/// Test summary command with JSON output
#[test]
fn summary_json_output() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "summary",
            "tests/data/mixed_rules.json",
            "--json",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify JSON structure
    assert!(stdout.contains("\"tax_year\""));
    assert!(stdout.contains("\"capital_gains\""));
    assert!(stdout.contains("\"income\""));
    assert!(stdout.contains("\"total_tax_liability\""));
}

/// Test events command with year filter
#[test]
fn events_filter_by_year() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "events",
            "tests/data/mixed_rules.json",
            "--year",
            "2025",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Should show events in 2024/25 tax year
    assert!(stdout.contains("2024/25"));
}

/// Ensure multiple disposals on the same date/asset map to the correct CGT record
#[test]
fn events_multiple_disposals_same_day() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "events",
            "tests/data/duplicate_disposals.json",
            "--csv",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command failed: {:?}", output);

    let mut rdr = csv::Reader::from_reader(stdout.as_bytes());
    let headers = rdr.headers().expect("missing CSV headers").clone();

    let row_num_idx = headers
        .iter()
        .position(|h| h == "row_num")
        .expect("row_num header missing");
    let event_type_idx = headers
        .iter()
        .position(|h| h == "event_type")
        .expect("event_type header missing");
    let proceeds_idx = headers
        .iter()
        .position(|h| h == "proceeds")
        .expect("proceeds header missing");

    let mut proceeds = Vec::new();
    for result in rdr.records() {
        let record = result.expect("invalid CSV record");
        let row_num = record.get(row_num_idx).unwrap_or_default();
        let event_type = record.get(event_type_idx).unwrap_or_default();
        let proceeds_val = record.get(proceeds_idx).unwrap_or_default();
        if row_num.starts_with('#') && event_type.contains("Disposal") {
            proceeds.push(proceeds_val.to_string());
        }
    }

    proceeds.sort();
    proceeds.dedup();

    assert!(
        proceeds.contains(&"£12000.00".to_string()),
        "Expected proceeds for first disposal not found. Got: {:?}",
        proceeds
    );
    assert!(
        proceeds.contains(&"£9000.00".to_string()),
        "Expected proceeds for second disposal not found. Got: {:?}",
        proceeds
    );
}

/// Ensure HTML report maps disposals correctly when descriptions are duplicated
#[test]
fn report_duplicate_descriptions() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/duplicate_descriptions.json",
            "--json",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command failed: {:?}", output);

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Invalid JSON report output");

    let events = json
        .get("events")
        .and_then(|v| v.as_array())
        .expect("Missing events array");

    let mut proceeds = Vec::new();
    for e in events {
        let event_type = e.get("event_type").and_then(|v| v.as_str()).unwrap_or("");
        if event_type.contains("Disposal") {
            let cgt = e.get("cgt");
            let proceeds_gbp = cgt
                .and_then(|c| c.get("proceeds_gbp"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !proceeds_gbp.is_empty() {
                proceeds.push(proceeds_gbp);
            }
        }
    }

    proceeds.sort();
    proceeds.dedup();

    assert!(
        proceeds.contains(&"12000.00".to_string()),
        "Expected proceeds for first disposal not found. Got: {:?}",
        proceeds
    );
    assert!(
        proceeds.contains(&"9000.00".to_string()),
        "Expected proceeds for second disposal not found. Got: {:?}",
        proceeds
    );
}

// Integration tests for pools command

/// Test pools command basic output
#[test]
fn pools_basic_output() {
    let output = Command::new("cargo")
        .args(["run", "--", "pools", "tests/data/mixed_rules.json"])
        .output()
        .expect("Failed to execute command");

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
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "pools",
            "tests/data/mixed_rules.json",
            "--json",
        ])
        .output()
        .expect("Failed to execute command");

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

/// Test pools command with --daily flag
#[test]
fn pools_daily_output() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "pools",
            "tests/data/mixed_rules.json",
            "--daily",
        ])
        .output()
        .expect("Failed to execute command");

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
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "pools",
            "tests/data/mixed_rules.json",
            "--daily",
            "--json",
        ])
        .output()
        .expect("Failed to execute command");

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
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "pools",
            "tests/data/mixed_rules.json",
            "-a",
            "BTC",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Should show BTC pools
    assert!(stdout.contains("BTC"));
}

/// Test pools command with year filter
#[test]
fn pools_filter_by_year() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "pools",
            "tests/data/mixed_rules.json",
            "-y",
            "2025",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Should show 2024/25 tax year
    assert!(stdout.contains("2024/25"));
}

/// Test pools command with combined filters
#[test]
fn pools_combined_filters() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "pools",
            "tests/data/mixed_rules.json",
            "-y",
            "2025",
            "-a",
            "BTC",
            "--json",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify JSON parses and has expected structure
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Failed to parse filtered pools JSON");

    let snapshots = json["year_end_snapshots"].as_array().unwrap();
    // Should have exactly one snapshot for 2024/25
    assert!(!snapshots.is_empty());
    assert_eq!(snapshots[0]["tax_year"], "2024/25");
}
