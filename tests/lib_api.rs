//! Tests of the public library surface: document construction, serialization
//! contract, typed validation errors, calculation parity with the CLI, and
//! the input schema.

mod common;

use chrono::DateTime;
use common::run_taxc;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use taxc::input::{
    Amount, Asset, AssetClass, Fee, Price, Tag, Transaction, TransactionError, TransactionType,
    Transactions, Valuation,
};
use taxc::results::{TaxBand, TaxYear};
use taxc::CalculationOptions;

fn tx(id: &str, datetime: &str, details: TransactionType) -> Transaction {
    Transaction {
        id: id.to_string(),
        datetime: DateTime::parse_from_rfc3339(datetime).unwrap(),
        account: "kraken".to_string(),
        description: None,
        valuation: None,
        fee: None,
        tag: Tag::Unclassified,
        details,
    }
}

fn amount(asset: &str, quantity: rust_decimal::Decimal) -> Amount {
    Amount {
        asset: asset.to_string(),
        quantity,
    }
}

/// A document exercising every field a producer emits: a GBP trade with a
/// fee, a crypto-to-crypto trade with a price valuation, a linked transfer
/// pair, a GBP salary deposit with no valuation, and a priced dividend.
fn representative_document() -> Transactions {
    Transactions {
        assets: vec![
            Asset {
                symbol: "BTC".to_string(),
                asset_class: AssetClass::Crypto,
            },
            Asset {
                symbol: "ETH".to_string(),
                asset_class: AssetClass::Crypto,
            },
        ],
        transactions: vec![
            Transaction {
                description: Some("Buy BTC".to_string()),
                fee: Some(Fee {
                    asset: "GBP".to_string(),
                    amount: dec!(10),
                    price: None,
                }),
                tag: Tag::Trade,
                ..tx(
                    "t1",
                    "2024-05-01T10:00:00Z",
                    TransactionType::Trade {
                        sold: amount("GBP", dec!(10000)),
                        bought: amount("BTC", dec!(0.5)),
                    },
                )
            },
            Transaction {
                valuation: Some(Valuation::Price(Price {
                    base: "ETH".to_string(),
                    quote: Some("USD".to_string()),
                    rate: dec!(3000),
                    fx_rate: Some(dec!(0.8)),
                    source: Some("coingecko".to_string()),
                })),
                ..tx(
                    "t2",
                    "2024-06-01T10:00:00Z",
                    TransactionType::Trade {
                        sold: amount("BTC", dec!(0.25)),
                        bought: amount("ETH", dec!(5)),
                    },
                )
            },
            tx(
                "t3",
                "2024-07-01T10:00:00Z",
                TransactionType::Withdrawal {
                    amount: amount("ETH", dec!(2)),
                    linked_deposit: Some("t4".to_string()),
                },
            ),
            Transaction {
                account: "ledger".to_string(),
                ..tx(
                    "t4",
                    "2024-07-01T11:00:00Z",
                    TransactionType::Deposit {
                        amount: amount("ETH", dec!(2)),
                        linked_withdrawal: Some("t3".to_string()),
                    },
                )
            },
            Transaction {
                account: "bank".to_string(),
                tag: Tag::Salary,
                ..tx(
                    "t5",
                    "2024-06-30T09:00:00Z",
                    TransactionType::Deposit {
                        amount: amount("GBP", dec!(1000)),
                        linked_withdrawal: None,
                    },
                )
            },
            Transaction {
                account: "bank".to_string(),
                tag: Tag::Dividend,
                ..tx(
                    "t6",
                    "2024-08-01T09:00:00Z",
                    TransactionType::Deposit {
                        amount: amount("GBP", dec!(200)),
                        linked_withdrawal: None,
                    },
                )
            },
            Transaction {
                tag: Tag::Trade,
                ..tx(
                    "t7",
                    "2024-09-01T10:00:00Z",
                    TransactionType::Trade {
                        sold: amount("BTC", dec!(0.25)),
                        bought: amount("GBP", dec!(15000)),
                    },
                )
            },
        ],
    }
}

/// Pins the wire format: optional fields absent (never null), default tag
/// omitted, decimals as numeric strings, `type` as an internal tag.
#[test]
fn document_serializes_to_the_cli_wire_format() {
    let value = serde_json::to_value(representative_document()).unwrap();
    assert_eq!(
        value,
        json!({
            "assets": [
                { "symbol": "BTC", "asset_class": "Crypto" },
                { "symbol": "ETH", "asset_class": "Crypto" }
            ],
            "transactions": [
                {
                    "id": "t1",
                    "datetime": "2024-05-01T10:00:00Z",
                    "account": "kraken",
                    "description": "Buy BTC",
                    "fee": { "asset": "GBP", "amount": "10" },
                    "tag": "Trade",
                    "type": "Trade",
                    "sold": { "asset": "GBP", "quantity": "10000" },
                    "bought": { "asset": "BTC", "quantity": "0.5" }
                },
                {
                    "id": "t2",
                    "datetime": "2024-06-01T10:00:00Z",
                    "account": "kraken",
                    "valuation": {
                        "base": "ETH",
                        "quote": "USD",
                        "rate": "3000",
                        "fx_rate": "0.8",
                        "source": "coingecko"
                    },
                    "type": "Trade",
                    "sold": { "asset": "BTC", "quantity": "0.25" },
                    "bought": { "asset": "ETH", "quantity": "5" }
                },
                {
                    "id": "t3",
                    "datetime": "2024-07-01T10:00:00Z",
                    "account": "kraken",
                    "type": "Withdrawal",
                    "amount": { "asset": "ETH", "quantity": "2" },
                    "linked_deposit": "t4"
                },
                {
                    "id": "t4",
                    "datetime": "2024-07-01T11:00:00Z",
                    "account": "ledger",
                    "type": "Deposit",
                    "amount": { "asset": "ETH", "quantity": "2" },
                    "linked_withdrawal": "t3"
                },
                {
                    "id": "t5",
                    "datetime": "2024-06-30T09:00:00Z",
                    "account": "bank",
                    "tag": "Salary",
                    "type": "Deposit",
                    "amount": { "asset": "GBP", "quantity": "1000" }
                },
                {
                    "id": "t6",
                    "datetime": "2024-08-01T09:00:00Z",
                    "account": "bank",
                    "tag": "Dividend",
                    "type": "Deposit",
                    "amount": { "asset": "GBP", "quantity": "200" }
                },
                {
                    "id": "t7",
                    "datetime": "2024-09-01T10:00:00Z",
                    "account": "kraken",
                    "tag": "Trade",
                    "type": "Trade",
                    "sold": { "asset": "BTC", "quantity": "0.25" },
                    "bought": { "asset": "GBP", "quantity": "15000" }
                }
            ]
        })
    );
}

#[test]
fn document_round_trips_through_json() {
    let original = representative_document();
    let json = serde_json::to_string(&original).unwrap();
    let parsed: Transactions = serde_json::from_str(&json).unwrap();
    let again = serde_json::to_string(&parsed).unwrap();
    assert_eq!(json, again);
    taxc::validate(&parsed).expect("round-tripped document is valid");
}

#[test]
fn document_parses_when_embedded_in_a_larger_envelope() {
    let mut envelope = serde_json::to_value(representative_document()).unwrap();
    envelope["warnings"] = json!(["something"]);
    envelope["unexported_count"] = json!(3);
    let parsed: Transactions = serde_json::from_value(envelope).unwrap();
    assert_eq!(parsed.transactions.len(), 7);
}

#[test]
fn value_gbp_valuation_serializes_as_a_string() {
    let t = Transaction {
        valuation: Some(Valuation::ValueGbp(dec!(15000.5))),
        ..tx(
            "v",
            "2024-06-01T10:00:00Z",
            TransactionType::Trade {
                sold: amount("BTC", dec!(0.25)),
                bought: amount("ETH", dec!(5)),
            },
        )
    };
    let value = serde_json::to_value(&t).unwrap();
    assert_eq!(value["valuation"], json!("15000.5"));
}

#[test]
fn validate_reports_gbp_income_valuation_as_typed_error() {
    let mut doc = representative_document();
    doc.transactions[4].valuation = Some(Valuation::ValueGbp(dec!(1000)));
    assert_eq!(
        taxc::validate(&doc).unwrap_err(),
        TransactionError::GbpIncomeValuationNotAllowed {
            id: "t5".to_string(),
            tag: "Salary".to_string(),
        }
    );
}

#[test]
fn validate_reports_undefined_asset_as_typed_error() {
    let mut doc = representative_document();
    doc.assets.pop();
    assert_eq!(
        taxc::validate(&doc).unwrap_err(),
        TransactionError::UndefinedAsset {
            symbol: "ETH".to_string()
        }
    );
}

fn cli_summary(path: &std::path::Path, band: &str) -> Value {
    let output = run_taxc(&[
        "summary",
        path.to_str().unwrap(),
        "--json",
        "--year",
        "2025",
        "--tax-band",
        band,
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn f64_of(d: rust_decimal::Decimal) -> f64 {
    use rust_decimal::prelude::ToPrimitive;
    d.round_dp(2).to_f64().unwrap()
}

/// The library returns the same numbers `taxc summary --json` prints.
#[test]
fn calculate_matches_cli_summary() {
    let doc = representative_document();
    let path = common::unique_tmp_file("lib-api", "json");
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

    for (band, band_name) in [(TaxBand::Basic, "basic"), (TaxBand::Higher, "higher")] {
        let cli = cli_summary(&path, band_name);
        let results = taxc::calculate(
            doc.clone(),
            &CalculationOptions {
                tax_band: band,
                tax_year: Some(TaxYear(2025)),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(results.years.len(), 1);
        let s = &results.years[0].summary;
        assert_eq!(s.tax_year, TaxYear(2025));
        assert_eq!(cli["tax_year"], "2024/25");
        assert_eq!(cli["disposal_count"], s.cgt.disposal_count);
        assert_eq!(cli["gross_gains"], f64_of(s.cgt.summary.gross_gains));
        assert_eq!(cli["in_year_losses"], f64_of(s.cgt.summary.in_year_losses));
        assert_eq!(cli["aea"], f64_of(s.cgt.summary.aea));
        assert_eq!(cli["taxable_gain"], f64_of(s.cgt.summary.taxable_gain));
        assert_eq!(cli["estimated_cgt"], f64_of(s.cgt.estimated_cgt));
        assert_eq!(cli["income"], f64_of(s.income.taxable));
        assert_eq!(cli["salary_income"], f64_of(s.income.salary));
        assert_eq!(cli["dividend_income"], f64_of(s.income.dividend));
        assert_eq!(
            cli["estimated_income_tax"],
            f64_of(s.income.estimated_income_tax)
        );
        assert_eq!(cli["estimated_total_tax"], f64_of(s.estimated_total_tax));

        // Sanity-check the values are non-trivial, not just mutually zero.
        assert_eq!(s.cgt.disposal_count, 2);
        assert_eq!(s.income.salary, dec!(1000));
        assert_eq!(s.income.by_tag[&Tag::Dividend], dec!(200));
        assert_eq!(s.income.taxable, dec!(200));
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn calculate_summarises_every_year_with_events_by_default() {
    let mut doc = representative_document();
    doc.transactions.push(Transaction {
        tag: Tag::Trade,
        ..tx(
            "t8",
            "2025-06-01T10:00:00Z",
            TransactionType::Trade {
                sold: amount("ETH", dec!(1)),
                bought: amount("GBP", dec!(2000)),
            },
        )
    });
    let results = taxc::calculate(doc, &CalculationOptions::default()).unwrap();
    let years: Vec<TaxYear> = results.years.iter().map(|y| y.summary.tax_year).collect();
    assert_eq!(years, vec![TaxYear(2025), TaxYear(2026)]);
    assert_eq!(results.events.len(), results.cgt.pool_history.entries.len());
}

#[test]
fn calculate_surfaces_event_warnings() {
    let doc = Transactions {
        assets: vec![Asset {
            symbol: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
        }],
        transactions: vec![Transaction {
            tag: Tag::Trade,
            ..tx(
                "sell-without-basis",
                "2024-09-01T10:00:00Z",
                TransactionType::Trade {
                    sold: amount("BTC", dec!(1)),
                    bought: amount("GBP", dec!(15000)),
                },
            )
        }],
    };
    let results = taxc::calculate(doc, &CalculationOptions::default()).unwrap();
    let warnings = &results.years[0].warnings;
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].source_transaction_id, "sell-without-basis");
    assert!(matches!(
        warnings[0].warning,
        taxc::results::Warning::InsufficientCostBasis { .. }
    ));
}

#[test]
fn input_schema_matches_cli_schema_command() {
    let output = run_taxc(&["schema", "input"]);
    assert!(output.status.success());
    let cli: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(taxc::input_schema(), cli);
}
