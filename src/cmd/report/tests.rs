use super::*;
use crate::cmd::filter::EventFilter;
use crate::core::{AssetClass, EventType, Tag, TaxableEvent};
use chrono::DateTime;
use rust_decimal_macros::dec;

fn dt(date: &str) -> chrono::DateTime<chrono::FixedOffset> {
    DateTime::parse_from_rfc3339(&format!("{date}T00:00:00+00:00")).unwrap()
}

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
            id: 1,
            source_transaction_id: "tx-test".to_string(),
            datetime: dt("2024-01-01"),
            event_type: EventType::Acquisition,
            tag: Tag::Gift,
            asset: "ETH".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(2),
            value_gbp: dec!(2000),
            fee_gbp: None,
            description: Some("Gift received".to_string()),
        },
        TaxableEvent {
            id: 2,
            source_transaction_id: "tx-test".to_string(),
            datetime: dt("2024-02-01"),
            event_type: EventType::Disposal,
            tag: Tag::Gift,
            asset: "ETH".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1),
            value_gbp: dec!(1500),
            fee_gbp: None,
            description: Some("Gift given".to_string()),
        },
    ];

    let cgt_report = calculate_cgt(events.clone()).unwrap();
    let data = build_report_data(&events, &cgt_report, &no_filter()).unwrap();

    let event_types: Vec<String> = data.events.iter().map(|e| e.event_type.clone()).collect();
    assert!(event_types.iter().any(|t| t == "GiftIn"));
    assert!(event_types.iter().any(|t| t == "GiftOut"));
}

#[test]
fn same_day_duplicate_acquisitions_link_to_first_row() {
    let events = vec![
        TaxableEvent {
            id: 1,
            source_transaction_id: "tx-test".to_string(),
            datetime: dt("2024-06-15"),
            event_type: EventType::Acquisition,
            tag: Tag::Trade,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1),
            value_gbp: dec!(30000),
            fee_gbp: None,
            description: None,
        },
        TaxableEvent {
            id: 2,
            source_transaction_id: "tx-test".to_string(),
            datetime: dt("2024-06-15"),
            event_type: EventType::Acquisition,
            tag: Tag::Trade,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1),
            value_gbp: dec!(40000),
            fee_gbp: None,
            description: None,
        },
        TaxableEvent {
            id: 3,
            source_transaction_id: "tx-test".to_string(),
            datetime: dt("2024-06-15"),
            event_type: EventType::Disposal,
            tag: Tag::Trade,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(2),
            value_gbp: dec!(80000),
            fee_gbp: None,
            description: None,
        },
    ];

    let cgt_report = calculate_cgt(events.clone()).unwrap();
    let data = build_report_data(&events, &cgt_report, &no_filter()).unwrap();

    let disposal = data
        .events
        .iter()
        .find(|e| e.event_type == "Disposal")
        .and_then(|e| e.cgt.as_ref())
        .expect("expected disposal with CGT details");

    for component in &disposal.matching_components {
        assert_eq!(
            component.matched_row_id,
            Some(0),
            "expected same-day match to point to first acquisition row"
        );
    }
}

#[test]
fn bnb_duplicate_acquisitions_link_to_first_row() {
    let events = vec![
        TaxableEvent {
            id: 1,
            source_transaction_id: "tx-test".to_string(),
            datetime: dt("2024-01-01"),
            event_type: EventType::Acquisition,
            tag: Tag::Trade,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(5),
            value_gbp: dec!(100000),
            fee_gbp: None,
            description: None,
        },
        TaxableEvent {
            id: 2,
            source_transaction_id: "tx-test".to_string(),
            datetime: dt("2024-06-01"),
            event_type: EventType::Disposal,
            tag: Tag::Trade,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1),
            value_gbp: dec!(25000),
            fee_gbp: None,
            description: None,
        },
        TaxableEvent {
            id: 3,
            source_transaction_id: "tx-test".to_string(),
            datetime: dt("2024-06-10"),
            event_type: EventType::Acquisition,
            tag: Tag::Trade,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1),
            value_gbp: dec!(22000),
            fee_gbp: None,
            description: None,
        },
        TaxableEvent {
            id: 4,
            source_transaction_id: "tx-test".to_string(),
            datetime: dt("2024-06-10"),
            event_type: EventType::Acquisition,
            tag: Tag::Trade,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1),
            value_gbp: dec!(24000),
            fee_gbp: None,
            description: None,
        },
    ];

    let cgt_report = calculate_cgt(events.clone()).unwrap();
    let data = build_report_data(&events, &cgt_report, &no_filter()).unwrap();

    let disposal = data
        .events
        .iter()
        .find(|e| e.event_type == "Disposal")
        .and_then(|e| e.cgt.as_ref())
        .expect("expected disposal with CGT details");

    for component in &disposal.matching_components {
        assert_eq!(
            component.matched_row_id,
            Some(2),
            "expected B&B match to point to first acquisition row for the matched date"
        );
    }
}

#[test]
fn warning_records_link_source_transaction_and_event_ids() {
    let events = vec![TaxableEvent {
        id: 1,
        source_transaction_id: "tx-1".to_string(),
        datetime: dt("2024-06-01"),
        event_type: EventType::Disposal,
        tag: Tag::Trade,
        asset: "BTC".to_string(),
        asset_class: AssetClass::Crypto,
        quantity: dec!(1),
        value_gbp: dec!(25000),
        fee_gbp: None,
        description: None,
    }];

    let cgt_report = calculate_cgt(events.clone()).unwrap();
    let data = build_report_data(&events, &cgt_report, &no_filter()).unwrap();

    assert!(data.warnings.iter().any(|w| matches!(
        w.warning,
        Warning::InsufficientCostBasis { .. }
    ) && w.source_transaction_ids
        == vec!["tx-1".to_string()]
        && w.related_event_ids == vec![1]));
}

#[test]
fn summary_includes_dividend_and_interest_totals() {
    let events = vec![
        TaxableEvent {
            id: 1,
            source_transaction_id: "tx-1".to_string(),
            datetime: dt("2024-06-01"),
            event_type: EventType::Acquisition,
            tag: Tag::Salary,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(0.1),
            value_gbp: dec!(1000),
            fee_gbp: None,
            description: None,
        },
        TaxableEvent {
            id: 2,
            source_transaction_id: "tx-2".to_string(),
            datetime: dt("2024-06-02"),
            event_type: EventType::Acquisition,
            tag: Tag::Dividend,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(0.02),
            value_gbp: dec!(200),
            fee_gbp: None,
            description: None,
        },
        TaxableEvent {
            id: 3,
            source_transaction_id: "tx-3".to_string(),
            datetime: dt("2024-06-03"),
            event_type: EventType::Acquisition,
            tag: Tag::Interest,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(0.03),
            value_gbp: dec!(300),
            fee_gbp: None,
            description: None,
        },
    ];

    let cgt_report = calculate_cgt(events.clone()).unwrap();
    let data = build_report_data(&events, &cgt_report, &no_filter()).unwrap();

    assert_eq!(data.summary.total_income, "1500.00");
    assert_eq!(data.summary.total_dividend_income, "200.00");
    assert_eq!(data.summary.total_interest_income, "300.00");
}
