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

    let cgt_report = calculate_cgt(events.clone()).unwrap();
    let data = build_report_data(&[], &events, &cgt_report, &no_filter()).unwrap();

    let event_types: Vec<String> = data.events.iter().map(|e| e.event_type.clone()).collect();
    assert!(event_types
        .iter()
        .any(|t| t == display_event_type(EventType::Acquisition, Tag::Gift)));
    assert!(event_types
        .iter()
        .any(|t| t == display_event_type(EventType::Disposal, Tag::Gift)));
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
    let data = build_report_data(&[], &events, &cgt_report, &no_filter()).unwrap();

    let disposal = data
        .events
        .iter()
        .find(|e| e.event_type == display_event_type(EventType::Disposal, Tag::Trade))
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
    let data = build_report_data(&[], &events, &cgt_report, &no_filter()).unwrap();

    let disposal = data
        .events
        .iter()
        .find(|e| e.event_type == display_event_type(EventType::Disposal, Tag::Trade))
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
    let data = build_report_data(&[], &events, &cgt_report, &no_filter()).unwrap();

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
    let data = build_report_data(&[], &events, &cgt_report, &no_filter()).unwrap();

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

    let cgt_report = calculate_cgt(events.clone()).unwrap();
    let data = build_report_data(&[], &events, &cgt_report, &no_filter()).unwrap();

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

    let cgt_report = calculate_cgt(events.clone()).unwrap();
    let data = build_report_data(&[], &events, &cgt_report, &no_filter()).unwrap();

    let ngnl = data
        .events
        .iter()
        .find(|e| e.tag == Tag::NoGainNoLoss)
        .expect("expected no gain/no loss event");

    assert_eq!(ngnl.value_gbp, "25000.00");
    assert_eq!(ngnl.value_gbp_note.as_deref(), Some(NGNL_VALUE_NOTE));
}
