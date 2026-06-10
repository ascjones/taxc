---
title: "HTML report JS drifted from Rust serde JSON contract: warning badges and matched-acquisition links silently broken"
date: 2026-06-10
category: integration-issues
module: cmd/report/html
problem_type: integration_issue
component: frontend_stimulus
severity: high
symptoms:
  - Every warning badge in the HTML report rendered the literal string "type" instead of the variant name
  - JS-computed unclassified count was 0 while the Rust-computed DATA.summary reported 123, and "inc. unclassified" sub-totals never displayed
  - Warning highlighting on event-type arrows never applied
  - All 3,608 matched-acquisition links in disposal details pointed at nonexistent "#row-N" anchors and were dead
  - No console errors or crashes — every failure was silent
root_cause: wrong_api
resolution_type: code_fix
related_components:
  - rust-serde
  - headless-browser-tests
tags:
  - serde
  - json-contract
  - internally-tagged-enum
  - rust-js-interop
  - silent-failure
  - schema-drift
  - html-report
  - dead-links
---

# HTML report JS drifted from Rust serde JSON contract: warning badges and matched-acquisition links silently broken

## Problem

The HTML report's JavaScript (`src/cmd/report/html/report.js`) silently drifted from the serde JSON contract produced by Rust (`src/cmd/report/mod.rs`): warning objects were parsed with the wrong serde enum shape, and matched-acquisition links used array indices instead of event ids. Users saw warning badges rendering the literal word "type", warning counts/highlights missing entirely, and every matched-acquisition navigation link (~3,608 of them in realistic data) dead.

## Symptoms

- Warning badges on event rows rendered the literal text "type" instead of "Unclassified" / "Insufficient Cost Basis"
- JS-computed unclassified count was 0 while Rust's `DATA.summary.unclassified_count` was 123 — the "inc. unclassified" metric sub-totals never appeared
- Warning highlighting on event arrows never applied (`hasWarningType()` never matched)
- All matched-acquisition links were dead: `<a href="#row-N">` anchors pointed at elements that didn't exist (rows are keyed by `data-event-id`, not index)
- No console errors whatsoever — everything failed silently

## What Didn't Work

- **Waiting for errors**: `Object.keys(warning)[0]` returns a valid string (`"type"`) for any object, so nothing threw. The bug shipped and sat in an open GitHub issue (#10 "HTML report: warnings not visible on main event table rows").
- **Existing tests**: the Rust tests asserted the buggy semantics — they expected `matched_row_id` to be the enumerate index into the filtered events array (0, 2), so they passed while the links were broken.
- **Small test data**: the drift only became obvious with a realistic 16,677-event dataset. Screenshotting revealed the literal "type" pills; the decisive diagnostic was comparing JS-recomputed aggregates against Rust-computed ones in the browser console: `calculateFilteredSummary(DATA.events).unclassifiedCount` → 0 vs `DATA.summary.unclassified_count` → 123.
- The dead links were confirmed by checking `matched_row_id: 0` pointed at an event whose `id` was 1, and `document.getElementById('row-0')` was null.

## Solution

### Fix 1: parse internally-tagged warnings correctly

Rust serializes `Warning` with `#[serde(tag = "type")]` (`src/core/warnings.rs`), producing flat objects: `{"type": "UnclassifiedEvent"}` and `{"type": "InsufficientCostBasis", "available": "0", "required": "5"}`.

Before (assumed externally-tagged shape):

```js
function warningTypeName(warning) {
    if (!warning) return '';
    if (typeof warning === 'string') return warning;
    return Object.keys(warning)[0] || '';   // always returns "type"
}
function formatWarningDisplay(w) {
    if (typeof w === 'string') return w;
    const type = Object.keys(w)[0] || '';
    if (type === 'InsufficientCostBasis') {
        const detail = w[type];   // expects nested object — wrong for internally tagged
        // ...
    }
    return type;
}
```

After (`src/cmd/report/html/report.js`):

```js
// Warnings are serialized internally tagged: {"type": "UnclassifiedEvent", ...fields}.
function warningTypeName(warning) {
    if (!warning) return '';
    if (typeof warning === 'string') return warning;
    return warning.type || '';
}

function formatWarningDisplay(w) {
    if (typeof w === 'string') return w;
    const type = warningTypeName(w);
    if (type === 'UnclassifiedEvent') return 'Unclassified';
    if (type === 'InsufficientCostBasis') {
        if (w.available == null) return 'Insufficient Cost Basis';
        if (parseFloat(w.available) === 0) return 'No Cost Basis';
        return `Insufficient Cost Basis (${w.available}/${w.required})`;
    }
    return type;
}
```

Verified: JS-computed unclassified = 123 = Rust count; banner shows "124 events with warnings, 123 unclassified, 1 cost basis issues".

### Fix 2: key navigation by event id, not array index (TDD)

First, updated the tests in `src/cmd/report/tests.rs` (`same_day_duplicate_acquisitions_link_to_first_row`, `bnb_duplicate_acquisitions_link_to_first_row`) to expect event ids (1, 3) instead of indices (0, 2) — they failed, proving the bug. Then in `src/cmd/report/mod.rs`: renamed `matched_row_id` → `matched_event_id` and built the acquisition index from `e.id` instead of the enumerate index.

Before (index into the filtered array):

```rust
for (idx, e) in filtered_events.iter().enumerate() { /* ... */ acquisition_row_index.entry(key).or_insert(idx); }
```

After:

```rust
// Build index of acquisitions by (date, asset) -> event id for navigation
// Multiple acquisitions on the same day for the same asset share an id (first one)
let mut acquisition_event_index: HashMap<(NaiveDate, String), usize> = HashMap::new();
for e in &filtered_events {
    if e.event_type == EventType::Acquisition && e.tag != Tag::Unclassified {
        let key = (e.date(), e.asset.clone());
        acquisition_event_index.entry(key).or_insert(e.id);
    }
}
```

Regenerated the schema: `cargo run -- schema output > schema/output.json`. On the JS side, links are buttons handled by event delegation instead of fragment anchors:

```js
if (mc.matched_event_id != null) {
    return `<button class="acquisition-link" data-nav-event="${mc.matched_event_id}">${details}</button>`;
}
// delegated click handler:
const nav = event.target.closest('[data-nav-event], [data-nav-tx]');
// ... navigateToEvent(Number(nav.dataset.navEvent));
```

## Why This Works

**Serde enum representations.** Serde's default is *externally tagged*: `{"InsufficientCostBasis": {"available": "0", "required": "5"}}` — the variant name is the single key, and `Object.keys(w)[0]` is the correct way to read it. With `#[serde(tag = "type")]` (*internally tagged*), the variant name moves into a `type` field and the variant's fields are flattened onto the same object: `{"type": "InsufficientCostBasis", "available": "0", "required": "5"}`. For that shape, `Object.keys(w)[0]` is exactly wrong — it returns the tag field's *name* (`"type"`) for every variant, and there is no nested detail object. The fix reads the discriminant from `w.type` and the fields directly off `w`.

**Id vs index.** `matched_row_id` was an index into the *filtered* events array at serialization time, but the DOM keys rows by the event's stable `id` (`data-event-id`). Any filter/sort boundary breaks index identity — index 0 of the filtered array was the event with `id` 1. Stable ids survive filtering, sorting, and re-rendering; array positions don't.

## Prevention

- **Mirror the serde contract at the JS parse site.** When JS consumes serde-produced JSON, put a comment next to the parsing code stating the serde representation (e.g. `// Warning is #[serde(tag = "type")] — internally tagged: {type, ...fields}`) so the shape is checkable without opening the Rust source.
- **Read discriminants via the tag field, never `Object.keys()[0]`**, unless the contract is explicitly externally tagged.
- **Cross-check JS aggregates against Rust aggregates in browser tests.** `tests/html_report.rs` already runs headless-chrome assertions; it could assert `calculateFilteredSummary(DATA.events).unclassifiedCount === DATA.summary.unclassified_count` — that single equality would have caught this class of drift immediately.
- **Regenerate `schema/*.json` whenever report structs change** (`cargo run -- schema output > schema/output.json`) so the JSON contract is reviewable in diffs and field renames like `matched_row_id` → `matched_event_id` are visible to reviewers.
- **Key cross-references and navigation by stable ids, not array indices** — indices don't survive filtering or reordering.
- **Test against realistic-scale data.** The 16,677-event dataset surfaced in minutes what tiny fixtures never showed.

## Related Issues

- GitHub #10 "HTML report: warnings not visible on main event table rows" — describes the user-visible warning-visibility symptom; the same overhaul also moved warning badges into expandable row cards with an amber event-arrow indicator on every warned row, so this issue is likely resolvable.
- GitHub #11 "Improve HTML report UI" — tracking issue; several items (sortable columns, inline warning highlighting, empty states, tax-year view, matched-acquisition links) were addressed by the same overhaul (commit 72168b4).
