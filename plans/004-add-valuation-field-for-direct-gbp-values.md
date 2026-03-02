# Add `value_gbp` as universal valuation alternative (type-safe)

## Context

Broker statements often show total GBP values rather than per-unit prices. Currently users must back-calculate rates. We replace `price: Option<Price>` with `Option<Valuation>` — an enum that makes it type-impossible to have both `price` and `value_gbp`.

## Design: `Option<Valuation>` (no flatten)

Replace `price: Option<Price>` on `Transaction` with:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub valuation: Option<Valuation>,
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Valuation {
    Price(Price),                                  // object: { "base": "ETH", "rate": 2000 }
    ValueGbp(#[schemars(with = "f64")] Decimal),   // number: 15000
}
```

**JSON format** (breaking: `"price"` → `"valuation"`):
- `"valuation": { "base": "ETH", "rate": 2000 }` → `Some(Price(...))`
- `"valuation": 15000` → `Some(ValueGbp(15000))`
- Field omitted → `None`
- Both price + value_gbp → impossible by construction

No custom serde, no flatten. Untagged works because object vs number is unambiguous.

## Valuation rules per transaction type

| Transaction type | Price | ValueGbp | None |
|---|---|---|---|
| Trade (non-GBP) | calc from rate | use directly | error: `MissingTradeValuation` |
| Trade (GBP) | error: `GbpTradeValuationNotAllowed` | error: `GbpTradeValuationNotAllowed` | value from GBP quantity |
| Tagged deposit (income, non-GBP) | calc from rate | use directly | error: `MissingTaggedValuation` |
| Tagged deposit (GBP income) | error: `GbpIncomeValuationNotAllowed` | error: `GbpIncomeValuationNotAllowed` | value from quantity |
| Airdrop deposit | error: `AirdropValuationNotAllowed` | error: `AirdropValuationNotAllowed` | zero |
| Gift deposit/withdrawal | calc from rate | use directly | error: `MissingTaggedValuation` |
| Unlinked deposit/withdrawal | calc from rate | use directly | zero |

Key insight: every place that accepts `Price` also accepts `ValueGbp`. No `ValueGbpNotAllowedForType` needed.

## Files to change

### 1. New: `src/core/transaction/valuation.rs`

- `Valuation` enum with `Price(Price)` and `ValueGbp(Decimal)` variants
- Derive `Serialize`, `Deserialize`, `JsonSchema` with `#[serde(untagged)]`
- Helper method: `fn price(&self) -> Option<&Price>`
- No custom serde needed — untagged handles object vs number

### 2. `src/core/transaction/model.rs`

- Remove `price: Option<Price>` from `Transaction`
- Add `#[serde(default, skip_serializing_if = "Option::is_none")] pub valuation: Option<Valuation>`

### 3. `src/core/transaction/mod.rs`

- Add `pub mod valuation;`
- Re-export `Valuation` alongside the existing transaction model types if needed by tests or callers

### 4. `src/core/transaction/error.rs`

- Rename `GbpTradePriceNotAllowed` → `GbpTradeValuationNotAllowed { id }`
- Rename `GbpIncomePriceNotAllowed` → `GbpIncomeValuationNotAllowed { id, tag }`
- Rename `AirdropPriceNotAllowed` → `AirdropValuationNotAllowed { id }`
- Rename `MissingTradePrice` → `MissingTradeValuation { id }`
- Rename `MissingTaggedPrice` → `MissingTaggedValuation { id, tag, tx_type }`
- Keep `PriceBaseMismatch` as-is for `Valuation::Price`
- Update error strings to say `valuation` instead of `price` where appropriate

### 5. `src/core/transaction/convert.rs`

**Destructuring**: `valuation` replaces `price` (it is `&Option<Valuation>`)

**Trade branch**:
```rust
let is_gbp_trade = is_gbp(&sold.asset) || is_gbp(&bought.asset);
let value_gbp = if is_gbp_trade {
    match valuation {
        None => /* GBP quantity */,
        Some(_) => return Err(TransactionError::GbpTradeValuationNotAllowed {
            id: id.clone(),
        }),
    }
} else {
    match valuation {
        Some(Valuation::Price(p)) => {
            validate_price_base(id, p, &bought.asset)?;
            p.to_gbp(bought.quantity)?
        }
        Some(Valuation::ValueGbp(v)) => *v,
        None => return Err(TransactionError::MissingTradeValuation {
            id: id.clone(),
        }),
    }
};
```

**Tagged deposits**:
- GBP income: `Some(_)` → `GbpIncomeValuationNotAllowed`; `None` → use quantity
- Airdrop: `Some(_)` → `AirdropValuationNotAllowed`; `None` → zero
- Other income/gift: `Some(Price(p))` → validate + calculate; `Some(ValueGbp(v))` → use directly; `None` → `MissingTaggedValuation`

**Tagged withdrawals**:
- `Gift` mirrors tagged deposit gift handling: `Some(Price(p))` → validate + calculate; `Some(ValueGbp(v))` → use directly; `None` → `MissingTaggedValuation`

**Unlinked deposits/withdrawals**:
- Accept all three variants
- `None` keeps current zero-value behavior
- `Some(Price(p))` validates base and calculates value
- `Some(ValueGbp(v))` uses the direct total

**Fee handling**:
```rust
let tx_price = valuation.as_ref().and_then(|v| v.price());
```
When `ValueGbp` is used, `tx_price` is `None`, so fee inference only works for GBP fees or fees with explicit `fee.price`.

### 6. `src/core/transaction/validate.rs`

- Update `validate_assets` to use `tx.valuation.as_ref().and_then(Valuation::price)` instead of `tx.price.as_ref()`
- Keep validating `fee.price` exactly as today

### 7. `src/core/transaction/normalize.rs`

- Update transaction-level price normalization to inspect `valuation`
- Only normalize `base` and `quote` when `valuation` is `Some(Valuation::Price(_))`
- Keep `fee.price` normalization as-is

### 8. `src/core/transaction/tests.rs`

**Builder changes**:
- `with_price` → `valuation = Some(Valuation::Price(price))`
- New `with_value_gbp` → `valuation = Some(Valuation::ValueGbp(value))`
- Factory functions use `valuation: None`

**Update existing tests**:
- Rename/update all current assertions that mention `price`-specific errors to the new `valuation` error variants
- In particular, update the existing `gbp_trade_rejects_price` test to assert `GbpTradeValuationNotAllowed`, since GBP trades now reject any valuation variant
- Update existing missing-valuation tests such as `trade_without_price_no_gbp_errors`, `staking_reward_requires_price`, `income_tags_require_price`, `gift_deposit_missing_price_errors`, and `gift_withdrawal_missing_price_errors` to the renamed `Missing*Valuation` errors
- Update the current airdrop and GBP income rejection tests to the renamed `*ValuationNotAllowed` errors

**New tests** (TDD):

**Value/event tests:**

| Test | Scenario |
|------|----------|
| `trade_crypto_to_crypto_with_value_gbp` | Two events, value used directly |
| `trade_gbp_with_value_gbp_errors` | GBP trade + `ValueGbp` → `GbpTradeValuationNotAllowed` |
| `deposit_income_with_value_gbp` | `StakingReward` with `ValueGbp` works |
| `deposit_gbp_income_with_value_gbp_errors` | GBP dividend + `ValueGbp` → `GbpIncomeValuationNotAllowed` |
| `deposit_airdrop_with_value_gbp_errors` | `ValueGbp` on airdrop → `AirdropValuationNotAllowed` |
| `deposit_gift_with_value_gbp` | Gift deposit with `ValueGbp` works |
| `withdrawal_gift_with_value_gbp` | Gift withdrawal with `ValueGbp` works |
| `unlinked_deposit_with_value_gbp` | Value used directly |
| `unlinked_withdrawal_with_value_gbp` | Value used directly |

**Fee + `ValueGbp` tests**:

| Test | Branch |
|------|--------|
| `trade_value_gbp_with_gbp_fee` | Trade fee path |
| `trade_value_gbp_crypto_fee_needs_own_price` | Trade: no `tx_price` → `MissingFeePrice` |
| `trade_value_gbp_crypto_fee_with_explicit_price` | Trade: fee has own price |
| `deposit_income_value_gbp_with_gbp_fee` | Tagged deposit fee path |
| `deposit_income_value_gbp_crypto_fee_needs_own_price` | Tagged deposit: no `tx_price` → `MissingFeePrice` |
| `deposit_gift_value_gbp_with_fee` | Gift deposit fee path |
| `withdrawal_gift_value_gbp_with_fee` | Gift withdrawal fee path |
| `unlinked_deposit_value_gbp_with_fee` | Unlinked deposit fee path |
| `unlinked_withdrawal_value_gbp_with_fee` | Unlinked withdrawal fee path |

**Serde tests:**

| Test | Scenario |
|------|----------|
| `serde_round_trip_valuation_price` | Serialize/deserialize `Transaction` with `Price` valuation |
| `serde_round_trip_valuation_value_gbp` | Serialize/deserialize `Transaction` with `ValueGbp` valuation |
| `serde_round_trip_valuation_none` | Serialize/deserialize `Transaction` with no valuation |

### 9. Schema & docs

- Regenerate: `cargo run -- schema input > schema/input.json`
- Update `README.md` sections:
  - **Shared Fields**: rename `price` to `valuation`
  - **Types**: update all “requires `price`” rules to “requires `valuation`”, and note that valuation may be either a `Price` object or a direct GBP number
  - **Valuation**: add a new section documenting the two forms of the field and when to use each
  - **Price**: keep the section for the `Price` object shape used by both `valuation` and `fee.price`
  - **Fee**: clarify that transaction valuation can only be reused for fee inference when it is `Valuation::Price`; direct GBP totals do not provide a per-unit price
- Update examples to use the top-level `TransactionInput` structure
- Call out the breaking CLI/input change from `price` to `valuation`

## Example JSON

Full input structure:

```json
{
  "assets": [
    { "symbol": "BTC", "asset_class": "Crypto" },
    { "symbol": "ETH", "asset_class": "Crypto" }
  ],
  "transactions": [
    {
      "id": "tx-1",
      "datetime": "2024-01-15T10:00:00+00:00",
      "account": "kraken",
      "type": "Trade",
      "sold": { "asset": "BTC", "quantity": 0.5 },
      "bought": { "asset": "ETH", "quantity": 8.0 },
      "valuation": 15000
    },
    {
      "id": "tx-2",
      "datetime": "2024-02-01T10:00:00+00:00",
      "account": "kraken",
      "type": "Trade",
      "sold": { "asset": "BTC", "quantity": 0.5 },
      "bought": { "asset": "ETH", "quantity": 8.0 },
      "valuation": { "base": "ETH", "rate": 1875 }
    },
    {
      "id": "tx-3",
      "datetime": "2024-02-01T10:00:00+00:00",
      "account": "kraken",
      "type": "Deposit",
      "tag": "StakingReward",
      "amount": { "asset": "ETH", "quantity": 0.5 },
      "valuation": 800
    }
  ]
}
```

## Verification

```bash
cargo test
cargo clippy
cargo run -- schema input > schema/input.json
```
