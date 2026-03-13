# taxc

UK Tax Calculator for Capital Gains and Income.

Calculates UK taxes from JSON transaction input, implementing HMRC share identification rules for CGT (same-day, bed & breakfast, section 104 pool).

## Installation

```bash
cargo install --git https://github.com/ascjones/taxc
```

## Commands

All commands accept an optional positional `FILE` (JSON). If omitted or set to `-`, input is read from stdin.

```
taxc summary transactions.json -y 2025
taxc report transactions.json
taxc pools transactions.json --daily
taxc schema input
```

### `taxc summary` - Tax Calculations

Aggregated CGT and income calculations. Use `-y 2025` for a tax year, or `--from`/`--to` for a date range. Add `--json` for machine-readable output, `-t higher` for different tax bands.

### `taxc report` - Tax Report

Self-contained HTML report opened in your browser, with summary cards, interactive filtering, expandable disposal matching details, and color-coded tags/rules. Use `-o file.html` to save instead, or `--json` for structured data.

### `taxc pools` - Pool Balances

Section 104 pool balances over time. Year-end snapshots by default, or `--daily` for daily history.

### `taxc schema` - Format Reference

Print JSON schemas for input (`taxc schema input`, default) or output (`taxc schema output`) formats. Schemas are also checked into `schema/` for version tracking.

All filtering commands share: `-y`/`--from`/`--to` (date), `-a` (asset), `--event-kind` (disposal/acquisition), `--exclude-unlinked`.

## Input Format

JSON with top-level `assets` and `transactions` fields. Run `taxc schema input` for the full schema.

Three transaction types: **Trade** (asset swap via `sold`/`bought`), **Deposit** (asset received), **Withdrawal** (asset sent). Transactions can be tagged for tax classification (income types, gifts, transfers, no gain/no loss).

### Example

```json
{
  "assets": [
    { "symbol": "BTC" },
    { "symbol": "ETH" },
    { "symbol": "AAPL", "asset_class": "Stock" }
  ],
  "transactions": [
    {
      "id": "tx-001",
      "datetime": "2024-01-02T09:00:00+00:00",
      "account": "kraken",
      "type": "Trade",
      "sold": { "asset": "GBP", "quantity": 1000 },
      "bought": { "asset": "BTC", "quantity": 0.025 }
    },
    {
      "id": "tx-002",
      "datetime": "2024-08-31T10:00:00+00:00",
      "account": "kraken",
      "type": "Trade",
      "sold": { "asset": "BTC", "quantity": 0.01 },
      "bought": { "asset": "ETH", "quantity": 0.5 },
      "valuation": { "base": "ETH", "rate": 2000, "quote": "USD", "fx_rate": 0.79 }
    },
    {
      "id": "tx-003",
      "datetime": "2024-10-01T00:00:00+00:00",
      "account": "ledger",
      "type": "Deposit",
      "tag": "StakingReward",
      "amount": { "asset": "ETH", "quantity": 0.01 },
      "valuation": 20
    }
  ]
}
```

## HMRC Share Identification Rules

CGT calculations implement the HMRC share matching rules in order:

1. **Same-Day Rule** - Match disposals with acquisitions on the same day
2. **Bed & Breakfast Rule** - Match with acquisitions within 30 days after disposal
3. **Section 104 Pool** - Match remaining shares from the pooled cost basis

## Tax Years Supported

- CGT annual exempt amounts and rates for 2024/25 onwards
- Income tax rates for basic, higher, and additional rate taxpayers

## Development

Enable pre-commit hooks (runs fmt, clippy, and tests):

```bash
git config core.hooksPath .githooks
```

## Project Structure

- `src/main.rs` - CLI entry point
- `src/cmd/` - CLI command implementations
- `src/core/` - Domain logic and tax calculations (flat public surface via re-exports)

## License

MIT
