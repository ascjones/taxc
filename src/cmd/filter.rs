//! Shared event filtering infrastructure for CLI commands.

use crate::core::fmt::iso_date;
use crate::core::{DisposalRecord, EventType, TaxYear, TaxableEvent};
use chrono::NaiveDate;
use clap::{Args, ValueEnum};

/// Event kind for CLI filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EventKind {
    /// Disposal events only.
    Disposal,
    /// Acquisition events only.
    Acquisition,
    /// Pool adjustments (demergers, rights issues, small capital
    /// distributions) only.
    Adjustment,
}

impl From<EventType> for EventKind {
    fn from(event_type: EventType) -> Self {
        match event_type {
            EventType::Acquisition => EventKind::Acquisition,
            EventType::Disposal => EventKind::Disposal,
            EventType::PoolAdjustment(_) => EventKind::Adjustment,
        }
    }
}

impl EventKind {
    pub fn matches(self, event_type: EventType) -> bool {
        self == EventKind::from(event_type)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Disposal => "disposal",
            EventKind::Acquisition => "acquisition",
            EventKind::Adjustment => "adjustment",
        }
    }
}

/// Shared CLI args for date/event-kind filtering.
#[derive(Args, Debug, Default)]
pub struct FilterArgs {
    /// Tax year alias. Expands to --from <year-start> and --to <year-end>.
    /// Cannot be combined with --from/--to.
    #[arg(short = 'y', long, help_heading = "Filtering")]
    pub year: Option<i32>,

    /// Filter events from this date inclusive (YYYY-MM-DD).
    /// Example: --from 2024-04-06
    #[arg(long, value_name = "DATE", help_heading = "Filtering")]
    pub from: Option<String>,

    /// Filter events to this date inclusive (YYYY-MM-DD).
    /// Example: --to 2025-04-05
    #[arg(long, value_name = "DATE", help_heading = "Filtering")]
    pub to: Option<String>,

    /// Filter by event kind (disposal or acquisition).
    #[arg(long, value_enum, help_heading = "Filtering")]
    pub event_kind: Option<EventKind>,
}

impl FilterArgs {
    /// Parse and validate CLI arguments, producing an EventFilter.
    pub fn build(&self, asset: Option<String>) -> anyhow::Result<EventFilter> {
        if self.year.is_some() && (self.from.is_some() || self.to.is_some()) {
            anyhow::bail!("--year cannot be combined with --from/--to");
        }

        let (from, to) = if let Some(year) = self.year {
            year_bounds(year)?
        } else {
            let from = self
                .from
                .as_deref()
                .map(|s| parse_date(s, "--from"))
                .transpose()?;
            let to = self
                .to
                .as_deref()
                .map(|s| parse_date(s, "--to"))
                .transpose()?;
            (from, to)
        };

        for date in from.iter().chain(to.iter()) {
            anyhow::ensure!(
                TaxYear::from_date(*date).try_bounds().is_some(),
                "date {} is out of range",
                iso_date(*date)
            );
        }

        if let (Some(f), Some(t)) = (from, to) {
            anyhow::ensure!(f <= t, "--from ({}) must be on or before --to ({})", f, t);
        }

        Ok(EventFilter {
            from,
            to,
            asset,
            event_kind: self.event_kind,
        })
    }
}

/// Resolved event filter used by CLI commands.
#[derive(Debug)]
pub struct EventFilter {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub asset: Option<String>,
    pub event_kind: Option<EventKind>,
}

impl EventFilter {
    pub fn matches_date(&self, date: NaiveDate) -> bool {
        self.from.is_none_or(|from| date >= from) && self.to.is_none_or(|to| date <= to)
    }

    /// Asset symbols are normalised to upper case on input, so the filter's
    /// symbol is compared the same way -- Unicode-aware, not ASCII-only.
    pub fn matches_asset(&self, asset: &str) -> bool {
        self.asset
            .as_ref()
            .is_none_or(|a| a.trim().to_uppercase() == asset.to_uppercase())
    }

    pub fn matches_event(&self, event: &TaxableEvent) -> bool {
        self.matches_date(event.date())
            && self.matches_asset(&event.asset)
            && self
                .event_kind
                .is_none_or(|kind| kind.matches(event.event_type))
    }

    pub fn matches_disposal(&self, disposal: &DisposalRecord) -> bool {
        // A disposal record only passes an event-kind filter for disposals.
        self.matches_date(disposal.date)
            && self.matches_asset(&disposal.asset)
            && self
                .event_kind
                .is_none_or(|kind| kind == EventKind::Disposal)
    }

    pub fn apply<'a>(&self, events: &'a [TaxableEvent]) -> Vec<&'a TaxableEvent> {
        events.iter().filter(|e| self.matches_event(e)).collect()
    }

    /// Human-readable label for the active date range, e.g. "All Years",
    /// "2024/25" (when the bounds exactly span a tax year), or an explicit
    /// date range.
    pub fn scope_label(&self) -> String {
        match (self.from, self.to) {
            (None, None) => "All Years".to_string(),
            (Some(from), Some(to)) => {
                let tax_year = TaxYear::from_date(from);
                if tax_year.try_bounds() == Some((from, to)) {
                    tax_year.display()
                } else {
                    format!("{} to {}", iso_date(from), iso_date(to))
                }
            }
            (Some(from), None) => format!("From {}", iso_date(from)),
            (None, Some(to)) => format!("Up to {}", iso_date(to)),
        }
    }

    /// Tax year used for AEA/rate lookups in the summary.
    ///
    /// Prefers the filter bounds (`--year` sets `from`), then the latest
    /// filtered event, then today's date.
    pub fn rate_year(&self, events: &[&TaxableEvent]) -> TaxYear {
        let date = self
            .from
            .or(self.to)
            .or_else(|| events.iter().map(|e| e.date()).max())
            .unwrap_or_else(|| chrono::Local::now().date_naive());
        TaxYear::from_date(date)
    }
}

/// Parse a YYYY-MM-DD date string.
pub fn parse_date(s: &str, flag: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("{} must be in YYYY-MM-DD format, got: {:?}", flag, s))
}

fn year_bounds(year: i32) -> anyhow::Result<(Option<NaiveDate>, Option<NaiveDate>)> {
    let (from, to) = TaxYear(year)
        .try_bounds()
        .ok_or_else(|| anyhow::anyhow!("invalid --year value: {}", year))?;
    Ok((Some(from), Some(to)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{AssetClass, Tag};
    use chrono::DateTime;
    use rust_decimal_macros::dec;

    fn make_event(date: &str, event_type: EventType, asset: &str) -> TaxableEvent {
        TaxableEvent {
            id: 1,
            source_transaction_id: "tx".to_string(),
            account: String::new(),
            datetime: DateTime::parse_from_rfc3339(&format!("{date}T00:00:00+00:00")).unwrap(),
            event_type,
            tag: Tag::Trade,
            asset: asset.to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1),
            value_gbp: dec!(1000),
            fee_gbp: None,
            description: None,
            demerged_from: None,
        }
    }

    #[test]
    fn extreme_date_is_rejected_not_panicked_on() {
        // chrono parses signed years this large, but their tax year has no
        // representable 5 April end.
        let args = FilterArgs {
            from: Some("+262142-04-06".to_string()),
            to: Some("+262142-04-07".to_string()),
            ..Default::default()
        };
        let err = args.build(None).unwrap_err().to_string();
        assert!(err.contains("out of range"), "{err}");
    }

    #[test]
    fn asset_filter_matches_non_ascii_symbols_case_insensitively() {
        let args = FilterArgs::default();
        let filter = args.build(Some("stéth".to_string())).unwrap();
        assert!(filter.matches_asset("STÉTH"));
        assert!(!filter.matches_asset("STETH"));
    }

    #[test]
    fn year_expands_to_date_range() {
        let args = FilterArgs {
            year: Some(2025),
            ..Default::default()
        };
        let filter = args.build(None).unwrap();
        assert_eq!(filter.from, NaiveDate::from_ymd_opt(2024, 4, 6));
        assert_eq!(filter.to, NaiveDate::from_ymd_opt(2025, 4, 5));
    }

    #[test]
    fn year_with_from_rejected() {
        let args = FilterArgs {
            year: Some(2025),
            from: Some("2024-04-06".to_string()),
            ..Default::default()
        };
        let err = args.build(None).unwrap_err().to_string();
        assert!(err.contains("--year"));
        assert!(err.contains("--from"));
    }

    #[test]
    fn year_with_to_rejected() {
        let args = FilterArgs {
            year: Some(2025),
            to: Some("2025-04-05".to_string()),
            ..Default::default()
        };
        let err = args.build(None).unwrap_err().to_string();
        assert!(err.contains("--year"));
        assert!(err.contains("--to"));
    }

    #[test]
    fn from_filter_inclusive() {
        let filter = EventFilter {
            from: Some(NaiveDate::from_ymd_opt(2024, 6, 1).unwrap()),
            to: None,
            asset: None,
            event_kind: None,
        };
        assert!(filter.matches_event(&make_event("2024-06-01", EventType::Acquisition, "BTC")));
        assert!(!filter.matches_event(&make_event("2024-05-31", EventType::Acquisition, "BTC")));
    }

    #[test]
    fn to_filter_inclusive() {
        let filter = EventFilter {
            from: None,
            to: Some(NaiveDate::from_ymd_opt(2024, 6, 30).unwrap()),
            asset: None,
            event_kind: None,
        };
        assert!(filter.matches_event(&make_event("2024-06-30", EventType::Acquisition, "BTC")));
        assert!(!filter.matches_event(&make_event("2024-07-01", EventType::Acquisition, "BTC")));
    }

    #[test]
    fn both_bounds_inclusive() {
        let filter = EventFilter {
            from: Some(NaiveDate::from_ymd_opt(2024, 4, 6).unwrap()),
            to: Some(NaiveDate::from_ymd_opt(2024, 9, 30).unwrap()),
            asset: None,
            event_kind: None,
        };
        assert!(!filter.matches_event(&make_event("2024-04-05", EventType::Acquisition, "BTC")));
        assert!(filter.matches_event(&make_event("2024-04-06", EventType::Acquisition, "BTC")));
        assert!(filter.matches_event(&make_event("2024-06-15", EventType::Acquisition, "BTC")));
        assert!(filter.matches_event(&make_event("2024-09-30", EventType::Acquisition, "BTC")));
        assert!(!filter.matches_event(&make_event("2024-10-01", EventType::Acquisition, "BTC")));
    }

    #[test]
    fn event_kind_disposal() {
        let filter = EventFilter {
            from: None,
            to: None,
            asset: None,
            event_kind: Some(EventKind::Disposal),
        };
        assert!(filter.matches_event(&make_event("2024-06-01", EventType::Disposal, "BTC")));
        assert!(!filter.matches_event(&make_event("2024-06-01", EventType::Acquisition, "BTC")));
    }

    #[test]
    fn event_kind_acquisition() {
        let filter = EventFilter {
            from: None,
            to: None,
            asset: None,
            event_kind: Some(EventKind::Acquisition),
        };
        assert!(filter.matches_event(&make_event("2024-06-01", EventType::Acquisition, "BTC")));
        assert!(!filter.matches_event(&make_event("2024-06-01", EventType::Disposal, "BTC")));
    }

    #[test]
    fn from_gt_to_rejected() {
        let args = FilterArgs {
            from: Some("2024-06-30".to_string()),
            to: Some("2024-06-01".to_string()),
            ..Default::default()
        };
        let err = args.build(None).unwrap_err().to_string();
        assert!(err.contains("--from"));
        assert!(err.contains("--to"));
    }

    #[test]
    fn invalid_date_format() {
        let err = parse_date("2024/06/01", "--from").unwrap_err().to_string();
        assert!(err.contains("YYYY-MM-DD"));
    }

    #[test]
    fn rate_year_prefers_from_bound() {
        let filter = EventFilter {
            from: Some(NaiveDate::from_ymd_opt(2023, 4, 6).unwrap()),
            to: Some(NaiveDate::from_ymd_opt(2025, 4, 5).unwrap()),
            asset: None,
            event_kind: None,
        };
        let event = make_event("2024-12-01", EventType::Disposal, "BTC");
        assert_eq!(filter.rate_year(&[&event]), TaxYear(2024));
    }

    #[test]
    fn rate_year_falls_back_to_to_bound() {
        let filter = EventFilter {
            from: None,
            to: Some(NaiveDate::from_ymd_opt(2023, 1, 1).unwrap()),
            asset: None,
            event_kind: None,
        };
        assert_eq!(filter.rate_year(&[]), TaxYear(2023));
    }

    #[test]
    fn rate_year_falls_back_to_latest_event() {
        let filter = EventFilter {
            from: None,
            to: None,
            asset: None,
            event_kind: None,
        };
        let events = [
            make_event("2023-06-01", EventType::Acquisition, "BTC"),
            make_event("2024-12-01", EventType::Disposal, "BTC"),
        ];
        let refs: Vec<&TaxableEvent> = events.iter().collect();
        assert_eq!(filter.rate_year(&refs), TaxYear(2025));
    }

    #[test]
    fn empty_result() {
        let filter = EventFilter {
            from: Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()),
            to: Some(NaiveDate::from_ymd_opt(2025, 12, 31).unwrap()),
            asset: None,
            event_kind: None,
        };
        let events = vec![
            make_event("2024-06-01", EventType::Acquisition, "BTC"),
            make_event("2024-12-01", EventType::Disposal, "BTC"),
        ];
        assert!(filter.apply(&events).is_empty());
    }
}
