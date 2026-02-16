# Plan: Rename Label → Tag, Add Tag to Transaction, Remove StakingReward Type

## Context

The input model currently has a separate `TransactionType::StakingReward` variant. We want to simplify the type system by expressing income through a `Tag` on `Deposit` transactions instead. This also enables new income types (Salary, OtherIncome, Airdrop, AirdropIncome) without adding new transaction type variants. The `Label` enum is renamed to `Tag` throughout.

**Breaking changes** (hard break, no compatibility shim):
- Removes `"type": "StakingReward"` from JSON input — users must migrate to `"type": "Deposit", "tag": "StakingReward"`. Old format will fail deserialization with a clear serde error.
- Removes `--event-type` CLI flag entirely (use `jq` for JSON filtering, HTML report has client-side tag filtering)
- Renames JSON output fields (`total_staking` → `total_income`, `staking_events` → `income_events`)

## Changes

### 1. Core enum rename + new variants (`src/core/events.rs`)

- Rename `Label` → `Tag`, `label` → `tag` on `TaxableEvent`
- Add variants: `Salary`, `OtherIncome`, `Airdrop`, `AirdropIncome`
- Add `Tag::is_income()` → true for `StakingReward | Salary | OtherIncome | AirdropIncome`
- Update `display_event_type(event_type, tag)` with new mappings:
  - `(Acquisition, Salary)` → `"Salary"`
  - `(Acquisition, OtherIncome)` → `"OtherIncome"`
  - `(Acquisition, Airdrop)` → `"Airdrop"`
  - `(Acquisition, AirdropIncome)` → `"AirdropIncome"`
- Update tests

### 2. Re-exports (`src/core/mod.rs`)

- `Label` → `Tag` in pub use line

### 3. Add `tag` to Transaction, remove StakingReward variant (`src/core/transaction.rs`)

- Add to `Transaction` struct:
  ```rust
  #[serde(default)]
  pub tag: Tag,
  ```
- Remove `TransactionType::StakingReward { asset: Asset }` variant
- Rename `MissingStakingPrice` → `MissingTaggedPrice`:
  ```rust
  #[error("price required for {tag} {tx_type}: {id}")]
  MissingTaggedPrice { id: String, tag: String, tx_type: String },
  ```
  Covers income deposits, Gift deposits, and Gift withdrawals.
- Add new error variants:
  ```rust
  #[error("tagged deposit cannot have linked_withdrawal: {id}")]
  TaggedDepositLinked { id: String },
  #[error("tagged withdrawal cannot have linked_deposit: {id}")]
  TaggedWithdrawalLinked { id: String },
  #[error("airdrop deposit must not include price: {id}")]
  AirdropPriceNotAllowed { id: String },
  #[error("{tag} tag not allowed on {tx_type}: {id}")]
  InvalidTagForType { id: String, tag: String, tx_type: String },
  ```

**Tag/type validation matrix** (fail fast, never silently ignore):

| Tag | Trade | Deposit | Withdrawal |
|-----|-------|---------|------------|
| Unclassified | ok (→ Tag::Trade) | ok (existing behavior) | ok (existing behavior) |
| Trade | ok | **InvalidTagForType** | **InvalidTagForType** |
| StakingReward | **InvalidTagForType** | income deposit (price req) | **InvalidTagForType** |
| Salary | **InvalidTagForType** | income deposit (price req) | **InvalidTagForType** |
| OtherIncome | **InvalidTagForType** | income deposit (price req) | **InvalidTagForType** |
| AirdropIncome | **InvalidTagForType** | income deposit (price req) | **InvalidTagForType** |
| Airdrop | **InvalidTagForType** | zero-cost acquisition | **InvalidTagForType** |
| Gift | **InvalidTagForType** | gift-in (price req) | gift-out (price req) |

**Deposit rules** (non-Unclassified tag):
- Error if `linked_withdrawal` is set (`TaggedDepositLinked`).
- Trade tag: error (`InvalidTagForType`).
- Income tags (StakingReward/Salary/OtherIncome/AirdropIncome): require price (`MissingTaggedPrice`), validate price.base, create Acquisition with the tag.
- Airdrop: no price required; if `price` is provided, error (`AirdropPriceNotAllowed`). Create Acquisition with Tag::Airdrop and £0 value/cost basis.
- Gift: require price (`MissingTaggedPrice`), create Acquisition with Tag::Gift.

**Withdrawal rules** (non-Unclassified tag):
- Error if `linked_deposit` is set (`TaggedWithdrawalLinked`).
- Gift: require price (`MissingTaggedPrice`), create Disposal with Tag::Gift.
- Everything else (Trade, income tags, Airdrop, AirdropIncome): error (`InvalidTagForType`).

- Remove StakingReward match arm and its normalize_transactions arm
- Update all `Label::` → `Tag::`, `label:` → `tag:` in event construction

### 4. Income calculation (`src/core/income.rs`)

- `Label` → `Tag`, `label` → `tag`
- Filter by `event.tag.is_income()` instead of `== Label::StakingReward`
- Rename `staking_events` → `income_events` in `IncomeReport` and `calculate_income_tax`
- Update tests

### 5. CGT calculation (`src/core/cgt.rs`)

- `Label` → `Tag`, `label` → `tag` throughout (struct fields, comparisons, tests)
- `PoolHistoryEntry.label` → `PoolHistoryEntry.tag`

### 6. Report module (`src/cmd/report/mod.rs`)

- `Label` → `Tag`, `label` → `tag`
- Remove `EventTypeFilter` enum, `--event-type` arg, and `matches_filter()` function entirely
- Remove `event_type_filter` parameter from `build_report_data`
- `total_staking` → `total_income` in Summary
- `income_count`: use `tag.is_income()` predicate on source events (not string matching)
- `income_report.staking_events` → `income_report.income_events`
- Add `tag: Tag` field to `EventRow` (for tag-based filtering in HTML)

### 7. HTML report (`src/cmd/report/html.rs`)

- Remove "Staking" checkbox from event type filters
- Add Tag filter: checkboxes for each Tag variant (Trade, StakingReward, Salary, OtherIncome, Airdrop, AirdropIncome, Gift, Unclassified)
- CSS: remove `type-staking`, add tag-based color classes
- JS filter logic: filter rows by tag value using the `EventRow.tag` field

### 8. Pools command (`src/cmd/pools.rs`)

- `e.label` → `e.tag` in display_event_type call

### 9. Summary command (`src/cmd/summary.rs`)

- `income_report.staking_events` → `income_report.income_events`
- Rename summary fields: `staking_income` → `income`, etc.

### 10. Test fixtures + schemas + docs

- `tests/data/staking_matched.json`: `"type": "StakingReward"` → `"type": "Deposit", "tag": "StakingReward"`
- Regenerate `schema/input.json` and `schema/output.json`
- `README.md`: update transaction type docs, add tag field docs, update examples
- `tests/detailed_report.rs`: update any affected assertions

## Tests (transaction.rs)

**Negative tests** (invalid tag/type combos):
1. **Income deposit missing price** → `MissingTaggedPrice { tag: "StakingReward", tx_type: "deposit" }`
2. **Income deposit with mismatched price.base** → `PriceBaseMismatch`
3. **Tagged deposit with linked_withdrawal** → `TaggedDepositLinked`
4. **Tagged withdrawal with linked_deposit** → `TaggedWithdrawalLinked`
5. **Income tag on withdrawal** → `InvalidTagForType { tag: "StakingReward", tx_type: "withdrawal" }`
6. **Airdrop tag on withdrawal** → `InvalidTagForType { tag: "Airdrop", tx_type: "withdrawal" }`
7. **Non-Trade/Unclassified tag on trade** → `InvalidTagForType { tag: "StakingReward", tx_type: "trade" }`
8. **Gift deposit missing price** → `MissingTaggedPrice { tag: "Gift", tx_type: "deposit" }`
9. **Gift withdrawal missing price** → `MissingTaggedPrice { tag: "Gift", tx_type: "withdrawal" }`
10. **Trade tag on deposit** → `InvalidTagForType { tag: "Trade", tx_type: "deposit" }`
11. **Trade tag on withdrawal** → `InvalidTagForType { tag: "Trade", tx_type: "withdrawal" }`
12. **Airdrop deposit with price provided** → `AirdropPriceNotAllowed`

**Positive tests** (new tag behaviors):
13. **Gift deposit creates GiftIn acquisition** with price as value
14. **Gift withdrawal creates GiftOut disposal** with price as value
15. **Airdrop deposit creates acquisition at £0** — no price required
16. **AirdropIncome deposit creates income acquisition** — requires price, value = market price
17. **Salary deposit creates income acquisition**
18. **OtherIncome deposit creates income acquisition**

## Verification

1. `cargo check` after each file group
2. `cargo test` — all tests pass
3. `cargo clippy` — no warnings
4. `cargo run -- schema input` — verify tag field appears, StakingReward type gone
5. `cargo run -- report tests/data/staking_matched.json --json` — verify income events work
