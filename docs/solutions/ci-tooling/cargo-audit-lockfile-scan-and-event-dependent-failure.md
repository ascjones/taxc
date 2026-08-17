---
title: "cargo-audit findings on crates that are never compiled, and audit-check silently passing on schedule"
date: 2026-08-17
category: ci-tooling
module: ci/audit
problem_type: false_positive
component: github_actions
severity: medium
symptoms:
  - cargo audit reports a vulnerability in a crate that no code path can reach
  - cargo tree finds no dependents for the crate cargo audit just flagged
  - Weekly scheduled "Security audit" runs report success for months while an advisory issue stays open
  - The same commit passes the scheduled audit run and fails the push-triggered one
  - A dependency upgrade lands on master before anything audits it
root_cause: tool_semantics_misunderstood
resolution_type: config_change
related_components:
  - cargo-audit
  - rustsec-audit-check
  - dependency-management
tags:
  - cargo-audit
  - rustsec
  - github-actions
  - optional-dependencies
  - feature-resolution
  - false-positive
  - security-advisory
  - ci-triggers
---

# cargo-audit findings on crates that are never compiled, and audit-check silently passing on schedule

## Problem

Two independent surprises, both encountered while clearing RUSTSEC advisories:

1. `cargo audit` reported a vulnerability (RUSTSEC-2026-0235, `rkyv` 0.7.46) in a crate that is not part of taxc's build graph at all.
2. The weekly scheduled "Security audit" workflow had been reporting **success** for months while advisory issues sat open — and the *same commit* that passed a scheduled run failed a push-triggered run 20 minutes later.

## Symptoms

- `cargo audit` exits 1 on `rkyv` 0.7.46, but `cargo tree -i rkyv -e normal` prints "nothing to print", and so does `cargo tree -i rkyv -e all --target all`.
- Issue #21 (`anyhow`, RUSTSEC-2026-0190) was filed 2026-07-06 by a scheduled run and stayed open through four consecutive green weekly runs (2026-07-13, 07-20, 07-27, 08-03) before anyone acted on it.
- Run 31369597111 (`schedule`, 2026-08-10 08:19) → **success**, with `{"vulnerabilities":{"found":true,"count":1}}` in its own JSON output.
- Run 31368131067 (`push`, same commit, 07:59) → **failure**, `##[error]Critical vulnerabilities were found, marking check as failed`.
- PR #24 upgraded 155 packages and merged with four green checks; the audit only ran — and failed — *after* the merge.

## What Didn't Work

- **Assuming the green scheduled runs meant no findings.** They contained the findings. The job simply did not fail.
- **Explaining the green runs by advisory severity.** The initial theory was that `unsound`/informational advisories don't fail the build, which fit issue #21 (`anyhow` is `informational = "unsound"`). It was wrong: the 2026-08-10 scheduled run passed while reporting `rkyv`, which has `"informational": null` — a full vulnerability. Severity was a coincidence; the trigger event was the variable.
- **Upgrading to clear the `rkyv` finding.** `cargo update` and raising `Cargo.toml` minimums both left `rkyv` at 0.7.46. The fix is in `rkyv` 0.8.17, and `rust_decimal` 1.42.1 — the latest release — still depends on the unsupported 0.7 series. No reachable upgrade existed.
- **`cargo tree -i rkyv` alone as proof of absence.** It prints "nothing to print" plus a hint to retry with `--target all`, which reads like an incomplete answer rather than a negative result. Running `-e all --target all` is what makes the conclusion defensible.

## Solution

### Fix 1: ignore the unreachable advisory, with the reasoning recorded

`.cargo/audit.toml` (new file — `cargo-audit` reads it from the working directory, which is also where the CI action invokes `cargo audit`):

```toml
[advisories]
ignore = [
    # RUSTSEC-2026-0235 -- rkyv 0.7.46, out-of-bounds read via forged pointer
    # metadata in checked archive access.
    #
    # Not reachable from taxc:
    #   * rkyv is an *optional* dependency of rust_decimal. Cargo.toml enables
    #     only features = ["serde"], so rkyv is never compiled -- confirmed by
    #     `cargo tree -i rkyv -e all --target all` reporting "nothing to print".
    #   * The advisory requires deserialising untrusted archives through
    #     rkyv::access or rkyv::from_bytes. taxc reads CSV and JSON only.
    #
    # Remove once rust_decimal depends on rkyv >= 0.8.17.
    "RUSTSEC-2026-0235",
]
```

Verified in CI rather than assumed — a `workflow_dispatch` run on the branch passed, and its JSON showed the config had actually been read:

```json
"settings":{...,"ignore":["RUSTSEC-2026-0235"],...}
"vulnerabilities":{"found":false,"count":0}
```

The equivalent dispatch run *before* the change failed on the same advisory, so the ignore is what changed the outcome.

### Fix 2: audit pull requests, and audit changes to the audit config

`.github/workflows/audit.yml` gained a `pull_request` trigger, and `.cargo/audit.toml` was added to both path filters:

```yaml
  push:
    branches: [ master ]
    paths:
      - 'Cargo.toml'
      - 'Cargo.lock'
      - '.cargo/audit.toml'
  pull_request:
    paths:
      - 'Cargo.toml'
      - 'Cargo.lock'
      - '.cargo/audit.toml'
```

Note that GitHub Actions does **not** support YAML anchors, so the duplicated list cannot be factored out with `&`/`*`.

## Why This Works

**`cargo audit` scans `Cargo.lock`, not the build graph.** `Cargo.lock` records optional dependencies of your dependencies whether or not the feature gating them is enabled, so the lockfile is a superset of what is compiled. `rust_decimal` declares `rkyv` as optional; taxc enables only `features = ["serde"]`; `rkyv` is therefore locked but never built. `cargo audit` sees the lockfile entry and reports it. `cargo tree` walks the resolved feature graph and correctly finds no dependents. Neither tool is wrong — they answer different questions, and only `cargo tree` answers "is this compiled into the binary?".

The same distinction explains RUSTSEC-2026-0190 (`anyhow`): there the crate *was* compiled, but the advisory named a single function (`[affected.functions] "anyhow::Error::downcast_mut" = ["< 1.0.103"]`) that the codebase never calls. Advisories are matched by package version, so reachability is always a separate question from the version match.

**`rustsec/audit-check` chooses its failure behaviour from the trigger event.** On `schedule` it files or updates a GitHub issue and concludes success — the run is a reporting mechanism, not a gate. On `push`, `pull_request` and `workflow_dispatch` it creates a check run and fails the job. A repository whose only audit trigger is `schedule` therefore gets advisory *notifications* but no enforcement, and the green checkmarks actively disguise this.

## Prevention

- **Never conclude "no findings" from a green scheduled audit run.** Read the JSON in the log, or re-run via `workflow_dispatch`, which does fail on findings.
- **Before acting on a `cargo audit` hit, establish reachability**: `cargo tree -i <crate> -e all --target all` for whether it is compiled at all, and the advisory's `[affected.functions]` for whether the vulnerable API is called. Fetch the advisory's own metadata (`https://raw.githubusercontent.com/rustsec/advisory-db/main/crates/<crate>/<ID>.md`) for the authoritative `patched` range instead of guessing the fixed version.
- **Every `ignore` entry states why it is unreachable and what retires it.** An ignore without a removal condition becomes permanent by default; with one, it is a dated exception a reviewer can re-check.
- **Path-filtered workflows must include their own config file.** `audit.yml` originally filtered on `Cargo.toml`/`Cargo.lock` only, so editing `.cargo/audit.toml` — the file that determines what the audit reports — would not have re-run the audit.
- **Gate on the event that precedes the merge.** An audit that only runs on `push` to master reports problems that have already landed.

## Related Issues

- GitHub #21 — RUSTSEC-2026-0190 (`anyhow` `downcast_mut` unsoundness); resolved by bumping to 1.0.104 in PR #23. Reachability analysis: `downcast_mut` is never called in `src/` or `tests/`.
- GitHub #25 — RUSTSEC-2026-0235 (`rkyv` archive validation); resolved by the ignore entry in PR #26, since no upgrade path exists.
- PR #24 — the 155-package `cargo update` that merged without an audit ever running against it, which motivated the `pull_request` trigger.
