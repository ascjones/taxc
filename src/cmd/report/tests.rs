use super::*;
use crate::cmd::filter::EventFilter;
use crate::core::events::builders::{acq, disp, event};
use crate::core::{AssetClass, EventType, Tag, TaxableEvent};
use rust_decimal_macros::dec;

fn no_filter() -> EventFilter {
    EventFilter {
        from: None,
        to: None,
        asset: None,
        event_kind: None,
    }
}

#[test]
fn gift_event_types_in_report_data() {
    let events = vec![
        TaxableEvent {
            tag: Tag::Gift,
            description: Some("Gift received".to_string()),
            ..acq("2024-01-01", "ETH", dec!(2), dec!(2000))
        },
        TaxableEvent {
            tag: Tag::Gift,
            description: Some("Gift given".to_string()),
            ..disp("2024-02-01", "ETH", dec!(1), dec!(1500))
        },
    ];

    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());

    let event_types: Vec<&str> = data.events.iter().map(|e| e.event_type.as_str()).collect();
    assert_eq!(event_types, vec!["GiftIn", "GiftOut"]);
}

#[test]
fn same_day_duplicate_acquisitions_link_to_first_row() {
    let events = vec![
        TaxableEvent {
            id: 1,
            ..acq("2024-06-15", "BTC", dec!(1), dec!(30000))
        },
        TaxableEvent {
            id: 2,
            ..acq("2024-06-15", "BTC", dec!(1), dec!(40000))
        },
        TaxableEvent {
            id: 3,
            ..disp("2024-06-15", "BTC", dec!(2), dec!(80000))
        },
    ];

    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());

    let disposal = data
        .events
        .iter()
        .find(|e| e.event_type == display_event_type(EventType::Disposal, Tag::Trade))
        .and_then(|e| e.cgt.as_ref())
        .expect("expected disposal with CGT details");

    assert_eq!(disposal.matching_components.len(), 1);
    for component in &disposal.matching_components {
        assert_eq!(
            component.matched_event_id,
            Some(1),
            "expected same-day match to point to first acquisition event id"
        );
    }
}

#[test]
fn bnb_duplicate_acquisitions_link_to_first_row() {
    let events = vec![
        TaxableEvent {
            id: 1,
            ..acq("2024-01-01", "BTC", dec!(5), dec!(100000))
        },
        TaxableEvent {
            id: 2,
            ..disp("2024-06-01", "BTC", dec!(1), dec!(25000))
        },
        TaxableEvent {
            id: 3,
            ..acq("2024-06-10", "BTC", dec!(1), dec!(22000))
        },
        TaxableEvent {
            id: 4,
            ..acq("2024-06-10", "BTC", dec!(1), dec!(24000))
        },
    ];

    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());

    let disposal = data
        .events
        .iter()
        .find(|e| e.event_type == display_event_type(EventType::Disposal, Tag::Trade))
        .and_then(|e| e.cgt.as_ref())
        .expect("expected disposal with CGT details");

    assert_eq!(disposal.matching_components.len(), 1);
    for component in &disposal.matching_components {
        assert_eq!(
            component.matched_event_id,
            Some(3),
            "expected B&B match to point to first acquisition event id for the matched date"
        );
    }
}

#[test]
fn warning_records_grouped_by_value_and_ordered_by_first_event() {
    let events = vec![
        TaxableEvent {
            id: 1,
            source_transaction_id: "tx-1".to_string(),
            ..disp("2024-06-01", "BTC", dec!(1), dec!(25000))
        },
        TaxableEvent {
            id: 2,
            source_transaction_id: "tx-2".to_string(),
            ..acq("2024-05-01", "ETH", dec!(2), dec!(2000))
        },
        TaxableEvent {
            id: 3,
            source_transaction_id: "tx-3".to_string(),
            tag: Tag::Unclassified,
            ..disp("2024-06-02", "ETH", dec!(1), dec!(1500))
        },
        TaxableEvent {
            id: 4,
            source_transaction_id: "tx-4".to_string(),
            tag: Tag::Unclassified,
            ..disp("2024-06-03", "ETH", dec!(1), dec!(1500))
        },
    ];

    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());

    assert_eq!(
        data.warnings.len(),
        2,
        "equal warnings should share a record"
    );
    assert!(matches!(
        data.warnings[0].warning,
        Warning::InsufficientCostBasis { .. }
    ));
    assert!(matches!(
        data.warnings[1].warning,
        Warning::UnclassifiedEvent
    ));

    let unclassified = &data.warnings[1];
    assert_eq!(unclassified.source_transaction_ids, vec!["tx-3", "tx-4"]);
    assert_eq!(unclassified.related_event_ids, vec![3, 4]);
}

#[test]
fn warning_records_distinct_insufficient_cost_basis_values_stay_separate() {
    let events = vec![
        TaxableEvent {
            id: 1,
            source_transaction_id: "tx-1".to_string(),
            ..disp("2024-06-01", "BTC", dec!(1), dec!(25000))
        },
        TaxableEvent {
            id: 2,
            source_transaction_id: "tx-2".to_string(),
            ..disp("2024-07-01", "BTC", dec!(2), dec!(50000))
        },
    ];

    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());

    assert_eq!(
        data.warnings.len(),
        2,
        "warnings with different field values must not merge"
    );
    assert!(data
        .warnings
        .iter()
        .all(|w| matches!(w.warning, Warning::InsufficientCostBasis { .. })));
    assert_eq!(data.warnings[0].related_event_ids, vec![1]);
    assert_eq!(data.warnings[1].related_event_ids, vec![2]);
}

#[test]
fn warning_records_same_first_event_tie_breaks_deterministically() {
    // One unclassified disposal with no pool carries both warning values, so
    // both grouped records share first related event id 1 and ordering falls
    // to the warning-value tie-breaker.
    let events = vec![TaxableEvent {
        id: 1,
        source_transaction_id: "tx-1".to_string(),
        tag: Tag::Unclassified,
        ..disp("2024-06-01", "BTC", dec!(1), dec!(25000))
    }];

    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());

    // The tie-breaker is Warning's derived Ord, i.e. variant declaration
    // order, so UnclassifiedEvent sorts before InsufficientCostBasis.
    assert_eq!(data.warnings.len(), 2);
    assert!(matches!(
        data.warnings[0].warning,
        Warning::UnclassifiedEvent
    ));
    assert!(matches!(
        data.warnings[1].warning,
        Warning::InsufficientCostBasis { .. }
    ));
}

#[test]
fn summary_includes_dividend_and_interest_totals() {
    let events = vec![
        TaxableEvent {
            id: 1,
            source_transaction_id: "tx-1".to_string(),
            tag: Tag::Salary,
            ..acq("2024-06-01", "BTC", dec!(0.1), dec!(1000))
        },
        TaxableEvent {
            id: 2,
            source_transaction_id: "tx-2".to_string(),
            tag: Tag::Dividend,
            ..acq("2024-06-02", "BTC", dec!(0.02), dec!(200))
        },
        TaxableEvent {
            id: 3,
            source_transaction_id: "tx-3".to_string(),
            tag: Tag::Interest,
            ..acq("2024-06-03", "BTC", dec!(0.03), dec!(300))
        },
    ];

    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());

    assert_eq!(data.summary.total_income, "1500.00");
    assert_eq!(data.summary.total_dividend_income, "200.00");
    assert_eq!(data.summary.total_interest_income, "300.00");
}

#[test]
fn summary_separates_crypto_and_stock_cgt_totals() {
    let events = vec![
        // Crypto: buy then sell
        acq("2024-01-01", "BTC", dec!(1), dec!(20000)),
        TaxableEvent {
            id: 2,
            ..disp("2024-06-01", "BTC", dec!(1), dec!(25000))
        },
        // Stock: buy then sell
        TaxableEvent {
            asset_class: AssetClass::Stock,
            ..acq("2024-01-01", "AAPL", dec!(10), dec!(1500))
        },
        TaxableEvent {
            id: 4,
            asset_class: AssetClass::Stock,
            ..disp("2024-06-01", "AAPL", dec!(10), dec!(2000))
        },
    ];

    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());

    // Combined totals
    assert_eq!(data.summary.total_proceeds, "27000.00");
    assert_eq!(data.summary.total_gain, "5500.00");

    // Crypto totals
    assert_eq!(data.summary.crypto.proceeds, "25000.00");
    assert_eq!(data.summary.crypto.costs, "20000.00");
    assert_eq!(data.summary.crypto.gain, "5000.00");

    // Stock totals
    assert_eq!(data.summary.stocks.proceeds, "2000.00");
    assert_eq!(data.summary.stocks.costs, "1500.00");
    assert_eq!(data.summary.stocks.gain, "500.00");

    // Fiat totals (no fiat disposals in this test)
    assert_eq!(data.summary.fiat.proceeds, "0.00");
    assert_eq!(data.summary.fiat.costs, "0.00");
    assert_eq!(data.summary.fiat.gain, "0.00");
}

fn disposal_cgt(data: &ReportData) -> &CgtDetails {
    data.events
        .iter()
        .find(|e| e.event_type == display_event_type(EventType::Disposal, Tag::Trade))
        .and_then(|e| e.cgt.as_ref())
        .expect("expected disposal with CGT details")
}

#[test]
fn report_rule_label_is_pool_for_pure_pool_disposal() {
    // A disposal matched entirely from the Section 104 pool is labelled "Pool".
    let events = vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        TaxableEvent {
            id: 2,
            ..disp("2024-06-01", "BTC", dec!(1), dec!(25000))
        },
    ];

    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());

    assert_eq!(disposal_cgt(&data).rule, "Pool");
}

#[test]
fn report_rule_label_is_mixed_for_multi_component_disposal() {
    // Same-day plus pool matching yields more than one component → "Mixed".
    let events = vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        TaxableEvent {
            id: 2,
            ..acq("2024-06-15", "BTC", dec!(2), dec!(30000))
        },
        TaxableEvent {
            id: 3,
            ..disp("2024-06-15", "BTC", dec!(5), dec!(75000))
        },
    ];

    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());

    assert_eq!(disposal_cgt(&data).rule, "Mixed");
}

#[test]
fn report_summary_aggregates_unclassified_disposals_separately() {
    // Unclassified disposals are excluded from classified totals but counted in
    // the "with unclassified" conservative totals.
    let events = vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        TaxableEvent {
            id: 2,
            tag: Tag::Unclassified,
            ..disp("2024-06-01", "BTC", dec!(1), dec!(25000))
        },
    ];

    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());

    assert_eq!(data.summary.unclassified_count, 1);
    assert_eq!(data.summary.total_gain, "0.00"); // unclassified excluded
    assert_eq!(data.summary.total_gain_with_unclassified, "15000.00"); // 25000 - 10000
}

#[test]
fn no_gain_no_loss_report_value_uses_cost_basis_with_note() {
    let events = vec![
        acq("2024-01-01", "BTC", dec!(2), dec!(50000)),
        TaxableEvent {
            id: 2,
            ..event(
                EventType::Disposal,
                Tag::NoGainNoLoss,
                "2024-06-01",
                "BTC",
                dec!(1),
                dec!(0),
                None,
            )
        },
    ];

    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());

    let ngnl = data
        .events
        .iter()
        .find(|e| e.tag == Tag::NoGainNoLoss)
        .expect("expected no gain/no loss event");

    assert_eq!(ngnl.value_gbp, "25000.00");
    assert_eq!(ngnl.value_gbp_note.as_deref(), Some(NGNL_VALUE_NOTE));
}

#[test]
fn event_datetime_is_rendered_in_uk_local_time() {
    // 23:30 UTC on 5 April 2024 is 00:30 BST on 6 April, in 2024/25.
    let events = vec![TaxableEvent {
        id: 1,
        datetime: chrono::DateTime::parse_from_rfc3339("2024-04-05T23:30:00Z").unwrap(),
        ..acq("2024-04-05", "BTC", dec!(1), dec!(1000))
    }];
    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());
    assert_eq!(data.events[0].datetime, "2024-04-06T00:30:00+01:00");
    assert_eq!(data.events[0].tax_year, "2024/25");
}

/// Parse a document and convert it the way the CLI does.
fn load(
    json: &str,
) -> (
    Vec<crate::core::transactions::Transaction>,
    Vec<TaxableEvent>,
) {
    let (txs, registry) = crate::core::read_transactions_json(json.as_bytes()).unwrap();
    let events = crate::core::transactions_to_events(
        &txs,
        &registry,
        crate::core::ConversionOptions::default(),
    )
    .unwrap();
    (txs, events)
}

const TWO_ASSETS: &str = r#"{
  "assets": [
    {"symbol": "BTC", "asset_class": "Crypto"},
    {"symbol": "ETH", "asset_class": "Crypto"}
  ],
  "transactions": [
    {"id": "btc-buy", "datetime": "2024-05-01T10:00:00Z", "account": "k", "type": "Trade",
     "sold": {"asset": "GBP", "quantity": 1000}, "bought": {"asset": "BTC", "quantity": 1}},
    {"id": "eth-buy", "datetime": "2024-05-02T10:00:00Z", "account": "k", "type": "Trade",
     "sold": {"asset": "GBP", "quantity": 500}, "bought": {"asset": "ETH", "quantity": 1}},
    {"id": "btc-sell", "datetime": "2024-06-01T10:00:00Z", "account": "k", "type": "Trade",
     "sold": {"asset": "BTC", "quantity": 1}, "bought": {"asset": "GBP", "quantity": 2000}},
    {"id": "btc-rebuy", "datetime": "2024-06-10T10:00:00Z", "account": "k", "type": "Trade",
     "sold": {"asset": "GBP", "quantity": 1800}, "bought": {"asset": "BTC", "quantity": 1}}
  ]
}"#;

#[test]
fn transaction_rows_respect_the_asset_filter() {
    let (txs, events) = load(TWO_ASSETS);
    let cgt_report = calculate_cgt(events.clone());
    let filter = EventFilter {
        asset: Some("BTC".to_string()),
        ..no_filter()
    };
    let data = build_report_data(&txs, &events, &cgt_report, &filter);
    let ids: Vec<&str> = data.transactions.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, vec!["btc-buy", "btc-sell", "btc-rebuy"]);
}

#[test]
fn transaction_rows_respect_the_date_filter() {
    let (txs, events) = load(TWO_ASSETS);
    let cgt_report = calculate_cgt(events.clone());
    let filter = EventFilter {
        from: chrono::NaiveDate::from_ymd_opt(2024, 6, 1),
        ..no_filter()
    };
    let data = build_report_data(&txs, &events, &cgt_report, &filter);
    let ids: Vec<&str> = data.transactions.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, vec!["btc-sell", "btc-rebuy"]);
}

#[test]
fn bnb_link_survives_when_the_acquisition_is_outside_the_filter() {
    let (txs, events) = load(TWO_ASSETS);
    let cgt_report = calculate_cgt(events.clone());
    let filter = EventFilter {
        to: chrono::NaiveDate::from_ymd_opt(2024, 6, 5),
        ..no_filter()
    };
    let data = build_report_data(&txs, &events, &cgt_report, &filter);
    let sale = data
        .events
        .iter()
        .find(|e| e.source_transaction_id == "btc-sell")
        .unwrap();
    let components = &sale.cgt.as_ref().unwrap().matching_components;
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].rule, "B&B");
    let rebuy_id = events
        .iter()
        .find(|e| e.source_transaction_id == "btc-rebuy")
        .unwrap()
        .id;
    assert_eq!(components[0].matched_event_id, Some(rebuy_id));
    assert_eq!(
        components[0].matched_original_value.as_deref(),
        Some("1800.00")
    );
}

#[test]
fn report_quantities_use_the_canonical_rendering() {
    let events = vec![TaxableEvent {
        id: 1,
        ..acq("2024-05-01", "ETH", dec!(1.1234567890), dec!(1000))
    }];
    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());
    assert_eq!(data.events[0].quantity, "1.12345679");
}

#[test]
fn summary_disposal_count_excludes_unclassified_like_taxc_summary() {
    let events = vec![
        TaxableEvent {
            id: 1,
            ..acq("2024-05-01", "BTC", dec!(2), dec!(1000))
        },
        TaxableEvent {
            id: 2,
            ..disp("2024-06-01", "BTC", dec!(1), dec!(900))
        },
        TaxableEvent {
            id: 3,
            tag: Tag::Unclassified,
            ..disp("2024-06-02", "BTC", dec!(1), dec!(900))
        },
    ];
    let cgt_report = calculate_cgt(events.clone());
    let data = build_report_data(&[], &events, &cgt_report, &no_filter());
    assert_eq!(data.summary.disposal_count, 1);
}

#[cfg(unix)]
#[test]
fn private_temp_report_is_unique_and_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let a = write_private_temp("<html>a</html>").unwrap();
    let b = write_private_temp("<html>b</html>").unwrap();
    assert_ne!(a, b, "each run gets its own file");
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "<html>a</html>");
    let mode = std::fs::metadata(&a).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

const WITH_EVENTLESS: &str = r#"{
  "assets": [
    {"symbol": "BTC", "asset_class": "Crypto"},
    {"symbol": "ETH", "asset_class": "Crypto"}
  ],
  "transactions": [
    {"id": "gbp-in", "datetime": "2024-05-01T10:00:00Z", "account": "k", "type": "Deposit",
     "amount": {"asset": "GBP", "quantity": 1000}},
    {"id": "btc-buy", "datetime": "2024-05-02T10:00:00Z", "account": "k", "type": "Trade",
     "sold": {"asset": "GBP", "quantity": 1000}, "bought": {"asset": "BTC", "quantity": 1}},
    {"id": "btc-out", "datetime": "2024-06-01T10:00:00Z", "account": "k", "type": "Withdrawal",
     "amount": {"asset": "BTC", "quantity": 1}, "linked_deposit": "btc-in"},
    {"id": "btc-in", "datetime": "2024-06-01T10:05:00Z", "account": "ledger", "type": "Deposit",
     "amount": {"asset": "BTC", "quantity": 1}, "linked_withdrawal": "btc-out"},
    {"id": "eth-buy", "datetime": "2024-06-02T10:00:00Z", "account": "k", "type": "Trade",
     "sold": {"asset": "GBP", "quantity": 500}, "bought": {"asset": "ETH", "quantity": 1}}
  ]
}"#;

fn shown_ids(filter: EventFilter) -> Vec<String> {
    let (txs, events) = load(WITH_EVENTLESS);
    let cgt_report = calculate_cgt(events.clone());
    build_report_data(&txs, &events, &cgt_report, &filter)
        .transactions
        .into_iter()
        .map(|t| t.id)
        .collect()
}

#[test]
fn eventless_transactions_follow_the_date_and_asset_filters() {
    // The GBP deposit and the exact linked BTC pair produce no events.
    assert_eq!(
        shown_ids(no_filter()),
        vec!["gbp-in", "btc-buy", "btc-out", "btc-in", "eth-buy"]
    );
    assert_eq!(
        shown_ids(EventFilter {
            asset: Some("BTC".to_string()),
            ..no_filter()
        }),
        vec!["btc-buy", "btc-out", "btc-in"]
    );
    assert_eq!(
        shown_ids(EventFilter {
            from: chrono::NaiveDate::from_ymd_opt(2024, 6, 1),
            ..no_filter()
        }),
        vec!["btc-out", "btc-in", "eth-buy"]
    );
}

#[test]
fn eventless_transactions_are_hidden_under_an_event_kind_filter() {
    // An event-kind filter selects events; a transaction without any has none
    // of that kind.
    assert_eq!(
        shown_ids(EventFilter {
            event_kind: Some(EventKind::Acquisition),
            ..no_filter()
        }),
        vec!["btc-buy", "eth-buy"]
    );
}
