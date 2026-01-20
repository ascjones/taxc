use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::io::Read;

/// Type of taxable event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    Acquisition,
    Disposal,
    StakingReward,
    Dividend,
}

impl EventType {
    #[cfg(test)]
    pub fn is_income(&self) -> bool {
        matches!(self, EventType::StakingReward | EventType::Dividend)
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
    pub date: NaiveDate,
    pub event_type: EventType,
    pub asset: String,
    pub asset_class: AssetClass,
    pub quantity: Decimal,
    pub value_gbp: Decimal,
    pub fees_gbp: Option<Decimal>,
    pub description: Option<String>,
}

impl TaxableEvent {
    pub fn total_cost_gbp(&self) -> Decimal {
        self.value_gbp + self.fees_gbp.unwrap_or(Decimal::ZERO)
    }
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
        let date = NaiveDate::parse_from_str(&record.date, "%Y-%m-%d")
            .unwrap_or_else(|_| panic!("Invalid date format: {}", record.date));

        let event_type = match record.event_type.as_str() {
            "Acquisition" => EventType::Acquisition,
            "Disposal" => EventType::Disposal,
            "StakingReward" => EventType::StakingReward,
            "Dividend" => EventType::Dividend,
            _ => panic!("Invalid event type: {}", record.event_type),
        };

        let asset_class = match record.asset_class.as_str() {
            "Crypto" => AssetClass::Crypto,
            "Stock" => AssetClass::Stock,
            _ => panic!("Invalid asset class: {}", record.asset_class),
        };

        TaxableEvent {
            date,
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
        }
        .to_string();

        let asset_class = match event.asset_class {
            AssetClass::Crypto => "Crypto",
            AssetClass::Stock => "Stock",
        }
        .to_string();

        TaxableEventRecord {
            date: event.date.format("%Y-%m-%d").to_string(),
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
    events.sort_by_key(|e| e.date);
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
            events[0].date,
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
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
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
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
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
}
