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
    calculate_cgt, CgtReport, DisposalIndex, DisposalRecord, PoolHistoryEntry, PoolState,
    YearEndSnapshot,
};
pub use events::{display_event_type, AssetClass, EventType, Tag, TaxableEvent};
pub use summary::{event_warnings, summarize, TaxSummary};
pub use transactions::{
    document_to_events, read_transactions_json, transactions_to_events, ConversionOptions,
    TransactionError, Transactions,
};
pub use uk::{TaxBand, TaxYear};
pub use warnings::Warning;
