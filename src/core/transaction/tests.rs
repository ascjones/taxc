use super::datetime::parse_datetime;
use super::*;
use crate::core::events::{AssetClass, EventType, Tag};
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
    let tx = Transaction {
        id: "tx-1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(fx_price("ETH", dec!(2000), "USD", dec!(0.79))),
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(0.01),
            },
            bought: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(0.5),
            },
        },
    };

    let events = tx.to_taxable_events(&test_registry(), false).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, EventType::Disposal);
    assert_eq!(events[1].event_type, EventType::Acquisition);
    assert_eq!(events[0].value_gbp, events[1].value_gbp);
}

#[test]
fn transactions_to_events_assigns_sequential_event_ids() {
    let tx1 = Transaction {
        id: "tx-1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(fx_price("ETH", dec!(2000), "USD", dec!(0.79))),
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(0.01),
            },
            bought: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(0.5),
            },
        },
    };

    let tx2 = Transaction {
        id: "tx-2".to_string(),
        datetime: dt("2024-01-02T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: Some(gbp_price("ETH", dec!(2000))),
        fee: None,
        tag: Tag::StakingReward,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(0.01),
            },
            linked_withdrawal: None,
        },
    };

    let events = transactions_to_events(
        &[tx1, tx2],
        &test_registry(),
        ConversionOptions {
            exclude_unlinked: false,
        },
    )
    .unwrap();

    let ids: Vec<usize> = events.iter().map(|e| e.id).collect();
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn trade_gbp_to_crypto_only_acquisition() {
    let tx = Transaction {
        id: "tx-2".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "GBP".to_string(),
                quantity: dec!(1000),
            },
            bought: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(0.02),
            },
        },
    };

    let events = tx.to_taxable_events(&test_registry(), false).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Acquisition);
    assert_eq!(events[0].value_gbp, dec!(1000));
}

#[test]
fn trade_crypto_to_gbp_only_disposal() {
    let tx = Transaction {
        id: "tx-3".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(0.02),
            },
            bought: Amount {
                asset: "GBP".to_string(),
                quantity: dec!(1000),
            },
        },
    };

    let events = tx.to_taxable_events(&test_registry(), false).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Disposal);
    assert_eq!(events[0].value_gbp, dec!(1000));
}

#[test]
fn trade_without_price_no_gbp_errors() {
    let tx = Transaction {
        id: "tx-4".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(0.02),
            },
            bought: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(0.5),
            },
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingTradePrice {
            id: "tx-4".to_string()
        }
    );
}

#[test]
fn linked_deposit_withdrawal_no_events() {
    let deposit = Transaction {
        id: "d1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            linked_withdrawal: Some("w1".to_string()),
        },
    };
    let withdrawal = Transaction {
        id: "w1".to_string(),
        datetime: dt("2024-01-01T09:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Withdrawal {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            linked_deposit: Some("d1".to_string()),
        },
    };

    let events = transactions_to_events(
        &[deposit, withdrawal],
        &test_registry(),
        ConversionOptions {
            exclude_unlinked: false,
        },
    )
    .unwrap();
    assert!(events.is_empty());
}

#[test]
fn unlinked_crypto_deposit_warns_and_creates_acquisition() {
    let deposit = Transaction {
        id: "d1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            linked_withdrawal: None,
        },
    };

    let events = transactions_to_events(
        &[deposit],
        &test_registry(),
        ConversionOptions {
            exclude_unlinked: false,
        },
    )
    .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Acquisition);
    assert_eq!(events[0].tag, Tag::Unclassified);
}

#[test]
fn exclude_unlinked_flag_skips_events() {
    let withdrawal = Transaction {
        id: "w1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Withdrawal {
            amount: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(1),
            },
            linked_deposit: None,
        },
    };

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
fn staking_reward_generates_income_event() {
    let tx = Transaction {
        id: "s1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: Some(gbp_price("ETH", dec!(2000))),
        fee: None,
        tag: Tag::StakingReward,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(0.01),
            },
            linked_withdrawal: None,
        },
    };

    let events = tx.to_taxable_events(&test_registry(), false).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Acquisition);
    assert_eq!(events[0].tag, Tag::StakingReward);
    assert_eq!(events[0].value_gbp, dec!(20));
}

#[test]
fn fee_allocated_to_disposal() {
    let tx = Transaction {
        id: "t1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(gbp_price("ETH", dec!(1000))),
        fee: Some(Fee {
            asset: "GBP".to_string(),
            amount: dec!(5),
            price: None,
        }),
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(1),
            },
            bought: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(10),
            },
        },
    };

    let events = tx.to_taxable_events(&test_registry(), false).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].fee_gbp, Some(dec!(5)));
    assert_eq!(events[1].fee_gbp, None);
}

#[test]
fn fee_uses_trade_price_when_asset_matches_bought() {
    // Trade: 1 ETH -> 0.05 BTC at £15000/BTC
    // Fee: 0.0001 BTC (no explicit price, but matches bought asset)
    // Fee value = 0.0001 * 15000 = £1.50
    let tx = Transaction {
        id: "t1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(gbp_price("BTC", dec!(15000))),
        fee: Some(Fee {
            asset: "BTC".to_string(),
            amount: dec!(0.0001),
            price: None,
        }),
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            bought: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(0.05),
            },
        },
    };

    let events = tx.to_taxable_events(&test_registry(), false).unwrap();
    assert_eq!(events.len(), 2);
    // Fee uses trade price directly: 0.0001 * 15000 = £1.50
    assert_eq!(events[0].fee_gbp, Some(dec!(1.50)));
}

#[test]
fn fee_in_sold_asset_requires_explicit_price() {
    // Trade: 1 ETH -> 0.05 BTC at £15000/BTC
    // Fee: 0.01 ETH (no explicit price, doesn't match bought asset)
    // Should error - sold asset doesn't get automatic price
    let tx = Transaction {
        id: "t1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(gbp_price("BTC", dec!(15000))),
        fee: Some(Fee {
            asset: "ETH".to_string(),
            amount: dec!(0.01),
            price: None,
        }),
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            bought: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(0.05),
            },
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingFeePrice {
            asset: "ETH".to_string()
        }
    );
}

#[test]
fn fee_explicit_price_takes_precedence() {
    // Fee has explicit price even though asset matches traded asset
    let tx = Transaction {
        id: "t1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(gbp_price("BTC", dec!(15000))),
        fee: Some(Fee {
            asset: "BTC".to_string(),
            amount: dec!(0.0001),
            // Explicit price overrides trade price
            price: Some(gbp_price("BTC", dec!(20000))),
        }),
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            bought: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(0.05),
            },
        },
    };

    let events = tx.to_taxable_events(&test_registry(), false).unwrap();
    assert_eq!(events.len(), 2);
    // Fee should use explicit price: 0.0001 * 20000 = £2.00
    assert_eq!(events[0].fee_gbp, Some(dec!(2)));
}

#[test]
fn fee_unrelated_asset_requires_price() {
    // Fee in USDT but trade is ETH/BTC - should error
    let tx = Transaction {
        id: "t1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(gbp_price("BTC", dec!(15000))),
        fee: Some(Fee {
            asset: "USDT".to_string(),
            amount: dec!(5),
            price: None,
        }),
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            bought: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(0.05),
            },
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingFeePrice {
            asset: "USDT".to_string()
        }
    );
}

#[test]
fn fee_asset_match_is_case_insensitive() {
    // Fee asset "btc" should match bought asset "BTC"
    let tx = Transaction {
        id: "t1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(gbp_price("BTC", dec!(15000))),
        fee: Some(Fee {
            asset: "btc".to_string(), // lowercase
            amount: dec!(0.0001),
            price: None,
        }),
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            bought: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(0.05),
            },
        },
    };

    let events = tx.to_taxable_events(&test_registry(), false).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].fee_gbp, Some(dec!(1.50)));
}

#[test]
fn staking_reward_requires_price() {
    let tx = Transaction {
        id: "s1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::StakingReward,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(0.01),
            },
            linked_withdrawal: None,
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingTaggedPrice {
            id: "s1".to_string(),
            tag: "StakingReward".to_string(),
            tx_type: "deposit".to_string(),
        }
    );
}

#[test]
fn income_deposit_with_mismatched_price_base_errors() {
    let tx = Transaction {
        id: "s1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: Some(gbp_price("BTC", dec!(2000))),
        fee: None,
        tag: Tag::StakingReward,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(0.01),
            },
            linked_withdrawal: None,
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
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
    let tx = Transaction {
        id: "s1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: Some(gbp_price("ETH", dec!(2000))),
        fee: None,
        tag: Tag::StakingReward,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(0.01),
            },
            linked_withdrawal: Some("w1".to_string()),
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::TaggedDepositLinked {
            id: "s1".to_string()
        }
    );
}

#[test]
fn tagged_withdrawal_with_linked_deposit_errors() {
    let tx = Transaction {
        id: "w1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(gbp_price("ETH", dec!(2000))),
        fee: None,
        tag: Tag::Gift,
        details: TransactionType::Withdrawal {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(0.01),
            },
            linked_deposit: Some("d1".to_string()),
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::TaggedWithdrawalLinked {
            id: "w1".to_string()
        }
    );
}

#[test]
fn income_tag_on_withdrawal_errors() {
    let tx = Transaction {
        id: "w1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(gbp_price("ETH", dec!(2000))),
        fee: None,
        tag: Tag::StakingReward,
        details: TransactionType::Withdrawal {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(0.01),
            },
            linked_deposit: None,
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::InvalidTagForType {
            id: "w1".to_string(),
            tag: "StakingReward".to_string(),
            tx_type: "withdrawal".to_string(),
        }
    );
}

#[test]
fn airdrop_tag_on_withdrawal_errors() {
    let tx = Transaction {
        id: "w1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(gbp_price("ETH", dec!(2000))),
        fee: None,
        tag: Tag::Airdrop,
        details: TransactionType::Withdrawal {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(0.01),
            },
            linked_deposit: None,
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::InvalidTagForType {
            id: "w1".to_string(),
            tag: "Airdrop".to_string(),
            tx_type: "withdrawal".to_string(),
        }
    );
}

#[test]
fn non_trade_tag_on_trade_errors() {
    let tx = Transaction {
        id: "t1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(gbp_price("BTC", dec!(2000))),
        fee: None,
        tag: Tag::StakingReward,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            bought: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(0.05),
            },
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::InvalidTagForType {
            id: "t1".to_string(),
            tag: "StakingReward".to_string(),
            tx_type: "trade".to_string(),
        }
    );
}

#[test]
fn gift_deposit_missing_price_errors() {
    let tx = Transaction {
        id: "d1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Gift,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            linked_withdrawal: None,
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingTaggedPrice {
            id: "d1".to_string(),
            tag: "Gift".to_string(),
            tx_type: "deposit".to_string(),
        }
    );
}

#[test]
fn gift_withdrawal_missing_price_errors() {
    let tx = Transaction {
        id: "w1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Gift,
        details: TransactionType::Withdrawal {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            linked_deposit: None,
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::MissingTaggedPrice {
            id: "w1".to_string(),
            tag: "Gift".to_string(),
            tx_type: "withdrawal".to_string(),
        }
    );
}

#[test]
fn trade_tag_on_deposit_errors() {
    let tx = Transaction {
        id: "d1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Trade,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            linked_withdrawal: None,
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
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
    let tx = Transaction {
        id: "w1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Trade,
        details: TransactionType::Withdrawal {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            linked_deposit: None,
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
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
    let tx = Transaction {
        id: "d1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: Some(gbp_price("ETH", dec!(1000))),
        fee: None,
        tag: Tag::Airdrop,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            linked_withdrawal: None,
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::AirdropPriceNotAllowed {
            id: "d1".to_string(),
        }
    );
}

#[test]
fn gift_deposit_creates_gift_in() {
    let tx = Transaction {
        id: "d1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: Some(gbp_price("ETH", dec!(1000))),
        fee: None,
        tag: Tag::Gift,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(2),
            },
            linked_withdrawal: None,
        },
    };

    let events = tx.to_taxable_events(&test_registry(), false).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Acquisition);
    assert_eq!(events[0].tag, Tag::Gift);
    assert_eq!(events[0].value_gbp, dec!(2000));
}

#[test]
fn gift_withdrawal_creates_gift_out() {
    let tx = Transaction {
        id: "w1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: Some(gbp_price("ETH", dec!(1000))),
        fee: None,
        tag: Tag::Gift,
        details: TransactionType::Withdrawal {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(2),
            },
            linked_deposit: None,
        },
    };

    let events = tx.to_taxable_events(&test_registry(), false).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Disposal);
    assert_eq!(events[0].tag, Tag::Gift);
    assert_eq!(events[0].value_gbp, dec!(2000));
}

#[test]
fn airdrop_deposit_creates_zero_cost_acquisition() {
    let tx = Transaction {
        id: "d1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Airdrop,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(2),
            },
            linked_withdrawal: None,
        },
    };

    let events = tx.to_taxable_events(&test_registry(), false).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tag, Tag::Airdrop);
    assert_eq!(events[0].value_gbp, Decimal::ZERO);
}

#[test]
fn airdrop_income_deposit_requires_price_and_counts_as_income_tag() {
    let tx = Transaction {
        id: "d1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "ledger".to_string(),
        description: None,
        price: Some(gbp_price("ETH", dec!(1000))),
        fee: None,
        tag: Tag::AirdropIncome,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(2),
            },
            linked_withdrawal: None,
        },
    };

    let events = tx.to_taxable_events(&test_registry(), false).unwrap();
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
        let tx = Transaction {
            id: id.to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(1000))),
            fee: None,
            tag,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                linked_withdrawal: None,
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events[0].tag, tag);
    }
}

#[test]
fn dividend_and_interest_tags_on_trade_error() {
    let cases = [(Tag::Dividend, "Dividend"), (Tag::Interest, "Interest")];

    for (tag, tag_name) in cases {
        let tx = Transaction {
            id: "t1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("BTC", dec!(2000))),
            fee: None,
            tag,
            details: TransactionType::Trade {
                sold: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                bought: Amount {
                    asset: "BTC".to_string(),
                    quantity: dec!(0.05),
                },
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
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
fn dividend_and_interest_tags_on_withdrawal_error() {
    let cases = [(Tag::Dividend, "Dividend"), (Tag::Interest, "Interest")];

    for (tag, tag_name) in cases {
        let tx = Transaction {
            id: "w1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "kraken".to_string(),
            description: None,
            price: Some(gbp_price("ETH", dec!(2000))),
            fee: None,
            tag,
            details: TransactionType::Withdrawal {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(0.01),
                },
                linked_deposit: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
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
fn dividend_and_interest_deposits_require_price() {
    let cases = [(Tag::Dividend, "Dividend"), (Tag::Interest, "Interest")];

    for (tag, tag_name) in cases {
        let tx = Transaction {
            id: "d1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "ledger".to_string(),
            description: None,
            price: None,
            fee: None,
            tag,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "ETH".to_string(),
                    quantity: dec!(1),
                },
                linked_withdrawal: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::MissingTaggedPrice {
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
        let tx = Transaction {
            id: "d1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "bank".to_string(),
            description: None,
            price: None,
            fee: None,
            tag,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "GBP".to_string(),
                    quantity: dec!(500),
                },
                linked_withdrawal: None,
            },
        };

        let events = tx.to_taxable_events(&test_registry(), false).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value_gbp, dec!(500));
        assert_eq!(events[0].asset_class, AssetClass::Fiat);
    }
}

#[test]
fn gbp_dividend_and_interest_deposits_reject_price() {
    let cases = [(Tag::Dividend, "Dividend"), (Tag::Interest, "Interest")];

    for (tag, tag_name) in cases {
        let tx = Transaction {
            id: "d1".to_string(),
            datetime: dt("2024-01-01T10:00:00+00:00"),
            account: "bank".to_string(),
            description: None,
            price: Some(gbp_price("GBP", dec!(1))),
            fee: None,
            tag,
            details: TransactionType::Deposit {
                amount: Amount {
                    asset: "GBP".to_string(),
                    quantity: dec!(500),
                },
                linked_withdrawal: None,
            },
        };

        let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
        assert_eq!(
            err,
            TransactionError::GbpIncomePriceNotAllowed {
                id: "d1".to_string(),
                tag: tag_name.to_string(),
            }
        );
    }
}

#[test]
fn trade_sell_to_gbp_rejects_price() {
    let tx = Transaction {
        id: "t1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "broker".to_string(),
        description: None,
        price: Some(gbp_price("AAPL", dec!(150))),
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "AAPL".to_string(),
                quantity: dec!(10),
            },
            bought: Amount {
                asset: "GBP".to_string(),
                quantity: dec!(1500),
            },
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::GbpTradePriceNotAllowed {
            id: "t1".to_string()
        }
    );
}

#[test]
fn trade_buy_from_gbp_rejects_price() {
    let tx = Transaction {
        id: "t2".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "broker".to_string(),
        description: None,
        price: Some(gbp_price("AAPL", dec!(150))),
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "GBP".to_string(),
                quantity: dec!(1500),
            },
            bought: Amount {
                asset: "AAPL".to_string(),
                quantity: dec!(10),
            },
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::GbpTradePriceNotAllowed {
            id: "t2".to_string()
        }
    );
}

#[test]
fn price_base_must_match_bought_asset() {
    let tx = Transaction {
        id: "t1".to_string(),
        datetime: dt("2024-01-01T10:00:00+00:00"),
        account: "kraken".to_string(),
        description: None,
        price: Some(gbp_price("ETH", dec!(2000))), // Wrong base - should be BTC
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            bought: Amount {
                asset: "BTC".to_string(),
                quantity: dec!(0.05),
            },
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
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
          "price": { "base": "BTC", "rate": 1000 }
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
          "price": { "base": "ETH", "rate": 2000 }
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
    let tx = Transaction {
        id: "tx-1".to_string(),
        datetime: dt("2024-01-01T00:00:00+00:00"),
        account: "broker".to_string(),
        description: None,
        price: None,
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Trade {
            sold: Amount {
                asset: "GBP".to_string(),
                quantity: dec!(1000),
            },
            bought: Amount {
                asset: "AAPL".to_string(),
                quantity: dec!(10),
            },
        },
    };

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
fn unclassified_deposit_price_base_mismatch_errors() {
    let tx = Transaction {
        id: "d1".to_string(),
        datetime: dt("2024-01-01T00:00:00+00:00"),
        account: "wallet".to_string(),
        description: None,
        price: Some(gbp_price("BTC", dec!(1000))),
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Deposit {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            linked_withdrawal: None,
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::PriceBaseMismatch {
            id: "d1".to_string(),
            base: "BTC".to_string(),
            expected: "ETH".to_string(),
        }
    );
}

#[test]
fn unclassified_withdrawal_price_base_mismatch_errors() {
    let tx = Transaction {
        id: "w1".to_string(),
        datetime: dt("2024-01-01T00:00:00+00:00"),
        account: "wallet".to_string(),
        description: None,
        price: Some(gbp_price("BTC", dec!(1000))),
        fee: None,
        tag: Tag::Unclassified,
        details: TransactionType::Withdrawal {
            amount: Amount {
                asset: "ETH".to_string(),
                quantity: dec!(1),
            },
            linked_deposit: None,
        },
    };

    let err = tx.to_taxable_events(&test_registry(), false).unwrap_err();
    assert_eq!(
        err,
        TransactionError::PriceBaseMismatch {
            id: "w1".to_string(),
            base: "BTC".to_string(),
            expected: "ETH".to_string(),
        }
    );
}
