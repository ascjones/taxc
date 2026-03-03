pub mod cgt;
pub mod events;
pub mod price;
pub mod transactions;
pub mod uk;
pub mod warnings;

// Flat public surface for domain types and functions.
pub use cgt::{
    calculate_cgt, CgtReport, DisposalIndex, DisposalRecord, MatchingRule, PoolHistoryEntry,
    PoolState, YearEndSnapshot,
};
pub use events::{display_event_type, AssetClass, EventType, Tag, TaxableEvent};
#[allow(unused_imports)]
pub use transactions::{
    read_transactions_json, transactions_to_events, Amount, Asset, AssetRegistry,
    ConversionOptions, TransactionError, Transactions,
};
pub use uk::{TaxBand, TaxYear};
pub use warnings::Warning;
