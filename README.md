# taxc

UK tax calculator for capital gains and income. Reads JSON transactions and applies the HMRC share identification rules for CGT (same-day, bed & breakfast, section 104 pool).

## Installation

```bash
cargo install --git https://github.com/ascjones/taxc
```

## Usage

```
taxc summary transactions.json -y 2025   # aggregated tax calculations
taxc report transactions.json            # interactive HTML report
taxc pools transactions.json --daily     # section 104 pool history
taxc schema input                        # input format reference
```

All commands take an optional positional `FILE` (JSON); if omitted or `-`, input is read from stdin. Filtering commands share `-y`/`--from`/`--to` (date), `-a` (asset), `--event-kind` (disposal/acquisition), and `--exclude-unlinked`.

### `taxc summary`

Aggregated CGT and income calculations. Filter with `-y 2025` or `--from`/`--to`; add `--json` for machine-readable output, `-t higher` for a different tax band.

In `--json` output every monetary field is a 2dp string (`"12345.67"`), matching `taxc report --json`, so amounts survive JSON parsing exactly. Counts and the `*_rate_pct` fields remain numbers; `dividend_rate_pct` carries two decimal places (e.g. `8.75`).

When the selected range spans more than one tax year, each year is summarised on its own — with that year's AEA and rates, and losses netted only within the year — and the totals are the sums. The text output prints a block per year; the JSON carries the per-year figures in `years` and the sums at the top level, where `tax_year` reads e.g. `"2022/23 to 2024/25"` and a `*_rate_pct` is `null` if the years' rates differ. Use `-y` to select a single year.

Salary is treated as PAYE-settled (already taxed at source): it is reported on its own line (`salary_income` in JSON) but excluded from the income tax estimate, since UK employers must operate PAYE even on salary paid in crypto. For the rare case of employment income received gross (non-RCA tokens, or an overseas employer with no UK presence), tag it `OtherIncome` instead.

### `taxc report`

Self-contained HTML report, opened in your browser: summary cards, interactive filtering, sortable columns, expandable per-disposal detail (fees, warnings, matching), and a Tax Years view with a gain/loss chart. Use `-o file.html` to save instead, or `--json` for structured data.

### `taxc pools`

Section 104 pool balances over time — year-end snapshots by default, `--daily` for daily history.

Quantities render to at most 8 decimal places, rounded half away from zero — the same rule monetary amounts use. A quantity carrying more decimals is rounded, not truncated, so a non-zero balance below `0.00000001` shows as `0.00000001` rather than `0`.

### `taxc schema`

Print the JSON schema for the input (default) or output (`taxc schema output`) format. Schemas are also checked into `schema/`.

## Input Format

JSON with top-level `assets` and `transactions` fields — run `taxc schema input` for the full schema. Three transaction types:

- **Trade** — asset swap (`sold`/`bought`)
- **Deposit** — asset received (`amount`)
- **Withdrawal** — asset sent (`amount`)

An optional `tag` classifies a transaction for tax. Income tags (`Salary`, `OtherIncome`, `Dividend`, `Interest`, `StakingReward`, `AirdropIncome`) count toward the income tax estimate; other tags cover cashback, gifts, transfers, and no gain/no loss. `Cashback` is an ordinary acquisition at market value but **not** income — HMRC treats cashback on personal spending as tax-free (Statement of Practice 4/97).

GBP deposits tagged `Salary`, `OtherIncome`, `Dividend`, `Interest`, or `Cashback` need no `valuation` (the amount is the value); other assets require one to establish market value. Quantities must be positive and fees non-negative; violations are rejected with an error.

**Fees paid in crypto.** Following HMRC (CRYPTO22280), tokens spent on a fee are a disposal of those tokens at market value, and the fee's value is an allowable cost of the transaction it paid for. taxc therefore records a separate disposal of `fee.amount` of `fee.asset` alongside the transaction. Record `sold`/`amount` quantities **net of the fee** — the fee tokens go in `fee`, not in the traded quantity — or they will be disposed of twice. A GBP fee is an allowable cost only.

**Linked transfers** (`linked_deposit`/`linked_withdrawal`) move one asset between your own accounts and are not disposals. Both legs must be the same asset, and the deposit cannot exceed the withdrawal. A fee paid in the moved asset is disposed of as above; any further amount that left but did not arrive is recorded as an unclassified disposal and flagged for review, rather than staying in the pool.

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

`datetime` may carry any UTC offset (a date-only or offset-less value is read as UTC). Every tax rule counts **UK calendar days**: the tax year, the same-day rule and the 30-day bed-and-breakfast window all use the date in Europe/London time. So `2024-04-05T23:30:00Z` — 00:30 BST on 6 April — falls in 2024/25. Report timestamps are shown in UK local time.

## HMRC Share Identification Rules

Disposals are matched against acquisitions in order:

1. **Same-Day Rule** — acquisitions on the same day
2. **Bed & Breakfast Rule** — acquisitions within 30 days after the disposal
3. **Section 104 Pool** — remaining shares from the pooled cost basis

All acquisitions of an asset on one UK day are treated as a single acquisition (TCGA 1992 s105). Sterling is not a chargeable asset, so GBP never enters a pool. Estimated tax is rounded down to the penny, as HMRC does.

Losses are netted against all gains in the same tax year. taxc does not model the connected-person rule (TCGA 1992 s18), under which a loss on a disposal to a connected person — typically a family gift — can only be set against gains on disposals to that same person; review such losses by hand.

## Tax Years Supported

CGT annual exempt amounts and rates (non-residential-property assets, e.g. crypto and shares):

| Tax years         | Annual exempt amount | Basic rate | Higher rate |
| ----------------- | -------------------- | ---------- | ----------- |
| 2024/25 onwards   | £3,000               | 18%        | 24%         |
| 2023/24           | £6,000               | 10%        | 20%         |
| 2016/17 – 2022/23 | £11,100 – £12,300    | 10%        | 20%         |
| 2010/11 – 2015/16 | £10,100 – £11,100    | 18%        | 28%         |
| 2007/08 – 2009/10 | £9,200 – £10,100     | 18%        | 28%         |

> **Note:** CGT rates changed mid-year on 30 October 2024 (10%/20% → 18%/24%).
> For 2024/25 each gain is taxed at the rate in force on its disposal date, and
> losses and the AEA are set against the 18%/24% gains first — the allocation
> HMRC permits that gives the lowest liability. Rates before 2010/11 are
> approximate.

Income tax is a flat estimate by band:

- **Dividends** are taxed at the dividend rates after the dividend allowance (£500 from 2024/25, £1,000 in 2023/24, £2,000 from 2018/19, £5,000 in 2016/17–2017/18). Rates are 8.75%/33.75%/39.35% for 2022/23–2025/26, 10.75%/35.75%/39.35% from 2026/27, and 7.5%/32.5%/38.1% for 2016/17–2021/22.
- **Other income** (e.g. staking rewards, interest) is taxed at 20%/40%/45%. The personal allowance and personal savings allowance are not applied.
- **Salary** is PAYE-settled and excluded, as above.

## Library

`taxc` is also a Rust library: build the input document with compile-time checking and run the same calculations the CLI does. Depend on it by git tag:

```toml
[dependencies]
taxc = { git = "https://github.com/ascjones/taxc", tag = "<latest release tag>" }
```

The stable public surface:

- `taxc::input` — the input document root `Transactions` and its field types (`Asset`, `Transaction`, `Amount`, `Valuation`, `Tag`, …), plus `TransactionError`, the typed rejection returned by validation
- `taxc::results` — calculation outputs (`TaxSummary`, `CgtReport`, `TaxableEvent`, `Warning`, `TaxYear`, `TaxBand`, …)
- `taxc::validate(&doc, &options)` — check a document the way the CLI would, returning the first `TransactionError` (wrapped in `taxc::Error`)
- `taxc::calculate(doc, &CalculationOptions)` — run CGT matching and the per-year summary (CGT after AEA, income by tag, warnings), returning `TaxResults` as plain values with no formatting
- `taxc::input_schema()` — the input JSON Schema, identical to `taxc schema input`

Serialization contract for `taxc::input` types: optional fields are omitted when absent (never `null`), the default `Unclassified` tag is omitted, decimal quantities are written as numeric strings (`"0.5"`, exact through any JSON parser; bare numbers are still accepted on input), and UTC datetimes end in `Z`. Everything outside these paths is internal and may change without notice.

```rust
let doc: taxc::input::Transactions = serde_json::from_str(json)?;
let options = taxc::CalculationOptions::default();
taxc::validate(&doc, &options)?;
let results = taxc::calculate(doc, &options)?;
for year in &results.years {
    println!("{}: {}", year.summary.tax_year.display(), year.summary.estimated_total_tax);
}
```

## Development

Enable pre-commit hooks (runs fmt, clippy, and tests):

```bash
git config core.hooksPath .githooks
```

Project structure:

- `src/main.rs` — CLI binary entry point (calls `taxc::cli::run`)
- `src/lib.rs` — library surface (`taxc::input`, `taxc::results`, `validate`, `calculate`)
- `src/cli.rs` — Clap command wiring
- `src/cmd/` — CLI command implementations
- `src/core/` — domain logic and tax calculations (flat public surface via re-exports)

## License

MIT
