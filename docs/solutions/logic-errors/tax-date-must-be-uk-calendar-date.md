---
title: "Tax dates must be the UK calendar date, not the date in the input's UTC offset"
date: 2026-10-08
category: logic-errors
module: core/uk
problem_type: logic_error
component: tax_date
severity: high
symptoms:
  - A disposal at 2024-04-05T23:30Z (00:30 BST on 6 April) lands in 2023/24 instead of 2024/25
  - The same two instants match as bed-and-breakfast when written in +00:00 and as same-day when written in +01:00
  - A repurchase drops out of the 30-day window, or into it, depending on the offset in the export
  - The HTML report shows a different calendar date from the tax year beside it for a viewer outside the UK
root_cause: wrong_api
resolution_type: code_fix
related_components:
  - cgt-matching
  - html-report
tags:
  - timezone
  - europe-london
  - tax-year
  - same-day-rule
  - bed-and-breakfast
  - chrono
---

# Tax dates must be the UK calendar date, not the date in the input's UTC offset

## Problem

`TaxableEvent::date()` called `self.datetime.date_naive()` on a `DateTime<FixedOffset>`. That returns the calendar date in **whatever offset the input was written in**. Exchange exports are usually UTC (`Z`), so every trade on a BST evening took the UTC date — one day early. Every tax rule keys on that date: the tax year, the same-day rule, the 30-day bed-and-breakfast window, and the CLI date filters.

## Solution

`core::uk::uk_date(datetime)` converts to `Europe/London` (chrono-tz) before taking the date, and is the single entry point: `TaxableEvent::date()`, the report's transaction rows and the filters all go through it. The report also renders timestamps with `core::uk::uk_rfc3339`, so a timestamp's first ten characters are the UK date — which `report.js` filters on — and its `Intl.DateTimeFormat` pins `timeZone: 'Europe/London'` so a viewer in another zone sees the same day. Date-only values (`matched_date`) are formatted with `timeZone: 'UTC'`, because `new Date('YYYY-MM-DD')` parses as UTC midnight.

## Prevention

- Never call `.date_naive()` on an input `DateTime<FixedOffset>` for a tax rule; use `uk_date`.
- Never compare offset-bearing timestamp strings for ordering or ranges; compare the UK date prefix, or parse to an instant (`Date.parse`).
- Any browser date formatter needs an explicit `timeZone`.
- Regression shape: a summer instant just before midnight UTC on 5 April, a winter instant (GMT = UTC), and a far offset such as `+09:00` (`src/core/events.rs` tests; `same_day_rule_uses_uk_calendar_day_not_utc_day` in `src/core/cgt/tests.rs`).
