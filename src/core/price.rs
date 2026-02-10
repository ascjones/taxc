use super::TransactionError;
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Price for an asset - either direct GBP or foreign currency with FX conversion
///
/// For direct GBP prices: value_gbp = quantity * rate
/// For FX prices: value_gbp = quantity * rate * fx_rate
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Price {
    /// The asset this price refers to (e.g., "BTC", "ETH")
    pub base: String,
    /// Foreign currency quote (e.g., "USD") - requires fx_rate
    #[serde(default)]
    pub quote: Option<String>,
    /// Price per unit (in GBP, or in quote currency if FX fields present)
    #[schemars(with = "f64")]
    pub rate: Decimal,
    /// FX rate to convert quote currency to GBP - requires quote
    #[serde(default)]
    #[schemars(with = "Option<f64>")]
    pub fx_rate: Option<Decimal>,
    /// Optional source of price data
    #[serde(default)]
    pub source: Option<String>,
}

impl Price {
    pub fn to_gbp(&self, quantity: Decimal) -> Result<Decimal, TransactionError> {
        match (&self.quote, &self.fx_rate) {
            (None, None) => Ok(quantity * self.rate),
            (Some(quote), Some(fx_rate)) => {
                if quote.trim().is_empty() {
                    return Err(TransactionError::InvalidPrice(
                        "quote is required and cannot be empty".to_string(),
                    ));
                }
                Ok(quantity * self.rate * fx_rate)
            }
            _ => Err(TransactionError::InvalidPrice(
                "quote and fx_rate must both be present or both absent".to_string(),
            )),
        }
    }
}
