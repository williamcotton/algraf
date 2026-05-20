//! Temporal parsing for version 0.1 (spec §10.3).
//!
//! Recognized forms, in priority order:
//! 1. RFC3339 timestamps with an offset (converted to a UTC instant).
//! 2. Naive datetimes `YYYY-MM-DDTHH:MM:SS` (no offset).
//! 3. Naive datetimes with a space separator `YYYY-MM-DD HH:MM:SS`.
//! 4. ISO dates `YYYY-MM-DD` (lifted to midnight).
//!
//! Anything else remains a string.

use chrono::{DateTime, NaiveDate, NaiveDateTime};

use crate::value::{DateTimeValue, TemporalPrecision};

/// The result of parsing a temporal string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedTemporal {
    pub value: DateTimeValue,
    /// Whether the source carried an explicit timezone offset.
    pub offset_aware: bool,
}

/// Parse a string as a temporal value, or return `None` if it is not temporal.
pub fn parse_temporal(text: &str) -> Option<ParsedTemporal> {
    // 1. RFC3339 with an offset (e.g. `2020-01-01T12:00:00Z`).
    if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
        return Some(ParsedTemporal {
            value: DateTimeValue::new(dt.naive_utc(), TemporalPrecision::DateTime),
            offset_aware: true,
        });
    }

    // 2. Naive datetime with a `T` separator.
    if let Ok(ndt) = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S") {
        return Some(ParsedTemporal {
            value: DateTimeValue::new(ndt, TemporalPrecision::DateTime),
            offset_aware: false,
        });
    }

    // 3. Naive datetime with a space separator.
    if let Ok(ndt) = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S") {
        return Some(ParsedTemporal {
            value: DateTimeValue::new(ndt, TemporalPrecision::DateTime),
            offset_aware: false,
        });
    }

    // 4. Date only, lifted to midnight.
    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        let instant = date.and_hms_opt(0, 0, 0).expect("midnight is valid");
        return Some(ParsedTemporal {
            value: DateTimeValue::new(instant, TemporalPrecision::Date),
            offset_aware: false,
        });
    }

    None
}
