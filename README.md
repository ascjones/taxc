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
cat transactions.json | taxc summary --year 2025
taxc report - < transactions.json
```

### Summary - Tax Calculations

Show aggregated tax summary with CGT and income calculations:

```
taxc summary [OPTIONS] [FILE]
```

| Option | Description |
|--------|-------------|
| `-y, --year <YEAR>` | Tax year to report (e.g., 2025 for 2024/25) |
| `-a, --asset <ASSET>` | Filter by asset (e.g., BTC, ETH) |
| `-t, --tax-band <BAND>` | Tax band: `basic`, `higher`, `additional` (default: basic) |
| `--json` | Output as JSON instead of formatted text |
| `--exclude-unlinked` | Don't include unlinked deposits/withdrawals in calculations |

### Report - Tax Report

Generate a tax report (HTML by default, or JSON):

```
taxc report [OPTIONS] [FILE]
```

| Option | Description |
|--------|-------------|
| `-y, --year <YEAR>` | Tax year to filter (e.g., 2025 for 2024/25) |
| `-o, --output <FILE>` | Output file path (default: opens in browser for HTML) |
| `--json` | Output as JSON instead of HTML |
| `-a, --asset <ASSET>` | Filter by asset (e.g., BTC, ETH) |
| `--exclude-unlinked` | Don't include unlinked deposits/withdrawals in calculations |

### Pools - Pool Balances

Show pool balances over time (year-end snapshots by default, or daily history):

```
taxc pools [OPTIONS] [FILE]
```

| Option | Description |
|--------|-------------|
| `-y, --year <YEAR>` | Tax year to filter (e.g., 2025 for 2024/25) |
| `-a, --asset <ASSET>` | Filter by asset (e.g., BTC, ETH) |
| `--daily` | Show daily time-series instead of year-end snapshots |
| `--json` | Output as JSON instead of formatted table |
| `--exclude-unlinked` | Don't include unlinked deposits/withdrawals in calculations |

### Schema - Format Reference

Print JSON schemas for input or output formats. Useful for coding agents or tooling integration.

```
taxc schema [input|output]
```

- `taxc schema` or `taxc schema input` - Input transaction format (default)
- `taxc schema output` - Output report format (JSON mode)

Schemas are also checked into `schema/` for version tracking.

## Input Format (JSON Transactions)

taxc accepts JSON with top-level `assets` and `transactions` fields.

| Field | Description |
|-------|-------------|
| `assets` | Required list of asset definitions (symbol + optional asset_class) |
| `transactions` | Required list of transaction records |

- Every non-GBP symbol referenced in transactions must appear in `assets`.
- `GBP` is implicit and does not need to be listed.

**Datetime** must be RFC3339 with an offset (e.g., `2024-06-15T09:00:00+00:00`). Date-only values are accepted and assumed to be UTC midnight.

### Shared Fields

| Field | Description |
|-------|-------------|
| `id` | Unique identifier for linking and traceability |
| `datetime` | RFC3339 datetime with offset |
| `account` | Account/wallet label (e.g., `kraken`, `ledger`) |
| `description` | Optional description |
| `type` | Transaction type: `Trade`, `Deposit`, `Withdrawal` |
| `tag` | Optional classification tag: `Unclassified` (default), `Trade`, `StakingReward`, `Salary`, `OtherIncome`, `Airdrop`, `AirdropIncome`, `Dividend`, `Interest`, `Gift` |
| `price` | Optional price for valuation (see Price section below) |
| `fee` | Optional fee (see Fee section below) |

### Types

**Trade**
- `sold`: asset you gave up
- `bought`: asset you received
- Requires `price` when neither side is GBP (price.base must match bought asset)
- `tag` must be `Unclassified` (default) or `Trade`

**Deposit**
- `amount`
- `linked_withdrawal` to mark transfers
- `tag: Unclassified` (default): existing transfer/unclassified behavior
- Income tags (`StakingReward`, `Salary`, `OtherIncome`, `AirdropIncome`, `Dividend`, `Interest`): require `price` and create income acquisitions
- `tag: Gift`: requires `price` and creates `GiftIn`
- `tag: Airdrop`: must not include `price`, creates zero-cost acquisition

**Withdrawal**
- `amount`
- `linked_deposit` to mark transfers
- `tag: Unclassified` (default): existing transfer/unclassified behavior
- `tag: Gift`: requires `price` and creates `GiftOut`
- Other explicit tags on withdrawals are rejected

### Asset Registry Entry

| Field | Description |
|-------|-------------|
| `symbol` | Asset identifier (e.g., BTC, ETH, AAPL) |
| `asset_class` | `Crypto` (default) or `Stock` |

### Amount

| Field | Description |
|-------|-------------|
| `asset` | Asset identifier (must exist in top-level `assets`, unless GBP) |
| `quantity` | Amount of asset |

### Price

| Field | Description |
|-------|-------------|
| `base` | Asset symbol this price refers to (e.g., "BTC") |
| `quote` | Optional foreign currency (e.g., "USD") - requires `fx_rate` |
| `rate` | Price per unit (in GBP, or in quote currency if FX fields present) |
| `fx_rate` | Optional FX rate to convert quote to GBP - requires `quote` |
| `source` | Optional source of price data |

For direct GBP prices: `value = quantity * rate`
For FX prices: `value = quantity * rate * fx_rate`

### Fee

| Field | Description |
|-------|-------------|
| `asset` | Fee asset symbol |
| `amount` | Fee amount |
| `price` | Optional if `asset` is GBP or matches the priced asset; required otherwise |

Fee pricing rules:
- GBP fees need no price
- If the fee has an explicit `price`, that is used
- For Trade: if fee asset matches the `bought` asset, the trade's `price` is used
- For tagged Deposit/Withdrawal with price: if fee asset matches the transaction asset, transaction `price` is used
- For unclassified Deposit/Withdrawal: fee must be GBP or have an explicit `price` unless `price` is present and fee asset matches `amount.asset`
- For `Airdrop` deposits (no price): fee must be GBP or have an explicit `price`

**Notes**
- Unlinked crypto deposits/withdrawals with `tag: Unclassified` become unclassified events (`UnclassifiedIn`/`UnclassifiedOut`) with `value_gbp = 0` unless `--exclude-unlinked` is set.
- For crypto-to-crypto trades, the GBP value is taken from the acquired asset price.

### Example JSON

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
      "datetime": "2024-01-01T10:00:00+00:00",
      "account": "bank",
      "description": "Fund exchange",
      "type": "Deposit",
      "amount": { "asset": "GBP", "quantity": 5000 }
    },
    {
      "id": "tx-002",
      "datetime": "2024-01-02T09:00:00+00:00",
      "account": "kraken",
      "description": "Buy BTC with GBP",
      "type": "Trade",
      "sold": { "asset": "GBP", "quantity": 1000 },
      "bought": { "asset": "BTC", "quantity": 0.025 }
    },
    {
      "id": "tx-003",
      "datetime": "2024-08-31T10:00:00+00:00",
      "account": "kraken",
      "description": "Swap BTC for ETH (priced in USD)",
      "type": "Trade",
      "sold": { "asset": "BTC", "quantity": 0.01 },
      "bought": { "asset": "ETH", "quantity": 0.5 },
      "price": { "base": "ETH", "rate": 2000, "quote": "USD", "fx_rate": 0.79 }
    },
    {
      "id": "tx-004",
      "datetime": "2024-10-01T00:00:00+00:00",
      "account": "ledger",
      "description": "ETH staking reward",
      "type": "Deposit",
      "tag": "StakingReward",
      "amount": { "asset": "ETH", "quantity": 0.01 },
      "price": { "base": "ETH", "rate": 2000 }
    }
  ]
}
```

## Example Output

### Summary Command

```
taxc summary transactions.json -y 2025
```

```
TAX SUMMARY (2024/25) - basic rate

CAPITAL GAINS
  Disposals: 2
  Proceeds: £15,000.00 | Costs: £10,017.50 | Gain: £4,962.50
  Exempt: £3,000.00 | Taxable: £1,962.50
  CGT @ 18%: £353.25 | @ 24%: £471.00

INCOME
  Income: £250.00 (Tax @ 20%: £50.00)

TOTAL TAX LIABILITY: £403.25 (basic)
```

### Report Command

```
taxc report transactions.json
```

Generates a self-contained HTML file and opens it in your default browser. Features:

- **Summary cards** - Total proceeds, costs, gains/losses, total income
- **Interactive filtering** - Filter by date range, tax year, event type, tag, asset class, or search by asset
- **Event table with drill-down** - All taxable events with expandable disposal matching details
- **Color-coded tags** - Badges for income tags (including Dividend and Interest), trade, gift, airdrop, and unclassified events
- **Color-coded matching rules** - Same-Day (blue), B&B (amber), Pool (gray), Mixed (purple)
- **Expandable disposal rows** - Click to see linked acquisition details with matched dates and costs
- **Color-coded gains/losses** - Green for gains, red for losses

Use `-o report.html` to write to a specific file instead of opening in browser.

Use `--json` to output the report data as JSON (for integration with other tools):

```
taxc report transactions.json --json > report.json
```

Report JSON includes:
- Per-event `warnings` attached to each event row
- Per-event `source_transaction_id` to link warnings/events back to input transactions
- Event `id` values are sequential integers (`1..n`) in event order
- A top-level `warnings` list with `source_transaction_ids` and `related_event_ids`

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
