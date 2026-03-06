use super::*;
use crate::cmd::filter::EventFilter;
use crate::core::events::builders::{acq, disp};
use crate::core::{Tag, TaxableEvent};
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
        ..disp("2024-06-01", "BTC", dec!(1), dec!(25000))
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

    let cgt_report = calculate_cgt(events.clone()).unwrap();
    let data = build_report_data(&events, &cgt_report, &no_filter()).unwrap();

    assert_eq!(data.summary.total_income, "1500.00");
    assert_eq!(data.summary.total_dividend_income, "200.00");
    assert_eq!(data.summary.total_interest_income, "300.00");
}
