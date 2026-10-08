pub mod cgt;
pub mod events;
pub mod fmt;
pub mod price;
pub mod summary;
pub mod transactions;
pub mod uk;
pub mod warnings;

// Flat public surface for domain types and functions.
pub use cgt::{
    calculate_cgt, CgtReport, DisposalIndex, DisposalRecord, DisposalTotals, PoolHistoryEntry,
    YearEndSnapshot,
};
pub use events::{display_event_type, AssetClass, EventType, Tag, TaxableEvent};
pub use summary::{event_warnings, summarize, summarize_by_year, TaxSummary};
pub use transactions::{
    document_to_events, read_transactions_json, transactions_to_events, ConversionOptions,
    TransactionError, Transactions,
};
pub use uk::{cgt_rate_change_2024, cgt_rate_on, uk_date, uk_rfc3339, TaxBand, TaxYear};
pub use warnings::Warning;
