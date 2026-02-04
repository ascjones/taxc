# taxc

UK Tax Calculator for Capital Gains and Income.

Calculates UK taxes from CSV or JSON input, implementing HMRC share identification rules for CGT (same-day, bed & breakfast, section 104 pool).

## Installation

```bash
cargo install --git https://github.com/ascjones/taxc
```

## Commands

All commands accept an optional positional `FILE` (CSV or JSON). If omitted or set to `-`, input is read from stdin.

```
cat events.csv | taxc summary --year 2025
taxc events - < events.csv
```

### Events - Transaction View

Show all transactions/events in a detailed table:

```
taxc events [OPTIONS] [FILE]
```

| Option | Description |
|--------|-------------|
| `-y, --year <YEAR>` | Tax year to filter (e.g., 2025 for 2024/25) |
| `-t, --event-type <TYPE>` | Filter by event type: `acquisition`, `disposal`, `staking`, `dividend` |
| `-a, --asset <ASSET>` | Filter by asset (e.g., BTC, ETH) |
| `--csv` | Output as CSV instead of formatted table |

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

### Validate - Data Quality Check

Surface data quality issues without generating full reports. Useful for quick checks or CI/CD pipelines.

```
taxc validate [OPTIONS] [FILE]
```

| Option | Description |
|--------|-------------|
| `-y, --year <YEAR>` | Tax year to filter (e.g., 2025 for 2024/25) |
| `--json` | Output as JSON instead of formatted text |

**Exit codes:**
- `0` - No issues found
- `1` - Issues found (useful for CI)

**Issue types detected:**
- `NoCostBasis` - Disposal with no matching acquisitions (cost basis is £0)
- `InsufficientCostBasis` - Pool had less than required (partial cost basis)
- `Unclassified` - UnclassifiedOut event that may need review

## Input Formats

Supports both CSV and JSON input.

### CSV Format

CSV file with the following columns:

| Column | Description |
|--------|-------------|
| `id` | Unique identifier for linking back to source data (optional) |
| `date` | Event date (YYYY-MM-DD) |
| `event_type` | `Acquisition`, `Disposal`, `StakingReward`, `Dividend` |
| `asset` | Asset identifier (e.g., BTC, ETH, AAPL) |
| `asset_class` | `Crypto` or `Stock` |
| `quantity` | Amount of asset |
| `value_gbp` | Value in GBP |
| `fees_gbp` | Transaction fees in GBP (optional) |
| `description` | Description (optional) |

#### Example CSV

```csv
id,date,event_type,asset,asset_class,quantity,value_gbp,fees_gbp,description
tx-001,2024-01-15,Acquisition,BTC,Crypto,0.5,15000.00,25.00,Coinbase
tx-002,2024-03-20,Disposal,BTC,Crypto,0.25,12000.00,15.00,Coinbase
tx-003,2024-04-01,StakingReward,ETH,Crypto,0.1,250.00,,Kraken
tx-004,2024-05-15,Dividend,AAPL,Stock,100,150.00,,Hargreaves
```

### JSON Format

```json
{
  "tax_year": "2024-25",
  "events": [
    {
      "id": "tx-001",
      "date": "2024-04-15",
      "event_type": "Disposal",
      "asset": "BTC",
      "asset_class": "Crypto",
      "quantity": 5.0,
      "value_gbp": 75000.00,
      "fees_gbp": 10.00,
      "description": "Partial sale"
    }
  ]
}
```

| Field | Description |
|-------|-------------|
| `tax_year` | Optional metadata |
| `events` | Array of taxable events (same fields as CSV) |

## Example Output

### Events Command

```
taxc events events.csv
```

Shows a detailed table with all transactions, including sub-rows for disposals matched via multiple rules (Same-Day, B&B, Pool).

### Summary Command

```
taxc summary events.csv -y 2025
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
  Dividends: £150.00 (Allowance: £150.00, Tax @ 8.75%: £0.00)

TOTAL TAX LIABILITY: £403.25 (basic)
```

### Report Command

```
taxc report events.csv
```

Generates a self-contained HTML file and opens it in your default browser. Features:

- **Summary cards** - Total proceeds, costs, gains/losses, staking income, dividends
- **Interactive filtering** - Filter by date range, tax year, event type, asset class, or search by asset
- **Three data tables** - All taxable events, CGT disposals, and income events
- **Color-coded event types** - Badges for Acquisition (green), Disposal (red), Staking (purple), Dividend (teal)
- **Color-coded matching rules** - Same-Day (blue), B&B (amber), Pool (gray), Mixed (purple)
- **Expandable disposal rows** - Click to see linked acquisition details with matched dates and costs
- **Color-coded gains/losses** - Green for gains, red for losses

Use `-o report.html` to write to a specific file instead of opening in browser.

Use `--json` to output the report data as JSON (for integration with other tools):

```
taxc report events.csv --json > report.json
```

## HMRC Share Identification Rules

CGT calculations implement the HMRC share matching rules in order:

1. **Same-Day Rule** - Match disposals with acquisitions on the same day
2. **Bed & Breakfast Rule** - Match with acquisitions within 30 days after disposal
3. **Section 104 Pool** - Match remaining shares from the pooled cost basis

## Tax Years Supported

- CGT annual exempt amounts and rates for 2024/25 onwards
- Income tax rates for basic, higher, and additional rate taxpayers
- Dividend allowances and rates

## Development

Enable pre-commit hooks (runs fmt, clippy, and tests):

```bash
git config core.hooksPath .githooks
```

## License

MIT
