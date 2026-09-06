use std::collections::HashMap;
use std::io::Read;

use crate::core::events::AssetClass;
use crate::core::events::TaxableEvent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod convert;
mod datetime;
mod error;
mod normalize;
mod transaction;
mod validate;
mod valuation;

pub use error::TransactionError;
pub use transaction::{Amount, Fee, Transaction, TransactionType};
pub use valuation::Valuation;

/// Input root for transaction JSON
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Transactions {
    pub assets: Vec<Asset>,
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Asset {
    pub symbol: String,
    pub asset_class: AssetClass,
}

pub type AssetRegistry = HashMap<String, Asset>;

#[derive(JsonSchema)]
#[schemars(untagged)]
pub(crate) enum DecimalJson {
    #[allow(dead_code)]
    Number(f64),
    #[allow(dead_code)]
    String(#[schemars(regex(pattern = r"^-?[0-9]+(\.[0-9]+)?$"))] String),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ConversionOptions {
    pub exclude_unlinked: bool,
}

use normalize::{normalize_assets, normalize_transactions};
use validate::{validate_amounts, validate_assets, validate_links};

/// Read transactions from JSON
pub fn read_transactions_json<R: Read>(
    reader: R,
) -> anyhow::Result<(Vec<Transaction>, AssetRegistry)> {
    let input: Transactions = serde_json::from_reader(reader)?;
    Ok(prepare(input)?)
}

pub fn prepare(input: Transactions) -> Result<(Vec<Transaction>, AssetRegistry), TransactionError> {
    let mut assets = input.assets;
    let mut transactions = input.transactions;
    normalize_assets(&mut assets);
    normalize_transactions(&mut transactions);
    let registry = validate_assets(&assets, &transactions)?;
    transactions.sort_by_key(|t| t.datetime);
    Ok((transactions, registry))
}

pub fn document_to_events(
    input: Transactions,
    options: ConversionOptions,
) -> Result<Vec<TaxableEvent>, TransactionError> {
    let (transactions, registry) = prepare(input)?;
    transactions_to_events(&transactions, &registry, options)
}

/// Convert transactions to taxable events
pub fn transactions_to_events(
    transactions: &[Transaction],
    registry: &AssetRegistry,
    options: ConversionOptions,
) -> Result<Vec<TaxableEvent>, TransactionError> {
    validate_amounts(transactions)?;
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
