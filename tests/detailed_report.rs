//! E2E tests for events and summary command functionality

use std::process::Command;

/// Test that the events command works with mixed matching rules
#[test]
fn events_mixed_rules() {
    let output = Command::new("cargo")
        .args(["run", "--", "events", "-e", "tests/data/mixed_rules.csv"])
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
            "-e",
            "tests/data/mixed_rules.csv",
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
            "-e",
            "tests/data/mixed_rules.csv",
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
        .args(["run", "--", "summary", "-e", "tests/data/basic_json.json"])
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
            "-e",
            "tests/data/mixed_rules.csv",
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
            "-e",
            "tests/data/mixed_rules.csv",
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
