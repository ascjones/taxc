use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use std::io::Read;

/// Unified JSON input format
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaxInput {
    #[serde(default)]
    pub tax_year: Option<String>,
    pub events: Vec<TaxableEvent>,
}

/// Type of taxable event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum EventType {
    Acquisition,
    Disposal,
    StakingReward,
    Dividend,
    /// Unclassified inbound - treated as Acquisition for conservative estimates
    UnclassifiedIn,
    /// Unclassified outbound - treated as Disposal for conservative estimates
    UnclassifiedOut,
}

impl EventType {
    #[cfg(test)]
    pub fn is_income(&self) -> bool {
        matches!(self, EventType::StakingReward | EventType::Dividend)
    }

    /// Check if this event type represents an acquisition (or acts like one)
    pub fn is_acquisition_like(&self) -> bool {
        matches!(
            self,
            EventType::Acquisition | EventType::StakingReward | EventType::UnclassifiedIn
        )
    }

    /// Check if this event type represents a disposal (or acts like one)
    pub fn is_disposal_like(&self) -> bool {
        matches!(self, EventType::Disposal | EventType::UnclassifiedOut)
    }
}

/// Asset class for tax treatment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AssetClass {
    Crypto,
    Stock,
}

/// Price with quote currency and metadata
/// Convention: rate represents base/quote (e.g., BTC/USD = 40000 means 1 BTC = 40000 USD)
/// For fx_rate: base is the foreign currency, quote is always GBP
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct Price {
    #[schemars(with = "f64")]
    pub rate: Decimal,
    pub quote: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default, deserialize_with = "deserialize_flexible_datetime")]
    pub timestamp: Option<NaiveDateTime>,
}

/// A taxable event (acquisition, disposal, or income)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaxableEvent {
    /// Optional identifier to link back to source data
    #[serde(default)]
    pub id: Option<String>,
    #[serde(
        rename = "date",
        deserialize_with = "deserialize_datetime",
        serialize_with = "serialize_datetime"
    )]
    #[schemars(with = "String")]
    pub datetime: NaiveDateTime,
    pub event_type: EventType,
    pub asset: String,
    pub asset_class: AssetClass,
    #[schemars(with = "f64")]
    pub quantity: Decimal,

    #[serde(default)]
    pub price: Option<Price>,
    #[serde(default)]
    pub fx_rate: Option<Price>,

    #[serde(default)]
    #[schemars(with = "Option<f64>")]
    pub fee_amount: Option<Decimal>,
    #[serde(default)]
    pub fee_asset: Option<String>,
    #[serde(default)]
    pub fee_price: Option<Price>,
    #[serde(default)]
    pub fee_fx_rate: Option<Price>,

    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TaxcError {
    #[error("price required for non-GBP asset: {0}")]
    MissingPrice(String),

    #[error("fx_rate required when price.quote is {0}")]
    MissingFxRate(String),

    #[error("invalid price configuration: {0}")]
    InvalidPrice(String),

    #[error("invalid currency: {0}")]
    InvalidCurrency(String),

    #[error("fee_asset required when fee_amount is set")]
    MissingFeeAsset,

    #[error("invalid timestamp format: {0}")]
    InvalidTimestamp(String),
}

impl TaxableEvent {
    /// Get just the date portion for tax calculations
    pub fn date(&self) -> NaiveDate {
        self.datetime.date()
    }

    pub fn value_gbp(&self) -> Result<Decimal, TaxcError> {
        to_gbp(
            self.quantity,
            &self.asset,
            self.price.as_ref(),
            self.fx_rate.as_ref(),
        )
    }

    pub fn fees_gbp(&self) -> Result<Decimal, TaxcError> {
        match (&self.fee_amount, &self.fee_asset) {
            (None, _) => Ok(Decimal::ZERO),
            (Some(amount), Some(asset)) => to_gbp(
                *amount,
                asset,
                self.fee_price.as_ref(),
                self.fee_fx_rate.as_ref(),
            ),
            (Some(_), None) => Err(TaxcError::MissingFeeAsset),
        }
    }

    pub fn total_cost_gbp(&self) -> Result<Decimal, TaxcError> {
        Ok(self.value_gbp()? + self.fees_gbp()?)
    }
}

fn normalize_currency(s: &str) -> String {
    s.trim().to_uppercase()
}

fn is_gbp(s: &str) -> bool {
    s.eq_ignore_ascii_case("GBP")
}

/// Calculate GBP value from amount and optional pricing
fn to_gbp(
    amount: Decimal,
    base_asset: &str,
    price: Option<&Price>,
    fx_rate: Option<&Price>,
) -> Result<Decimal, TaxcError> {
    if let Some(p) = price {
        if p.quote.trim().is_empty() {
            return Err(TaxcError::InvalidCurrency(
                "quote is required and cannot be empty".to_string(),
            ));
        }
    }

    match (is_gbp(base_asset), price, fx_rate) {
        (true, None, _) => Ok(amount),

        (_, Some(p), None) if is_gbp(&p.quote) => Ok(amount * p.rate),

        (_, Some(p), Some(_)) if is_gbp(&p.quote) => Err(TaxcError::InvalidPrice(
            "fx_rate should not be provided when price.quote is GBP".to_string(),
        )),

        (_, Some(p), Some(fx)) => Ok(amount * p.rate * fx.rate),

        (_, Some(p), None) => Err(TaxcError::MissingFxRate(p.quote.clone())),

        (false, None, _) => Err(TaxcError::MissingPrice(base_asset.to_string())),
    }
}

/// Parse a date string that may be date-only or datetime format
fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
    // Try datetime format first: "2024-01-15T10:30:00" or "2024-01-15 10:30:00"
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(dt);
    }
    // Try with milliseconds
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(dt);
    }
    // Fall back to date-only format, defaulting to midnight
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()));
    }
    None
}

fn deserialize_datetime<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    parse_datetime(&s)
        .ok_or_else(|| serde::de::Error::custom(format!("Invalid date format: {}", s)))
}

fn serialize_datetime<S>(datetime: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&datetime.format("%Y-%m-%dT%H:%M:%S").to_string())
}

/// Custom deserializer: accepts date-only or datetime, normalizes to NaiveDateTime
fn deserialize_flexible_datetime<'de, D>(deserializer: D) -> Result<Option<NaiveDateTime>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => {
            if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
                return Ok(Some(dt));
            }
            if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                return Ok(Some(date.and_hms_opt(0, 0, 0).unwrap()));
            }
            Err(serde::de::Error::custom(
                TaxcError::InvalidTimestamp(s.to_string()).to_string(),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
struct CsvEvent {
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "date", deserialize_with = "deserialize_datetime")]
    datetime: NaiveDateTime,
    event_type: EventType,
    asset: String,
    asset_class: AssetClass,
    quantity: Decimal,

    #[serde(default)]
    price_rate: Option<Decimal>,
    #[serde(default)]
    price_quote: Option<String>,
    #[serde(default)]
    price_source: Option<String>,
    #[serde(default, deserialize_with = "deserialize_flexible_datetime")]
    price_time: Option<NaiveDateTime>,

    #[serde(default)]
    fx_rate: Option<Decimal>,
    #[serde(default)]
    fx_source: Option<String>,
    #[serde(default, deserialize_with = "deserialize_flexible_datetime")]
    fx_time: Option<NaiveDateTime>,

    #[serde(default)]
    fee_amount: Option<Decimal>,
    #[serde(default)]
    fee_asset: Option<String>,
    #[serde(default)]
    fee_price_rate: Option<Decimal>,
    #[serde(default)]
    fee_price_quote: Option<String>,
    #[serde(default)]
    fee_fx_rate: Option<Decimal>,
    #[serde(default)]
    fee_fx_source: Option<String>,

    #[serde(default)]
    description: Option<String>,
}

impl CsvEvent {
    fn into_taxable_event(self) -> Result<TaxableEvent, TaxcError> {
        let price = build_price(
            self.price_rate,
            self.price_quote,
            self.price_source,
            self.price_time,
        )?;

        let fx_rate = self.fx_rate.map(|rate| Price {
            rate,
            quote: "GBP".to_string(),
            source: self.fx_source,
            timestamp: self.fx_time,
        });

        let fee_price = build_price(self.fee_price_rate, self.fee_price_quote, None, None)?;

        let fee_fx_rate = self.fee_fx_rate.map(|rate| Price {
            rate,
            quote: "GBP".to_string(),
            source: self.fee_fx_source,
            timestamp: None,
        });

        Ok(TaxableEvent {
            id: self.id,
            datetime: self.datetime,
            event_type: self.event_type,
            asset: normalize_currency(&self.asset),
            asset_class: self.asset_class,
            quantity: self.quantity,
            price,
            fx_rate,
            fee_amount: self.fee_amount,
            fee_asset: self.fee_asset.map(|s| normalize_currency(&s)),
            fee_price,
            fee_fx_rate,
            description: self.description,
        })
    }
}

fn build_price(
    rate: Option<Decimal>,
    quote: Option<String>,
    source: Option<String>,
    timestamp: Option<NaiveDateTime>,
) -> Result<Option<Price>, TaxcError> {
    if rate.is_none() && quote.is_none() && source.is_none() && timestamp.is_none() {
        return Ok(None);
    }

    let rate = rate.ok_or_else(|| {
        TaxcError::InvalidPrice("price_rate required when any price field is provided".to_string())
    })?;

    let quote = quote.ok_or_else(|| {
        TaxcError::InvalidCurrency("quote is required and cannot be empty".to_string())
    })?;
    let quote = normalize_currency(&quote);
    if quote.is_empty() {
        return Err(TaxcError::InvalidCurrency(
            "quote is required and cannot be empty".to_string(),
        ));
    }

    Ok(Some(Price {
        rate,
        quote,
        source,
        timestamp,
    }))
}

/// Read taxable events from CSV
pub fn read_csv<R: Read>(reader: R) -> anyhow::Result<Vec<TaxableEvent>> {
    let mut rdr = csv::Reader::from_reader(reader);
    let records: Result<Vec<CsvEvent>, _> = rdr.deserialize::<CsvEvent>().collect();
    let events = records?
        .into_iter()
        .map(CsvEvent::into_taxable_event)
        .collect::<Result<Vec<_>, _>>()?;

    let mut events = events;
    events.sort_by_key(|e| e.datetime);
    Ok(events)
}

/// Read taxable events from JSON
pub fn read_json<R: Read>(reader: R) -> anyhow::Result<Vec<TaxableEvent>> {
    let input: TaxInput = serde_json::from_reader(reader)?;
    let mut events = input.events;
    for event in &mut events {
        event.asset = normalize_currency(&event.asset);
        if let Some(asset) = event.fee_asset.as_mut() {
            *asset = normalize_currency(asset);
        }
        if let Some(price) = event.price.as_mut() {
            price.quote = normalize_currency(&price.quote);
        }
        if let Some(fx_rate) = event.fx_rate.as_mut() {
            fx_rate.quote = normalize_currency(&fx_rate.quote);
        }
        if let Some(fee_price) = event.fee_price.as_mut() {
            fee_price.quote = normalize_currency(&fee_price.quote);
        }
        if let Some(fee_fx_rate) = event.fee_fx_rate.as_mut() {
            fee_fx_rate.quote = normalize_currency(&fee_fx_rate.quote);
        }
    }
    events.sort_by_key(|e| e.datetime);
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn price(rate: Decimal, quote: &str) -> Price {
        Price {
            rate,
            quote: quote.to_string(),
            source: None,
            timestamp: None,
        }
    }

    #[test]
    fn to_gbp_gbp_asset_no_price() {
        let result = to_gbp(dec!(100), "GBP", None, None).unwrap();
        assert_eq!(result, dec!(100));
    }

    #[test]
    fn to_gbp_direct_gbp_price() {
        let result = to_gbp(dec!(2), "BTC", Some(&price(dec!(25000), "GBP")), None).unwrap();
        assert_eq!(result, dec!(50000));
    }

    #[test]
    fn to_gbp_foreign_price_with_fx() {
        let result = to_gbp(
            dec!(0.5),
            "BTC",
            Some(&price(dec!(40000), "USD")),
            Some(&price(dec!(0.79), "GBP")),
        )
        .unwrap();
        assert_eq!(result, dec!(15800));
    }

    #[test]
    fn to_gbp_missing_price() {
        let err = to_gbp(dec!(1), "BTC", None, None).unwrap_err();
        assert_eq!(err, TaxcError::MissingPrice("BTC".to_string()));
    }

    #[test]
    fn to_gbp_missing_fx_rate() {
        let err = to_gbp(dec!(1), "BTC", Some(&price(dec!(1000), "USD")), None).unwrap_err();
        assert_eq!(err, TaxcError::MissingFxRate("USD".to_string()));
    }

    #[test]
    fn to_gbp_gbp_price_with_fx_is_error() {
        let err = to_gbp(
            dec!(1),
            "BTC",
            Some(&price(dec!(1000), "GBP")),
            Some(&price(dec!(1), "GBP")),
        )
        .unwrap_err();
        assert_eq!(
            err,
            TaxcError::InvalidPrice(
                "fx_rate should not be provided when price.quote is GBP".to_string()
            )
        );
    }

    #[test]
    fn to_gbp_case_insensitive_currency() {
        let result = to_gbp(dec!(2), "BTC", Some(&price(dec!(100), "gbp")), None).unwrap();
        assert_eq!(result, dec!(200));
    }

    #[test]
    fn to_gbp_empty_quote_is_error() {
        let err = to_gbp(dec!(1), "BTC", Some(&price(dec!(1000), "")), None).unwrap_err();
        assert_eq!(
            err,
            TaxcError::InvalidCurrency("quote is required and cannot be empty".to_string())
        );
    }

    #[test]
    fn value_gbp_and_fees_gbp() {
        let event = TaxableEvent {
            id: None,
            datetime: NaiveDate::from_ymd_opt(2024, 1, 15)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            event_type: EventType::Acquisition,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(0.5),
            price: Some(price(dec!(40000), "USD")),
            fx_rate: Some(price(dec!(0.79), "GBP")),
            fee_amount: Some(dec!(31.65)),
            fee_asset: Some("USD".to_string()),
            fee_price: Some(price(dec!(1), "USD")),
            fee_fx_rate: Some(price(dec!(0.79), "GBP")),
            description: None,
        };

        assert_eq!(event.value_gbp().unwrap(), dec!(15800));
        assert_eq!(event.fees_gbp().unwrap(), dec!(25.0035));
    }

    #[test]
    fn total_cost_includes_fees() {
        let event = TaxableEvent {
            id: None,
            datetime: NaiveDate::from_ymd_opt(2024, 1, 15)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            event_type: EventType::Acquisition,
            asset: "GBP".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1000),
            price: None,
            fx_rate: None,
            fee_amount: Some(dec!(50)),
            fee_asset: Some("GBP".to_string()),
            fee_price: None,
            fee_fx_rate: None,
            description: None,
        };
        assert_eq!(event.total_cost_gbp().unwrap(), dec!(1050));
    }

    #[test]
    fn total_cost_without_fees() {
        let event = TaxableEvent {
            id: None,
            datetime: NaiveDate::from_ymd_opt(2024, 1, 15)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            event_type: EventType::Acquisition,
            asset: "GBP".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1000),
            price: None,
            fx_rate: None,
            fee_amount: None,
            fee_asset: None,
            fee_price: None,
            fee_fx_rate: None,
            description: None,
        };
        assert_eq!(event.total_cost_gbp().unwrap(), dec!(1000));
    }

    #[test]
    fn parse_csv_round_trip() {
        let csv_data = r#"date,event_type,asset,asset_class,quantity,price_rate,price_quote,price_source,price_time,fx_rate,fx_source,fx_time,fee_amount,fee_asset,fee_price_rate,fee_price_quote,fee_fx_rate,fee_fx_source,description
2024-01-15,Acquisition,BTC,Crypto,0.5,40000,USD,CoinGecko,2024-01-15T10:30:00,0.79,BoE,2024-01-15,31.65,USD,1,USD,0.79,BoE,Coinbase
2024-03-20,Disposal,BTC,Crypto,0.25,30000,GBP,Kraken,2024-03-20,,,,15,GBP,,,,,Sold
2024-04-01,StakingReward,ETH,Crypto,0.01,2500,GBP,Kraken,2024-04-01,,,,,,,,,,Kraken
2024-05-15,Dividend,GBP,Stock,150,,,,,,,,,,,,,,UK dividend"#;

        let events = read_csv(csv_data.as_bytes()).unwrap();
        assert_eq!(events.len(), 4);

        assert_eq!(
            events[0].date(),
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
        );
        assert_eq!(events[0].event_type, EventType::Acquisition);
        assert_eq!(events[0].asset, "BTC");
        assert_eq!(events[0].asset_class, AssetClass::Crypto);
        assert_eq!(events[0].quantity, dec!(0.5));
        assert_eq!(events[0].value_gbp().unwrap(), dec!(15800));
        assert_eq!(events[0].fees_gbp().unwrap(), dec!(25.0035));
        assert_eq!(events[0].description, Some("Coinbase".to_string()));

        assert_eq!(events[1].event_type, EventType::Disposal);
        assert_eq!(events[1].fees_gbp().unwrap(), dec!(15));

        assert_eq!(events[2].event_type, EventType::StakingReward);
        assert!(events[2].event_type.is_income());
        assert_eq!(events[2].fees_gbp().unwrap(), dec!(0));

        assert_eq!(events[3].event_type, EventType::Dividend);
        assert_eq!(events[3].asset_class, AssetClass::Stock);
        assert!(events[3].event_type.is_income());
        assert_eq!(events[3].value_gbp().unwrap(), dec!(150));
    }

    #[test]
    fn parse_json_events() {
        let json_data = r#"{
            "events": [
                {
                    "date": "2024-04-15",
                    "event_type": "Acquisition",
                    "asset": "GBP",
                    "asset_class": "Crypto",
                    "quantity": 1.0
                }
            ]
        }"#;

        let events = read_json(json_data.as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value_gbp().unwrap(), dec!(1.0));
    }

    #[test]
    fn parse_json_events_sorted_by_date() {
        let json_data = r#"{
            "events": [
                {
                    "date": "2024-06-15",
                    "event_type": "Disposal",
                    "asset": "BTC",
                    "asset_class": "Crypto",
                    "quantity": 1.0,
                    "price": {"rate": 60000.0, "quote": "GBP"}
                },
                {
                    "date": "2024-01-15",
                    "event_type": "Acquisition",
                    "asset": "BTC",
                    "asset_class": "Crypto",
                    "quantity": 1.0,
                    "price": {"rate": 50000.0, "quote": "GBP"}
                }
            ]
        }"#;

        let events = read_json(json_data.as_bytes()).unwrap();

        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0].date(),
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
        );
        assert_eq!(
            events[1].date(),
            NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()
        );
    }

    #[test]
    fn parse_csv_with_id_field() {
        let csv_data = r#"id,date,event_type,asset,asset_class,quantity,price_rate,price_quote,price_source,price_time,fx_rate,fx_source,fx_time,fee_amount,fee_asset,fee_price_rate,fee_price_quote,fee_fx_rate,fee_fx_source,description
tx-001,2024-01-15,Acquisition,BTC,Crypto,0.5,30000,GBP,Kraken,2024-01-15,,,,,,,,,,Coinbase
tx-002,2024-03-20,Disposal,BTC,Crypto,0.25,25000,GBP,Kraken,2024-03-20,,,,,,,,,,Sold
,2024-04-01,StakingReward,ETH,Crypto,0.01,2000,GBP,Kraken,2024-04-01,,,,,,,,,,No ID"#;

        let events = read_csv(csv_data.as_bytes()).unwrap();
        assert_eq!(events.len(), 3);

        assert_eq!(events[0].id, Some("tx-001".to_string()));
        assert_eq!(events[0].asset, "BTC");

        assert_eq!(events[1].id, Some("tx-002".to_string()));

        assert!(events[2].id.is_none() || events[2].id.as_deref() == Some(""));
    }

    #[test]
    fn parse_json_with_id_field() {
        let json_data = r#"{
            "events": [
                {
                    "id": "json-001",
                    "date": "2024-04-15",
                    "event_type": "Acquisition",
                    "asset": "BTC",
                    "asset_class": "Crypto",
                    "quantity": 1.0,
                    "price": {"rate": 50000.0, "quote": "GBP"}
                },
                {
                    "date": "2024-05-15",
                    "event_type": "Disposal",
                    "asset": "BTC",
                    "asset_class": "Crypto",
                    "quantity": 0.5,
                    "price": {"rate": 30000.0, "quote": "GBP"}
                }
            ]
        }"#;

        let events = read_json(json_data.as_bytes()).unwrap();
        assert_eq!(events.len(), 2);

        assert_eq!(events[0].id, Some("json-001".to_string()));
        assert_eq!(events[1].id, None);
    }
}
