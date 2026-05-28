//! Temporal parsing and explicit temporal parse policies (spec §10.3).
//!
//! Recognized forms, in priority order:
//! 1. Offset-aware RFC3339/ISO timestamps (converted to a UTC instant).
//! 2. RFC2822 timestamps.
//! 3. Unambiguous year-first naive datetimes.
//! 4. Unambiguous English-month datetimes and dates.
//! 5. Unambiguous year-first dates (lifted to midnight).
//!
//! Anything else remains a string.

use chrono::format::{Item, StrftimeItems};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone};

use crate::value::{DateTimeValue, TemporalPrecision};

/// The result of parsing a temporal string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedTemporal {
    pub value: DateTimeValue,
    /// Whether the source carried an explicit timezone offset.
    pub offset_aware: bool,
}

/// User-declared temporal parse policy for a source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemporalParsePolicy {
    pub columns: Vec<TemporalColumnParse>,
}

impl TemporalParsePolicy {
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    pub fn for_column(&self, name: &str) -> Option<&TemporalColumnParse> {
        self.columns.iter().find(|column| column.column == name)
    }
}

/// One declared temporal parse target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalColumnParse {
    pub column: String,
    pub as_type: TemporalParseType,
    pub formats: Vec<String>,
    pub unit: Option<EpochUnit>,
    pub timezone: TemporalTimezone,
}

/// The declared temporal precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalParseType {
    Date,
    DateTime,
}

/// Numeric epoch unit accepted by explicit parse declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpochUnit {
    Seconds,
    Milliseconds,
    Microseconds,
    Nanoseconds,
}

/// Timezone interpretation for naive explicit datetime formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalTimezone {
    Utc,
    FixedOffset { seconds_east: i32 },
}

impl TemporalTimezone {
    fn offset(self) -> FixedOffset {
        match self {
            TemporalTimezone::Utc => FixedOffset::east_opt(0).expect("UTC offset is valid"),
            TemporalTimezone::FixedOffset { seconds_east } => {
                FixedOffset::east_opt(seconds_east).expect("validated fixed offset")
            }
        }
    }
}

/// Parse a string as a temporal value, or return `None` if it is not temporal.
pub fn parse_temporal(text: &str) -> Option<ParsedTemporal> {
    let text = text.trim();

    // 1. RFC3339 with an offset (e.g. `2020-01-01T12:00:00Z`).
    if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
        return Some(ParsedTemporal {
            value: DateTimeValue::new(dt.naive_utc(), TemporalPrecision::DateTime),
            offset_aware: true,
        });
    }

    for pattern in OFFSET_DATETIME_PATTERNS {
        if let Ok(dt) = DateTime::parse_from_str(text, pattern) {
            return Some(ParsedTemporal {
                value: DateTimeValue::new(dt.naive_utc(), TemporalPrecision::DateTime),
                offset_aware: true,
            });
        }
    }

    if let Ok(dt) = DateTime::parse_from_rfc2822(text) {
        return Some(ParsedTemporal {
            value: DateTimeValue::new(dt.naive_utc(), TemporalPrecision::DateTime),
            offset_aware: true,
        });
    }

    for pattern in NAIVE_DATETIME_PATTERNS {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(text, pattern) {
            return Some(ParsedTemporal {
                value: DateTimeValue::new(ndt, TemporalPrecision::DateTime),
                offset_aware: false,
            });
        }
    }

    for pattern in DATE_PATTERNS {
        if let Ok(date) = NaiveDate::parse_from_str(text, pattern) {
            return Some(date_to_temporal(date));
        }
    }

    None
}

/// Parse using one explicit declaration. Failed parses are represented by
/// `None`; the caller aggregates data warnings.
pub fn parse_temporal_explicit(text: &str, policy: &TemporalColumnParse) -> Option<ParsedTemporal> {
    let text = text.trim();
    if let Some(unit) = policy.unit {
        return parse_epoch(text, unit, policy.as_type);
    }

    for pattern in &policy.formats {
        if let Some(parsed) = parse_with_format(text, pattern, policy) {
            return Some(parsed);
        }
    }
    None
}

/// Validate that a chrono-style format string can be consumed deterministically.
pub fn validate_temporal_format(pattern: &str) -> bool {
    !pattern.is_empty()
        && StrftimeItems::new(pattern).all(|item| !matches!(item, Item::Error))
        && pattern.contains('%')
}

fn parse_with_format(
    text: &str,
    pattern: &str,
    policy: &TemporalColumnParse,
) -> Option<ParsedTemporal> {
    if let Ok(dt) = DateTime::parse_from_str(text, pattern) {
        return Some(coerce_temporal(dt.naive_utc(), policy.as_type, true));
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(text, pattern) {
        let offset = policy.timezone.offset();
        let dt = offset.from_local_datetime(&ndt).single()?;
        return Some(coerce_temporal(dt.naive_utc(), policy.as_type, false));
    }
    if let Ok(date) = NaiveDate::parse_from_str(text, pattern) {
        return Some(match policy.as_type {
            TemporalParseType::Date => date_to_temporal(date),
            TemporalParseType::DateTime => {
                let instant = date.and_hms_opt(0, 0, 0)?;
                ParsedTemporal {
                    value: DateTimeValue::new(instant, TemporalPrecision::DateTime),
                    offset_aware: false,
                }
            }
        });
    }
    None
}

fn parse_epoch(text: &str, unit: EpochUnit, as_type: TemporalParseType) -> Option<ParsedTemporal> {
    let raw = text.parse::<i128>().ok()?;
    let micros = match unit {
        EpochUnit::Seconds => raw.checked_mul(1_000_000)?,
        EpochUnit::Milliseconds => raw.checked_mul(1_000)?,
        EpochUnit::Microseconds => raw,
        EpochUnit::Nanoseconds => raw / 1_000,
    };
    let instant = DateTime::from_timestamp_micros(i64::try_from(micros).ok()?)?.naive_utc();
    Some(coerce_temporal(instant, as_type, true))
}

fn coerce_temporal(
    instant: NaiveDateTime,
    as_type: TemporalParseType,
    offset_aware: bool,
) -> ParsedTemporal {
    match as_type {
        TemporalParseType::Date => {
            let midnight = instant
                .date()
                .and_hms_opt(0, 0, 0)
                .expect("midnight is valid");
            ParsedTemporal {
                value: DateTimeValue::new(midnight, TemporalPrecision::Date),
                offset_aware,
            }
        }
        TemporalParseType::DateTime => ParsedTemporal {
            value: DateTimeValue::new(instant, TemporalPrecision::DateTime),
            offset_aware,
        },
    }
}

fn date_to_temporal(date: NaiveDate) -> ParsedTemporal {
    let instant = date.and_hms_opt(0, 0, 0).expect("midnight is valid");
    ParsedTemporal {
        value: DateTimeValue::new(instant, TemporalPrecision::Date),
        offset_aware: false,
    }
}

const OFFSET_DATETIME_PATTERNS: &[&str] = &[
    "%Y-%m-%dT%H:%M%:z",
    "%Y-%m-%dT%H:%M:%S%:z",
    "%Y-%m-%dT%H:%M:%S%.f%:z",
];

const NAIVE_DATETIME_PATTERNS: &[&str] = &[
    "%Y-%m-%dT%H:%M",
    "%Y-%m-%dT%H:%M:%S",
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%d %H:%M",
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%d %H:%M:%S%.f",
    "%b %e, %Y %H:%M",
    "%B %e, %Y %H:%M",
    "%e %b %Y %H:%M",
    "%e %B %Y %H:%M",
];

const DATE_PATTERNS: &[&str] = &[
    "%Y-%m-%d",
    "%Y/%m/%d",
    "%Y%m%d",
    "%b %e, %Y",
    "%B %e, %Y",
    "%e %b %Y",
    "%e %B %Y",
];
