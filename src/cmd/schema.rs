//! Schema command - print expected input format

use crate::transaction::TransactionInput;
use clap::Args;
use schemars::schema_for;

#[derive(Args, Debug)]
pub struct SchemaCommand {}

impl SchemaCommand {
    pub fn exec(&self) -> anyhow::Result<()> {
        let schema = schema_for!(TransactionInput);
        println!("{}", serde_json::to_string_pretty(&schema)?);
        Ok(())
    }
}
