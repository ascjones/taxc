//! Schema command - print expected input formats

use crate::events::TaxInput;
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
        println!("{}", CSV_COLUMNS.join(","));
        Ok(())
    }

    fn print_csv_fields(&self) -> anyhow::Result<()> {
        println!("CSV Input Format");
        println!("================");
        println!();
        for (name, required, description) in CSV_FIELD_DESCRIPTIONS {
            let req = if *required { "required" } else { "optional" };
            println!("{:20} ({:8})  {}", name, req, description);
        }
        println!();
        println!("FX rate convention: fx_rate is always price_quote/GBP");
        Ok(())
    }
}

const CSV_COLUMNS: &[&str] = &[
    "id",
    "date",
    "event_type",
    "asset",
    "asset_class",
    "quantity",
    "price_rate",
    "price_quote",
    "price_source",
    "price_time",
    "fx_rate",
    "fx_source",
    "fx_time",
    "fee_amount",
    "fee_asset",
    "fee_price_rate",
    "fee_price_quote",
    "fee_fx_rate",
    "fee_fx_source",
    "description",
];

const CSV_FIELD_DESCRIPTIONS: &[(&str, bool, &str)] = &[
    (
        "id",
        false,
        "Unique identifier for linking back to source data",
    ),
    (
        "date",
        true,
        "Event date (YYYY-MM-DD or YYYY-MM-DDThh:mm:ss)",
    ),
    (
        "event_type",
        true,
        "Acquisition, Disposal, StakingReward, Dividend",
    ),
    ("asset", true, "Asset identifier (e.g., BTC, ETH, AAPL)"),
    ("asset_class", true, "Crypto or Stock"),
    ("quantity", true, "Amount of asset"),
    (
        "price_rate",
        false,
        "Asset price (required if asset != GBP)",
    ),
    (
        "price_quote",
        false,
        "Quote currency for price_rate (GBP, USD, EUR)",
    ),
    ("price_source", false, "Price data source"),
    ("price_time", false, "Price timestamp"),
    (
        "fx_rate",
        false,
        "FX rate to GBP (required if price_quote != GBP)",
    ),
    ("fx_source", false, "FX rate source"),
    ("fx_time", false, "FX rate timestamp"),
    ("fee_amount", false, "Fee amount"),
    ("fee_asset", false, "Fee asset (required if fee_amount set)"),
    (
        "fee_price_rate",
        false,
        "Fee asset price (required if fee_asset != GBP)",
    ),
    ("fee_price_quote", false, "Fee price quote currency"),
    (
        "fee_fx_rate",
        false,
        "Fee FX rate (required if fee_price_quote != GBP)",
    ),
    ("fee_fx_source", false, "Fee FX rate source"),
    ("description", false, "Optional description"),
];
