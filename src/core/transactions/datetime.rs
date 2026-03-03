use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Deserializer};

use super::error::TransactionError;

pub(super) fn parse_datetime(s: &str) -> Result<DateTime<FixedOffset>, TransactionError> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(dt.and_utc().fixed_offset());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(dt.and_utc().fixed_offset());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Ok(dt.and_utc().fixed_offset());
    }
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(date.and_hms_opt(0, 0, 0).unwrap().and_utc().fixed_offset());
    }
    Err(TransactionError::InvalidDatetime(s.to_string()))
}

pub(super) fn deserialize_datetime<'de, D>(
    deserializer: D,
) -> Result<DateTime<FixedOffset>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    parse_datetime(&s).map_err(|err| serde::de::Error::custom(err.to_string()))
}
