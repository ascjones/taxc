# Rename `transaction` module to `transactions` and split singular vs plural types

## Context

The current `transaction` module does more than model a single transaction. It owns the full input pipeline, collection-level parsing, normalization, validation, and conversion to events. Renaming the module to `transactions` better reflects that scope.

At the same time, the single-record types should move into `transaction.rs` (singular), while the collection/root input type should live at the `transactions` module root.

## Target layout

```text
src/core/transactions/
  mod.rs          - Transactions, Asset, AssetRegistry, ConversionOptions + public functions + re-exports
  transaction.rs  - Transaction, TransactionType, Amount, Fee
  convert.rs      - conversion logic
  datetime.rs     - datetime parsing helpers
  error.rs        - TransactionError
  normalize.rs    - normalization helpers
  validate.rs     - validation helpers
  valuation.rs    - Valuation enum
  tests.rs        - unit tests
```

## Changes

### 1. Rename the module directory

- Move `src/core/transaction/` to `src/core/transactions/`

### 2. Rename `model.rs` to `transaction.rs`

- `model.rs` becomes `transaction.rs`
- Keep in `transaction.rs`:
  - `Transaction`
  - `TransactionType`
  - `Amount`
  - `Fee`
- Move out of `transaction.rs`:
  - `TransactionInput`
  - `Asset`
  - `AssetRegistry`
  - `ConversionOptions`

### 3. Rewrite `src/core/transactions/mod.rs`

- Define the collection/root types directly in `mod.rs`:
  - `Transactions` (renamed from `TransactionInput`)
  - `Asset`
  - `AssetRegistry`
  - `ConversionOptions`
- Preserve the current derives and schema metadata on the root input type:
  - `#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]`
  - existing doc comment describing the JSON root
- Replace `mod model;` with `mod transaction;`
- Re-export:
  - `pub use transaction::{Amount, Fee, Transaction, TransactionType};`
  - `pub use valuation::Valuation;`
  - `pub use error::TransactionError;`
- Update `read_transactions_json`:
  - parse `Transactions` instead of `TransactionInput`

### 4. Update internal imports in submodules

- `convert.rs`:
  - `super::model::{...}` → `super::{AssetRegistry, Fee, Transaction, TransactionType}`
- `validate.rs`:
  - `super::model::{...}` → `super::{Asset, AssetRegistry, Transaction, TransactionType}`
- `normalize.rs`:
  - `super::model::{...}` → `super::{Asset, Transaction, TransactionType}`
- `tests.rs`:
  - keep `use super::*;`
  - ensure `Valuation` remains re-exported from module root so existing tests still compile

### 5. Update `src/core/mod.rs`

- `pub mod transaction;` → `pub mod transactions;`
- Update flat re-exports:
  - `transaction::{...}` → `transactions::{...}`
  - `TransactionInput` → `Transactions`

### 6. Update schema input type references

- `src/cmd/schema.rs`:
  - `TransactionInput` → `Transactions`
- Preserve the input schema output shape; only the Rust type name changes

### 7. Update any remaining code or docs references

- Replace remaining `TransactionInput` references in code/comments/plans that must stay current
- Update any path references in active plan files if they are meant to be used after this refactor

### 8. Regenerate schema

- Run:

```bash
cargo run -- schema input > schema/input.json
```

## Verification

```bash
cargo test
cargo clippy
cargo run -- schema input > schema/input.json
```
