// Each integration test binary includes this module and uses a different subset
// of its helpers, so unused-helper warnings are expected and suppressed here.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::SystemTime;

/// Run the prebuilt `taxc` binary with the given arguments.
///
/// Uses `CARGO_BIN_EXE_taxc` (the binary Cargo compiles for integration tests)
/// rather than `cargo run`, which avoids per-test rebuild cost and the
/// "Text file busy" build-lock flakiness of spawning `cargo` concurrently.
pub fn run_taxc(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_taxc"))
        .args(args)
        .output()
        .expect("Failed to execute taxc")
}

/// A unique temp-file path for test output artifacts (HTML, etc.).
pub fn unique_tmp_file(name: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("taxc-{name}-{nanos}.{ext}"))
}
