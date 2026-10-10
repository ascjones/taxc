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

All commands take an optional positional `FILE` (JSON); if omitted or `-`, input is read from stdin. Filtering commands share `-y`/`--from`/`--to` (date), `-a` (asset), `--event-kind` (disposal/acquisition/adjustment), and `--exclude-unlinked`.

### `taxc summary`

Aggregated CGT and income calculations. Filter with `-y 2025` or `--from`/`--to`; add `--json` for machine-readable output, `-t higher` for a different tax band.

In `--json` output every monetary field is a 2dp string (`"12345.67"`), matching `taxc report --json`, so amounts survive JSON parsing exactly. Counts and the `*_rate_pct` fields are numbers (`dividend_rate_pct` can be fractional, e.g. `8.75`); `dividend_allowance` is the money amount of the year's dividend allowance. `cgt_rate_pct` is `null` whenever no single rate explains `estimated_cgt` — the years summed apply different rates, or the year is 2024/25, whose rates changed on 30 October 2024. `estimated_cgt` is always authoritative.

When the selected range spans more than one tax year, each year is summarised on its own — with that year's AEA and rates, and losses netted only within the year — and the totals are the sums. The text output prints a block per year; the JSON carries the per-year figures in `years` and the sums at the top level, where `tax_year` reads e.g. `"2022/23 to 2024/25"` and a `*_rate_pct` is `null` if the years' rates differ. Use `-y` to select a single year.

Unclassified disposals (untagged withdrawals, unexplained transfer shortfalls) are left out of the figures. The text output ends with a note when there are any, and the JSON counts them in `unclassified_disposal_count`; run `taxc report` to review and classify them.

Salary is treated as PAYE-settled (already taxed at source): it is reported on its own line (`salary_income` in JSON) but excluded from the income tax estimate, since UK employers must operate PAYE even on salary paid in crypto. For the rare case of employment income received gross (non-RCA tokens, or an overseas employer with no UK presence), tag it `OtherIncome` instead.

### `taxc report`

Self-contained HTML report, opened in your browser: summary cards, interactive filtering, sortable columns, expandable per-disposal detail (fees, warnings, matching), and a Tax Years view with a gain/loss chart. Use `-o file.html` to save instead, or `--json` for structured data. Timestamps are in UK local time, quantities use the same 8-decimal rounding as `taxc pools`, and `summary.disposal_count` counts classified disposals (unclassified ones are in the `*_with_unclassified` totals). The CLI filters (`-y`, `--from`/`--to`, `-a`, `--event-kind`) also narrow the Transactions tab.

Pool adjustments (demergers, rights issues, small capital distributions) are listed as events with `event_kind: "adjustment"` and `event_type` naming the reorganisation. They are never disposals, acquisitions or income in any total, with one exception: when a small capital distribution exceeds its pool's cost, its row carries the excess as a gain (`cgt`) with a `CapitalDistributionExceedsCost` warning, and that gain counts in the totals like a disposal's. It follows its row under `--event-kind`: `adjustment` keeps it, `disposal` leaves it out.

### `taxc pools`

Section 104 pool balances over time — year-end snapshots by default, `--daily` for daily history. The daily history includes pool adjustments, labelled by type (`Demerger`, `RightsIssue`, `SmallCapitalDistribution`); a demerger has an entry for both the original and the new holding. Every tax year from the first event to the last gets a snapshot, including years with no activity. Sterling is not a chargeable asset and never appears as a pool.

Quantities render to at most 8 decimal places, rounded half away from zero — the same rule monetary amounts use. A quantity carrying more decimals is rounded, not truncated, so a non-zero balance below `0.00000001` shows as `0.00000001` rather than `0`.

### `taxc schema`

Print the JSON schema for the input (default) or output (`taxc schema output`) format. Schemas are also checked into `schema/`.

## Input Format

JSON with top-level `assets` and `transactions` fields — run `taxc schema input` for the full schema. Seven transaction types:

- **Trade** — asset swap (`sold`/`bought`)
- **Deposit** — asset received (`amount`)
- **Withdrawal** — asset sent (`amount`)
- **Demerger** — part of a holding's cost moves to a new holding (`original`, `new_holding`, `cost_fraction`)
- **RightsIssue** — rights shares join an existing holding (`new_shares`, `consideration`)
- **SmallCapitalDistribution** — a distribution reduces a holding's cost (`asset`, `amount`)
- **Fee** — a fee paid with nothing else moving (`fee`)

The last four are described under [Share reorganisations and fee-only transactions](#share-reorganisations-and-fee-only-transactions).

An optional `tag` classifies a transaction for tax. Income tags (`Salary`, `OtherIncome`, `Dividend`, `Interest`, `StakingReward`, `AirdropIncome`) count toward the income tax estimate; other tags cover cashback, gifts, transfers, and no gain/no loss. `Cashback` is an ordinary acquisition at market value but **not** income — HMRC treats cashback on personal spending as tax-free (Statement of Practice 4/97).

GBP deposits tagged `Salary`, `OtherIncome`, `Dividend`, `Interest`, or `Cashback` need no `valuation` (the amount is the value); other assets require one to establish market value. Quantities must be positive, and fees and `valuation` amounts non-negative; violations are rejected with an error.

**Fees paid in crypto.** Following HMRC (CRYPTO22280), tokens spent on a fee are a disposal of those tokens at market value, and the fee's value is an allowable cost of the transaction it paid for. taxc therefore records a separate disposal of `fee.amount` of `fee.asset` alongside the transaction. **Every quantity excludes the fee**: `sold`, `bought` and `amount` are the amounts traded or moved, and the fee is a separate outflow recorded only in `fee`. So a buy of 1 ETH with a 0.01 ETH fee taken from it is `bought: 1` plus `fee: 0.01 ETH`, and a sale of 1 ETH that also cost 0.01 ETH is `sold: 1` plus that fee. Folding the fee into a quantity disposes of it twice. A GBP fee is an allowable cost only.

**Linked transfers** (`linked_deposit`/`linked_withdrawal`) move one asset between your own accounts and are not disposals. Both legs must be the same asset, and the deposit cannot exceed the withdrawal. A fee on either leg is disposed of as above. Because quantities exclude fees, a transfer that arrived intact has equal quantities on both legs; any amount sent that did not arrive is recorded as an unclassified disposal and flagged for review, rather than staying in the pool.

### Example

```json
{
  "assets": [
    { "symbol": "BTC", "asset_class": "Crypto" },
    { "symbol": "ETH", "asset_class": "Crypto" },
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

### Share reorganisations and fee-only transactions

`Demerger`, `RightsIssue` and `SmallCapitalDistribution` change a Section 104 pool without a disposal. They are never matched under the same-day or 30-day rules, and on any UK date they apply before that day's disposals are matched, in time order (at the same instant: rights issue, then demerger, then small capital distribution). taxc does not test whether a reorganisation qualifies: choosing the type is your assertion that it does, so check the company's tax guidance. These three types take no `tag` and no `valuation`, and never apply to GBP.

- **Demerger** — a demerger the company's guidance treats as a share reorganisation: an exempt distribution (TCGA 1992 s.192; CTA 2010 s.1076) or a scheme of reconstruction (s.136). `cost_fraction` (strictly between 0 and 1) of the `original` pool's cost moves to the `new_holding`, apportioned by market value: on the first dealing day if the shares are quoted (s.130), otherwise at the first disposal (s.129); the moved cost is rounded to the penny. No disposal is recorded, and the new holding counts as held since the original shares were. HMRC: [CG45620](https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg45620), [CG51702](https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51702), [CG51890](https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51890), [CG52742](https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg52742). A demerger taxed as a dividend in specie is not a `Demerger`: record a `Dividend`-tagged `Deposit` of the new shares at market value. No fee is allowed.
- **RightsIssue** — take-up of your own pro-rata rights entitlement in the same company. The `new_shares` and their `consideration` (GBP, plus any `fee`) join the existing pool (TCGA 1992 s.126(2)(a), s.127, s.128; HMRC [CG51746](https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51746), [CG51590](https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51590)). Shares from purchased rights or excess applications, and rights to shares in another company ([CG52065](https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg52065)), are a `Trade`.
- **SmallCapitalDistribution** — a capital distribution small enough to reduce the holding's allowable cost instead of being a disposal (TCGA 1992 s.122(2); HMRC [CG57835](https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg57835)), including cash for fractional entitlements on a reorganisation (s.128(3); [CG57855](https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg57855)). HMRC treats a distribution as small when it is 5% or less of the holding's value, or £3,000 or less. If `amount` exceeds the pool's cost, the cost becomes zero and the excess is a chargeable gain, as under a s.122(4) election ([CG57847](https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg57847)), with a warning. A distribution you elect to treat as a part disposal ([CG57838](https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg57838)) is a `Trade`. No fee is allowed.

A demerger or small capital distribution on an asset with no pool carries an `InsufficientCostBasis` warning: the demerger still adds the new holding (at zero cost), and the whole distribution is a gain. Share conversions (s.135 exchanges), share splits and consolidations, and the unquoted-shares timing rule (s.129) are not modelled; adjust earlier acquisitions for a split or consolidation yourself.

- **Fee** — a fee paid with nothing else moving, such as a network fee on a staking operation. `fee` is required and must be positive; a non-GBP fee needs its own `price`. The fee tokens are disposed of at market value (HMRC [CRYPTO22100](https://www.gov.uk/hmrc-internal-manuals/cryptoassets-manual/crypto22100), [CRYPTO22280](https://www.gov.uk/hmrc-internal-manuals/cryptoassets-manual/crypto22280)), exactly as a fee attached to another transaction is. A GBP fee records nothing. Takes no `tag` and no `valuation`.

```json
{
  "assets": [
    { "symbol": "ULVR", "asset_class": "Stock" },
    { "symbol": "MICC", "asset_class": "Stock" },
    { "symbol": "CSN", "asset_class": "Stock" },
    { "symbol": "DOT", "asset_class": "Crypto" }
  ],
  "transactions": [
    {
      "id": "demerger",
      "datetime": "2025-12-17T08:00:00Z",
      "account": "ii",
      "type": "Demerger",
      "original": "ULVR",
      "new_holding": { "asset": "MICC", "quantity": "177" },
      "cost_fraction": "0.051151"
    },
    {
      "id": "rights",
      "datetime": "2025-07-18T09:00:00+01:00",
      "account": "ii",
      "type": "RightsIssue",
      "new_shares": { "asset": "CSN", "quantity": "1473" },
      "consideration": "2592.48"
    },
    {
      "id": "fractional-cash",
      "datetime": "2025-12-18T08:00:00Z",
      "account": "ii",
      "type": "SmallCapitalDistribution",
      "asset": "ULVR",
      "amount": "21.64"
    },
    {
      "id": "network-fee",
      "datetime": "2025-06-02T10:00:00+01:00",
      "account": "polkadot",
      "type": "Fee",
      "fee": { "asset": "DOT", "amount": "0.02", "price": { "base": "DOT", "rate": "5.00" } }
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

All acquisitions of an asset on one UK day are treated as a single acquisition (TCGA 1992 s105). Pool adjustments (see [Share reorganisations](#share-reorganisations-and-fee-only-transactions)) are not acquisitions, so they are never matched; they apply to the pool before the day's disposals. Sterling is not a chargeable asset, so GBP never enters a pool. Estimated tax is rounded down to the penny, as HMRC does.

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
- `taxc::results` — calculation outputs (`TaxSummary`, `CgtReport`, `TaxableEvent`, `EventType`, `AdjustmentKind`, `Warning`, `TaxYear`, `TaxBand`, …). `CgtReport::warnings_for_adjustment` returns the warnings raised applying a pool adjustment; `TaxYearResults::warnings` already includes them
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
