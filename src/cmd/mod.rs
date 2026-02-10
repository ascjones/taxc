pub mod html_report;
pub mod pools;
pub mod schema;
pub mod summary;
pub mod validate;

use crate::events::TaxableEvent;
use crate::transaction::{self, ConversionOptions};
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

/// Read transactions (JSON) and convert to events (or stdin with "-")
pub fn read_events(path: &Path, exclude_unlinked: bool) -> anyhow::Result<Vec<TaxableEvent>> {
    let options = ConversionOptions { exclude_unlinked };
    if path.as_os_str() == "-" {
        read_from_stdin(options)
    } else {
        read_from_file(path, options)
    }
}

fn read_from_file(path: &Path, options: ConversionOptions) -> anyhow::Result<Vec<TaxableEvent>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let transactions = transaction::read_transactions_json(reader)?;
    let events = transaction::transactions_to_events(&transactions, options)?;
    Ok(events)
}

fn read_from_stdin(options: ConversionOptions) -> anyhow::Result<Vec<TaxableEvent>> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());

    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;

    if buffer.is_empty() {
        anyhow::bail!("No input received. Provide a file or pipe data to stdin.");
    }

    let cursor = io::Cursor::new(buffer);
    let transactions = transaction::read_transactions_json(cursor)?;
    let events = transaction::transactions_to_events(&transactions, options)?;
    Ok(events)
}
