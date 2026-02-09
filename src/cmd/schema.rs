//! Schema command - print JSON schemas for input/output formats

use crate::cmd::html_report::HtmlReportData;
use crate::transaction::TransactionInput;
use clap::{Args, ValueEnum};
use schemars::schema_for;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SchemaType {
    /// Input transaction format
    Input,
    /// Output report format (JSON mode)
    Output,
}

#[derive(Args, Debug)]
pub struct SchemaCommand {
    /// Which schema to output
    #[arg(value_enum, default_value = "input")]
    schema_type: SchemaType,
}

impl SchemaCommand {
    pub fn exec(&self) -> anyhow::Result<()> {
        let schema = match self.schema_type {
            SchemaType::Input => schema_for!(TransactionInput),
            SchemaType::Output => schema_for!(HtmlReportData),
        };
        println!("{}", serde_json::to_string_pretty(&schema)?);
        Ok(())
    }
}
