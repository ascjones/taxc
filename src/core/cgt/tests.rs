use super::*;
use crate::core::events::builders::{
    acq, acq_with_fee, demerger, disp, disp_with_fee, event, rights_issue, small_distribution,
    staking,
};
use crate::core::events::AdjustmentKind;
use rust_decimal_macros::dec;

/// Final pool state for an asset, derived from the last pool-history entry.
fn final_pool(report: &CgtReport, asset: &str) -> (Decimal, Decimal) {
    report
        .pool_history
        .entries
        .iter()
        .rev()
        .find(|e| e.asset == asset)
        .map(|e| (e.quantity, e.cost_gbp))
        .unwrap_or((Decimal::ZERO, Decimal::ZERO))
}

/// Pool state immediately after a specific disposal. Tests using this helper
/// must not have multiple disposals of the same asset on the same day.
fn pool_state_after(report: &CgtReport, disposal: &DisposalRecord) -> (Decimal, Decimal) {
    let entry = report
        .pool_history
        .entries
        .iter()
        .find(|e| {
            e.date == disposal.date
                && e.asset == disposal.asset
                && e.event_type == EventType::Disposal
        })
        .expect("pool_history entry for disposal");
    (entry.quantity, entry.cost_gbp)
}

#[test]
fn pool_basic_operations() {
    let mut pool = Pool::new("BTC".to_string());
    pool.add(dec!(10), dec!(1000));
    assert_eq!(pool.quantity, dec!(10));
    assert_eq!(pool.cost_gbp, dec!(1000));

    let cost = pool.remove(dec!(5));
    assert_eq!(cost, dec!(500));
    assert_eq!(pool.quantity, dec!(5));
    assert_eq!(pool.cost_gbp, dec!(500));
}

#[test]
fn pool_remove_all() {
    let mut pool = Pool::new("BTC".to_string());
    pool.add(dec!(10), dec!(1000));

    let cost = pool.remove(dec!(15)); // More than available
    assert_eq!(cost, dec!(1000));
    assert_eq!(pool.quantity, Decimal::ZERO);
    assert_eq!(pool.cost_gbp, Decimal::ZERO);
}

#[test]
fn hmrc_pooling_example() {
    // HMRC example: https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
    // Buy 100 BTC for £1,000 in 2016
    // Buy 50 BTC for £125,000 in 2017
    // Sell 50 BTC for £300,000 in 2018
    let events = vec![
        acq("2016-01-01", "BTC", dec!(100), dec!(1000)),
        acq("2017-01-01", "BTC", dec!(50), dec!(125000)),
        disp("2018-01-01", "BTC", dec!(50), dec!(300000)),
    ];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 1);
    let disposal = &report.disposals[0];

    // Pool: 150 BTC, cost £126,000
    // Selling 50 BTC = 50/150 * £126,000 = £42,000 allowable cost
    assert_eq!(disposal.proceeds_gbp, dec!(300000));
    assert_eq!(disposal.allowable_cost_gbp, dec!(42000));
    assert_eq!(disposal.gain_gbp, dec!(258000));
}

#[test]
fn hmrc_pooling_example_out_of_order() {
    // Same as above but events in wrong order - should still work
    let events = vec![
        disp("2018-01-01", "BTC", dec!(50), dec!(300000)),
        acq("2017-01-01", "BTC", dec!(50), dec!(125000)),
        acq("2016-01-01", "BTC", dec!(100), dec!(1000)),
    ];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 1);
    let disposal = &report.disposals[0];

    assert_eq!(disposal.proceeds_gbp, dec!(300000));
    assert_eq!(disposal.allowable_cost_gbp, dec!(42000));
    assert_eq!(disposal.gain_gbp, dec!(258000));
}

#[test]
fn hmrc_bnb_example_1() {
    // HMRC example 1: https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
    // Section 104 holding 1,000 shares, disposal of all 1,000,
    // then buy 1,000 within 30 days.
    let events = vec![
        acq("2011-01-01", "X", dec!(1000), dec!(10000)),
        disp("2011-07-01", "X", dec!(1000), dec!(15000)),
        acq("2011-07-31", "X", dec!(1000), dec!(12000)),
    ];

    let report = calculate_cgt(events);
    let disposal = &report.disposals[0];

    assert_eq!(disposal.matching_components.len(), 1);
    assert_eq!(
        disposal.matching_components[0].rule,
        MatchingRule::BedAndBreakfast
    );
    assert_eq!(disposal.matching_components[0].quantity, dec!(1000));
    assert_eq!(
        disposal.matching_components[0].matched_date,
        Some(chrono::NaiveDate::from_ymd_opt(2011, 7, 31).unwrap())
    );
    assert_eq!(disposal.allowable_cost_gbp, dec!(12000));

    // Pool should remain as the original holding (B&B acquisition not added).
    let (qty, cost) = final_pool(&report, "X");
    assert_eq!(qty, dec!(1000));
    assert_eq!(cost, dec!(10000));
}

#[test]
fn hmrc_bnb_example_2() {
    // HMRC example 2: https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
    // Section 104 holding 2,500 shares. Dispose 1,700, then buy 500 within 30 days.
    let events = vec![
        acq("2012-01-01", "Y", dec!(2500), dec!(2500)),
        disp("2012-03-27", "Y", dec!(1700), dec!(1700)),
        acq("2012-03-30", "Y", dec!(500), dec!(1000)),
    ];

    let report = calculate_cgt(events);
    let disposal = &report.disposals[0];

    assert_eq!(disposal.matching_components.len(), 2);
    let bnb = disposal
        .matching_components
        .iter()
        .find(|c| c.rule == MatchingRule::BedAndBreakfast)
        .expect("expected B&B match");
    assert_eq!(bnb.quantity, dec!(500));
    assert_eq!(
        bnb.matched_date,
        Some(chrono::NaiveDate::from_ymd_opt(2012, 3, 30).unwrap())
    );
    assert!(disposal
        .matching_components
        .iter()
        .any(|c| c.rule == MatchingRule::Pool && c.quantity == dec!(1200)));
    assert_eq!(disposal.allowable_cost_gbp, dec!(2200));

    let (qty, cost) = final_pool(&report, "Y");
    assert_eq!(qty, dec!(1300));
    assert_eq!(cost, dec!(1300));
}

#[test]
fn hmrc_bnb_example_3() {
    // HMRC example 3: https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51560
    // Disposal on 28 Feb, acquisition on 31 Mar (outside 30 days).
    let events = vec![
        acq("2008-01-01", "Z", dec!(10000), dec!(10000)),
        disp("2009-02-28", "Z", dec!(2000), dec!(2000)),
        acq("2009-03-31", "Z", dec!(3000), dec!(6000)),
    ];

    let report = calculate_cgt(events);
    let disposal = &report.disposals[0];

    assert_eq!(disposal.matching_components.len(), 1);
    assert_eq!(disposal.matching_components[0].rule, MatchingRule::Pool);
    assert_eq!(disposal.matching_components[0].quantity, dec!(2000));
    assert_eq!(disposal.matching_components[0].matched_date, None);
    assert_eq!(disposal.allowable_cost_gbp, dec!(2000));

    // Pool after later acquisition should include remaining + new shares.
    let (qty, cost) = final_pool(&report, "Z");
    assert_eq!(qty, dec!(11000));
    assert_eq!(cost, dec!(14000));
}

#[test]
fn same_day_rule() {
    // Buy and sell on same day - should match same-day acquisition
    let events = vec![
        acq("2024-01-15", "BTC", dec!(1), dec!(40000)),
        disp("2024-01-15", "BTC", dec!(1), dec!(45000)),
    ];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 1);
    let disposal = &report.disposals[0];

    // Should use same-day cost of £40,000
    assert_eq!(disposal.allowable_cost_gbp, dec!(40000));
    assert_eq!(disposal.gain_gbp, dec!(5000));

    // Should be pure same-day match
    assert_eq!(disposal.matching_components.len(), 1);
    assert_eq!(disposal.matching_components[0].rule, MatchingRule::SameDay);
}

#[test]
fn same_day_rule_partial() {
    // Buy 2 BTC, sell 1 BTC on same day
    let events = vec![
        acq("2024-01-15", "BTC", dec!(2), dec!(80000)),
        disp("2024-01-15", "BTC", dec!(1), dec!(45000)),
    ];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 1);
    let disposal = &report.disposals[0];

    // Should use proportional same-day cost: 1/2 * £80,000 = £40,000
    assert_eq!(disposal.allowable_cost_gbp, dec!(40000));
    assert_eq!(disposal.gain_gbp, dec!(5000));

    // Pool should have remaining 1 BTC at £40,000
    let (qty, cost) = final_pool(&report, "BTC");
    assert_eq!(qty, dec!(1));
    assert_eq!(cost, dec!(40000));
}

#[test]
fn one_disposal_matches_same_day_then_bed_and_breakfast() {
    // Same-day rule should apply before B&B rule
    let events = vec![
        acq("2024-06-15", "BTC", dec!(3), dec!(45000)), // Same day
        disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
        acq("2024-06-20", "BTC", dec!(5), dec!(60000)), // B&B
    ];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 1);
    let disposal = &report.disposals[0];

    // 3 BTC from same-day at £45,000, then 2 BTC from B&B at 2/5 of £60,000.
    let components: Vec<(MatchingRule, Decimal, Decimal)> = disposal
        .matching_components
        .iter()
        .map(|c| (c.rule, c.quantity, c.cost))
        .collect();
    assert_eq!(
        components,
        vec![
            (MatchingRule::SameDay, dec!(3), dec!(45000)),
            (MatchingRule::BedAndBreakfast, dec!(2), dec!(24000)),
        ]
    );
    assert_eq!(disposal.allowable_cost_gbp, dec!(69000));
    assert_eq!(disposal.gain_gbp, dec!(6000));
}

#[test]
fn multiple_assets_separate_pools() {
    let events = vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        acq("2024-01-01", "ETH", dec!(100), dec!(50000)),
        disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
        disp("2024-06-15", "ETH", dec!(50), dec!(30000)),
    ];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 2);

    // BTC disposal
    let btc_disposal = report.disposals.iter().find(|d| d.asset == "BTC").unwrap();
    assert_eq!(btc_disposal.allowable_cost_gbp, dec!(50000));
    assert_eq!(btc_disposal.gain_gbp, dec!(25000));

    // ETH disposal
    let eth_disposal = report.disposals.iter().find(|d| d.asset == "ETH").unwrap();
    assert_eq!(eth_disposal.allowable_cost_gbp, dec!(25000));
    assert_eq!(eth_disposal.gain_gbp, dec!(5000));
}

#[test]
fn disposal_with_fees() {
    let events = vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        disp_with_fee("2024-06-15", "BTC", dec!(5), dec!(75000), dec!(100)),
    ];

    let report = calculate_cgt(events);

    let disposal = &report.disposals[0];
    assert_eq!(disposal.fees_gbp, dec!(100));
    // Gain = proceeds - allowable cost - fees = 75000 - 50000 - 100 = 24900
    assert_eq!(disposal.gain_gbp, dec!(24900));
}

#[test]
fn acquisition_fees_added_to_pool() {
    let events = vec![
        acq_with_fee("2024-01-01", "BTC", dec!(10), dec!(100000), dec!(500)),
        disp("2024-06-15", "BTC", dec!(10), dec!(150000)),
    ];

    let report = calculate_cgt(events);

    let disposal = &report.disposals[0];
    // Allowable cost should include the £500 fee
    assert_eq!(disposal.allowable_cost_gbp, dec!(100500));
    assert_eq!(disposal.gain_gbp, dec!(49500));
}

#[test]
fn disposal_below_cost_produces_capital_loss() {
    // Selling below allowable cost must yield a negative gain (capital loss).
    let events = vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        disp("2024-06-15", "BTC", dec!(10), dec!(60000)),
    ];

    let report = calculate_cgt(events);

    let disposal = &report.disposals[0];
    assert_eq!(disposal.allowable_cost_gbp, dec!(100000));
    assert_eq!(disposal.proceeds_gbp, dec!(60000));
    assert_eq!(disposal.gain_gbp, dec!(-40000));
}

#[test]
fn disposal_with_fees_can_tip_gain_into_loss() {
    // A marginal gain can become a loss once disposal fees are deducted.
    let events = vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        disp_with_fee("2024-06-15", "BTC", dec!(10), dec!(100050), dec!(100)),
    ];

    let report = calculate_cgt(events);

    let disposal = &report.disposals[0];
    // proceeds - cost - fees = 100050 - 100000 - 100 = -50
    assert_eq!(disposal.gain_gbp, dec!(-50));
}

// Tests for new detailed reporting functionality

#[test]
fn pool_snapshot_accuracy_after_disposal() {
    // Test that pool_after accurately reflects pool state after each disposal
    let events = vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        disp("2024-06-15", "BTC", dec!(3), dec!(45000)),
        disp("2024-07-15", "BTC", dec!(2), dec!(30000)),
    ];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 2);

    // After first disposal: 10 - 3 = 7 BTC remaining
    let (qty, cost) = pool_state_after(&report, &report.disposals[0]);
    assert_eq!(qty, dec!(7));
    // Cost: 100000 * (7/10) = 70000
    assert_eq!(cost, dec!(70000));

    // After second disposal: 7 - 2 = 5 BTC remaining
    let (qty, cost) = pool_state_after(&report, &report.disposals[1]);
    assert_eq!(qty, dec!(5));
    // Cost: 70000 * (5/7) = 50000
    assert_eq!(cost, dec!(50000));
}

#[test]
fn matching_components_same_day_and_pool() {
    // Test same-day + pool matching creates correct components
    let events = vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        acq("2024-06-15", "BTC", dec!(2), dec!(30000)), // Same-day
        disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
    ];

    let report = calculate_cgt(events);
    let disposal = &report.disposals[0];

    // Should have 2 components: same-day (2) + pool (3)
    assert_eq!(disposal.matching_components.len(), 2);

    // Find same-day component
    let same_day = disposal
        .matching_components
        .iter()
        .find(|c| c.rule == MatchingRule::SameDay)
        .unwrap();
    assert_eq!(same_day.quantity, dec!(2));
    assert_eq!(same_day.cost, dec!(30000));
    assert_eq!(same_day.matched_date, Some(disposal.date));

    // Find pool component
    let pool = disposal
        .matching_components
        .iter()
        .find(|c| c.rule == MatchingRule::Pool)
        .unwrap();
    assert_eq!(pool.quantity, dec!(3));
    // Pool cost: 3/10 * 100000 = 30000
    assert_eq!(pool.cost, dec!(30000));
    assert!(pool.matched_date.is_none());
}

#[test]
fn staking_rewards_matched_same_day() {
    // Staking rewards are acquisitions at FMV and should be matchable
    // via same-day rule when there's a disposal on the same day
    let events = vec![
        staking("2024-03-08", "DOT", dec!(100), dec!(800)), // Staking reward
        disp("2024-03-08", "DOT", dec!(10), dec!(85)),      // Fee disposal same day
    ];

    let report = calculate_cgt(events);
    assert_eq!(report.disposals.len(), 1);

    let disposal = &report.disposals[0];

    // Should have same-day matching component
    assert!(
        !disposal.matching_components.is_empty(),
        "Expected matching components but got none"
    );

    let same_day = disposal
        .matching_components
        .iter()
        .find(|c| c.rule == MatchingRule::SameDay);
    assert!(
        same_day.is_some(),
        "Expected Same-Day matching but got: {:?}",
        disposal.matching_components
    );

    let same_day = same_day.unwrap();
    assert_eq!(same_day.quantity, dec!(10));
    // Cost should be proportional: 10/100 * 800 = 80
    assert_eq!(same_day.cost, dec!(80));
}

#[test]
fn staking_rewards_matched_bnb() {
    // Staking rewards should also be matchable via B&B rule
    let events = vec![
        disp("2024-03-08", "DOT", dec!(10), dec!(85)), // Disposal
        staking("2024-03-15", "DOT", dec!(100), dec!(800)), // Staking reward within 30 days
    ];

    let report = calculate_cgt(events);
    assert_eq!(report.disposals.len(), 1);

    let disposal = &report.disposals[0];

    let bnb = disposal
        .matching_components
        .iter()
        .find(|c| c.rule == MatchingRule::BedAndBreakfast);
    assert!(
        bnb.is_some(),
        "Expected B&B matching but got: {:?}",
        disposal.matching_components
    );

    let bnb = bnb.unwrap();
    assert_eq!(bnb.quantity, dec!(10));
    // Cost should be proportional: 10/100 * 800 = 80
    assert_eq!(bnb.cost, dec!(80));
    assert_eq!(
        bnb.matched_date,
        Some(NaiveDate::parse_from_str("2024-03-15", "%Y-%m-%d").unwrap())
    );
}

#[test]
fn same_day_has_priority_over_bnb() {
    // Scenario: Same-day rule should have priority over B&B
    // - April 8: Disposal of 100 BTC (will try to B&B with April 11 acquisition)
    // - April 11: Acquisition of 80 BTC at £40000
    // - April 11: Disposal of 50 BTC at £30000
    //
    // Expected: April 11 disposal should get same-day match FIRST (50 BTC at £25000 cost)
    // Then April 8 disposal can B&B with remaining 30 BTC from April 11
    //
    // Bug: Without the fix, April 8 disposal consumes all 80 BTC via B&B,
    // leaving nothing for April 11's same-day match.

    // Need some initial pool for the April 8 disposal that can't fully B&B match
    let events = vec![
        // Initial acquisition to seed the pool
        acq("2024-01-01", "BTC", dec!(100), dec!(50000)), // 100 BTC at £500 each
        // April 8: Disposal - should use B&B with leftover from April 11, plus pool
        disp("2024-04-08", "BTC", dec!(100), dec!(60000)), // Sell 100 BTC at £600 each
        // April 11: Acquisition - should be reserved for same-day first
        acq("2024-04-11", "BTC", dec!(80), dec!(40000)), // 80 BTC at £500 each
        // April 11: Disposal - MUST get same-day match with April 11 acquisition
        disp("2024-04-11", "BTC", dec!(50), dec!(30000)), // Sell 50 BTC at £600 each
    ];

    let report = calculate_cgt(events);
    assert_eq!(report.disposals.len(), 2);

    // Find the April 11 disposal
    let apr11_disposal = report
        .disposals
        .iter()
        .find(|d| d.date == NaiveDate::from_ymd_opt(2024, 4, 11).unwrap())
        .unwrap();

    // The April 11 disposal should use same-day matching
    // 50 BTC at £500 each = £25000 cost
    assert_eq!(
        apr11_disposal.allowable_cost_gbp,
        dec!(25000),
        "April 11 disposal should use same-day matching at £500/BTC"
    );

    // Check matching components - should be Same-Day
    assert!(
        apr11_disposal
            .matching_components
            .iter()
            .any(|mc| mc.rule == MatchingRule::SameDay),
        "April 11 disposal should have Same-Day matching component"
    );

    // Find the April 8 disposal
    let apr8_disposal = report
        .disposals
        .iter()
        .find(|d| d.date == NaiveDate::from_ymd_opt(2024, 4, 8).unwrap())
        .unwrap();

    // April 8 disposal (100 BTC) should:
    // - B&B match with remaining 30 BTC from April 11 (80 - 50 used for same-day) at £500 each = £15000
    // - Pool match with 70 BTC from Jan 1 at £500 each = £35000
    // Total cost: £50000
    assert_eq!(
        apr8_disposal.allowable_cost_gbp,
        dec!(50000),
        "April 8 disposal should use B&B (30 BTC) + Pool (70 BTC)"
    );

    // Check that April 8 has B&B component
    assert!(
        apr8_disposal
            .matching_components
            .iter()
            .any(|mc| mc.rule == MatchingRule::BedAndBreakfast),
        "April 8 disposal should have B&B matching component"
    );
}

// Tests for disposal warnings

#[test]
fn warning_no_cost_basis() {
    // Disposal with no prior acquisitions should have InsufficientCostBasis warning with available=0
    let events = vec![disp("2024-06-15", "BTC", dec!(5), dec!(75000))];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 1);
    let disposal = &report.disposals[0];

    // Should have zero allowable cost
    assert_eq!(disposal.allowable_cost_gbp, dec!(0));

    // Should have InsufficientCostBasis warning with available=0
    let warning = disposal
        .warnings
        .iter()
        .find(|w| matches!(w, Warning::InsufficientCostBasis { .. }));
    assert!(
        warning.is_some(),
        "Expected InsufficientCostBasis warning, got: {:?}",
        disposal.warnings
    );

    if let Some(Warning::InsufficientCostBasis {
        available,
        required,
    }) = warning
    {
        assert_eq!(*available, dec!(0));
        assert_eq!(*required, dec!(5));
    }
}

#[test]
fn warning_insufficient_pool() {
    // Selling more than the pool holds uses the whole pool's cost and warns.
    let events = vec![
        acq("2024-01-01", "BTC", dec!(5), dec!(50000)),
        disp("2024-06-15", "BTC", dec!(10), dec!(150000)),
    ];

    let report = calculate_cgt(events);
    let disposal = &report.disposals[0];

    assert_eq!(disposal.allowable_cost_gbp, dec!(50000));
    assert_eq!(disposal.gain_gbp, dec!(100000));
    assert_eq!(
        disposal.warnings,
        vec![Warning::InsufficientCostBasis {
            available: dec!(5),
            required: dec!(10),
        }]
    );
}

#[test]
fn warning_unclassified_out() {
    // Unclassified event should have Unclassified warning
    let events = vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        event(
            EventType::Disposal,
            Tag::Unclassified,
            "2024-06-15",
            "BTC",
            dec!(5),
            dec!(75000),
            None,
        ),
    ];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 1);
    let disposal = &report.disposals[0];

    // Should have Unclassified warning
    assert!(
        disposal.warnings.contains(&Warning::UnclassifiedEvent),
        "Expected Unclassified warning, got: {:?}",
        disposal.warnings
    );

    // Should also be detected by is_unclassified helper
    assert!(disposal.is_unclassified());
}

#[test]
fn no_warning_for_normal_disposal() {
    // Normal disposal with sufficient pool should have no warnings
    let events = vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
    ];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 1);
    let disposal = &report.disposals[0];

    // Should have no warnings
    assert!(
        disposal.warnings.is_empty(),
        "Expected no warnings, got: {:?}",
        disposal.warnings
    );
}

#[test]
fn year_end_snapshots_omit_zero_balance() {
    let events = vec![
        acq("2024-01-15", "BTC", dec!(5), dec!(50000)),
        disp("2024-06-15", "BTC", dec!(5), dec!(75000)), // Dispose all
    ];
    let report = calculate_cgt(events);

    // Final snapshot should have no pools (BTC is zero)
    let final_snapshot = report.pool_history.year_end_snapshots.last().unwrap();
    assert!(
        final_snapshot.pools.is_empty(),
        "Expected no pools after disposing all, got: {:?}",
        final_snapshot.pools
    );
}

#[test]
fn pool_history_multiple_assets() {
    let events = vec![
        acq("2024-01-15", "BTC", dec!(10), dec!(100000)),
        acq("2024-01-20", "ETH", dec!(50), dec!(25000)),
        disp("2024-06-15", "BTC", dec!(3), dec!(45000)),
    ];
    let report = calculate_cgt(events);

    // Should have 3 entries (2 acquisitions + 1 disposal)
    assert_eq!(report.pool_history.entries.len(), 3);

    // Final snapshot should have both assets
    let final_snapshot = report.pool_history.year_end_snapshots.last().unwrap();
    assert_eq!(final_snapshot.pools.len(), 2);

    // Verify BTC state after disposal (10 - 3 = 7)
    let btc_pool = final_snapshot
        .pools
        .iter()
        .find(|p| p.asset == "BTC")
        .unwrap();
    assert_eq!(btc_pool.quantity, dec!(7));

    // Verify ETH state unchanged
    let eth_pool = final_snapshot
        .pools
        .iter()
        .find(|p| p.asset == "ETH")
        .unwrap();
    assert_eq!(eth_pool.quantity, dec!(50));
}

#[test]
fn year_end_snapshots_at_boundaries() {
    let events = vec![
        acq("2024-01-15", "BTC", dec!(10), dec!(100000)), // 2023/24
        disp("2024-04-10", "BTC", dec!(3), dec!(45000)),  // 2024/25
    ];
    let report = calculate_cgt(events);

    assert_eq!(report.pool_history.year_end_snapshots.len(), 2);

    let snapshot_2324 = &report.pool_history.year_end_snapshots[0];
    assert_eq!(snapshot_2324.tax_year, TaxYear(2024));
    assert_eq!(snapshot_2324.pools.len(), 1);
    assert_eq!(snapshot_2324.pools[0].quantity, dec!(10));

    let snapshot_2425 = &report.pool_history.year_end_snapshots[1];
    assert_eq!(snapshot_2425.tax_year, TaxYear(2025));
    assert_eq!(snapshot_2425.pools[0].quantity, dec!(7));
}

// Edge case tests for pool history

#[test]
fn pool_history_empty_events() {
    let events: Vec<TaxableEvent> = vec![];
    let report = calculate_cgt(events);

    assert!(report.pool_history.entries.is_empty());
    assert!(report.pool_history.year_end_snapshots.is_empty());
    assert!(report.disposals.is_empty());
}

#[test]
fn pool_history_single_tax_year() {
    // All events in same tax year (2024/25: April 6, 2024 - April 5, 2025)
    let events = vec![
        acq("2024-04-10", "BTC", dec!(10), dec!(100000)),
        acq("2024-06-15", "BTC", dec!(5), dec!(60000)),
        disp("2024-12-01", "BTC", dec!(3), dec!(45000)),
    ];
    let report = calculate_cgt(events);

    // Should have only 1 year-end snapshot
    assert_eq!(report.pool_history.year_end_snapshots.len(), 1);
    assert_eq!(
        report.pool_history.year_end_snapshots[0].tax_year,
        TaxYear(2025)
    );

    // Should have 3 daily entries
    assert_eq!(report.pool_history.entries.len(), 3);
}

#[test]
fn pool_history_old_events() {
    // Test events from before 2020
    let events = vec![
        acq("2017-01-15", "BTC", dec!(100), dec!(1000)), // Very old
        acq("2018-06-20", "BTC", dec!(50), dec!(200000)), // 2018/19
        disp("2019-01-10", "BTC", dec!(30), dec!(150000)), // 2018/19
        disp("2024-06-15", "BTC", dec!(50), dec!(500000)), // 2024/25
    ];
    let report = calculate_cgt(events);

    // One snapshot per tax year from 2016/17 to 2024/25, idle years included.
    let years: Vec<TaxYear> = report
        .pool_history
        .year_end_snapshots
        .iter()
        .map(|s| s.tax_year)
        .collect();
    assert_eq!(years, (2017..=2025).map(TaxYear).collect::<Vec<_>>());
}

#[test]
fn pool_history_staking_rewards_tracked() {
    // Staking rewards should appear in pool history
    let events = vec![
        staking("2024-01-15", "DOT", dec!(100), dec!(500)),
        staking("2024-02-15", "DOT", dec!(50), dec!(280)),
    ];
    let report = calculate_cgt(events);

    assert_eq!(report.pool_history.entries.len(), 2);
    assert_eq!(
        report.pool_history.entries[0].event_type,
        EventType::Acquisition
    );
    assert_eq!(report.pool_history.entries[0].tag, Tag::StakingReward);
    assert_eq!(report.pool_history.entries[1].quantity, dec!(150)); // Accumulated
}

/// HMRC s58 TCGA 1992: Transfer between spouses is "no gain no loss".
/// Person buys 1000 shares at £5 each, then transfers 500 to spouse.
/// The disposal should produce zero gain regardless of market value.
#[test]
fn no_gain_no_loss_spouse_transfer() {
    let events = vec![
        acq("2024-01-01", "XYZ", dec!(1000), dec!(5000)),
        // Transfer 500 shares to spouse at market value of £8,000 (£16/share)
        // but tagged NoGainNoLoss so gain must be zero
        event(
            EventType::Disposal,
            Tag::NoGainNoLoss,
            "2024-06-15",
            "XYZ",
            dec!(500),
            dec!(8000), // market value - irrelevant for gain
            None,
        ),
    ];

    let report = calculate_cgt(events);
    assert_eq!(report.disposals.len(), 1);

    let disposal = &report.disposals[0];
    // Gain must be zero
    assert_eq!(disposal.gain_gbp, dec!(0));
    // Allowable cost is proportional pool cost: 500/1000 * £5000 = £2500
    assert_eq!(disposal.allowable_cost_gbp, dec!(2500));
    // Proceeds are deemed to equal cost (not market value)
    assert_eq!(disposal.proceeds_gbp, dec!(2500));

    // Pool should retain the other 500 shares at £2500
    let (qty, cost) = final_pool(&report, "XYZ");
    assert_eq!(qty, dec!(500));
    assert_eq!(cost, dec!(2500));
}

/// HMRC example: No gain no loss disposal followed by a normal sale.
/// Verifies that the pool is correctly reduced after a NGNL transfer
/// and subsequent disposal uses the reduced pool.
#[test]
fn no_gain_no_loss_then_normal_sale() {
    let events = vec![
        // Buy 200 shares at £10 each = £2,000
        acq("2024-01-01", "ABC", dec!(200), dec!(2000)),
        // Transfer 100 shares to spouse (market value £1,500 = £15/share)
        event(
            EventType::Disposal,
            Tag::NoGainNoLoss,
            "2024-03-01",
            "ABC",
            dec!(100),
            dec!(1500),
            None,
        ),
        // Sell remaining 100 shares for £2,000 (£20/share)
        disp("2024-06-01", "ABC", dec!(100), dec!(2000)),
    ];

    let report = calculate_cgt(events);
    assert_eq!(report.disposals.len(), 2);

    // First disposal: no gain no loss
    let ngnl = &report.disposals[0];
    assert_eq!(ngnl.gain_gbp, dec!(0));
    assert_eq!(ngnl.allowable_cost_gbp, dec!(1000)); // 100/200 * £2000
    assert_eq!(ngnl.proceeds_gbp, dec!(1000));

    // Second disposal: normal sale
    // Pool after NGNL: 100 shares at £1000
    // Sell 100 shares for £2000, cost £1000, gain = £1000
    let sale = &report.disposals[1];
    assert_eq!(sale.proceeds_gbp, dec!(2000));
    assert_eq!(sale.allowable_cost_gbp, dec!(1000));
    assert_eq!(sale.gain_gbp, dec!(1000));

    // Pool should be empty
    let (qty, _cost) = final_pool(&report, "ABC");
    assert_eq!(qty, dec!(0));
}

/// HMRC s58: No gain no loss with same-day matching.
/// If a NGNL disposal matches a same-day acquisition, the cost from
/// the same-day rule is used and gain is still zero.
#[test]
fn no_gain_no_loss_with_same_day_acquisition() {
    let events = vec![
        // Existing pool: 100 shares at £1000
        acq("2024-01-01", "DEF", dec!(100), dec!(1000)),
        // NGNL disposal of 50 shares (market value £750)
        event(
            EventType::Disposal,
            Tag::NoGainNoLoss,
            "2024-06-15",
            "DEF",
            dec!(50),
            dec!(750),
            None,
        ),
        // Same-day acquisition of 30 shares at £600
        acq("2024-06-15", "DEF", dec!(30), dec!(600)),
    ];

    let report = calculate_cgt(events);
    assert_eq!(report.disposals.len(), 1);

    let disposal = &report.disposals[0];
    // 30 matched same-day at £600, the other 20 from the pool at £10 each.
    assert_eq!(disposal.allowable_cost_gbp, dec!(800));
    assert_eq!(disposal.proceeds_gbp, dec!(800));
    assert_eq!(disposal.gain_gbp, dec!(0));
}

// === CgtSummary: gain netting, AEA, and tax estimation ===

#[test]
fn cgt_summary_nets_losses_against_gains() {
    // gains +1000, -400, +200 → gross 1200, losses 400, net 800
    let summary = CgtSummary::calculate([dec!(1000), dec!(-400), dec!(200)], dec!(3000));
    assert_eq!(summary.gross_gains, dec!(1200));
    assert_eq!(summary.in_year_losses, dec!(400));
    assert_eq!(summary.net_gain_before_aea, dec!(800));
    // net (800) is below the AEA (3000) → nothing taxable
    assert_eq!(summary.taxable_gain, dec!(0));
}

#[test]
fn cgt_summary_subtracts_aea() {
    let summary = CgtSummary::calculate([dec!(10000)], dec!(3000));
    assert_eq!(summary.net_gain_before_aea, dec!(10000));
    assert_eq!(summary.taxable_gain, dec!(7000)); // 10000 - 3000 AEA
}

#[test]
fn cgt_summary_net_loss_clamps_taxable_and_tax_to_zero() {
    // A net loss must never produce negative taxable gain or negative tax.
    let summary = CgtSummary::calculate([dec!(1000), dec!(-5000)], dec!(3000));
    assert_eq!(summary.gross_gains, dec!(1000));
    assert_eq!(summary.in_year_losses, dec!(5000));
    assert_eq!(summary.net_gain_before_aea, dec!(-4000));
    assert_eq!(summary.taxable_gain, dec!(0));
}

// === Bug-probe tests: rounding drift, precision mismatch, B&B boundary ===

#[test]
fn pool_partial_removals_preserve_total_cost() {
    // After fully draining a pool with three equal partial disposals of a cost
    // that doesn't divide cleanly, the sum of returned costs must equal the
    // original total (no rounding drift leaked or invented).
    let mut pool = Pool::new("X".to_string());
    pool.add(dec!(3), dec!(100000));

    let c1 = pool.remove(dec!(1));
    let c2 = pool.remove(dec!(1));
    let c3 = pool.remove(dec!(1));

    assert_eq!(
        c1 + c2 + c3,
        dec!(100000),
        "sum of removed costs must equal original cost"
    );
    assert_eq!(pool.quantity, Decimal::ZERO);
    assert_eq!(pool.cost_gbp, Decimal::ZERO);
}

#[test]
fn pool_many_small_removals_do_not_drift() {
    // Repeatedly remove small portions. Pool's reported cost_gbp must remain
    // >= 0 and consistent with (original_cost - sum_of_removed_costs).
    let mut pool = Pool::new("X".to_string());
    pool.add(dec!(10), dec!(1));

    let mut removed_total = Decimal::ZERO;
    for _ in 0..9 {
        removed_total += pool.remove(dec!(1));
    }

    // With 2dp rounding on removal, each 1/10 slice rounds 0.10 exactly, so
    // after 9 removals the pool should hold 1 qty for the remaining cost.
    assert!(
        pool.cost_gbp >= Decimal::ZERO,
        "pool cost_gbp went negative: {}",
        pool.cost_gbp
    );
    assert_eq!(
        pool.cost_gbp + removed_total,
        dec!(1),
        "accounting invariant: removed + remaining == original"
    );
}

#[test]
fn pool_remove_never_goes_negative_on_awkward_split() {
    // Construct a case where proportional rounding could exceed remaining
    // cost. Pool cost of 0.01 with many units - removing one unit at a time
    // must not leave the pool with negative cost.
    let mut pool = Pool::new("X".to_string());
    pool.add(dec!(100), dec!(0.01));

    for _ in 0..50 {
        pool.remove(dec!(1));
        assert!(
            pool.cost_gbp >= Decimal::ZERO,
            "pool cost_gbp went negative: qty={}, cost={}",
            pool.quantity,
            pool.cost_gbp
        );
    }
}

#[test]
fn bnb_matches_across_tax_year_boundary() {
    // The 30-day B&B window ignores tax-year boundaries: a disposal late in
    // 2023/24 matches an acquisition early in 2024/25 within 30 days, and the
    // year-end snapshots still split correctly at 5 April.
    let events = vec![
        acq("2023-06-01", "BTC", dec!(10), dec!(100000)),
        disp("2024-03-20", "BTC", dec!(5), dec!(75000)), // 2023/24
        acq("2024-04-10", "BTC", dec!(5), dec!(60000)),  // 2024/25, 21 days later
    ];

    let report = calculate_cgt(events);
    let disposal = &report.disposals[0];

    assert_eq!(disposal.matching_components.len(), 1);
    assert_eq!(
        disposal.matching_components[0].rule,
        MatchingRule::BedAndBreakfast,
        "acquisition within 30 days must match via B&B even across the tax-year boundary"
    );
    assert_eq!(disposal.allowable_cost_gbp, dec!(60000));

    // Snapshots split at the boundary; B&B acquisition is not added to the pool.
    let snapshots = &report.pool_history.year_end_snapshots;
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].tax_year, TaxYear(2024));
    assert_eq!(snapshots[0].pools[0].quantity, dec!(10));
    assert_eq!(snapshots[1].tax_year, TaxYear(2025));
    assert_eq!(snapshots[1].pools[0].quantity, dec!(10));
}

#[test]
fn insufficient_cost_basis_uses_residual_after_same_day_match() {
    // When a same-day match partially covers a disposal, the InsufficientCostBasis
    // warning must report the *post-matching* residual and the pool available to it,
    // not the original disposal quantity.
    let events = vec![
        acq("2024-01-01", "BTC", dec!(1), dec!(10000)), // pool: 1
        acq("2024-06-15", "BTC", dec!(2), dec!(30000)), // same-day: 2
        disp("2024-06-15", "BTC", dec!(5), dec!(75000)), // dispose 5
    ];

    let report = calculate_cgt(events);
    let disposal = &report.disposals[0];

    // 2 covered same-day, 1 from pool, leaving a 3-unit shortfall over a 1-unit pool.
    let warning = disposal
        .warnings
        .iter()
        .find_map(|w| match w {
            Warning::InsufficientCostBasis {
                available,
                required,
            } => Some((*available, *required)),
            _ => None,
        })
        .expect("expected InsufficientCostBasis warning");
    assert_eq!(warning, (dec!(1), dec!(3)));
}

#[test]
fn bnb_matches_exactly_30_days_after_disposal() {
    // HMRC CG51560: B&B covers acquisitions within 30 days after disposal.
    // Disposal 2024-06-15 + 30 days = 2024-07-15 must match.
    let events = vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        disp("2024-06-15", "BTC", dec!(5), dec!(75000)),
        acq("2024-07-15", "BTC", dec!(5), dec!(60000)), // exactly 30 days
    ];

    let report = calculate_cgt(events);
    let disposal = &report.disposals[0];

    assert_eq!(disposal.matching_components.len(), 1);
    assert_eq!(
        disposal.matching_components[0].rule,
        MatchingRule::BedAndBreakfast,
        "acquisition exactly 30 days after disposal should be matched via B&B"
    );
    assert_eq!(disposal.allowable_cost_gbp, dec!(60000));
}

#[test]
fn cashback_acquisition_establishes_cost_basis() {
    // Cashback is an ordinary acquisition: it pools at market value and that
    // cost basis is allowable on a later disposal.
    let events = vec![
        event(
            EventType::Acquisition,
            Tag::Cashback,
            "2024-01-10",
            "ETH",
            dec!(2),
            dec!(1000),
            None,
        ),
        disp("2024-06-01", "ETH", dec!(2), dec!(1500)),
    ];

    let report = calculate_cgt(events);
    assert_eq!(report.disposals.len(), 1);

    let disposal = &report.disposals[0];
    assert_eq!(disposal.allowable_cost_gbp, dec!(1000));
    assert_eq!(disposal.gain_gbp, dec!(500));

    let (qty, cost) = final_pool(&report, "ETH");
    assert_eq!(qty, Decimal::ZERO);
    assert_eq!(cost, Decimal::ZERO);
}

fn at(e: TaxableEvent, datetime: &str) -> TaxableEvent {
    TaxableEvent {
        datetime: chrono::DateTime::parse_from_rfc3339(datetime).unwrap(),
        ..e
    }
}

#[test]
fn same_day_rule_uses_uk_calendar_day_not_utc_day() {
    // Both instants fall on 2 June 2024 in the UK (BST), although the
    // disposal is still 1 June in UTC. HMRC's "same day" is the UK day.
    let events = vec![
        acq("2024-01-01", "BTC", dec!(2), dec!(20000)),
        at(
            disp("2024-06-01", "BTC", dec!(1), dec!(30000)),
            "2024-06-01T23:30:00Z",
        ),
        at(
            acq("2024-06-02", "BTC", dec!(1), dec!(28000)),
            "2024-06-02T00:10:00Z",
        ),
    ];
    let report = calculate_cgt(events);
    let components = &report.disposals[0].matching_components;
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].rule, MatchingRule::SameDay);
    assert_eq!(report.disposals[0].allowable_cost_gbp, dec!(28000));
}

#[test]
fn matching_is_the_same_whatever_offset_the_instants_are_written_in() {
    let utc = vec![
        acq("2024-01-01", "BTC", dec!(2), dec!(20000)),
        at(
            disp("2024-06-01", "BTC", dec!(1), dec!(30000)),
            "2024-06-01T23:30:00+00:00",
        ),
        at(
            acq("2024-06-02", "BTC", dec!(1), dec!(28000)),
            "2024-06-02T00:10:00+00:00",
        ),
    ];
    let bst = vec![
        acq("2024-01-01", "BTC", dec!(2), dec!(20000)),
        at(
            disp("2024-06-01", "BTC", dec!(1), dec!(30000)),
            "2024-06-02T00:30:00+01:00",
        ),
        at(
            acq("2024-06-02", "BTC", dec!(1), dec!(28000)),
            "2024-06-02T01:10:00+01:00",
        ),
    ];
    let a = calculate_cgt(utc);
    let b = calculate_cgt(bst);
    assert_eq!(a.disposals[0].date, b.disposals[0].date);
    assert_eq!(
        a.disposals[0].matching_components[0].rule,
        b.disposals[0].matching_components[0].rule
    );
    assert_eq!(a.disposals[0].gain_gbp, b.disposals[0].gain_gbp);
}

#[test]
fn sterling_acquisitions_do_not_create_a_pool() {
    // GBP salary/dividends are income events, not chargeable assets.
    let mut salary = acq("2024-06-01", "GBP", dec!(1000), dec!(1000));
    salary.tag = crate::core::Tag::Salary;
    let report = calculate_cgt(vec![salary, acq("2024-06-02", "BTC", dec!(1), dec!(500))]);
    assert!(report.pool_history.entries.iter().all(|e| e.asset != "GBP"));
    let last = report.pool_history.year_end_snapshots.last().unwrap();
    assert!(last.pools.iter().all(|p| p.asset != "GBP"));
}

#[test]
fn full_disposal_of_an_18_decimal_quantity_leaves_no_dust_or_warning() {
    let qty = dec!(0.123456789012345678);
    let report = calculate_cgt(vec![
        acq("2024-06-01", "ETH", qty, dec!(300)),
        disp("2024-08-01", "ETH", qty, dec!(400)),
    ]);
    let d = &report.disposals[0];
    assert!(d.warnings.is_empty(), "{:?}", d.warnings);
    assert_eq!(d.allowable_cost_gbp, dec!(300));
    assert_eq!(final_pool(&report, "ETH"), (Decimal::ZERO, Decimal::ZERO));
}

#[test]
fn several_same_day_acquisitions_pool_their_exact_total() {
    let report = calculate_cgt(vec![
        acq("2024-06-01", "ETH", dec!(0.1), dec!(100)),
        acq("2024-06-01", "ETH", dec!(0.2), dec!(200)),
        acq("2024-06-01", "ETH", dec!(0.4), dec!(400)),
        disp("2024-08-01", "ETH", dec!(0.7), dec!(1000)),
    ]);
    let d = &report.disposals[0];
    assert!(d.warnings.is_empty(), "{:?}", d.warnings);
    assert_eq!(d.allowable_cost_gbp, dec!(700));
    assert_eq!(final_pool(&report, "ETH"), (Decimal::ZERO, Decimal::ZERO));
}

#[test]
fn year_end_snapshots_cover_idle_tax_years() {
    let report = calculate_cgt(vec![
        acq("2021-06-01", "BTC", dec!(1), dec!(1000)),
        acq("2024-06-01", "BTC", dec!(1), dec!(1000)),
    ]);
    let years: Vec<TaxYear> = report
        .pool_history
        .year_end_snapshots
        .iter()
        .map(|s| s.tax_year)
        .collect();
    assert_eq!(
        years,
        vec![TaxYear(2022), TaxYear(2023), TaxYear(2024), TaxYear(2025)]
    );
    // An idle year carries the holding forward unchanged.
    assert_eq!(
        report.pool_history.year_end_snapshots[1].pools[0].quantity,
        dec!(1)
    );
}

fn components(d: &DisposalRecord) -> Vec<(MatchingRule, Decimal, Decimal)> {
    d.matching_components
        .iter()
        .map(|c| (c.rule, c.quantity, c.cost))
        .collect()
}

#[test]
fn one_disposal_matches_same_day_then_bnb_then_pool() {
    let report = calculate_cgt(vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)), // pool at £10,000 each
        acq("2024-06-15", "BTC", dec!(1), dec!(30000)),   // same day
        disp("2024-06-15", "BTC", dec!(5), dec!(200000)),
        acq("2024-06-20", "BTC", dec!(2), dec!(50000)), // B&B
    ]);
    assert_eq!(
        components(&report.disposals[0]),
        vec![
            (MatchingRule::SameDay, dec!(1), dec!(30000)),
            (MatchingRule::BedAndBreakfast, dec!(2), dec!(50000)),
            (MatchingRule::Pool, dec!(2), dec!(20000)),
        ]
    );
    assert_eq!(report.disposals[0].allowable_cost_gbp, dec!(100000));
    assert_eq!(final_pool(&report, "BTC"), (dec!(8), dec!(80000)));
}

#[test]
fn acquisition_fee_is_split_between_matched_and_pooled_parts() {
    // 4 BTC for £40,000 plus a £400 fee; 1 is matched same-day, 3 are pooled.
    let report = calculate_cgt(vec![
        acq_with_fee("2024-06-15", "BTC", dec!(4), dec!(40000), dec!(400)),
        disp("2024-06-15", "BTC", dec!(1), dec!(12000)),
    ]);
    assert_eq!(
        components(&report.disposals[0]),
        vec![(MatchingRule::SameDay, dec!(1), dec!(10100))]
    );
    assert_eq!(final_pool(&report, "BTC"), (dec!(3), dec!(30300)));
}

#[test]
fn bnb_matches_the_earliest_later_acquisition_first() {
    let report = calculate_cgt(vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        disp("2024-06-01", "BTC", dec!(3), dec!(60000)),
        acq("2024-06-10", "BTC", dec!(2), dec!(30000)),
        acq("2024-06-20", "BTC", dec!(5), dec!(100000)),
    ]);
    assert_eq!(
        components(&report.disposals[0]),
        vec![
            (MatchingRule::BedAndBreakfast, dec!(2), dec!(30000)),
            (MatchingRule::BedAndBreakfast, dec!(1), dec!(20000)),
        ]
    );
}

#[test]
fn earlier_disposal_wins_a_contested_bnb_acquisition() {
    let report = calculate_cgt(vec![
        acq("2024-01-01", "BTC", dec!(10), dec!(100000)),
        disp("2024-06-01", "BTC", dec!(2), dec!(40000)),
        disp("2024-06-05", "BTC", dec!(2), dec!(40000)),
        acq("2024-06-10", "BTC", dec!(3), dec!(45000)),
    ]);
    assert_eq!(
        components(&report.disposals[0]),
        vec![(MatchingRule::BedAndBreakfast, dec!(2), dec!(30000))]
    );
    assert_eq!(
        components(&report.disposals[1]),
        vec![
            (MatchingRule::BedAndBreakfast, dec!(1), dec!(15000)),
            (MatchingRule::Pool, dec!(1), dec!(10000)),
        ]
    );
}

#[test]
fn no_gain_no_loss_with_fee_has_proceeds_of_cost_plus_fee() {
    let mut ngnl = disp_with_fee("2024-06-15", "BTC", dec!(1), dec!(50000), dec!(20));
    ngnl.tag = Tag::NoGainNoLoss;
    let report = calculate_cgt(vec![acq("2024-01-01", "BTC", dec!(2), dec!(20000)), ngnl]);
    let d = &report.disposals[0];
    assert_eq!(d.allowable_cost_gbp, dec!(10000));
    assert_eq!(d.fees_gbp, dec!(20));
    assert_eq!(d.proceeds_gbp, dec!(10020));
    assert_eq!(d.gain_gbp, dec!(0));
}

// ---- Pool adjustments (share reorganisations) ----

/// Pool-history entries recorded for one asset by pool adjustments.
fn adjustment_entries<'a>(report: &'a CgtReport, asset: &str) -> Vec<&'a PoolHistoryEntry> {
    report
        .pool_history
        .entries
        .iter()
        .filter(|e| e.asset == asset && matches!(e.event_type, EventType::PoolAdjustment(_)))
        .collect()
}

#[test]
fn demerger_moves_cost_to_new_holding_sold_the_same_day() {
    // The ULVR -> MICC demerger, with the MICC sold on the demerger
    // date. The sale is listed first to prove adjustments apply before
    // same-day disposals.
    let events = vec![
        acq("2025-01-10", "ULVR", dec!(887), dec!(36995.24)),
        disp_with_fee("2025-12-17", "MICC", dec!(177), dec!(2160.65), dec!(3.98)),
        demerger("2025-12-17", "ULVR", dec!(0.051151), "MICC", dec!(177)),
    ];

    let report = calculate_cgt(events);

    // 0.051151 x 36,995.24 = 1,892.3445... -> 1,892.34 moved.
    assert_eq!(report.disposals.len(), 1, "no ULVR disposal");
    let micc = &report.disposals[0];
    assert_eq!(micc.asset, "MICC");
    assert_eq!(micc.allowable_cost_gbp, dec!(1892.34));
    assert_eq!(micc.gain_gbp, dec!(2160.65) - dec!(3.98) - dec!(1892.34));
    assert_eq!(micc.matching_components.len(), 1);
    assert_eq!(micc.matching_components[0].rule, MatchingRule::Pool);
    assert!(micc.warnings.is_empty(), "{:?}", micc.warnings);

    assert_eq!(
        final_pool(&report, "ULVR"),
        (dec!(887), dec!(36995.24) - dec!(1892.34))
    );
    assert_eq!(final_pool(&report, "MICC"), (dec!(0), dec!(0)));

    // The demerger is in both pools' history, labelled as a demerger.
    let demerger = EventType::PoolAdjustment(AdjustmentKind::Demerger);
    let ulvr = adjustment_entries(&report, "ULVR");
    assert_eq!(ulvr.len(), 1);
    assert_eq!(ulvr[0].event_type, demerger);
    assert_eq!(ulvr[0].cost_gbp, dec!(35102.90));
    let micc_entries = adjustment_entries(&report, "MICC");
    assert_eq!(micc_entries.len(), 1);
    assert_eq!(micc_entries[0].event_type, demerger);
    assert_eq!(
        (micc_entries[0].quantity, micc_entries[0].cost_gbp),
        (dec!(177), dec!(1892.34))
    );
}

#[test]
fn rights_issue_joins_the_pool_and_is_never_matched() {
    // A sale 10 days before a rights issue matches the pool, not the
    // rights shares.
    let events = vec![
        acq("2024-04-10", "CSN", dec!(3800), dec!(10665.00)),
        disp("2025-07-08", "CSN", dec!(1000), dec!(2500)),
        rights_issue("2025-07-18", "CSN", dec!(1473), dec!(2592.48)),
    ];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 1);
    let sale = &report.disposals[0];
    // 1,000 / 3,800 x 10,665.00 = 2,806.578... -> 2,806.58
    assert_eq!(sale.allowable_cost_gbp, dec!(2806.58));
    for component in &sale.matching_components {
        assert_eq!(component.rule, MatchingRule::Pool);
        assert_eq!(component.matched_date, None);
    }
    assert_eq!(
        final_pool(&report, "CSN"),
        (dec!(4273), dec!(10665.00) - dec!(2806.58) + dec!(2592.48))
    );
}

#[test]
fn rights_issue_on_a_sale_date_is_pooled_not_matched_same_day() {
    let events = vec![
        acq("2024-04-10", "CSN", dec!(100), dec!(100)),
        disp("2025-07-18", "CSN", dec!(50), dec!(500)),
        rights_issue("2025-07-18", "CSN", dec!(100), dec!(300)),
    ];

    let report = calculate_cgt(events);

    let sale = &report.disposals[0];
    assert_eq!(sale.matching_components.len(), 1);
    assert_eq!(sale.matching_components[0].rule, MatchingRule::Pool);
    // The pool holds 200 at £400 when the sale draws on it.
    assert_eq!(sale.allowable_cost_gbp, dec!(100));
}

#[test]
fn small_capital_distribution_reduces_pool_cost_without_a_disposal() {
    // The £21.64 ULVR consolidation cash.
    let events = vec![
        acq("2025-01-10", "ULVR", dec!(887), dec!(36995.24)),
        small_distribution("2025-12-17", "ULVR", dec!(21.64)),
    ];

    let report = calculate_cgt(events);

    assert!(report.disposals.is_empty(), "{:?}", report.disposals);
    assert_eq!(
        final_pool(&report, "ULVR"),
        (dec!(887), dec!(36995.24) - dec!(21.64))
    );
    assert!(report.adjustment_warnings.is_empty());
}

#[test]
fn small_capital_distribution_above_pool_cost_is_a_gain_under_s122_4() {
    // £50 against a pool costing £30 zeroes the cost and records a £20
    // gain on the distribution's own event.
    let events = vec![
        acq("2024-01-10", "X", dec!(10), dec!(30)),
        TaxableEvent {
            id: 7,
            ..small_distribution("2025-03-01", "X", dec!(50))
        },
    ];

    let report = calculate_cgt(events);

    assert_eq!(final_pool(&report, "X"), (dec!(10), dec!(0)));
    assert_eq!(report.disposals.len(), 1);
    let excess = &report.disposals[0];
    assert_eq!(excess.id, 7);
    assert_eq!(excess.asset, "X");
    assert_eq!(excess.quantity, dec!(0));
    assert_eq!(excess.proceeds_gbp, dec!(20));
    assert_eq!(excess.allowable_cost_gbp, dec!(0));
    assert_eq!(excess.fees_gbp, dec!(0));
    assert_eq!(excess.gain_gbp, dec!(20));
    assert!(excess.matching_components.is_empty());
    assert_eq!(
        excess.warnings,
        vec![Warning::CapitalDistributionExceedsCost]
    );
    assert!(!excess.is_unclassified());
}

#[test]
fn demerger_from_an_empty_pool_adds_new_holding_at_zero_cost_with_warning() {
    let event = TaxableEvent {
        id: 3,
        ..demerger("2025-12-17", "ULVR", dec!(0.05), "MICC", dec!(177))
    };

    let report = calculate_cgt(vec![event.clone()]);

    assert!(report.disposals.is_empty());
    assert_eq!(final_pool(&report, "MICC"), (dec!(177), dec!(0)));
    assert_eq!(
        report.warnings_for_adjustment(&event),
        [Warning::InsufficientCostBasis {
            available: dec!(0),
            required: dec!(0),
        }]
    );
}

#[test]
fn small_capital_distribution_with_no_pool_is_all_gain_with_both_warnings() {
    let event = TaxableEvent {
        id: 4,
        ..small_distribution("2025-12-17", "ULVR", dec!(21.64))
    };

    let report = calculate_cgt(vec![event.clone()]);

    assert_eq!(report.disposals.len(), 1);
    assert_eq!(report.disposals[0].gain_gbp, dec!(21.64));
    assert_eq!(
        report.disposals[0].warnings,
        [Warning::CapitalDistributionExceedsCost]
    );
    assert_eq!(
        report.warnings_for_adjustment(&event),
        [Warning::InsufficientCostBasis {
            available: dec!(0),
            required: dec!(0),
        }]
    );
}

#[test]
fn adjustments_at_the_same_instant_apply_rights_issue_first() {
    // Listed distribution-first. Applied that way, the £50 would exceed the
    // £10 cost and leave a gain; rights first, the pool absorbs it.
    let events = vec![
        acq("2024-01-10", "X", dec!(100), dec!(10)),
        small_distribution("2025-03-01", "X", dec!(50)),
        rights_issue("2025-03-01", "X", dec!(50), dec!(100)),
    ];

    let report = calculate_cgt(events);

    assert!(report.disposals.is_empty(), "{:?}", report.disposals);
    assert_eq!(final_pool(&report, "X"), (dec!(150), dec!(60)));
}

#[test]
fn adjustments_on_one_date_apply_in_time_order() {
    // The distribution comes first in the day, so it exceeds the £10 cost
    // before the rights issue adds to it.
    let events = vec![
        acq("2024-01-10", "X", dec!(100), dec!(10)),
        at(
            rights_issue("2025-03-01", "X", dec!(50), dec!(100)),
            "2025-03-01T15:00:00Z",
        ),
        at(
            small_distribution("2025-03-01", "X", dec!(50)),
            "2025-03-01T09:00:00Z",
        ),
    ];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 1);
    assert_eq!(report.disposals[0].gain_gbp, dec!(40));
    assert_eq!(final_pool(&report, "X"), (dec!(150), dec!(100)));
}

#[test]
fn demerger_new_holding_is_never_matched_to_an_earlier_disposal() {
    // A MICC sale 10 days before the demerger has nothing to match: the
    // demerged shares are not an acquisition for the 30-day rule.
    let events = vec![
        acq("2025-01-10", "ULVR", dec!(100), dec!(1000)),
        disp("2025-12-07", "MICC", dec!(10), dec!(100)),
        demerger("2025-12-17", "ULVR", dec!(0.1), "MICC", dec!(10)),
    ];

    let report = calculate_cgt(events);

    let sale = &report.disposals[0];
    assert!(sale
        .matching_components
        .iter()
        .all(|c| c.rule == MatchingRule::Pool));
    assert_eq!(sale.allowable_cost_gbp, dec!(0));
    assert_eq!(final_pool(&report, "MICC"), (dec!(10), dec!(100)));
}

#[test]
fn disposal_index_finds_the_excess_record_for_its_adjustment_only() {
    let distribution = TaxableEvent {
        id: 2,
        ..small_distribution("2025-03-01", "X", dec!(50))
    };
    let buy = TaxableEvent {
        id: 1,
        ..acq("2024-01-10", "X", dec!(10), dec!(30))
    };
    let report = calculate_cgt(vec![buy.clone(), distribution.clone()]);
    let index = DisposalIndex::new(&report);

    assert_eq!(
        index.find(&distribution).map(|d| d.gain_gbp),
        Some(dec!(20))
    );
    assert!(index.find(&buy).is_none());
}

#[test]
fn adjustment_and_disposal_on_one_uk_date_across_utc_midnight() {
    // The sale at 23:30 UTC on 1 June is 00:30 BST on 2 June: the rights
    // issue's UK date. By UTC date the sale would come a day earlier and
    // miss the rights shares; by UK date the rights issue applies first.
    let events = vec![
        acq("2024-01-10", "CSN", dec!(100), dec!(100)),
        at(
            disp("2024-06-01", "CSN", dec!(50), dec!(500)),
            "2024-06-01T23:30:00Z",
        ),
        at(
            rights_issue("2024-06-02", "CSN", dec!(100), dec!(300)),
            "2024-06-02T10:00:00+01:00",
        ),
    ];

    let report = calculate_cgt(events);

    // The pool holds 200 at £400 when the sale draws on it.
    assert_eq!(report.disposals[0].allowable_cost_gbp, dec!(100));
}

#[test]
fn demerger_applies_after_rights_issue_and_before_distribution_at_one_instant() {
    // Listed in reverse. The rights cost joins ULVR before the fraction is
    // taken, and the fractional cash on MICC reduces the moved cost rather
    // than finding an empty pool.
    let events = vec![
        acq("2025-01-10", "ULVR", dec!(100), dec!(1000)),
        small_distribution("2025-12-17", "MICC", dec!(5)),
        demerger("2025-12-17", "ULVR", dec!(0.1), "MICC", dec!(10)),
        rights_issue("2025-12-17", "ULVR", dec!(10), dec!(200)),
    ];

    let report = calculate_cgt(events);

    assert!(report.disposals.is_empty(), "{:?}", report.disposals);
    // 0.1 x (1,000 + 200) = 120 moved, less the £5 cash.
    assert_eq!(final_pool(&report, "MICC"), (dec!(10), dec!(115)));
    assert_eq!(final_pool(&report, "ULVR"), (dec!(110), dec!(1080)));
}

// --- HMRC worked examples for share reorganisations ---
//
// HMRC rounds its examples to whole pounds; taxc keeps pence, so each figure
// is within £1 of the published one.

#[test]
fn hmrc_rights_issue_example_cg51590_mr_browne() {
    // https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51590
    // (Example 2): a 1-for-5 rights issue taken up for £1,060 joins the pool.
    let events = vec![
        acq("2008-08-17", "X", dec!(10000), dec!(2500)),
        acq("2009-04-01", "X", dec!(10000), dec!(2600)),
        rights_issue("2009-10-08", "X", dec!(4000), dec!(1060)),
        disp("2012-12-10", "X", dec!(7500), dec!(3000)),
    ];

    let report = calculate_cgt(events);

    // Pool 24,000 shares, cost £6,160. 7,500 shares cost £1,925.
    assert_eq!(report.disposals.len(), 1);
    assert_eq!(report.disposals[0].allowable_cost_gbp, dec!(1925));
    assert_eq!(report.disposals[0].gain_gbp, dec!(1075));
    // HMRC prints £4,236, but £6,160 - £1,925 = £4,235.
    assert_eq!(final_pool(&report, "X"), (dec!(16500), dec!(4235)));
}

#[test]
fn hmrc_rights_issue_example_cg51590_peninsula_trust() {
    // https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg51590
    // (Example 4): two rights issues either side of a purchase.
    let events = vec![
        acq("1997-09-24", "X", dec!(15000), dec!(6750)),
        rights_issue("2001-01-30", "X", dec!(9000), dec!(3600)),
        acq("2004-06-14", "X", dec!(12000), dec!(13800)),
        rights_issue("2005-11-26", "X", dec!(9000), dec!(9450)),
        disp("2010-02-23", "X", dec!(20000), dec!(39000)),
    ];

    let report = calculate_cgt(events);

    // Pool 45,000 shares, cost £33,600. HMRC: cost £14,934, gain £24,066,
    // remaining cost £18,666.
    assert_eq!(report.disposals.len(), 1);
    assert_eq!(report.disposals[0].allowable_cost_gbp, dec!(14933.33));
    assert_eq!(report.disposals[0].gain_gbp, dec!(24066.67));
    assert_eq!(final_pool(&report, "X"), (dec!(25000), dec!(18666.67)));
}

#[test]
fn hmrc_demerger_example_cg52742() {
    // https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg52742
    // Pacific Exploration demerges to Resolution Holdings. By market value,
    // 12/37 of the £15,000 pool cost moves to the 3,000 new shares. Neither
    // company is quoted, so s.129 takes the values at the disposal date; for
    // quoted shares s.130 takes them on the first dealing day. Either way
    // the input states the resulting fraction.
    let events = vec![
        acq("2005-06-01", "PAC", dec!(5000), dec!(15000)),
        demerger("2009-09-01", "PAC", dec!(12) / dec!(37), "RES", dec!(3000)),
        disp("2011-04-01", "RES", dec!(2000), dec!(8000)),
    ];

    let report = calculate_cgt(events);

    // HMRC: £4,865 moved; 2,000 shares cost £3,243 for a £4,757 gain,
    // leaving 1,000 shares at £1,622.
    assert_eq!(report.disposals.len(), 1);
    assert_eq!(report.disposals[0].allowable_cost_gbp, dec!(3243.24));
    assert_eq!(report.disposals[0].gain_gbp, dec!(4756.76));
    assert_eq!(final_pool(&report, "RES"), (dec!(1000), dec!(1621.62)));
    assert_eq!(final_pool(&report, "PAC"), (dec!(5000), dec!(10135.14)));
}

#[test]
fn hmrc_small_capital_distribution_example_cg57844() {
    // https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg57844
    // A £5,000 distribution (4.5% of the holding's value) on 10,000 shares
    // that cost £45,000 reduces the pool cost. It is not a disposal. HMRC's
    // shareholder is a company and also reduces an indexed pool; indexation
    // does not apply to individuals after 5 April 2008, so only the
    // qualifying-expenditure pool is checked.
    let events = vec![
        acq("2011-03-01", "X", dec!(10000), dec!(45000)),
        small_distribution("2017-09-01", "X", dec!(5000)),
    ];

    let report = calculate_cgt(events);

    assert!(report.disposals.is_empty(), "{:?}", report.disposals);
    assert_eq!(final_pool(&report, "X"), (dec!(10000), dec!(40000)));
}

#[test]
fn hmrc_distribution_exceeding_cost_example_cg57847() {
    // https://www.gov.uk/hmrc-internal-manuals/capital-gains-manual/cg57847
    // A £10,000 small distribution on shares with £6,000 allowable cost,
    // under a s.122(4) election: the proceeds are reduced by the £6,000, so
    // £4,000 is chargeable and no cost remains. HMRC's example predates 1988
    // and goes on to rebase to 1982 values, which taxc does not model, so
    // the dates and share count here are illustrative.
    let events = vec![
        acq("2020-01-01", "Z", dec!(1000), dec!(6000)),
        small_distribution("2021-01-01", "Z", dec!(10000)),
    ];

    let report = calculate_cgt(events);

    assert_eq!(report.disposals.len(), 1);
    let excess = &report.disposals[0];
    assert_eq!(excess.gain_gbp, dec!(4000));
    assert_eq!(
        excess.warnings,
        vec![Warning::CapitalDistributionExceedsCost]
    );
    assert_eq!(final_pool(&report, "Z"), (dec!(1000), dec!(0)));
}
