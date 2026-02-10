pub mod cgt;
pub mod events;
pub mod income;
pub mod price;
pub mod transaction;
pub mod uk;

// Flat public surface for domain types and functions.
pub use cgt::{
    calculate_cgt, CgtReport, DisposalIndex, DisposalWarning, MatchingRule, PoolHistoryEntry,
    PoolState, YearEndSnapshot,
};
pub use events::{display_event_type, AssetClass, EventType, Label, TaxableEvent};
pub use income::{calculate_income_tax, IncomeReport};
pub use transaction::{
    read_transactions_json, transactions_to_events, ConversionOptions, TransactionError,
    TransactionInput,
};
pub use uk::{TaxBand, TaxYear};
