# Transaction Tests Overhaul

## Context

The transaction test file (1600 lines, 57 tests) has significant boilerplate repetition, overlapping tests that can be parameterized, and missing edge case coverage. This plan addresses all three issues following TDD: add failing edge cases first, then refactor.

## Phase 1: Add missing edge case tests (TDD — failing tests first)

Write new tests using raw Transaction construction (no builders yet). Get them passing before refactoring.

**Happy paths:**
- `unlinked_withdrawal_creates_disposal` — withdrawal counterpart to existing deposit test
- `gbp_deposit_produces_no_events` — unclassified GBP deposit filtered out
- `gbp_withdrawal_produces_no_events` — unclassified GBP withdrawal filtered out
- `fee_on_single_event_trade` — fee attaches to sole event in GBP-buy and GBP-sell
- `fee_on_tagged_deposit` — e.g. staking reward with GBP fee
- `unlinked_deposit_with_price` — verify value_gbp computed from optional price
- `unlinked_withdrawal_with_price` — same for withdrawal

**Error paths — link validation (both directions per error):**
- `duplicate_transaction_id_errors`
- `linked_deposit_not_found_errors` — deposit references non-existent withdrawal
- `linked_withdrawal_not_found_errors` — withdrawal references non-existent deposit
- `linked_deposit_type_mismatch_errors` — deposit linked to another deposit
- `linked_withdrawal_type_mismatch_errors` — withdrawal linked to another withdrawal
- `linked_deposit_not_reciprocal_errors` — deposit→withdrawal but withdrawal doesn't link back
- `linked_withdrawal_not_reciprocal_errors` — withdrawal→deposit but deposit doesn't link back

**Error paths — income tags missing price (parameterized):**
- `income_tags_require_price` — covers [Salary, OtherIncome, AirdropIncome] in one test

**Checkpoint:** `cargo test && cargo clippy -- -D warnings`

## Phase 2: Extract builder helpers

Add builder helpers at the top of the test file to eliminate repetitive Transaction construction.

```rust
fn trade_tx(id: &str, sold: (&str, Decimal), bought: (&str, Decimal)) -> Transaction
fn deposit_tx(id: &str, asset: &str, qty: Decimal) -> Transaction
fn withdrawal_tx(id: &str, asset: &str, qty: Decimal) -> Transaction
```

Each returns a Transaction with sensible defaults (Unclassified tag, no price/fee/description, standard datetime/account). Add chainable modifier helpers:

```rust
fn with_tag(tx: Transaction, tag: Tag) -> Transaction
fn with_price(tx: Transaction, price: Price) -> Transaction
fn with_fee(tx: Transaction, fee: Fee) -> Transaction
fn with_deposit_link(tx: Transaction, link: &str) -> Transaction   // for deposits
fn with_withdrawal_link(tx: Transaction, link: &str) -> Transaction // for withdrawals
```

Two conversion shorthands (not one):
```rust
// Single-transaction conversion with default registry
fn convert_one(tx: &Transaction) -> Result<Vec<TaxableEvent>, TransactionError>
// wraps tx.to_taxable_events(&test_registry(), false)

// Multi-transaction conversion through transactions_to_events
fn convert_all(txs: &[Transaction]) -> Result<Vec<TaxableEvent>, TransactionError>
// wraps transactions_to_events(txs, &test_registry(), ConversionOptions { exclude_unlinked: false })
```

Tests that need custom registries or `exclude_unlinked: true` call the underlying functions directly — they don't use these helpers.

**Checkpoint:** `cargo test && cargo clippy -- -D warnings`

## Phase 3: Consolidate overlapping tests

Merge using builders from Phase 2. Each merged test uses parameterized loops.

**a. Invalid-tag-on-withdrawal** — merge 4 tests into 1:
→ `invalid_tags_on_withdrawal_error` covering [StakingReward, Airdrop, Dividend, Interest, Salary, OtherIncome, AirdropIncome]

**b. Invalid-tag-on-trade** — merge 2 tests into 1:
→ `invalid_tags_on_trade_error` covering [StakingReward, Dividend, Interest, Salary, OtherIncome, AirdropIncome, Airdrop, Gift]

**c. GBP trade rejects price** — merge 2 into 1:
→ `gbp_trade_rejects_price` covering both directions

**d. Price base mismatch on unclassified** — merge 2 into 1:
→ `unclassified_price_base_mismatch_errors` covering deposit and withdrawal

**e. Missing-fee-price errors** — merge 2 into 1:
→ `fee_without_price_errors` covering sold-asset and unrelated-asset cases

Net: 12 tests → 5 tests, with broader tag coverage.

**Checkpoint:** `cargo test && cargo clippy -- -D warnings`

## Phase 4: Rewrite existing tests using builders

Mechanical rewrite of all remaining tests to use builder helpers. Same assertions, same coverage, less boilerplate. Run tests after each logical group of rewrites to stay green.

**Checkpoint:** `cargo test && cargo clippy -- -D warnings`

## File changed

- `src/core/transaction/tests.rs` — sole file modified

## Verification

```bash
cargo test && cargo clippy -- -D warnings
```

Full test suite passes at each phase checkpoint. No hardcoded test count expectations.
