//! Schema command - print expected input formats

use crate::events::{csv_schema, TaxInput};
use clap::Args;
use schemars::schema_for;

#[derive(Args, Debug)]
pub struct SchemaCommand {
    /// Output format: json-schema or csv-header
    #[arg(value_enum, default_value = "json-schema")]
    format: SchemaFormat,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum SchemaFormat {
    /// JSON Schema for the input format
    JsonSchema,
    /// CSV header row with column names
    CsvHeader,
    /// CSV column descriptions
    CsvFields,
}

impl SchemaCommand {
    pub fn exec(&self) -> anyhow::Result<()> {
        match self.format {
            SchemaFormat::JsonSchema => self.print_json_schema(),
            SchemaFormat::CsvHeader => self.print_csv_header(),
            SchemaFormat::CsvFields => self.print_csv_fields(),
        }
    }

    fn print_json_schema(&self) -> anyhow::Result<()> {
        let schema = schema_for!(TaxInput);
        println!("{}", serde_json::to_string_pretty(&schema)?);
        Ok(())
    }

    fn print_csv_header(&self) -> anyhow::Result<()> {
        let headers: Vec<_> = csv_schema().iter().map(|f| f.name).collect();
        println!("{}", headers.join(","));
        Ok(())
    }

    fn print_csv_fields(&self) -> anyhow::Result<()> {
        println!("CSV Input Format");
        println!("================");
        println!();
        for field in csv_schema() {
            let req = if field.required {
                "required"
            } else {
                "optional"
            };
            println!("{:20} ({:8})  {}", field.name, req, field.description);
        }
        println!();
        println!("FX rate convention: fx_rate is always price_quote/GBP");
        Ok(())
    }
}
