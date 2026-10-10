use super::datetime::parse_datetime;
use super::*;
use crate::core::events::{AdjustmentKind, AssetClass, DemergedFrom, EventType, Tag, TaxableEvent};
use crate::core::price::Price;
use chrono::{DateTime, FixedOffset};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn dt(s: &str) -> DateTime<FixedOffset> {
    parse_datetime(s).unwrap()
}

/// Helper to create a direct GBP price
fn gbp_price(base: &str, rate: Decimal) -> Price {
    Price {
        base: base.to_string(),
        rate,
        source: None,
        quote: None,
        fx_rate: None,
    }
}

/// Helper to create an FX price
fn fx_price(base: &str, rate: Decimal, quote: &str, fx_rate: Decimal) -> Price {
    Price {
        base: base.to_string(),
        rate,
        source: None,
        quote: Some(quote.to_string()),
        fx_rate: Some(fx_rate),
    }
}

fn test_registry() -> AssetRegistry {
    let mut registry = AssetRegistry::new();
    for symbol in ["BTC", "ETH", "USDT", "BNB", "DOT"] {
        registry.insert(
            symbol.to_string(),
            Asset {
                symbol: symbol.to_string(),
                asset_class: AssetClass::Crypto,
            },
        );
    }
    registry.insert(
        "AAPL".to_string(),
        Asset {
            symbol: "AAPL".to_string(),
            asset_class: AssetClass::Stock,
        },
    );
    registry
}

#[derive(Debug, Clone)]
struct TransactionBuilder {
    tx: Transaction,
}

impl TransactionBuilder {
    fn new(tx: Transaction) -> Self {
        Self { tx }
    }

    fn with_tag(mut self, tag: Tag) -> Self {
        self.tx.tag = tag;
        self
    }

    fn with_price(mut self, price: Price) -> Self {
        self.tx.valuation = Some(Valuation::Price(price));
        self
    }

    fn with_value_gbp(mut self, value_gbp: Decimal) -> Self {
        self.tx.valuation = Some(Valuation::ValueGbp(value_gbp));
        self
    }

    fn with_fee(mut self, fee: Fee) -> Self {
        self.tx.fee = Some(fee);
        self
    }

    fn with_deposit_link(mut self, link: &str) -> Self {
        match &mut self.tx.details {
            TransactionType::Deposit {
                linked_withdrawal, ..
            } => *linked_withdrawal = Some(link.to_string()),
            _ => panic!("deposit_link expects a deposit transaction"),
        }
        self
    }

    fn with_withdrawal_link(mut self, link: &str) -> Self {
        match &mut self.tx.details {
            TransactionType::Withdrawal { linked_deposit, .. } => {
                *linked_deposit = Some(link.to_string())
            }
            _ => panic!("withdrawal_link expects a withdrawal transaction"),
        }
        self
    }

    fn datetime(mut self, value: &str) -> Self {
        self.tx.datetime = dt(value);
        self
    }

    fn build(self) -> Transaction {
        self.tx
    }
}

impl AsRef<Transaction> for TransactionBuilder {
    fn as_ref(&self) -> &Transaction {
        &self.tx
    }
}

fn trade_tx(id: &str, sold: (&str, Decimal), bought: (&str, Decimal)) -> TransactionBuilder {
    TransactionBuilder::new(Transaction {
        id: id.to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "test".to_string(),
        description: None,
        valuation: None,
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: sold.0.to_string(),
                quantity: sold.1,
            },
            bought: Amount {
                asset: bought.0.to_string(),
                quantity: bought.1,
            },
        },
    })
}

fn deposit_tx(id: &str, asset: &str, qty: Decimal) -> TransactionBuilder {
    TransactionBuilder::new(Transaction {
        id: id.to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "test".to_string(),
        description: None,
        valuation: None,
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: asset.to_string(),
                quantity: qty,
            },
            linked_withdrawal: None,
        },
    })
}

fn withdrawal_tx(id: &str, asset: &str, qty: Decimal) -> TransactionBuilder {
    TransactionBuilder::new(Transaction {
        id: id.to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "test".to_string(),
        description: None,
        valuation: None,
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Withdrawal {
            amount: Amount {
                asset: asset.to_string(),
                quantity: qty,
            },
            linked_deposit: None,
        },
    })
}

fn convert_one<T: AsRef<Transaction>>(tx: &T) -> Result<Vec<TaxableEvent>, TransactionError> {
    tx.as_ref().to_taxable_events(&test_registry(), false)
}

fn convert_all<T: AsRef<Transaction>>(txs: &[T]) -> Result<Vec<TaxableEvent>, TransactionError> {
    let txs: Vec<Transaction> = txs.iter().map(|tx| tx.as_ref().clone()).collect();
    transactions_to_events(
        &txs,
        &test_registry(),
        ConversionOptions {
            exclude_unlinked: false,
        },
    )
}

#[test]
fn price_gbp_multiplies_rate() {
    let price = gbp_price("BTC", dec!(2000));
    assert_eq!(price.to_gbp(dec!(0.5)).unwrap(), dec!(1000));
}

#[test]
fn price_fx_chain_applies_fx() {
    let price = fx_price("BTC", dec!(40000), "USD", dec!(0.79));
    assert_eq!(price.to_gbp(dec!(0.5)).unwrap(), dec!(15800));
}

#[test]
fn price_rejects_negative_rate() {
    let price = gbp_price("BTC", dec!(-1000));
    let err = price.to_gbp(dec!(1));
    assert!(
        matches!(err, Err(TransactionError::InvalidPrice(_))),
        "negative rate must be rejected, got {err:?}"
    );
}

#[test]
fn price_rejects_zero_rate() {
    let price = gbp_price("BTC", dec!(0));
    let err = price.to_gbp(dec!(1));
    assert!(
        matches!(err, Err(TransactionError::InvalidPrice(_))),
        "zero rate must be rejected (would produce a gain with no cost basis), got {err:?}"
    );
}

#[test]
fn price_rejects_negative_fx_rate() {
    let price = fx_price("BTC", dec!(40000), "USD", dec!(-0.79));
    let err = price.to_gbp(dec!(1));
    assert!(
        matches!(err, Err(TransactionError::InvalidPrice(_))),
        "negative fx_rate must be rejected, got {err:?}"
    );
}

#[test]
fn price_rejects_zero_fx_rate() {
    let price = fx_price("BTC", dec!(40000), "USD", dec!(0));
    let err = price.to_gbp(dec!(1));
    assert!(
        matches!(err, Err(TransactionError::InvalidPrice(_))),
        "zero fx_rate must be rejected, got {err:?}"
    );
}

#[test]
fn price_rejects_quote_without_fx_rate() {
    // A foreign-currency quote with no fx_rate cannot be converted to GBP.
    let price = Price {
        base: "BTC".to_string(),
        rate: dec!(40000),
        source: None,
        quote: Some("USD".to_string()),
        fx_rate: None,
    };
    let err = price.to_gbp(dec!(1));
    assert!(
        matches!(err, Err(TransactionError::InvalidPrice(_))),
        "quote without fx_rate must be rejected, got {err:?}"
    );
}

#[test]
fn price_rejects_fx_rate_without_quote() {
    // An fx_rate with no quote currency is ambiguous and must be rejected.
    let price = Price {
        base: "BTC".to_string(),
        rate: dec!(40000),
        source: None,
        quote: None,
        fx_rate: Some(dec!(0.79)),
    };
    let err = price.to_gbp(dec!(1));
    assert!(
        matches!(err, Err(TransactionError::InvalidPrice(_))),
        "fx_rate without quote must be rejected, got {err:?}"
    );
}

#[test]
fn price_rejects_empty_quote() {
    // A blank quote currency must be rejected even when fx_rate is present.
    let price = fx_price("BTC", dec!(40000), "", dec!(0.79));
    let err = price.to_gbp(dec!(1));
    assert!(
        matches!(err, Err(TransactionError::InvalidPrice(_))),
        "empty quote must be rejected, got {err:?}"
    );
}

#[test]
fn parse_datetime_accepts_space_separated_without_timezone() {
    // 'T' and space separators without a timezone both resolve to the same UTC instant.
    let t = parse_datetime("2024-06-15T10:30:00").unwrap();
    let space = parse_datetime("2024-06-15 10:30:00").unwrap();
    assert_eq!(t, space);
    assert_eq!(t.to_rfc3339(), "2024-06-15T10:30:00+00:00");
}

#[test]
fn parse_datetime_accepts_fractional_seconds() {
    let with_frac = parse_datetime("2024-06-15T10:30:00.123").unwrap();
    let canonical = parse_datetime("2024-06-15T10:30:00.123+00:00").unwrap();
    assert_eq!(with_frac, canonical);
}

#[test]
fn parse_datetime_bare_date_defaults_to_utc_midnight() {
    let bare = parse_datetime("2024-06-15").unwrap();
    assert_eq!(bare.to_rfc3339(), "2024-06-15T00:00:00+00:00");
}

#[test]
fn parse_datetime_rejects_unparseable_input() {
    let err = parse_datetime("not-a-date");
    assert!(
        matches!(err, Err(TransactionError::InvalidDatetime(_))),
        "unparseable datetime must error, got {err:?}"
    );
}

#[test]
fn trade_crypto_to_crypto_generates_two_events() {
    let tx = trade_tx("tx-1", ("BTC", dec!(0.01)), ("ETH", dec!(0.5))).with_price(fx_price(
        "ETH",
        dec!(2000),
        "USD",
        dec!(0.79),
    ));

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, EventType::Disposal);
    assert_eq!(events[1].event_type, EventType::Acquisition);
    assert_eq!(events[0].value_gbp, events[1].value_gbp);
}

#[test]
fn transactions_to_events_assigns_sequential_event_ids() {
    let tx1 = trade_tx("tx-1", ("BTC", dec!(0.01)), ("ETH", dec!(0.5))).with_price(fx_price(
        "ETH",
        dec!(2000),
        "USD",
        dec!(0.79),
    ));
    let tx2 = deposit_tx("tx-2", "ETH", dec!(0.01))
        .with_tag(Tag::StakingReward)
        .with_price(gbp_price("ETH", dec!(2000)))
        .datetime("2024-01-02T10:00:00+00:00");

    let events = convert_all(&[tx1, tx2]).unwrap();

    let ids: Vec<usize> = events.iter().map(|e| e.id).collect();
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn trade_gbp_to_crypto_only_acquisition() {
    let tx = trade_tx("tx-2", ("GBP", dec!(1000)), ("BTC", dec!(0.02)));
    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Acquisition);
    assert_eq!(events[0].value_gbp, dec!(1000));
}

#[test]
fn trade_crypto_to_gbp_only_disposal() {
    let tx = trade_tx("tx-3", ("BTC", dec!(0.02)), ("GBP", dec!(1000)));
    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Disposal);
    assert_eq!(events[0].value_gbp, dec!(1000));
}

#[test]
fn trade_without_price_no_gbp_errors() {
    let tx = trade_tx("tx-4", ("BTC", dec!(0.02)), ("ETH", dec!(0.5)));
    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingTradeValuation {
            id: "tx-4".to_string()
        }
    );
}

#[test]
fn trade_crypto_to_crypto_with_value_gbp() {
    let tx = trade_tx("tx-v1", ("BTC", dec!(0.01)), ("ETH", dec!(0.5))).with_value_gbp(dec!(750));

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].value_gbp, dec!(750));
    assert_eq!(events[1].value_gbp, dec!(750));
}

#[test]
fn linked_deposit_withdrawal_no_events() {
    let deposit = deposit_tx("d1", "ETH", dec!(1)).with_deposit_link("w1");
    let withdrawal = withdrawal_tx("w1", "ETH", dec!(1))
        .with_withdrawal_link("d1")
        .datetime("2024-01-01T09:00:00+00:00");

    let events = convert_all(&[deposit, withdrawal]).unwrap();
    assert!(events.is_empty());
}

#[test]
fn unlinked_crypto_deposit_warns_and_creates_acquisition() {
    let events = convert_all(&[deposit_tx("d1", "ETH", dec!(1))]).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Acquisition);
    assert_eq!(events[0].tag, Tag::Unclassified);
}

#[test]
fn unlinked_withdrawal_creates_disposal() {
    let events = convert_all(&[withdrawal_tx("w1", "ETH", dec!(1))]).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Disposal);
    assert_eq!(events[0].tag, Tag::Unclassified);
}

#[test]
fn gbp_deposit_produces_no_events() {
    let events = convert_all(&[deposit_tx("d1", "GBP", dec!(100))]).unwrap();
    assert!(events.is_empty());
}

#[test]
fn gbp_withdrawal_produces_no_events() {
    let events = convert_all(&[withdrawal_tx("w1", "GBP", dec!(100))]).unwrap();
    assert!(events.is_empty());
}

#[test]
fn unlinked_deposit_with_price() {
    let tx = deposit_tx("d1", "ETH", dec!(2)).with_price(gbp_price("ETH", dec!(1000)));
    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value_gbp, dec!(2000));
}

#[test]
fn unlinked_deposit_with_value_gbp() {
    let tx = deposit_tx("d-value", "ETH", dec!(2)).with_value_gbp(dec!(2000));
    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value_gbp, dec!(2000));
}

#[test]
fn unlinked_withdrawal_with_price() {
    let tx = withdrawal_tx("w1", "ETH", dec!(2)).with_price(gbp_price("ETH", dec!(1000)));
    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value_gbp, dec!(2000));
}

#[test]
fn unlinked_withdrawal_with_value_gbp() {
    let tx = withdrawal_tx("w-value", "ETH", dec!(2)).with_value_gbp(dec!(2000));
    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value_gbp, dec!(2000));
}

#[test]
fn exclude_unlinked_flag_skips_events() {
    let withdrawal = withdrawal_tx("w1", "BTC", dec!(1)).build();

    let events = transactions_to_events(
        &[withdrawal],
        &test_registry(),
        ConversionOptions {
            exclude_unlinked: true,
        },
    )
    .unwrap();
    assert!(events.is_empty());
}

#[test]
fn duplicate_transaction_id_errors() {
    let err = convert_all(&[
        deposit_tx("dup", "ETH", dec!(1)),
        withdrawal_tx("dup", "ETH", dec!(1)),
    ])
    .unwrap_err();
    assert_eq!(
        err,
        TransactionError::DuplicateTransactionId("dup".to_string())
    );
}

#[test]
fn linked_deposit_not_found_errors() {
    let err = convert_all(&[deposit_tx("d1", "ETH", dec!(1)).with_deposit_link("w-missing")])
        .unwrap_err();
    assert_eq!(
        err,
        TransactionError::LinkedTransactionNotFound {
            id: "d1".to_string(),
            linked_id: "w-missing".to_string(),
        }
    );
}

#[test]
fn linked_withdrawal_not_found_errors() {
    let err = convert_all(&[withdrawal_tx("w1", "ETH", dec!(1)).with_withdrawal_link("d-missing")])
        .unwrap_err();
    assert_eq!(
        err,
        TransactionError::LinkedTransactionNotFound {
            id: "w1".to_string(),
            linked_id: "d-missing".to_string(),
        }
    );
}

#[test]
fn linked_deposit_type_mismatch_errors() {
    let d1 = deposit_tx("d1", "ETH", dec!(1)).with_deposit_link("d2");
    let d2 = deposit_tx("d2", "ETH", dec!(1));
    let err = convert_all(&[d1, d2]).unwrap_err();
    assert_eq!(
        err,
        TransactionError::LinkedTransactionTypeMismatch {
            id: "d1".to_string(),
            linked_id: "d2".to_string(),
        }
    );
}

#[test]
fn linked_withdrawal_type_mismatch_errors() {
    let w1 = withdrawal_tx("w1", "ETH", dec!(1)).with_withdrawal_link("w2");
    let w2 = withdrawal_tx("w2", "ETH", dec!(1));
    let err = convert_all(&[w1, w2]).unwrap_err();
    assert_eq!(
        err,
        TransactionError::LinkedTransactionTypeMismatch {
            id: "w1".to_string(),
            linked_id: "w2".to_string(),
        }
    );
}

#[test]
fn linked_deposit_not_reciprocal_errors() {
    let d1 = deposit_tx("d1", "ETH", dec!(1)).with_deposit_link("w1");
    let w1 = withdrawal_tx("w1", "ETH", dec!(1)).with_withdrawal_link("d2");
    let err = convert_all(&[d1, w1]).unwrap_err();
    assert_eq!(
        err,
        TransactionError::LinkedTransactionNotReciprocal {
            id: "d1".to_string(),
            linked_id: "w1".to_string(),
        }
    );
}

#[test]
fn linked_withdrawal_not_reciprocal_errors() {
    let d1 = deposit_tx("d1", "ETH", dec!(1)).with_deposit_link("w2");
    let w1 = withdrawal_tx("w1", "ETH", dec!(1)).with_withdrawal_link("d1");
    let err = convert_all(&[w1, d1]).unwrap_err();
    assert_eq!(
        err,
        TransactionError::LinkedTransactionNotReciprocal {
            id: "w1".to_string(),
            linked_id: "d1".to_string(),
        }
    );
}

#[test]
fn staking_reward_generates_income_event() {
    let tx = deposit_tx("s1", "ETH", dec!(0.01))
        .with_tag(Tag::StakingReward)
        .with_price(gbp_price("ETH", dec!(2000)));

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Acquisition);
    assert_eq!(events[0].tag, Tag::StakingReward);
    assert_eq!(events[0].value_gbp, dec!(20));
}

#[test]
fn fee_allocated_to_disposal() {
    let tx = trade_tx("t1", ("BTC", dec!(1)), ("ETH", dec!(10)))
        .with_price(gbp_price("ETH", dec!(1000)))
        .with_fee(Fee {
            asset: "GBP".to_string(),
            amount: dec!(5),
            price: None,
        });

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].fee_gbp, Some(dec!(5)));
    assert_eq!(events[1].fee_gbp, None);
}

#[test]
fn fee_on_single_event_trade() {
    let cases = [
        trade_tx("t-buy", ("GBP", dec!(1000)), ("BTC", dec!(0.02))),
        trade_tx("t-sell", ("BTC", dec!(0.02)), ("GBP", dec!(1000))),
    ];

    for tx in cases {
        let tx = tx.with_fee(Fee {
            asset: "GBP".to_string(),
            amount: dec!(5),
            price: None,
        });
        let events = convert_one(&tx).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].fee_gbp, Some(dec!(5)));
    }
}

#[test]
fn fee_on_tagged_deposit() {
    let tx = deposit_tx("s1", "ETH", dec!(1))
        .with_tag(Tag::StakingReward)
        .with_price(gbp_price("ETH", dec!(1000)))
        .with_fee(Fee {
            asset: "GBP".to_string(),
            amount: dec!(7),
            price: None,
        });
    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].fee_gbp, Some(dec!(7)));
}

#[test]
fn trade_value_gbp_crypto_fee_needs_own_price() {
    let tx = trade_tx("t-v-fee-missing", ("BTC", dec!(1)), ("ETH", dec!(10)))
        .with_value_gbp(dec!(1000))
        .with_fee(Fee {
            asset: "ETH".to_string(),
            amount: dec!(0.1),
            price: None,
        });

    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingFeePrice {
            asset: "ETH".to_string(),
        }
    );
}

#[test]
fn trade_value_gbp_crypto_fee_with_explicit_price() {
    let tx = trade_tx("t-v-fee-explicit", ("BTC", dec!(1)), ("ETH", dec!(10)))
        .with_value_gbp(dec!(1000))
        .with_fee(Fee {
            asset: "ETH".to_string(),
            amount: dec!(0.1),
            price: Some(gbp_price("ETH", dec!(100))),
        });

    let events = convert_one(&tx).unwrap();
    assert_eq!(events[0].fee_gbp, Some(dec!(10)));
}

#[test]
fn deposit_income_value_gbp_crypto_fee_needs_own_price() {
    let tx = deposit_tx("d-income-fee-missing", "ETH", dec!(1))
        .with_tag(Tag::StakingReward)
        .with_value_gbp(dec!(1000))
        .with_fee(Fee {
            asset: "ETH".to_string(),
            amount: dec!(0.1),
            price: None,
        });

    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingFeePrice {
            asset: "ETH".to_string(),
        }
    );
}

#[test]
fn fee_explicit_price_takes_precedence() {
    let tx = trade_tx("t1", ("ETH", dec!(1)), ("BTC", dec!(0.05)))
        .with_price(gbp_price("BTC", dec!(15000)))
        .with_fee(Fee {
            asset: "BTC".to_string(),
            amount: dec!(0.0001),
            price: Some(gbp_price("BTC", dec!(20000))),
        });

    let events = convert_one(&tx).unwrap();
    // Two trade legs, plus the BTC spent on the fee as its own disposal.
    assert_eq!(events.len(), 3);
    assert_eq!(fee_disposals(&events, "BTC"), vec![(dec!(0.0001), dec!(2))]);
    assert_eq!(events[0].fee_gbp, Some(dec!(2)));
}

#[test]
fn fee_uses_trade_price_when_asset_matches_bought() {
    let tx = trade_tx("t1", ("ETH", dec!(1)), ("BTC", dec!(0.05)))
        .with_price(gbp_price("BTC", dec!(15000)))
        .with_fee(Fee {
            asset: "BTC".to_string(),
            amount: dec!(0.0001),
            price: None,
        });

    let events = convert_one(&tx).unwrap();
    // Two trade legs, plus the BTC spent on the fee as its own disposal.
    assert_eq!(events.len(), 3);
    assert_eq!(
        fee_disposals(&events, "BTC"),
        vec![(dec!(0.0001), dec!(1.50))]
    );
    assert_eq!(events[0].fee_gbp, Some(dec!(1.50)));
}

#[test]
fn fee_asset_match_is_case_insensitive() {
    let tx = trade_tx("t1", ("ETH", dec!(1)), ("BTC", dec!(0.05)))
        .with_price(gbp_price("BTC", dec!(15000)))
        .with_fee(Fee {
            asset: "btc".to_string(),
            amount: dec!(0.0001),
            price: None,
        });

    let events = convert_one(&tx).unwrap();
    // Two trade legs, plus the BTC spent on the fee as its own disposal.
    assert_eq!(events.len(), 3);
    assert_eq!(
        fee_disposals(&events, "BTC"),
        vec![(dec!(0.0001), dec!(1.50))]
    );
    assert_eq!(events[0].fee_gbp, Some(dec!(1.50)));
}

#[test]
fn fee_without_price_errors() {
    let cases = [
        Fee {
            asset: "ETH".to_string(),
            amount: dec!(0.01),
            price: None,
        },
        Fee {
            asset: "USDT".to_string(),
            amount: dec!(5),
            price: None,
        },
    ];

    for fee in cases {
        let tx = trade_tx("t1", ("ETH", dec!(1)), ("BTC", dec!(0.05)))
            .with_price(gbp_price("BTC", dec!(15000)))
            .with_fee(fee.clone());

        let err = convert_one(&tx).unwrap_err();
        assert_eq!(
            err,
            TransactionError::MissingFeePrice {
                asset: fee.asset.clone(),
            }
        );
    }
}

#[test]
fn staking_reward_requires_price() {
    let tx = deposit_tx("s1", "ETH", dec!(0.01)).with_tag(Tag::StakingReward);
    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingTaggedValuation {
            id: "s1".to_string(),
            tag: "StakingReward".to_string(),
            tx_type: "deposit".to_string(),
        }
    );
}

#[test]
fn income_tags_require_price() {
    let cases = [
        (Tag::Salary, "Salary"),
        (Tag::OtherIncome, "OtherIncome"),
        (Tag::AirdropIncome, "AirdropIncome"),
    ];

    for (tag, tag_name) in cases {
        let tx = deposit_tx("d1", "ETH", dec!(1)).with_tag(tag);
        let err = convert_one(&tx).unwrap_err();
        assert_eq!(
            err,
            TransactionError::MissingTaggedValuation {
                id: "d1".to_string(),
                tag: tag_name.to_string(),
                tx_type: "deposit".to_string(),
            }
        );
    }
}

#[test]
fn income_deposit_with_mismatched_price_base_errors() {
    let tx = deposit_tx("s1", "ETH", dec!(0.01))
        .with_tag(Tag::StakingReward)
        .with_price(gbp_price("BTC", dec!(2000)));

    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::PriceBaseMismatch {
            id: "s1".to_string(),
            base: "BTC".to_string(),
            expected: "ETH".to_string(),
        }
    );
}

#[test]
fn tagged_deposit_with_linked_withdrawal_errors() {
    let tx = deposit_tx("s1", "ETH", dec!(0.01))
        .with_tag(Tag::StakingReward)
        .with_deposit_link("w1")
        .with_price(gbp_price("ETH", dec!(2000)));

    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::TaggedDepositLinked {
            id: "s1".to_string()
        }
    );
}

#[test]
fn tagged_withdrawal_with_linked_deposit_errors() {
    let tx = withdrawal_tx("w1", "ETH", dec!(0.01))
        .with_tag(Tag::Gift)
        .with_withdrawal_link("d1")
        .with_price(gbp_price("ETH", dec!(2000)));

    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::TaggedWithdrawalLinked {
            id: "w1".to_string()
        }
    );
}

#[test]
fn invalid_tags_on_withdrawal_error() {
    let cases = [
        (Tag::StakingReward, "StakingReward"),
        (Tag::Airdrop, "Airdrop"),
        (Tag::Dividend, "Dividend"),
        (Tag::Interest, "Interest"),
        (Tag::Salary, "Salary"),
        (Tag::OtherIncome, "OtherIncome"),
        (Tag::AirdropIncome, "AirdropIncome"),
        (Tag::Cashback, "Cashback"),
    ];

    for (tag, tag_name) in cases {
        let tx = withdrawal_tx("w1", "ETH", dec!(0.01))
            .with_tag(tag)
            .with_price(gbp_price("ETH", dec!(2000)));
        let err = convert_one(&tx).unwrap_err();
        assert_eq!(
            err,
            TransactionError::InvalidTagForType {
                id: "w1".to_string(),
                tag: tag_name.to_string(),
                tx_type: "withdrawal".to_string(),
            }
        );
    }
}

#[test]
fn invalid_tags_on_trade_error() {
    let cases = [
        (Tag::StakingReward, "StakingReward"),
        (Tag::Dividend, "Dividend"),
        (Tag::Interest, "Interest"),
        (Tag::Salary, "Salary"),
        (Tag::OtherIncome, "OtherIncome"),
        (Tag::AirdropIncome, "AirdropIncome"),
        (Tag::Airdrop, "Airdrop"),
        (Tag::Gift, "Gift"),
        (Tag::Cashback, "Cashback"),
    ];

    for (tag, tag_name) in cases {
        let tx = trade_tx("t1", ("ETH", dec!(1)), ("BTC", dec!(0.05)))
            .with_tag(tag)
            .with_price(gbp_price("BTC", dec!(2000)));
        let err = convert_one(&tx).unwrap_err();
        assert_eq!(
            err,
            TransactionError::InvalidTagForType {
                id: "t1".to_string(),
                tag: tag_name.to_string(),
                tx_type: "trade".to_string(),
            }
        );
    }
}

#[test]
fn gift_deposit_missing_price_errors() {
    let tx = deposit_tx("d1", "ETH", dec!(1)).with_tag(Tag::Gift);
    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingTaggedValuation {
            id: "d1".to_string(),
            tag: "Gift".to_string(),
            tx_type: "deposit".to_string(),
        }
    );
}

#[test]
fn gift_withdrawal_missing_price_errors() {
    let tx = withdrawal_tx("w1", "ETH", dec!(1)).with_tag(Tag::Gift);
    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingTaggedValuation {
            id: "w1".to_string(),
            tag: "Gift".to_string(),
            tx_type: "withdrawal".to_string(),
        }
    );
}

#[test]
fn trade_tag_on_deposit_errors() {
    let tx = deposit_tx("d1", "ETH", dec!(1)).with_tag(Tag::Trade);
    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::InvalidTagForType {
            id: "d1".to_string(),
            tag: "Trade".to_string(),
            tx_type: "deposit".to_string(),
        }
    );
}

#[test]
fn trade_tag_on_withdrawal_errors() {
    let tx = withdrawal_tx("w1", "ETH", dec!(1)).with_tag(Tag::Trade);
    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::InvalidTagForType {
            id: "w1".to_string(),
            tag: "Trade".to_string(),
            tx_type: "withdrawal".to_string(),
        }
    );
}

#[test]
fn airdrop_deposit_with_price_errors() {
    let tx = deposit_tx("d1", "ETH", dec!(1))
        .with_tag(Tag::Airdrop)
        .with_price(gbp_price("ETH", dec!(1000)));
    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::AirdropValuationNotAllowed {
            id: "d1".to_string(),
        }
    );
}

#[test]
fn deposit_airdrop_with_value_gbp_errors() {
    let tx = deposit_tx("d-airdrop-value", "ETH", dec!(1))
        .with_tag(Tag::Airdrop)
        .with_value_gbp(dec!(1000));
    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::AirdropValuationNotAllowed {
            id: "d-airdrop-value".to_string(),
        }
    );
}

#[test]
fn gift_deposit_creates_gift_in() {
    let tx = deposit_tx("d1", "ETH", dec!(2))
        .with_tag(Tag::Gift)
        .with_price(gbp_price("ETH", dec!(1000)));

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Acquisition);
    assert_eq!(events[0].tag, Tag::Gift);
    assert_eq!(events[0].value_gbp, dec!(2000));
}

#[test]
fn gift_withdrawal_creates_gift_out() {
    let tx = withdrawal_tx("w1", "ETH", dec!(2))
        .with_tag(Tag::Gift)
        .with_price(gbp_price("ETH", dec!(1000)));

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Disposal);
    assert_eq!(events[0].tag, Tag::Gift);
    assert_eq!(events[0].value_gbp, dec!(2000));
}

#[test]
fn withdrawal_gift_with_value_gbp() {
    let tx = withdrawal_tx("w1-value", "ETH", dec!(2))
        .with_tag(Tag::Gift)
        .with_value_gbp(dec!(2000));

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tag, Tag::Gift);
    assert_eq!(events[0].value_gbp, dec!(2000));
}

#[test]
fn no_gain_no_loss_withdrawal_creates_disposal() {
    let tx = withdrawal_tx("w-ngnl", "ETH", dec!(2))
        .with_tag(Tag::NoGainNoLoss)
        .with_price(gbp_price("ETH", dec!(1000)));

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Disposal);
    assert_eq!(events[0].tag, Tag::NoGainNoLoss);
    assert_eq!(events[0].value_gbp, dec!(2000));
}

#[test]
fn no_gain_no_loss_withdrawal_without_valuation_creates_disposal() {
    let tx = withdrawal_tx("w-ngnl-none", "ETH", dec!(2)).with_tag(Tag::NoGainNoLoss);

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Disposal);
    assert_eq!(events[0].tag, Tag::NoGainNoLoss);
    assert_eq!(events[0].value_gbp, Decimal::ZERO);
}

#[test]
fn no_gain_no_loss_without_valuation_crypto_fee_needs_own_price() {
    let tx = withdrawal_tx("w-ngnl-fee-missing", "ETH", dec!(2))
        .with_tag(Tag::NoGainNoLoss)
        .with_fee(Fee {
            asset: "ETH".to_string(),
            amount: dec!(0.1),
            price: None,
        });

    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingFeePrice {
            asset: "ETH".to_string(),
        }
    );
}

#[test]
fn no_gain_no_loss_deposit_errors() {
    let tx = deposit_tx("d-ngnl", "ETH", dec!(2))
        .with_tag(Tag::NoGainNoLoss)
        .with_price(gbp_price("ETH", dec!(1000)));

    let err = convert_one(&tx).unwrap_err();
    assert!(
        err.to_string().contains("NoGainNoLoss"),
        "Expected error about NoGainNoLoss on deposit, got: {}",
        err
    );
}

#[test]
fn withdrawal_gift_value_gbp_with_fee() {
    let tx = withdrawal_tx("w-gift-fee", "ETH", dec!(2))
        .with_tag(Tag::Gift)
        .with_value_gbp(dec!(2000))
        .with_fee(Fee {
            asset: "GBP".to_string(),
            amount: dec!(4),
            price: None,
        });

    let events = convert_one(&tx).unwrap();
    assert_eq!(events[0].fee_gbp, Some(dec!(4)));
}

#[test]
fn airdrop_deposit_creates_zero_cost_acquisition() {
    let tx = deposit_tx("d1", "ETH", dec!(2)).with_tag(Tag::Airdrop);
    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tag, Tag::Airdrop);
    assert_eq!(events[0].value_gbp, Decimal::ZERO);
}

#[test]
fn airdrop_income_deposit_requires_price_and_counts_as_income_tag() {
    let tx = deposit_tx("d1", "ETH", dec!(2))
        .with_tag(Tag::AirdropIncome)
        .with_price(gbp_price("ETH", dec!(1000)));

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tag, Tag::AirdropIncome);
    assert_eq!(events[0].value_gbp, dec!(2000));
}

#[test]
fn salary_other_dividend_and_interest_deposits_are_supported() {
    let cases = [
        ("d1", Tag::Salary),
        ("d2", Tag::OtherIncome),
        ("d3", Tag::Dividend),
        ("d4", Tag::Interest),
        ("d5", Tag::Cashback),
    ];

    for (id, tag) in cases {
        let tx = deposit_tx(id, "ETH", dec!(1))
            .with_tag(tag)
            .with_price(gbp_price("ETH", dec!(1000)));
        let events = convert_one(&tx).unwrap();
        assert_eq!(events[0].tag, tag);
    }
}

#[test]
fn cashback_crypto_deposit_acquires_at_market_value() {
    let tx = deposit_tx("d-cb", "ETH", dec!(2))
        .with_tag(Tag::Cashback)
        .with_price(gbp_price("ETH", dec!(1000)));

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Acquisition);
    assert_eq!(events[0].tag, Tag::Cashback);
    assert_eq!(events[0].value_gbp, dec!(2000));
}

#[test]
fn cashback_crypto_deposit_requires_price() {
    let tx = deposit_tx("d-cb", "ETH", dec!(2)).with_tag(Tag::Cashback);
    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingTaggedValuation {
            id: "d-cb".to_string(),
            tag: "Cashback".to_string(),
            tx_type: "deposit".to_string(),
        }
    );
}

#[test]
fn dividend_and_interest_deposits_require_price() {
    let cases = [(Tag::Dividend, "Dividend"), (Tag::Interest, "Interest")];

    for (tag, tag_name) in cases {
        let tx = deposit_tx("d1", "ETH", dec!(1)).with_tag(tag);
        let err = convert_one(&tx).unwrap_err();
        assert_eq!(
            err,
            TransactionError::MissingTaggedValuation {
                id: "d1".to_string(),
                tag: tag_name.to_string(),
                tx_type: "deposit".to_string(),
            }
        );
    }
}

#[test]
fn gbp_denominated_income_and_cashback_deposits_no_price_needed() {
    let cases = [
        (Tag::Dividend, "Dividend"),
        (Tag::Interest, "Interest"),
        (Tag::Salary, "Salary"),
        (Tag::OtherIncome, "OtherIncome"),
        (Tag::Cashback, "Cashback"),
    ];

    for (tag, _tag_name) in cases {
        let tx = deposit_tx("d1", "GBP", dec!(500)).with_tag(tag);
        let events = convert_one(&tx).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tag, tag);
        assert_eq!(events[0].value_gbp, dec!(500));
        assert_eq!(events[0].asset_class, AssetClass::Fiat);
    }
}

#[test]
fn gbp_denominated_income_and_cashback_deposits_reject_price() {
    let cases = [
        (Tag::Dividend, "Dividend"),
        (Tag::Interest, "Interest"),
        (Tag::Salary, "Salary"),
        (Tag::OtherIncome, "OtherIncome"),
        (Tag::Cashback, "Cashback"),
    ];

    for (tag, tag_name) in cases {
        let tx = deposit_tx("d1", "GBP", dec!(500))
            .with_tag(tag)
            .with_price(gbp_price("GBP", dec!(1)));
        let err = convert_one(&tx).unwrap_err();
        assert_eq!(
            err,
            TransactionError::GbpIncomeValuationNotAllowed {
                id: "d1".to_string(),
                tag: tag_name.to_string(),
            }
        );
    }
}

#[test]
fn gbp_trade_rejects_price() {
    let cases = [
        trade_tx("t1", ("AAPL", dec!(10)), ("GBP", dec!(1500)))
            .with_price(gbp_price("AAPL", dec!(150))),
        trade_tx("t2", ("GBP", dec!(1500)), ("AAPL", dec!(10)))
            .with_price(gbp_price("AAPL", dec!(150))),
    ];

    for tx in cases {
        let err = convert_one(&tx).unwrap_err();
        assert_eq!(
            err,
            TransactionError::GbpTradeValuationNotAllowed {
                id: tx.as_ref().id.clone(),
            }
        );
    }
}

#[test]
fn trade_gbp_with_value_gbp_errors() {
    let cases = [
        trade_tx("t1-value", ("AAPL", dec!(10)), ("GBP", dec!(1500))).with_value_gbp(dec!(1500)),
        trade_tx("t2-value", ("GBP", dec!(1500)), ("AAPL", dec!(10))).with_value_gbp(dec!(1500)),
    ];

    for tx in cases {
        let err = convert_one(&tx).unwrap_err();
        assert_eq!(
            err,
            TransactionError::GbpTradeValuationNotAllowed {
                id: tx.as_ref().id.clone(),
            }
        );
    }
}

#[test]
fn deposit_income_with_value_gbp() {
    let tx = deposit_tx("d-income-value", "ETH", dec!(0.5))
        .with_tag(Tag::StakingReward)
        .with_value_gbp(dec!(800));

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tag, Tag::StakingReward);
    assert_eq!(events[0].value_gbp, dec!(800));
}

#[test]
fn deposit_gbp_income_with_value_gbp_errors() {
    let tx = deposit_tx("d-gbp-income-value", "GBP", dec!(500))
        .with_tag(Tag::Dividend)
        .with_value_gbp(dec!(500));
    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::GbpIncomeValuationNotAllowed {
            id: "d-gbp-income-value".to_string(),
            tag: "Dividend".to_string(),
        }
    );
}

#[test]
fn price_base_must_match_bought_asset() {
    let tx = trade_tx("t1", ("ETH", dec!(1)), ("BTC", dec!(0.05)))
        .with_price(gbp_price("ETH", dec!(2000)));

    let err = convert_one(&tx).unwrap_err();
    assert_eq!(
        err,
        TransactionError::PriceBaseMismatch {
            id: "t1".to_string(),
            base: "ETH".to_string(),
            expected: "BTC".to_string(),
        }
    );
}

#[test]
fn validate_assets_detects_undefined_symbol() {
    let json = r#"{
      "assets": [{ "symbol": "BTC", "asset_class": "Crypto" }],
      "transactions": [
        {
          "id": "tx-1",
          "datetime": "2024-01-01T00:00:00+00:00",
          "account": "kraken",
          "type": "Trade",
          "sold": { "asset": "ETH", "quantity": 1.0 },
          "bought": { "asset": "BTC", "quantity": 0.05 },
          "valuation": { "base": "BTC", "rate": 1000 }
        }
      ]
    }"#;

    let err = read_transactions_json(std::io::Cursor::new(json)).unwrap_err();
    assert_eq!(
        err.downcast_ref::<TransactionError>(),
        Some(&TransactionError::UndefinedAsset {
            symbol: "ETH".to_string()
        })
    );
}

#[test]
fn validate_assets_detects_duplicate_symbol() {
    let json = r#"{
      "assets": [{ "symbol": "BTC", "asset_class": "Crypto" }, { "symbol": "BTC", "asset_class": "Crypto" }],
      "transactions": []
    }"#;

    let err = read_transactions_json(std::io::Cursor::new(json)).unwrap_err();
    assert_eq!(
        err.downcast_ref::<TransactionError>(),
        Some(&TransactionError::DuplicateAsset {
            symbol: "BTC".to_string()
        })
    );
}

#[test]
fn validate_assets_gbp_implicit() {
    let json = r#"{
      "assets": [{ "symbol": "BTC", "asset_class": "Crypto" }],
      "transactions": [
        {
          "id": "tx-1",
          "datetime": "2024-01-01T00:00:00+00:00",
          "account": "kraken",
          "type": "Trade",
          "sold": { "asset": "GBP", "quantity": 1000 },
          "bought": { "asset": "BTC", "quantity": 0.05 }
        }
      ]
    }"#;

    assert!(read_transactions_json(std::io::Cursor::new(json)).is_ok());
}

#[test]
fn validate_assets_gbp_in_assets_list_allowed() {
    let json = r#"{
      "assets": [{ "symbol": "gbp", "asset_class": "Stock" }, { "symbol": "BTC", "asset_class": "Crypto" }],
      "transactions": [
        {
          "id": "tx-1",
          "datetime": "2024-01-01T00:00:00+00:00",
          "account": "kraken",
          "type": "Trade",
          "sold": { "asset": "GBP", "quantity": 1000 },
          "bought": { "asset": "BTC", "quantity": 0.05 }
        }
      ]
    }"#;

    assert!(read_transactions_json(std::io::Cursor::new(json)).is_ok());
}

#[test]
fn validate_assets_case_insensitive_duplicate() {
    let json = r#"{
      "assets": [{ "symbol": "btc", "asset_class": "Crypto" }, { "symbol": "BTC", "asset_class": "Crypto" }],
      "transactions": []
    }"#;

    let err = read_transactions_json(std::io::Cursor::new(json)).unwrap_err();
    assert_eq!(
        err.downcast_ref::<TransactionError>(),
        Some(&TransactionError::DuplicateAsset {
            symbol: "BTC".to_string()
        })
    );
}

#[test]
fn validate_assets_checks_fee_and_price_symbols() {
    let invalid_fee_json = r#"{
      "assets": [{ "symbol": "BTC", "asset_class": "Crypto" }],
      "transactions": [
        {
          "id": "tx-1",
          "datetime": "2024-01-01T00:00:00+00:00",
          "account": "kraken",
          "type": "Trade",
          "sold": { "asset": "GBP", "quantity": 1000 },
          "bought": { "asset": "BTC", "quantity": 0.05 },
          "fee": { "asset": "ETH", "amount": 0.001 }
        }
      ]
    }"#;
    let err = read_transactions_json(std::io::Cursor::new(invalid_fee_json)).unwrap_err();
    assert_eq!(
        err.downcast_ref::<TransactionError>(),
        Some(&TransactionError::UndefinedAsset {
            symbol: "ETH".to_string()
        })
    );

    let invalid_price_json = r#"{
      "assets": [{ "symbol": "BTC", "asset_class": "Crypto" }],
      "transactions": [
        {
          "id": "tx-1",
          "datetime": "2024-01-01T00:00:00+00:00",
          "account": "kraken",
          "type": "Trade",
          "sold": { "asset": "GBP", "quantity": 1000 },
          "bought": { "asset": "BTC", "quantity": 0.05 },
          "valuation": { "base": "ETH", "rate": 2000 }
        }
      ]
    }"#;
    let err = read_transactions_json(std::io::Cursor::new(invalid_price_json)).unwrap_err();
    assert_eq!(
        err.downcast_ref::<TransactionError>(),
        Some(&TransactionError::UndefinedAsset {
            symbol: "ETH".to_string()
        })
    );
}

#[test]
fn validate_assets_missing_field_errors() {
    let json = r#"{
      "transactions": []
    }"#;

    let err = read_transactions_json(std::io::Cursor::new(json)).unwrap_err();
    assert!(err.to_string().contains("missing field `assets`"));
}

#[test]
fn validate_assets_empty_with_non_gbp_errors() {
    let json = r#"{
      "assets": [],
      "transactions": [
        {
          "id": "tx-1",
          "datetime": "2024-01-01T00:00:00+00:00",
          "account": "kraken",
          "type": "Trade",
          "sold": { "asset": "BTC", "quantity": 1.0 },
          "bought": { "asset": "GBP", "quantity": 1000.0 }
        }
      ]
    }"#;

    let err = read_transactions_json(std::io::Cursor::new(json)).unwrap_err();
    assert_eq!(
        err.downcast_ref::<TransactionError>(),
        Some(&TransactionError::UndefinedAsset {
            symbol: "BTC".to_string()
        })
    );
}

#[test]
fn stock_asset_class_from_registry() {
    let tx = trade_tx("tx-1", ("GBP", dec!(1000)), ("AAPL", dec!(10)))
        .datetime("2024-01-01T00:00:00+00:00")
        .build();

    let mut registry = AssetRegistry::new();
    registry.insert(
        "AAPL".to_string(),
        Asset {
            symbol: "AAPL".to_string(),
            asset_class: AssetClass::Stock,
        },
    );
    let events = tx.to_taxable_events(&registry, false).unwrap();
    assert_eq!(events[0].asset_class, AssetClass::Stock);
}

#[test]
fn unclassified_price_base_mismatch_errors() {
    let cases = [
        deposit_tx("d1", "ETH", dec!(1))
            .datetime("2024-01-01T00:00:00+00:00")
            .with_price(gbp_price("BTC", dec!(1000))),
        withdrawal_tx("w1", "ETH", dec!(1))
            .datetime("2024-01-01T00:00:00+00:00")
            .with_price(gbp_price("BTC", dec!(1000))),
    ];

    for tx in cases {
        let err = convert_one(&tx).unwrap_err();
        assert_eq!(
            err,
            TransactionError::PriceBaseMismatch {
                id: tx.as_ref().id.clone(),
                base: "BTC".to_string(),
                expected: "ETH".to_string(),
            }
        );
    }
}

#[test]
fn unlinked_deposit_value_gbp_with_fee() {
    let tx = deposit_tx("d-unlinked-fee", "ETH", dec!(2))
        .with_value_gbp(dec!(2000))
        .with_fee(Fee {
            asset: "GBP".to_string(),
            amount: dec!(3),
            price: None,
        });

    let events = convert_one(&tx).unwrap();
    assert_eq!(events[0].fee_gbp, Some(dec!(3)));
}

#[test]
fn serde_round_trip_valuation_value_gbp() {
    let tx = trade_tx("serde-value", ("BTC", dec!(0.5)), ("ETH", dec!(8)))
        .with_value_gbp(dec!(15000))
        .build();

    let json = serde_json::to_string(&tx).unwrap();
    let round_tripped: Transaction = serde_json::from_str(&json).unwrap();
    assert_eq!(
        round_tripped.valuation,
        Some(Valuation::ValueGbp(dec!(15000)))
    );
}

#[test]
fn trade_with_zero_sold_quantity_errors() {
    let tx = trade_tx("bad-qty", ("BTC", dec!(0)), ("GBP", dec!(1000)));
    let err = convert_all(&[tx]).unwrap_err();
    assert_eq!(
        err,
        TransactionError::NonPositiveQuantity {
            id: "bad-qty".to_string(),
            asset: "BTC".to_string(),
        }
    );
}

#[test]
fn trade_with_negative_bought_quantity_errors() {
    let tx = trade_tx("bad-qty", ("GBP", dec!(1000)), ("BTC", dec!(-0.5)));
    let err = convert_all(&[tx]).unwrap_err();
    assert_eq!(
        err,
        TransactionError::NonPositiveQuantity {
            id: "bad-qty".to_string(),
            asset: "BTC".to_string(),
        }
    );
}

#[test]
fn deposit_with_zero_quantity_errors() {
    let tx = deposit_tx("bad-qty", "ETH", dec!(0));
    let err = convert_all(&[tx]).unwrap_err();
    assert_eq!(
        err,
        TransactionError::NonPositiveQuantity {
            id: "bad-qty".to_string(),
            asset: "ETH".to_string(),
        }
    );
}

#[test]
fn withdrawal_with_negative_quantity_errors() {
    let tx = withdrawal_tx("bad-qty", "ETH", dec!(-1));
    let err = convert_all(&[tx]).unwrap_err();
    assert_eq!(
        err,
        TransactionError::NonPositiveQuantity {
            id: "bad-qty".to_string(),
            asset: "ETH".to_string(),
        }
    );
}

#[test]
fn negative_fee_amount_errors() {
    let tx = trade_tx("bad-fee", ("GBP", dec!(1000)), ("BTC", dec!(0.05))).with_fee(Fee {
        asset: "GBP".to_string(),
        amount: dec!(-5),
        price: None,
    });
    let err = convert_all(&[tx]).unwrap_err();
    assert_eq!(
        err,
        TransactionError::NegativeFeeAmount {
            id: "bad-fee".to_string(),
        }
    );
}

#[test]
fn zero_fee_amount_is_allowed() {
    let tx = trade_tx("zero-fee", ("GBP", dec!(1000)), ("BTC", dec!(0.05))).with_fee(Fee {
        asset: "GBP".to_string(),
        amount: dec!(0),
        price: None,
    });
    let events = convert_all(&[tx]).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].fee_gbp, Some(dec!(0)));
}

fn crypto_fee(asset: &str, amount: Decimal, gbp_rate: Decimal) -> Fee {
    Fee {
        asset: asset.to_string(),
        amount,
        price: Some(gbp_price(asset, gbp_rate)),
    }
}

fn fee_disposals(events: &[TaxableEvent], asset: &str) -> Vec<(Decimal, Decimal)> {
    events
        .iter()
        .filter(|e| e.event_type == EventType::Disposal && e.asset == asset && e.fee_gbp.is_none())
        .map(|e| (e.quantity, e.value_gbp))
        .collect()
}

// --- HMRC: tokens spent on a fee are themselves disposed of (CRYPTO22280) ---

#[test]
fn crypto_fee_on_trade_is_a_disposal_of_the_fee_tokens() {
    // Sell 1 ETH for BTC and pay 0.01 BNB (at £500) as the exchange fee.
    let tx = trade_tx("t1", ("ETH", dec!(1)), ("BTC", dec!(0.05)))
        .with_value_gbp(dec!(2000))
        .with_fee(crypto_fee("BNB", dec!(0.01), dec!(500)));
    let events = convert_one(&tx).unwrap();

    // The fee stays an allowable cost of the ETH disposal...
    let eth = events.iter().find(|e| e.asset == "ETH").unwrap();
    assert_eq!(eth.fee_gbp, Some(dec!(5)));
    // ...and the BNB spent on it is a disposal at its market value.
    assert_eq!(fee_disposals(&events, "BNB"), vec![(dec!(0.01), dec!(5))]);
    assert_eq!(events.len(), 3);
}

#[test]
fn gbp_fee_on_trade_is_not_a_disposal() {
    let tx = trade_tx("t1", ("ETH", dec!(1)), ("BTC", dec!(0.05)))
        .with_value_gbp(dec!(2000))
        .with_fee(Fee {
            asset: "GBP".to_string(),
            amount: dec!(5),
            price: None,
        });
    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.asset != "GBP"));
}

#[test]
fn crypto_fee_on_linked_transfer_is_a_disposal_of_the_fee_tokens() {
    // Moving 1 BTC between own wallets, paying a separate 0.001 BTC network
    // fee: quantities exclude the fee, so 1 left and 1 arrived. The transfer
    // is not a disposal; the fee tokens are.
    let txs = [
        withdrawal_tx("w1", "BTC", dec!(1))
            .with_withdrawal_link("d1")
            .with_fee(crypto_fee("BTC", dec!(0.001), dec!(50000))),
        deposit_tx("d1", "BTC", dec!(1)).with_deposit_link("w1"),
    ];
    let events = convert_all(&txs).unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].event_type, EventType::Disposal);
    assert_eq!(events[0].asset, "BTC");
    assert_eq!(events[0].quantity, dec!(0.001));
    assert_eq!(events[0].value_gbp, dec!(50));
    assert_eq!(events[0].fee_gbp, None);
}

#[test]
fn linked_transfer_fee_and_transit_shortfall_are_disposed_of_separately() {
    // 1 BTC sent plus a 0.001 BTC fee, but only 0.9995 arrived: the fee is a
    // priced disposal and the missing 0.0005 an unclassified one. The fee is
    // not subtracted from the shortfall -- it never was part of the 1 BTC.
    let txs = [
        withdrawal_tx("w1", "BTC", dec!(1))
            .with_withdrawal_link("d1")
            .with_price(gbp_price("BTC", dec!(50000)))
            .with_fee(crypto_fee("BTC", dec!(0.001), dec!(50000))),
        deposit_tx("d1", "BTC", dec!(0.9995)).with_deposit_link("w1"),
    ];
    let events = convert_all(&txs).unwrap();
    let mut got: Vec<(Tag, Decimal)> = events.iter().map(|e| (e.tag, e.quantity)).collect();
    got.sort();
    assert_eq!(
        got,
        vec![(Tag::Unclassified, dec!(0.0005)), (Tag::Trade, dec!(0.001))]
    );
}

#[test]
fn linked_deposit_fee_is_disposed_of_once() {
    // A fee on the receiving leg is its own outflow: 1 sent, 1 arrived, then
    // 0.03 DOT paid. One fee disposal, no shortfall.
    let txs = [
        withdrawal_tx("w1", "DOT", dec!(1)).with_withdrawal_link("d1"),
        deposit_tx("d1", "DOT", dec!(1))
            .with_deposit_link("w1")
            .with_fee(crypto_fee("DOT", dec!(0.03), dec!(5))),
    ];
    let events = convert_all(&txs).unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].quantity, dec!(0.03));
    assert_eq!(events[0].tag, Tag::Trade);
}

#[test]
fn linked_transfer_shortfall_valued_pro_rata_from_value_gbp() {
    let txs = [
        withdrawal_tx("w1", "BTC", dec!(1))
            .with_withdrawal_link("d1")
            .with_value_gbp(dec!(50000)),
        deposit_tx("d1", "BTC", dec!(0.999)).with_deposit_link("w1"),
    ];
    let events = convert_all(&txs).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value_gbp, dec!(50));
}

#[test]
fn linked_transfer_shortfall_without_valuation_is_valued_at_zero() {
    let txs = [
        withdrawal_tx("w1", "BTC", dec!(1)).with_withdrawal_link("d1"),
        deposit_tx("d1", "BTC", dec!(0.999)).with_deposit_link("w1"),
    ];
    let events = convert_all(&txs).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tag, Tag::Unclassified);
    assert_eq!(events[0].value_gbp, Decimal::ZERO);
}

#[test]
fn linked_transfer_undeclared_shortfall_is_an_unclassified_disposal() {
    // 1 BTC left, 0.999 arrived and no fee was declared: the missing 0.001
    // must leave the pool, flagged for review rather than silently vanishing.
    let txs = [
        withdrawal_tx("w1", "BTC", dec!(1))
            .with_withdrawal_link("d1")
            .with_price(gbp_price("BTC", dec!(50000))),
        deposit_tx("d1", "BTC", dec!(0.999)).with_deposit_link("w1"),
    ];
    let events = convert_all(&txs).unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].event_type, EventType::Disposal);
    assert_eq!(events[0].tag, Tag::Unclassified);
    assert_eq!(events[0].quantity, dec!(0.001));
    assert_eq!(events[0].value_gbp, dec!(50));
}

#[test]
fn linked_transfer_exact_quantities_produce_no_events() {
    let txs = [
        withdrawal_tx("w1", "BTC", dec!(1)).with_withdrawal_link("d1"),
        deposit_tx("d1", "BTC", dec!(1)).with_deposit_link("w1"),
    ];
    assert!(convert_all(&txs).unwrap().is_empty());
}

#[test]
fn linked_transfer_asset_mismatch_errors() {
    let txs = [
        withdrawal_tx("w1", "BTC", dec!(1)).with_withdrawal_link("d1"),
        deposit_tx("d1", "ETH", dec!(5)).with_deposit_link("w1"),
    ];
    assert!(matches!(
        convert_all(&txs),
        Err(TransactionError::LinkedTransactionAssetMismatch { .. })
    ));
}

#[test]
fn linked_deposit_exceeding_withdrawal_errors() {
    let txs = [
        withdrawal_tx("w1", "BTC", dec!(1)).with_withdrawal_link("d1"),
        deposit_tx("d1", "BTC", dec!(1.5)).with_deposit_link("w1"),
    ];
    assert!(matches!(
        convert_all(&txs),
        Err(TransactionError::LinkedDepositExceedsWithdrawal { .. })
    ));
}

#[test]
fn gbp_gift_withdrawal_is_not_a_disposal() {
    // Sterling is not a chargeable asset (TCGA 1992 s21(4)).
    let tx = withdrawal_tx("w1", "GBP", dec!(1000))
        .with_tag(Tag::Gift)
        .with_value_gbp(dec!(1000));
    assert!(convert_one(&tx).unwrap().is_empty());
}

#[test]
fn negative_value_gbp_valuation_errors() {
    let tx = trade_tx("t1", ("ETH", dec!(1)), ("BTC", dec!(0.05))).with_value_gbp(dec!(-5000));
    assert!(matches!(
        convert_one(&tx),
        Err(TransactionError::NegativeValuation { .. })
    ));
}

#[test]
fn exclude_unlinked_drops_unlinked_deposits_and_their_fees() {
    let tx = deposit_tx("d1", "BTC", dec!(1))
        .with_price(gbp_price("BTC", dec!(50000)))
        .with_fee(crypto_fee("BTC", dec!(0.001), dec!(50000)));
    let events = tx
        .as_ref()
        .to_taxable_events(&test_registry(), true)
        .unwrap();
    assert!(events.is_empty(), "{events:?}");
}

#[test]
fn crypto_fee_on_tagged_deposit_is_disposed_of() {
    let tx = deposit_tx("d1", "ETH", dec!(1))
        .with_tag(Tag::StakingReward)
        .with_price(gbp_price("ETH", dec!(2000)))
        .with_fee(crypto_fee("ETH", dec!(0.01), dec!(2000)));
    let events = convert_one(&tx).unwrap();
    // The reward is acquired in full; the fee tokens then leave.
    let acq = events
        .iter()
        .find(|e| e.event_type == EventType::Acquisition)
        .unwrap();
    assert_eq!(acq.quantity, dec!(1));
    assert_eq!(acq.fee_gbp, Some(dec!(20)));
    assert_eq!(fee_disposals(&events, "ETH"), vec![(dec!(0.01), dec!(20))]);
}

#[test]
fn crypto_fee_on_gift_withdrawal_is_disposed_of() {
    let tx = withdrawal_tx("w1", "ETH", dec!(1))
        .with_tag(Tag::Gift)
        .with_price(gbp_price("ETH", dec!(2000)))
        .with_fee(crypto_fee("ETH", dec!(0.01), dec!(2000)));
    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(fee_disposals(&events, "ETH"), vec![(dec!(0.01), dec!(20))]);
}

#[test]
fn crypto_fee_on_unlinked_withdrawal_is_disposed_of() {
    let tx = withdrawal_tx("w1", "ETH", dec!(1))
        .with_price(gbp_price("ETH", dec!(2000)))
        .with_fee(crypto_fee("ETH", dec!(0.01), dec!(2000)));
    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(fee_disposals(&events, "ETH"), vec![(dec!(0.01), dec!(20))]);
}

#[test]
fn crypto_fee_on_gbp_deposit_is_still_disposed_of() {
    // Sterling moves are not chargeable, but tokens spent on their fee are.
    let tx = deposit_tx("d1", "GBP", dec!(500)).with_fee(crypto_fee("ETH", dec!(0.01), dec!(2000)));
    let events = convert_one(&tx).unwrap();
    assert_eq!(fee_disposals(&events, "ETH"), vec![(dec!(0.01), dec!(20))]);
    assert_eq!(events.len(), 1);
}

#[test]
fn zero_crypto_fee_is_not_a_disposal() {
    let tx = trade_tx("t1", ("ETH", dec!(1)), ("BTC", dec!(0.05)))
        .with_value_gbp(dec!(2000))
        .with_fee(crypto_fee("BNB", dec!(0), dec!(500)));
    assert_eq!(convert_one(&tx).unwrap().len(), 2);
}

#[test]
fn exclude_unlinked_keeps_tagged_deposits_and_their_fees() {
    let tx = deposit_tx("d1", "ETH", dec!(1))
        .with_tag(Tag::StakingReward)
        .with_price(gbp_price("ETH", dec!(2000)))
        .with_fee(crypto_fee("ETH", dec!(0.01), dec!(2000)));
    let events = tx
        .as_ref()
        .to_taxable_events(&test_registry(), true)
        .unwrap();
    assert_eq!(events.len(), 2);
}

#[test]
fn exclude_unlinked_drops_unlinked_withdrawals_and_their_fees() {
    let tx = withdrawal_tx("w1", "ETH", dec!(1))
        .with_price(gbp_price("ETH", dec!(2000)))
        .with_fee(crypto_fee("ETH", dec!(0.01), dec!(2000)));
    let events = tx
        .as_ref()
        .to_taxable_events(&test_registry(), true)
        .unwrap();
    assert!(events.is_empty(), "{events:?}");
}

// ---- Share reorganisations and fee-only transactions ----

/// Convert a document holding the given transaction rows, with ULVR, MICC
/// and CSN shares and DOT tokens defined.
fn convert_rows(rows: serde_json::Value) -> Result<Vec<TaxableEvent>, TransactionError> {
    let doc: Transactions = serde_json::from_value(serde_json::json!({
        "assets": [
            {"symbol": "ULVR", "asset_class": "Stock"},
            {"symbol": "MICC", "asset_class": "Stock"},
            {"symbol": "CSN", "asset_class": "Stock"},
            {"symbol": "DOT", "asset_class": "Crypto"},
        ],
        "transactions": rows,
    }))
    .unwrap();
    document_to_events(doc, ConversionOptions::default())
}

/// One transaction row: the common fields plus `fields`.
fn row(id: &str, fields: serde_json::Value) -> serde_json::Value {
    let mut row = serde_json::json!({
        "id": id,
        "datetime": "2025-12-17T09:00:00Z",
        "account": "ii",
    });
    row.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    row
}

fn demerger_row(id: &str) -> serde_json::Value {
    row(
        id,
        serde_json::json!({
            "type": "Demerger",
            "original": "ULVR",
            "new_holding": {"asset": "MICC", "quantity": "177"},
            "cost_fraction": "0.051151",
        }),
    )
}

fn rights_issue_row(id: &str) -> serde_json::Value {
    row(
        id,
        serde_json::json!({
            "type": "RightsIssue",
            "new_shares": {"asset": "CSN", "quantity": "1473"},
            "consideration": "2592.48",
        }),
    )
}

fn small_distribution_row(id: &str) -> serde_json::Value {
    row(
        id,
        serde_json::json!({
            "type": "SmallCapitalDistribution",
            "asset": "ULVR",
            "amount": "21.64",
        }),
    )
}

fn fee_row(id: &str) -> serde_json::Value {
    row(
        id,
        serde_json::json!({
            "type": "Fee",
            "fee": {"asset": "DOT", "amount": "0.02", "price": {"base": "DOT", "rate": "5.00"}},
        }),
    )
}

/// `row` with `field` replaced by `value` (or removed when `value` is null).
fn with(mut row: serde_json::Value, field: &str, value: serde_json::Value) -> serde_json::Value {
    let obj = row.as_object_mut().unwrap();
    if value.is_null() {
        obj.remove(field);
    } else {
        obj.insert(field.to_string(), value);
    }
    row
}

#[test]
fn reorganisation_types_round_trip_with_numeric_string_decimals() {
    for row in [
        demerger_row("d"),
        rights_issue_row("r"),
        small_distribution_row("s"),
        fee_row("f"),
    ] {
        let tx: Transaction = serde_json::from_value(row.clone()).unwrap();
        let back = serde_json::to_value(&tx).unwrap();
        assert_eq!(back, row);
    }
}

#[test]
fn reorganisation_types_accept_valid_rows() {
    let rows = serde_json::json!([
        demerger_row("d"),
        rights_issue_row("r"),
        small_distribution_row("s"),
        fee_row("f"),
    ]);
    assert!(convert_rows(rows).is_ok());
}

#[test]
fn reorganisation_non_positive_quantities_and_amounts_are_rejected() {
    let non_positive = |row: serde_json::Value| convert_rows(serde_json::json!([row])).unwrap_err();
    for qty in ["0", "-1"] {
        assert_eq!(
            non_positive(with(
                demerger_row("d"),
                "new_holding",
                serde_json::json!({"asset": "MICC", "quantity": qty}),
            )),
            TransactionError::NonPositiveQuantity {
                id: "d".to_string(),
                asset: "MICC".to_string(),
            }
        );
        assert_eq!(
            non_positive(with(
                rights_issue_row("r"),
                "new_shares",
                serde_json::json!({"asset": "CSN", "quantity": qty}),
            )),
            TransactionError::NonPositiveQuantity {
                id: "r".to_string(),
                asset: "CSN".to_string(),
            }
        );
        assert_eq!(
            non_positive(with(
                rights_issue_row("r"),
                "consideration",
                serde_json::json!(qty)
            )),
            TransactionError::NonPositiveAmount {
                id: "r".to_string(),
                field: "consideration".to_string(),
            }
        );
        assert_eq!(
            non_positive(with(
                small_distribution_row("s"),
                "amount",
                serde_json::json!(qty)
            )),
            TransactionError::NonPositiveAmount {
                id: "s".to_string(),
                field: "amount".to_string(),
            }
        );
    }
}

#[test]
fn demerger_cost_fraction_must_be_strictly_between_zero_and_one() {
    for fraction in ["0", "1", "1.5", "-0.1"] {
        let err = convert_rows(serde_json::json!([with(
            demerger_row("d"),
            "cost_fraction",
            serde_json::json!(fraction),
        )]))
        .unwrap_err();
        assert_eq!(
            err,
            TransactionError::InvalidCostFraction {
                id: "d".to_string()
            },
            "fraction {fraction}"
        );
    }
}

#[test]
fn demerger_into_the_original_asset_is_rejected() {
    let err = convert_rows(serde_json::json!([with(
        demerger_row("d"),
        "new_holding",
        serde_json::json!({"asset": "ulvr", "quantity": "177"}),
    )]))
    .unwrap_err();
    assert_eq!(
        err,
        TransactionError::DemergerSameAsset {
            id: "d".to_string()
        }
    );
}

#[test]
fn reorganisation_undefined_assets_are_rejected() {
    let undefined = |row: serde_json::Value| convert_rows(serde_json::json!([row])).unwrap_err();
    let expected = TransactionError::UndefinedAsset {
        symbol: "XYZ".to_string(),
    };
    assert_eq!(
        undefined(with(
            demerger_row("d"),
            "original",
            serde_json::json!("XYZ")
        )),
        expected
    );
    assert_eq!(
        undefined(with(
            demerger_row("d"),
            "new_holding",
            serde_json::json!({"asset": "XYZ", "quantity": "1"}),
        )),
        expected
    );
    assert_eq!(
        undefined(with(
            rights_issue_row("r"),
            "new_shares",
            serde_json::json!({"asset": "XYZ", "quantity": "1"}),
        )),
        expected
    );
    assert_eq!(
        undefined(with(
            small_distribution_row("s"),
            "asset",
            serde_json::json!("XYZ")
        )),
        expected
    );
}

#[test]
fn reorganisation_of_sterling_is_rejected() {
    let sterling = |row: serde_json::Value| convert_rows(serde_json::json!([row])).unwrap_err();
    assert_eq!(
        sterling(with(
            demerger_row("d"),
            "original",
            serde_json::json!("GBP")
        )),
        TransactionError::SterlingNotAllowed {
            id: "d".to_string(),
            tx_type: "Demerger".to_string(),
        }
    );
    assert_eq!(
        sterling(with(
            rights_issue_row("r"),
            "new_shares",
            serde_json::json!({"asset": "GBP", "quantity": "1"}),
        )),
        TransactionError::SterlingNotAllowed {
            id: "r".to_string(),
            tx_type: "RightsIssue".to_string(),
        }
    );
    assert_eq!(
        sterling(with(
            small_distribution_row("s"),
            "asset",
            serde_json::json!("gbp")
        )),
        TransactionError::SterlingNotAllowed {
            id: "s".to_string(),
            tx_type: "SmallCapitalDistribution".to_string(),
        }
    );
}

#[test]
fn fee_transaction_requires_a_positive_fee() {
    for fee in [
        serde_json::Value::Null,
        serde_json::json!({"asset": "DOT", "amount": "0", "price": {"base": "DOT", "rate": "5"}}),
    ] {
        let err = convert_rows(serde_json::json!([with(fee_row("f"), "fee", fee)])).unwrap_err();
        assert_eq!(
            err,
            TransactionError::FeeRequired {
                id: "f".to_string()
            }
        );
    }
}

#[test]
fn fee_transaction_in_tokens_requires_a_price() {
    let err = convert_rows(serde_json::json!([with(
        fee_row("f"),
        "fee",
        serde_json::json!({"asset": "DOT", "amount": "0.02"}),
    )]))
    .unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingFeePrice {
            asset: "DOT".to_string()
        }
    );
}

#[test]
fn reorganisation_and_fee_types_reject_a_non_default_tag() {
    for (row, tx_type) in [
        (demerger_row("x"), "Demerger"),
        (rights_issue_row("x"), "RightsIssue"),
        (small_distribution_row("x"), "SmallCapitalDistribution"),
        (fee_row("x"), "Fee"),
    ] {
        let err = convert_rows(serde_json::json!([with(
            row,
            "tag",
            serde_json::json!("Trade")
        )]))
        .unwrap_err();
        assert_eq!(
            err,
            TransactionError::InvalidTagForType {
                id: "x".to_string(),
                tag: "Trade".to_string(),
                tx_type: tx_type.to_string(),
            }
        );
    }
}

#[test]
fn reorganisation_and_fee_types_reject_a_valuation() {
    for (row, tx_type) in [
        (demerger_row("x"), "Demerger"),
        (rights_issue_row("x"), "RightsIssue"),
        (small_distribution_row("x"), "SmallCapitalDistribution"),
        (fee_row("x"), "Fee"),
    ] {
        let err = convert_rows(serde_json::json!([with(
            row,
            "valuation",
            serde_json::json!("10")
        )]))
        .unwrap_err();
        assert_eq!(
            err,
            TransactionError::ValuationNotAllowed {
                id: "x".to_string(),
                tx_type: tx_type.to_string(),
            }
        );
    }
}

#[test]
fn demerger_and_small_distribution_reject_a_fee() {
    let gbp_fee = serde_json::json!({"asset": "GBP", "amount": "1"});
    for (row, tx_type) in [
        (demerger_row("x"), "Demerger"),
        (small_distribution_row("x"), "SmallCapitalDistribution"),
    ] {
        let err = convert_rows(serde_json::json!([with(row, "fee", gbp_fee.clone())])).unwrap_err();
        assert_eq!(
            err,
            TransactionError::FeeNotAllowed {
                id: "x".to_string(),
                tx_type: tx_type.to_string(),
            }
        );
    }
}

#[test]
fn demerger_converts_to_one_linked_pool_adjustment() {
    let events = convert_rows(serde_json::json!([demerger_row("d")])).unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    let e = &events[0];
    assert_eq!(
        e.event_type,
        EventType::PoolAdjustment(AdjustmentKind::Demerger)
    );
    assert_eq!(e.tag, Tag::Trade);
    assert_eq!(e.asset, "MICC");
    assert_eq!(e.asset_class, AssetClass::Stock);
    assert_eq!(e.quantity, dec!(177));
    assert_eq!(e.value_gbp, dec!(0));
    assert_eq!(
        e.demerged_from,
        Some(DemergedFrom {
            asset: "ULVR".to_string(),
            cost_fraction: dec!(0.051151),
        })
    );
}

#[test]
fn rights_issue_converts_to_one_adjustment_costing_consideration_plus_fee() {
    let events = convert_rows(serde_json::json!([with(
        rights_issue_row("r"),
        "fee",
        serde_json::json!({"asset": "GBP", "amount": "9.99"}),
    )]))
    .unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    let e = &events[0];
    assert_eq!(
        e.event_type,
        EventType::PoolAdjustment(AdjustmentKind::RightsIssue)
    );
    assert_eq!(e.tag, Tag::Trade);
    assert_eq!(e.asset, "CSN");
    assert_eq!(e.quantity, dec!(1473));
    assert_eq!(e.total_cost_gbp(), dec!(2602.47));
    assert_eq!(e.demerged_from, None);
}

#[test]
fn small_capital_distribution_converts_to_one_negative_cost_adjustment() {
    let events = convert_rows(serde_json::json!([small_distribution_row("s")])).unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    let e = &events[0];
    assert_eq!(
        e.event_type,
        EventType::PoolAdjustment(AdjustmentKind::SmallCapitalDistribution)
    );
    assert_eq!(e.asset, "ULVR");
    assert_eq!(e.quantity, dec!(0));
    assert_eq!(e.total_cost_gbp(), dec!(-21.64));
}

#[test]
fn fee_transaction_in_tokens_is_one_disposal_at_market_value() {
    // AE4: 0.02 DOT at £5.00 is disposed of for £0.10.
    let events = convert_rows(serde_json::json!([fee_row("f")])).unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    let e = &events[0];
    assert_eq!(e.event_type, EventType::Disposal);
    assert_eq!(e.tag, Tag::Trade);
    assert_eq!(e.asset, "DOT");
    assert_eq!(e.quantity, dec!(0.02));
    assert_eq!(e.value_gbp, dec!(0.10));
    assert_eq!(e.fee_gbp, None);
}

#[test]
fn fee_transaction_in_sterling_produces_no_event() {
    let events = convert_rows(serde_json::json!([with(
        fee_row("f"),
        "fee",
        serde_json::json!({"asset": "GBP", "amount": "1.50"}),
    )]))
    .unwrap();
    assert!(events.is_empty(), "{events:?}");
}

// --- HMRC CRYPTO22280 worked example: Terri pays a 1-token fee ---
//
// https://www.gov.uk/hmrc-internal-manuals/cryptoassets-manual/crypto22280
// Terri holds 10,000 tokens that cost £20,000.

fn terri_buys_10000_dot() -> serde_json::Value {
    with(
        row(
            "buy",
            serde_json::json!({
                "type": "Trade",
                "sold": {"asset": "GBP", "quantity": "20000"},
                "bought": {"asset": "DOT", "quantity": "10000"},
            }),
        ),
        "datetime",
        serde_json::json!("2025-01-01T09:00:00Z"),
    )
}

fn one_dot_fee_at_5() -> serde_json::Value {
    serde_json::json!({"asset": "DOT", "amount": "1", "price": {"base": "DOT", "rate": "5"}})
}

fn total_gain(rows: serde_json::Value) -> Decimal {
    let events = convert_rows(rows).unwrap();
    crate::core::cgt::calculate_cgt(events)
        .disposals
        .iter()
        .map(|d| d.gain_gbp)
        .sum()
}

#[test]
fn hmrc_fee_example_crypto22280_fee_only_token_is_a_disposal() {
    // The token given as the fee: £5 consideration less £2 pool cost
    // (1/10,000 x £20,000) is a £3 gain.
    let fee = with(fee_row("fee"), "fee", one_dot_fee_at_5());
    assert_eq!(
        total_gain(serde_json::json!([terri_buys_10000_dot(), fee])),
        dec!(3)
    );
}

#[test]
fn hmrc_fee_example_crypto22280_sale_with_token_fee() {
    // Terri sells 1,000 tokens for £5,000 and pays 1 token as the fee. HMRC
    // combines both same-day disposals: £5,005 - £2,002 - £5 = £2,998.
    let sale = with(
        row(
            "sell",
            serde_json::json!({
                "type": "Trade",
                "sold": {"asset": "DOT", "quantity": "1000"},
                "bought": {"asset": "GBP", "quantity": "5000"},
            }),
        ),
        "fee",
        one_dot_fee_at_5(),
    );
    assert_eq!(
        total_gain(serde_json::json!([terri_buys_10000_dot(), sale])),
        dec!(2998)
    );
}

#[test]
fn reorganisation_types_accept_numeric_amounts_and_an_explicit_unclassified_tag() {
    // A producer may write quantities and amounts as JSON numbers and spell
    // out the default tag.
    let unclassified = |row| with(row, "tag", serde_json::json!("Unclassified"));
    let rows = serde_json::json!([
        unclassified(with(
            demerger_row("d"),
            "new_holding",
            serde_json::json!({"asset": "MICC", "quantity": 177}),
        )),
        unclassified(with(
            rights_issue_row("r"),
            "new_shares",
            serde_json::json!({"asset": "CSN", "quantity": 1473}),
        )),
        unclassified(with(
            small_distribution_row("s"),
            "amount",
            serde_json::json!(5.03),
        )),
        unclassified(with(
            fee_row("f"),
            "fee",
            serde_json::json!({"asset": "DOT", "amount": 0.02, "price": {"base": "DOT", "rate": "5.00"}}),
        )),
    ]);

    let events = convert_rows(rows).unwrap();

    let micc = events.iter().find(|e| e.asset == "MICC").unwrap();
    assert_eq!(micc.quantity, dec!(177));
    let distribution = events
        .iter()
        .find(|e| {
            e.event_type == EventType::PoolAdjustment(AdjustmentKind::SmallCapitalDistribution)
        })
        .unwrap();
    assert_eq!(distribution.total_cost_gbp(), dec!(-5.03));
    let fee = events.iter().find(|e| e.asset == "DOT").unwrap();
    assert_eq!(fee.value_gbp, dec!(0.10));
}
