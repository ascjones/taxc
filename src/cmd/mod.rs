pub mod filter;
pub mod pools;
pub mod report;
pub mod schema;
pub mod summary;

use crate::core::transactions::Transaction;
use crate::core::{self, ConversionOptions, TaxableEvent};
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

/// Read transactions (JSON) and convert to events (or stdin with "-")
pub fn read_events(path: &Path, exclude_unlinked: bool) -> anyhow::Result<Vec<TaxableEvent>> {
    let (_, events) = read_transactions_and_events(path, exclude_unlinked)?;
    Ok(events)
}

/// Read transactions and convert to events, returning both
pub fn read_transactions_and_events(
    path: &Path,
    exclude_unlinked: bool,
) -> anyhow::Result<(Vec<Transaction>, Vec<TaxableEvent>)> {
    let options = ConversionOptions { exclude_unlinked };
    let (transactions, registry) = if path.as_os_str() == "-" {
        read_json_from_stdin()?
    } else {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        core::read_transactions_json(reader)?
    };
    let events = core::transactions_to_events(&transactions, &registry, options)?;
    Ok((transactions, events))
}

fn read_json_from_stdin() -> anyhow::Result<(Vec<Transaction>, core::transactions::AssetRegistry)> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());

    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;

    if buffer.is_empty() {
        anyhow::bail!("No input received. Provide a file or pipe data to stdin.");
    }

    let cursor = io::Cursor::new(buffer);
    core::read_transactions_json(cursor)
}
