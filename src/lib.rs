//! UK capital gains and income tax calculator.
//!
//! The crate ships a CLI (`taxc`) and this library, which exposes the CLI's
//! input document as typed, serializable structs and runs the same
//! calculations without any I/O.
//!
//! Stable public surface:
//!
//! - [`input`] — the JSON document `taxc` parses (`Transactions` and its
//!   row/field types) plus the validation error type.
//! - [`results`] — the domain values a calculation produces.
//! - [`validate`], [`calculate`], [`input_schema`] and their option/result
//!   types at the crate root.
//!
//! Everything else is internal and may change without notice.
//!
//! ```no_run
//! use taxc::input::Transactions;
//!
//! let doc: Transactions = serde_json::from_str(r#"{"assets":[],"transactions":[]}"#)?;
//! taxc::validate(&doc)?;
//! let results = taxc::calculate(doc, &taxc::CalculationOptions::default())?;
//! for year in &results.years {
//!     println!("{}: CGT {}", year.summary.tax_year.display(), year.summary.cgt.estimated_cgt);
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod cmd;
mod core;

#[doc(hidden)]
pub mod cli;

/// The input document and its field types.
///
/// `Transactions` is the document root (`{ "assets": [...], "transactions": [...] }`).
/// Unknown fields are ignored on parse, so a producer may embed these arrays
/// in a larger envelope.
pub mod input {
    pub use crate::core::events::{AssetClass, Tag};
    pub use crate::core::price::Price;
    pub use crate::core::transactions::{
        Amount, Asset, Fee, Transaction, TransactionError, TransactionType, Transactions, Valuation,
    };
}

/// Values produced by a calculation.
pub mod results {
    pub use crate::core::cgt::{
        CgtReport, CgtSummary, DisposalRecord, MatchingComponent, MatchingRule, PoolHistory,
        PoolHistoryEntry, PoolState, YearEndSnapshot,
    };
    pub use crate::core::events::{EventType, TaxableEvent};
    pub use crate::core::summary::{CgtPosition, IncomePosition, TaxSummary};
    pub use crate::core::uk::{TaxBand, TaxYear};
    pub use crate::core::warnings::Warning;
}

use core::transactions::ConversionOptions;
use core::{calculate_cgt, document_to_events, summarize};
use input::{TransactionError, Transactions};
use results::{CgtReport, DisposalRecord, TaxBand, TaxSummary, TaxYear, TaxableEvent, Warning};
use std::collections::BTreeSet;

/// Options for [`calculate`].
#[derive(Debug, Clone, Default)]
pub struct CalculationOptions {
    /// Band used for the income tax estimate and the CGT rate.
    pub tax_band: TaxBand,
    /// Drop unlinked deposits/withdrawals instead of treating them as
    /// unclassified acquisitions/disposals (the CLI's `--exclude-unlinked`).
    pub exclude_unlinked: bool,
    /// Summarise only this tax year. `None` summarises every year that has
    /// events. CGT matching always runs over the full history either way.
    pub tax_year: Option<TaxYear>,
}

/// A warning attached to one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventWarning {
    pub event_id: usize,
    pub source_transaction_id: String,
    pub warning: Warning,
}

/// One tax year's position, as `taxc summary --year` would print it.
#[derive(Debug, Clone)]
pub struct TaxYearResults {
    pub summary: TaxSummary,
    /// Warnings on events dated in this year, in event order.
    pub warnings: Vec<EventWarning>,
}

/// Everything a calculation produces.
#[derive(Debug)]
pub struct TaxResults {
    /// Taxable events derived from the document, in chronological order.
    pub events: Vec<TaxableEvent>,
    /// Disposal matching and pool history over the full event history.
    pub cgt: CgtReport,
    /// Per-year positions, ascending by tax year.
    pub years: Vec<TaxYearResults>,
}

/// Check that `document` would be accepted by `taxc`, without calculating
/// anything. Returns the first rejection the CLI would report.
pub fn validate(document: &Transactions) -> Result<(), TransactionError> {
    document_to_events(document.clone(), ConversionOptions::default()).map(|_| ())
}

/// Run the CGT and income calculations the CLI drives and return the
/// results as values.
pub fn calculate(
    document: Transactions,
    options: &CalculationOptions,
) -> Result<TaxResults, TransactionError> {
    let events = document_to_events(
        document,
        ConversionOptions {
            exclude_unlinked: options.exclude_unlinked,
        },
    )?;
    let cgt = calculate_cgt(events.clone());

    let years: BTreeSet<TaxYear> = match options.tax_year {
        Some(year) => BTreeSet::from([year]),
        None => events
            .iter()
            .map(|e| TaxYear::from_date(e.date()))
            .collect(),
    };
    let years = years
        .into_iter()
        .map(|year| summarize_year(&events, &cgt, year, options.tax_band))
        .collect();

    Ok(TaxResults { events, cgt, years })
}

fn summarize_year(
    events: &[TaxableEvent],
    cgt: &CgtReport,
    year: TaxYear,
    band: TaxBand,
) -> TaxYearResults {
    let in_year = |date: chrono::NaiveDate| date >= year.start_date() && date <= year.end_date();
    let year_events: Vec<&TaxableEvent> = events.iter().filter(|e| in_year(e.date())).collect();
    let disposals: Vec<&DisposalRecord> = cgt
        .disposals
        .iter()
        .filter(|d| !d.is_unclassified() && in_year(d.date))
        .collect();
    let summary = summarize(&year_events, &disposals, year, band);

    let mut disposal_index = core::DisposalIndex::new(cgt);
    let mut warnings = Vec::new();
    for event in &year_events {
        let mut event_warnings = if event.tag == input::Tag::Unclassified {
            vec![Warning::UnclassifiedEvent]
        } else {
            Vec::new()
        };
        if event.event_type == results::EventType::Disposal {
            if let Some(d) = disposal_index.find(event) {
                for warning in &d.warnings {
                    if !event_warnings.contains(warning) {
                        event_warnings.push(warning.clone());
                    }
                }
            }
        }
        warnings.extend(event_warnings.into_iter().map(|warning| EventWarning {
            event_id: event.id,
            source_transaction_id: event.source_transaction_id.clone(),
            warning,
        }));
    }

    TaxYearResults { summary, warnings }
}

/// JSON Schema for the input document, identical to `taxc schema input`.
pub fn input_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(Transactions)).expect("schema serializes to JSON")
}
