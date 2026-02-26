use std::io::Read;

use crate::core::events::TaxableEvent;

mod convert;
mod datetime;
mod error;
mod model;
mod normalize;
mod validate;

pub use error::TransactionError;
#[allow(unused_imports)]
pub use model::{
    Amount, Asset, AssetRegistry, ConversionOptions, Fee, Transaction, TransactionInput,
    TransactionType,
};

use normalize::{normalize_assets, normalize_transactions};
use validate::{validate_assets, validate_links};

/// Read transactions from JSON
pub fn read_transactions_json<R: Read>(
    reader: R,
) -> anyhow::Result<(Vec<Transaction>, AssetRegistry)> {
    let input: TransactionInput = serde_json::from_reader(reader)?;
    let mut assets = input.assets;
    let mut transactions = input.transactions;
    normalize_assets(&mut assets);
    normalize_transactions(&mut transactions);
    let registry = validate_assets(&assets, &transactions)?;
    transactions.sort_by_key(|t| t.datetime);
    Ok((transactions, registry))
}

/// Convert transactions to taxable events
pub fn transactions_to_events(
    transactions: &[Transaction],
    registry: &AssetRegistry,
    options: ConversionOptions,
) -> Result<Vec<TaxableEvent>, TransactionError> {
    validate_links(transactions)?;

    let mut events = Vec::new();

    for tx in transactions {
        let mut tx_events = tx.to_taxable_events(registry, options.exclude_unlinked)?;
        events.append(&mut tx_events);
    }

    events.sort_by_key(|e| e.datetime);
    for (idx, event) in events.iter_mut().enumerate() {
        event.id = idx + 1;
    }
    Ok(events)
}

#[cfg(test)]
mod tests;
