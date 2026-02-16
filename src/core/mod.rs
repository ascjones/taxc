pub mod cgt;
pub mod events;
pub mod income;
pub mod price;
pub mod transaction;
pub mod uk;
pub mod warnings;

// Flat public surface for domain types and functions.
pub use cgt::{
    calculate_cgt, CgtReport, DisposalIndex, MatchingRule, PoolHistoryEntry, PoolState,
    YearEndSnapshot,
};
pub use events::{display_event_type, AssetClass, EventType, Tag, TaxableEvent};
pub use income::{calculate_income_tax, IncomeReport};
#[allow(unused_imports)]
pub use transaction::{
    read_transactions_json, transactions_to_events, Amount, Asset, AssetRegistry,
    ConversionOptions, TransactionError, TransactionInput,
};
pub use uk::{TaxBand, TaxYear};
pub use warnings::Warning;
