# Add Tag::Dividend and Tag::Interest

## Context

Adding Dividend and Interest back as income tags on Deposit transactions, consistent with how other income types (StakingReward, Salary, etc.) are modeled. Both are treated as general income for now — specific tax rates/allowances can be added later.

## Changes

### 1. Add tag variants — `src/core/events.rs`

- Add `Dividend` and `Interest` to the `Tag` enum (after `AirdropIncome`, before `Gift`)
- Add both to `is_income()` match
- Add display mappings in `display_event_type()`:
  - `(Acquisition, Dividend) => "Dividend"`
  - `(Acquisition, Interest) => "Interest"`
- Add tests:
  - `display_event_type(Acquisition, Tag::Dividend)` → `"Dividend"`
  - `display_event_type(Acquisition, Tag::Interest)` → `"Interest"`
  - `Tag::Dividend.is_income()` → `true`
  - `Tag::Interest.is_income()` → `true`

### 2. Tag validation — `src/core/transaction.rs`

In `to_taxable_events()` match arms, add `Dividend` and `Interest` alongside existing income tags:
- **Deposit**: Allow — requires price (same as StakingReward/Salary/OtherIncome), no linked_withdrawal
- **Trade**: Reject — `InvalidTagForType`
- **Withdrawal**: Reject — `InvalidTagForType`

Tests for each tag (Dividend, Interest):
- Deposit with price → Acquisition event with income tag ✓
- Trade → `InvalidTagForType` error
- Withdrawal → `InvalidTagForType` error
- Deposit without price → `MissingTaggedPrice` error

### 3. HTML report — `src/cmd/report/html.rs`

All wiring points:

**CSS** — income tag group selector: add `.tag-dividend, .tag-interest` to the existing group with `.tag-stakingreward`, `.tag-salary`, etc.

**Tag filter checkboxes** — add Dividend and Interest checkboxes after AirdropIncome

**`applyFilters()` tags object** — add `dividend` and `interest` entries reading their checkbox state

**`resetFilters()`** — add `tag-dividend` and `tag-interest` reset to checked

**`calculateFilteredSummary()` income tags array** — add `'dividend'` and `'interest'` to the income tag list

### 4. README — `README.md`

- Tag list: Add `Dividend` and `Interest` to the tag enumeration
- Deposit rules: Add Dividend and Interest to the income tags description
- HTML report features: Add Dividend and Interest to tag color-coding description

## Verification

```bash
cargo test
cargo clippy
```
