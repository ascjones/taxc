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
taxc events - < transactions.json
```

### Events - Transaction View

Show all transactions/events in a detailed table:

```
taxc events [OPTIONS] [FILE]
```

| Option | Description |
|--------|-------------|
| `-y, --year <YEAR>` | Tax year to filter (e.g., 2025 for 2024/25) |
| `-t, --event-type <TYPE>` | Filter by event type: `acquisition`, `disposal`, `staking` |
| `-a, --asset <ASSET>` | Filter by asset (e.g., BTC, ETH) |
| `--csv` | Output as CSV instead of formatted table |
| `--exclude-unlinked` | Don't include unlinked deposits/withdrawals in calculations |

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

### Validate - Data Quality Check

Surface data quality issues without generating full reports. Useful for quick checks or CI/CD pipelines.

```
taxc validate [OPTIONS] [FILE]
```

| Option | Description |
|--------|-------------|
| `-y, --year <YEAR>` | Tax year to filter (e.g., 2025 for 2024/25) |
| `--json` | Output as JSON instead of formatted text |
| `--exclude-unlinked` | Don't include unlinked deposits/withdrawals in calculations |

**Exit codes:**
- `0` - No issues found
- `1` - Issues found (useful for CI)

**Issue types detected:**
- `NoCostBasis` - Disposal with no matching acquisitions (cost basis is £0)
- `InsufficientCostBasis` - Pool had less than required (partial cost basis)
- `Unclassified` - Unclassified disposal event that may need review

### Schema - Format Reference

Print JSON schemas for input or output formats. Useful for coding agents or tooling integration.

```
taxc schema [input|output]
```

- `taxc schema` or `taxc schema input` - Input transaction format (default)
- `taxc schema output` - Output report format (JSON mode)

Schemas are also checked into `schema/` for version tracking.

## Input Format (JSON Transactions)

taxc accepts JSON with a top-level `transactions` array. Each transaction has shared fields plus a type-specific payload.

**Datetime** must be RFC3339 with an offset (e.g., `2024-06-15T09:00:00+00:00`). Date-only values are accepted and assumed to be UTC midnight.

### Shared Fields

| Field | Description |
|-------|-------------|
| `id` | Unique identifier for linking and traceability |
| `datetime` | RFC3339 datetime with offset |
| `account` | Account/wallet label (e.g., `kraken`, `ledger`) |
| `description` | Optional description |
| `type` | Transaction type: `Trade`, `Deposit`, `Withdrawal`, `StakingReward` |
| `price` | Optional price for valuation (see Price section below) |
| `fee` | Optional fee (see Fee section below) |

### Types

**Trade**
- `sold`: asset you gave up
- `bought`: asset you received
- Requires `price` when neither side is GBP (price.base must match bought asset)

**Deposit / Withdrawal**
- `asset`
- `linked_withdrawal` or `linked_deposit` to mark transfers
- Optional `price` for valuing unlinked deposits/withdrawals

**StakingReward**
- `asset`
- Requires `price` (price.base must match asset)

### Asset

| Field | Description |
|-------|-------------|
| `symbol` | Asset identifier (e.g., BTC, ETH, GBP) |
| `quantity` | Amount of asset |
| `asset_class` | `Crypto` (default) or `Stock` |

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
- For StakingReward: if fee asset matches the reward asset, the staking `price` is used
- For Deposit/Withdrawal: fee must be GBP or have an explicit `price`

**Notes**
- Unlinked crypto deposits/withdrawals become unclassified events (shown as `Unclassified In/Out`) with `value_gbp = 0` unless `--exclude-unlinked` is set.
- For crypto-to-crypto trades, the GBP value is taken from the acquired asset price.

### Example JSON

```json
{
  "transactions": [
    {
      "id": "tx-001",
      "datetime": "2024-01-01T10:00:00+00:00",
      "account": "bank",
      "description": "Fund exchange",
      "type": "Deposit",
      "asset": { "symbol": "GBP", "quantity": 5000 }
    },
    {
      "id": "tx-002",
      "datetime": "2024-01-02T09:00:00+00:00",
      "account": "kraken",
      "description": "Buy BTC with GBP",
      "type": "Trade",
      "sold": { "symbol": "GBP", "quantity": 1000 },
      "bought": { "symbol": "BTC", "quantity": 0.025 }
    },
    {
      "id": "tx-003",
      "datetime": "2024-08-31T10:00:00+00:00",
      "account": "kraken",
      "description": "Swap BTC for ETH (priced in USD)",
      "type": "Trade",
      "sold": { "symbol": "BTC", "quantity": 0.01 },
      "bought": { "symbol": "ETH", "quantity": 0.5 },
      "price": { "base": "ETH", "rate": 2000, "quote": "USD", "fx_rate": 0.79 }
    },
    {
      "id": "tx-004",
      "datetime": "2024-10-01T00:00:00+00:00",
      "account": "ledger",
      "description": "ETH staking reward",
      "type": "StakingReward",
      "asset": { "symbol": "ETH", "quantity": 0.01 },
      "price": { "base": "ETH", "rate": 2000 }
    }
  ]
}
```

## Example Output

### Events Command

```
taxc events transactions.json
```

Shows a detailed table with all transactions, including sub-rows for disposals matched via multiple rules (Same-Day, B&B, Pool).

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
  Staking: £250.00 (Tax @ 20%: £50.00)

TOTAL TAX LIABILITY: £403.25 (basic)
```

### Report Command

```
taxc report transactions.json
```

Generates a self-contained HTML file and opens it in your default browser. Features:

- **Summary cards** - Total proceeds, costs, gains/losses, staking income
- **Interactive filtering** - Filter by date range, tax year, event type, asset class, or search by asset
- **Three data tables** - All taxable events, CGT disposals, and income events
- **Color-coded event types** - Badges for Acquisition (green), Disposal (red), Staking (purple)
- **Color-coded matching rules** - Same-Day (blue), B&B (amber), Pool (gray), Mixed (purple)
- **Expandable disposal rows** - Click to see linked acquisition details with matched dates and costs
- **Color-coded gains/losses** - Green for gains, red for losses

Use `-o report.html` to write to a specific file instead of opening in browser.

Use `--json` to output the report data as JSON (for integration with other tools):

```
taxc report transactions.json --json > report.json
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

## License

MIT
