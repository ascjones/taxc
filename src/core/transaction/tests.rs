use super::datetime::parse_datetime;
use super::*;
use crate::core::events::{AssetClass, EventType, Tag, TaxableEvent};
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
    for symbol in ["BTC", "ETH", "USDT"] {
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
fn trade_value_gbp_with_gbp_fee() {
    let tx = trade_tx("t-v-fee", ("BTC", dec!(1)), ("ETH", dec!(10)))
        .with_value_gbp(dec!(1000))
        .with_fee(Fee {
            asset: "GBP".to_string(),
            amount: dec!(5),
            price: None,
        });

    let events = convert_one(&tx).unwrap();
    assert_eq!(events[0].fee_gbp, Some(dec!(5)));
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
fn deposit_income_value_gbp_with_gbp_fee() {
    let tx = deposit_tx("d-income-fee", "ETH", dec!(1))
        .with_tag(Tag::StakingReward)
        .with_value_gbp(dec!(1000))
        .with_fee(Fee {
            asset: "GBP".to_string(),
            amount: dec!(7),
            price: None,
        });

    let events = convert_one(&tx).unwrap();
    assert_eq!(events[0].fee_gbp, Some(dec!(7)));
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
    assert_eq!(events.len(), 2);
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
    assert_eq!(events.len(), 2);
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
    assert_eq!(events.len(), 2);
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
fn deposit_gift_with_value_gbp() {
    let tx = deposit_tx("d1-value", "ETH", dec!(2))
        .with_tag(Tag::Gift)
        .with_value_gbp(dec!(2000));

    let events = convert_one(&tx).unwrap();
    assert_eq!(events.len(), 1);
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
fn deposit_gift_value_gbp_with_fee() {
    let tx = deposit_tx("d-gift-fee", "ETH", dec!(2))
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
fn gbp_dividend_and_interest_deposits_no_price_needed() {
    let cases = [(Tag::Dividend, "Dividend"), (Tag::Interest, "Interest")];

    for (tag, _tag_name) in cases {
        let tx = deposit_tx("d1", "GBP", dec!(500)).with_tag(tag);
        let events = convert_one(&tx).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value_gbp, dec!(500));
        assert_eq!(events[0].asset_class, AssetClass::Fiat);
    }
}

#[test]
fn gbp_dividend_and_interest_deposits_reject_price() {
    let cases = [(Tag::Dividend, "Dividend"), (Tag::Interest, "Interest")];

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
fn unlinked_withdrawal_value_gbp_with_fee() {
    let tx = withdrawal_tx("w-unlinked-fee", "ETH", dec!(2))
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
fn serde_round_trip_valuation_price() {
    let tx = trade_tx("serde-price", ("BTC", dec!(0.5)), ("ETH", dec!(8)))
        .with_price(gbp_price("ETH", dec!(1875)))
        .build();

    let json = serde_json::to_string(&tx).unwrap();
    let round_tripped: Transaction = serde_json::from_str(&json).unwrap();
    assert!(matches!(
        round_tripped.valuation,
        Some(Valuation::Price(ref p)) if p.base == "ETH"
    ));
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
fn serde_round_trip_valuation_none() {
    let tx = trade_tx("serde-none", ("GBP", dec!(1000)), ("BTC", dec!(0.05))).build();

    let json = serde_json::to_string(&tx).unwrap();
    assert!(!json.contains("\"valuation\""));

    let round_tripped: Transaction = serde_json::from_str(&json).unwrap();
    assert_eq!(round_tripped.valuation, None);
}
