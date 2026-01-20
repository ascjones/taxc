//! E2E tests for detailed CGT report functionality

use std::process::Command;

/// Test that the detailed report CLI works with mixed matching rules
#[test]
fn detailed_report_mixed_rules() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "-e",
            "tests/data/mixed_rules.csv",
            "--detailed",
            "-r",
            "cgt",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify key elements are present in output
    assert!(stdout.contains("DETAILED CAPITAL GAINS TAX REPORT"));
    assert!(stdout.contains("Same-Day"));
    assert!(stdout.contains("B&B"));
    assert!(stdout.contains("20/06")); // B&B matched date (UK format)
    assert!(stdout.contains("Running Gain"));
}

/// Test detailed CSV output with mixed matching rules
#[test]
fn detailed_csv_mixed_rules() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "-e",
            "tests/data/mixed_rules.csv",
            "--detailed",
            "--csv",
            "-r",
            "cgt",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the command succeeded
    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify CSV header
    assert!(stdout.contains("date,tax_year,asset,rule,matched_date"));
    assert!(stdout.contains("running_gain_gbp"));

    // Verify both rules are present
    assert!(stdout.contains("Same-Day"));
    assert!(stdout.contains("B&B"));

    // Should have header + 2 data rows
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 3);
}

/// Test that pool state is tracked correctly across disposals
#[test]
fn detailed_report_pool_tracking() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "report",
            "-e",
            "tests/data/mixed_rules.csv",
            "--detailed",
            "-r",
            "cgt",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify pool quantities are shown
    assert!(stdout.contains("Pool Qty"));
    assert!(stdout.contains("Pool Cost"));

    // The pool should have 10 BTC after the disposal
    // (original 10 + same-day 2 matched + B&B 3 matched = 10 remaining in pool)
    assert!(stdout.contains("10"));
    assert!(stdout.contains("£100000"));
}
