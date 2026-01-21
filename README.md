# taxc

UK Tax Calculator for Capital Gains and Income.

Calculates UK taxes from CSV or JSON input, implementing HMRC share identification rules for CGT (same-day, bed & breakfast, section 104 pool).

## Installation

```bash
cargo install --git https://github.com/ascjones/taxc
```

## Usage

```
taxc report [OPTIONS] --events <EVENTS>
```

### Options

| Option | Description |
|--------|-------------|
| `-e, --events <FILE>` | CSV or JSON file containing taxable events (required) |
| `-y, --year <YEAR>` | Tax year to report (e.g., 2025 for 2024/25) |
| `-t, --tax-band <BAND>` | Tax band: `basic`, `higher`, `additional` (default: basic) |
| `-r, --report <TYPE>` | Report type: `cgt`, `income`, `all` (default: all) |
| `--csv` | Output as CSV instead of formatted table |
| `--detailed` | Show detailed CGT breakdown with per-rule cost basis |
| `--html [OUTPUT]` | Generate interactive HTML report (opens in browser, or `-` for stdout) |

## Input Formats

Supports both CSV and JSON input. JSON format allows specifying opening pool balances.

### CSV Format

CSV file with the following columns:

| Column | Description |
|--------|-------------|
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
date,event_type,asset,asset_class,quantity,value_gbp,fees_gbp,description
2024-01-15,Acquisition,BTC,Crypto,0.5,15000.00,25.00,Coinbase
2024-03-20,Disposal,BTC,Crypto,0.25,12000.00,15.00,Coinbase
2024-04-01,StakingReward,ETH,Crypto,0.1,250.00,,Kraken
2024-05-15,Dividend,AAPL,Stock,100,150.00,,Hargreaves
```

### JSON Format

JSON input supports opening pool balances for scenarios where historical transactions have already established pool state.

```json
{
  "tax_year": "2024-25",
  "opening_pools": {
    "as_of_date": "2024-03-06",
    "pools": {
      "BTC": { "quantity": 10.0, "cost_gbp": 100000.00 },
      "ETH": { "quantity": 50.0, "cost_gbp": 50000.00 }
    }
  },
  "events": [
    {
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
| `opening_pools.as_of_date` | Date of pool snapshot (optional) |
| `opening_pools.pools` | Map of asset to `{ quantity, cost_gbp }` |
| `events` | Array of taxable events (same fields as CSV) |

## Example Output

### Standard CGT Report

```
taxc report -e events.csv -r cgt
```

```
CAPITAL GAINS TAX REPORT (All Years)

╭────────────┬───────┬───────────┬──────────┬────────┬───────────╮
│ Date       │ Asset │ Proceeds  │ Cost     │ Fees   │ Gain/Loss │
├────────────┼───────┼───────────┼──────────┼────────┼───────────┤
│ 2024-03-20 │   BTC │ £12000.00 │ £7512.50 │ £15.00 │  £4472.50 │
│ 2024-09-01 │   ETH │  £3000.00 │ £2505.00 │  £5.00 │   £490.00 │
╰────────────┴───────┴───────────┴──────────┴────────┴───────────╯

╭──────────────────────┬──────────╮
│                      │ Amount   │
├──────────────────────┼──────────┤
│      Total Gain/Loss │ £4962.50 │
│ Annual Exempt Amount │ £3000.00 │
│         Taxable Gain │ £1962.50 │
│  Tax @ 18.0% (basic) │  £353.25 │
│ Tax @ 20.0% (higher) │  £392.50 │
╰──────────────────────┴──────────╯
```

### Detailed CGT Report

```
taxc report -e events.csv -r cgt --detailed
```

Shows per-rule cost basis breakdown with running totals:

```
DETAILED CAPITAL GAINS TAX REPORT (All Years)

╭─────────┬───────┬─────────────┬─────┬──────────┬────────┬───────┬──────────┬───────────┬──────────────╮
│ Date    │ Asset │ Rule        │ Qty │ Proceeds │ Cost   │ Gain  │ Pool Qty │ Pool Cost │ Running Gain │
├─────────┼───────┼─────────────┼─────┼──────────┼────────┼───────┼──────────┼───────────┼──────────────┤
│ 15/6/24 │   BTC │    Same-Day │   2 │   £24000 │ £20000 │ £4000 │       10 │   £100000 │        £4000 │
│ 15/6/24 │   BTC │ B&B (20/06) │   3 │   £36000 │ £30000 │ £6000 │       10 │   £100000 │       £10000 │
╰─────────┴───────┴─────────────┴─────┴──────────┴────────┴───────┴──────────┴───────────┴──────────────╯
```

### Interactive HTML Report

```
taxc report -e events.csv --html
```

Generates a self-contained HTML file and opens it in your default browser. Features:

- **Summary cards** - Total proceeds, costs, gains/losses, staking income, dividends
- **Interactive filtering** - Filter by date range, tax year, event type, asset class, or search by asset
- **Three data tables** - All taxable events, CGT disposals, and income events
- **Color-coded gains/losses** - Green for gains, red for losses

Use `--html -` to output HTML to stdout instead of opening in browser.

## HMRC Share Identification Rules

CGT calculations implement the HMRC share matching rules in order:

1. **Same-Day Rule** - Match disposals with acquisitions on the same day
2. **Bed & Breakfast Rule** - Match with acquisitions within 30 days after disposal
3. **Section 104 Pool** - Match remaining shares from the pooled cost basis

## Tax Years Supported

- CGT annual exempt amounts and rates for 2024/25 onwards
- Income tax rates for basic, higher, and additional rate taxpayers
- Dividend allowances and rates

## License

MIT
