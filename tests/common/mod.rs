// Each integration test binary includes this module and uses a different subset
// of its helpers, so unused-helper warnings are expected and suppressed here.
#![allow(dead_code)]

use headless_chrome::{Browser, LaunchOptions};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

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

/// Serialises browser launches within a test binary.
static BROWSER_LAUNCH: Mutex<()> = Mutex::new(());

/// Launch a headless Chrome instance for a browser test.
///
/// Launching resolves and execs a Chrome binary. Several tests doing that
/// concurrently intermittently fail with "Text file busy" (ETXTBSY) on CI,
/// because the binary is still open for writing elsewhere, so launches are
/// serialised and retried with backoff. The lock covers only the launch --
/// the guard is released once the browser is running, so the tests still do
/// their page work in parallel.
pub fn launch_browser() -> Browser {
    let _guard = BROWSER_LAUNCH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let options = || {
        LaunchOptions::default_builder()
            .headless(true)
            .sandbox(false)
            .idle_browser_timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build launch options")
    };

    let mut last_error = None;
    for attempt in 0..5 {
        match Browser::new(options()) {
            Ok(browser) => return browser,
            Err(err) => {
                last_error = Some(err);
                std::thread::sleep(Duration::from_millis(200 * (attempt + 1)));
            }
        }
    }
    panic!(
        "Failed to launch browser after 5 attempts: {}",
        last_error.expect("a launch error after exhausting attempts")
    );
}
