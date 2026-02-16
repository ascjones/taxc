//! E2E tests for report, pools, summary, and validate command functionality

use std::process::Command;

/// Test that the report JSON output includes mixed matching rules
#[test]
fn report_mixed_rules() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/mixed_rules.json",
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
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/mixed_rules.json",
            "--json",
            "--asset",
            "BTC",
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

    assert!(!events.is_empty(), "Expected filtered events");
    for e in events {
        assert_eq!(
            e.get("asset").and_then(|v| v.as_str()),
            Some("BTC"),
            "Expected only BTC events"
        );
    }
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

/// Test report command with year filter
#[test]
fn report_filter_by_year() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/mixed_rules.json",
            "--year",
            "2025",
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

    assert!(!events.is_empty(), "Expected filtered events");
    for e in events {
        assert_eq!(
            e.get("tax_year").and_then(|v| v.as_str()),
            Some("2024/25"),
            "Expected all events in tax year 2024/25"
        );
    }
}

/// Ensure multiple disposals on the same date/asset map to the correct CGT record
#[test]
fn report_multiple_disposals_same_day() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "tests/data/duplicate_disposals.json",
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
            let proceeds_gbp = e
                .get("cgt")
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
