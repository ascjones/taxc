use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::io::Read;

/// Unified JSON input format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxInput {
    #[serde(default)]
    pub tax_year: Option<String>,
    pub events: Vec<TaxableEventRecord>,
}

/// Type of taxable event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Check if this is an unclassified event type
    #[allow(dead_code)]
    pub fn is_unclassified(&self) -> bool {
        matches!(self, EventType::UnclassifiedIn | EventType::UnclassifiedOut)
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetClass {
    Crypto,
    Stock,
}

/// A taxable event (acquisition, disposal, or income)
#[derive(Debug, Clone)]
pub struct TaxableEvent {
    pub datetime: NaiveDateTime,
    pub event_type: EventType,
    pub asset: String,
    pub asset_class: AssetClass,
    pub quantity: Decimal,
    pub value_gbp: Decimal,
    pub fees_gbp: Option<Decimal>,
    pub description: Option<String>,
}

impl TaxableEvent {
    /// Get just the date portion for tax calculations
    pub fn date(&self) -> NaiveDate {
        self.datetime.date()
    }
}

impl TaxableEvent {
    pub fn total_cost_gbp(&self) -> Decimal {
        self.value_gbp + self.fees_gbp.unwrap_or(Decimal::ZERO)
    }
}

/// Parse a date string that may be date-only or datetime format
fn parse_datetime(s: &str) -> NaiveDateTime {
    // Try datetime format first: "2024-01-15T10:30:00" or "2024-01-15 10:30:00"
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return dt;
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return dt;
    }
    // Try with milliseconds
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return dt;
    }
    // Fall back to date-only format, defaulting to midnight
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    }
    panic!("Invalid date/datetime format: {}", s);
}

/// CSV record format for taxable events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxableEventRecord {
    pub date: String,
    pub event_type: String,
    pub asset: String,
    pub asset_class: String,
    pub quantity: Decimal,
    pub value_gbp: Decimal,
    #[serde(default)]
    pub fees_gbp: Option<Decimal>,
    #[serde(default)]
    pub description: Option<String>,
}

impl From<TaxableEventRecord> for TaxableEvent {
    fn from(record: TaxableEventRecord) -> Self {
        let datetime = parse_datetime(&record.date);

        let event_type = match record.event_type.as_str() {
            "Acquisition" => EventType::Acquisition,
            "Disposal" => EventType::Disposal,
            "StakingReward" => EventType::StakingReward,
            "Dividend" => EventType::Dividend,
            "UnclassifiedIn" => EventType::UnclassifiedIn,
            "UnclassifiedOut" => EventType::UnclassifiedOut,
            _ => panic!("Invalid event type: {}", record.event_type),
        };

        let asset_class = match record.asset_class.as_str() {
            "Crypto" => AssetClass::Crypto,
            "Stock" => AssetClass::Stock,
            _ => panic!("Invalid asset class: {}", record.asset_class),
        };

        TaxableEvent {
            datetime,
            event_type,
            asset: record.asset,
            asset_class,
            quantity: record.quantity,
            value_gbp: record.value_gbp,
            fees_gbp: record.fees_gbp,
            description: record.description,
        }
    }
}

impl From<&TaxableEvent> for TaxableEventRecord {
    fn from(event: &TaxableEvent) -> Self {
        let event_type = match event.event_type {
            EventType::Acquisition => "Acquisition",
            EventType::Disposal => "Disposal",
            EventType::StakingReward => "StakingReward",
            EventType::Dividend => "Dividend",
            EventType::UnclassifiedIn => "UnclassifiedIn",
            EventType::UnclassifiedOut => "UnclassifiedOut",
        }
        .to_string();

        let asset_class = match event.asset_class {
            AssetClass::Crypto => "Crypto",
            AssetClass::Stock => "Stock",
        }
        .to_string();

        TaxableEventRecord {
            date: event.datetime.format("%Y-%m-%dT%H:%M:%S").to_string(),
            event_type,
            asset: event.asset.clone(),
            asset_class,
            quantity: event.quantity,
            value_gbp: event.value_gbp,
            fees_gbp: event.fees_gbp,
            description: event.description.clone(),
        }
    }
}

/// Read taxable events from CSV
pub fn read_csv<R: Read>(reader: R) -> color_eyre::Result<Vec<TaxableEvent>> {
    let mut rdr = csv::Reader::from_reader(reader);
    let records: Result<Vec<TaxableEventRecord>, _> =
        rdr.deserialize::<TaxableEventRecord>().collect();
    let mut events: Vec<TaxableEvent> = records?.into_iter().map(Into::into).collect();
    events.sort_by_key(|e| e.datetime);
    Ok(events)
}

/// Read taxable events from JSON
pub fn read_json<R: Read>(reader: R) -> color_eyre::Result<Vec<TaxableEvent>> {
    let input: TaxInput = serde_json::from_reader(reader)?;
    let mut events: Vec<TaxableEvent> = input.events.into_iter().map(Into::into).collect();
    events.sort_by_key(|e| e.datetime);
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn parse_csv_round_trip() {
        let csv_data = r#"date,event_type,asset,asset_class,quantity,value_gbp,fees_gbp,description
2024-01-15,Acquisition,BTC,Crypto,0.5,15000.00,25.00,Coinbase
2024-03-20,Disposal,BTC,Crypto,0.25,10000.00,15.00,
2024-04-01,StakingReward,ETH,Crypto,0.01,25.00,,Kraken
2024-05-15,Dividend,AAPL,Stock,100,150.00,,Hargreaves"#;

        let events = read_csv(csv_data.as_bytes()).unwrap();
        assert_eq!(events.len(), 4);

        // Check first event (Acquisition)
        assert_eq!(
            events[0].date(),
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
        );
        assert_eq!(events[0].event_type, EventType::Acquisition);
        assert_eq!(events[0].asset, "BTC");
        assert_eq!(events[0].asset_class, AssetClass::Crypto);
        assert_eq!(events[0].quantity, dec!(0.5));
        assert_eq!(events[0].value_gbp, dec!(15000.00));
        assert_eq!(events[0].fees_gbp, Some(dec!(25.00)));
        assert_eq!(events[0].description, Some("Coinbase".to_string()));

        // Check disposal
        assert_eq!(events[1].event_type, EventType::Disposal);
        assert_eq!(events[1].fees_gbp, Some(dec!(15.00)));

        // Check staking reward (income)
        assert_eq!(events[2].event_type, EventType::StakingReward);
        assert!(events[2].event_type.is_income());
        assert_eq!(events[2].fees_gbp, None);

        // Check dividend
        assert_eq!(events[3].event_type, EventType::Dividend);
        assert_eq!(events[3].asset_class, AssetClass::Stock);
        assert!(events[3].event_type.is_income());
    }

    #[test]
    fn total_cost_includes_fees() {
        let event = TaxableEvent {
            datetime: NaiveDate::from_ymd_opt(2024, 1, 15)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            event_type: EventType::Acquisition,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1),
            value_gbp: dec!(1000),
            fees_gbp: Some(dec!(50)),
            description: None,
        };
        assert_eq!(event.total_cost_gbp(), dec!(1050));
    }

    #[test]
    fn total_cost_without_fees() {
        let event = TaxableEvent {
            datetime: NaiveDate::from_ymd_opt(2024, 1, 15)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            event_type: EventType::Acquisition,
            asset: "BTC".to_string(),
            asset_class: AssetClass::Crypto,
            quantity: dec!(1),
            value_gbp: dec!(1000),
            fees_gbp: None,
            description: None,
        };
        assert_eq!(event.total_cost_gbp(), dec!(1000));
    }

    #[test]
    fn parse_json_events() {
        let json_data = r#"{
            "events": [
                {
                    "date": "2024-04-15",
                    "event_type": "Acquisition",
                    "asset": "BTC",
                    "asset_class": "Crypto",
                    "quantity": 1.0,
                    "value_gbp": 50000.00
                }
            ]
        }"#;

        let events = read_json(json_data.as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
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
                    "value_gbp": 60000.00
                },
                {
                    "date": "2024-01-15",
                    "event_type": "Acquisition",
                    "asset": "BTC",
                    "asset_class": "Crypto",
                    "quantity": 1.0,
                    "value_gbp": 50000.00
                }
            ]
        }"#;

        let events = read_json(json_data.as_bytes()).unwrap();

        assert_eq!(events.len(), 2);
        // Events should be sorted by date
        assert_eq!(
            events[0].date(),
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
        );
        assert_eq!(
            events[1].date(),
            NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()
        );
    }
}
