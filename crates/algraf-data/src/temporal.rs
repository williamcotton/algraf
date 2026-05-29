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
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};

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
    /// How per-cell parse failures are surfaced (spec §10.3).
    pub on_error: ParseErrorPolicy,
    /// Anchor date for time-only formats (e.g. `format: "%H:%M"`). A time-only
    /// value carries no date, so a temporal scale needs this anchor to place it
    /// (spec §10.3). `None` leaves time-only formats unparseable.
    pub anchor: Option<NaiveDate>,
}

/// How an explicit temporal `Parse(...)` treats cells that fail to parse
/// (spec §10.3). The default preserves the aggregated-warning behavior of
/// earlier versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParseErrorPolicy {
    /// Coerce failures to missing and emit an aggregated data warning.
    #[default]
    Warn,
    /// Coerce failures to missing and emit no warning.
    Missing,
    /// Treat any per-column parse failure as a blocking error.
    Error,
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

/// Timezone interpretation for naive explicit datetime formats (spec §10.3).
///
/// A timezone only changes how a *naive* declared datetime is resolved to a
/// UTC-equivalent instant; storage stays UTC microseconds and no DST-aware scale
/// arithmetic is introduced. An IANA zone applies its rules (including DST) at
/// the specific local datetime being interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalTimezone {
    Utc,
    FixedOffset {
        seconds_east: i32,
    },
    /// A named IANA zone (e.g. `America/Chicago`).
    Iana(chrono_tz::Tz),
}

impl TemporalTimezone {
    /// Parse a declared `timezone:` string: `"UTC"`, a `±HH:MM` fixed offset, or
    /// an IANA zone name (e.g. `"America/Chicago"`). Returns `None` for an
    /// unrecognized value (spec §10.3).
    pub fn parse_declared(value: &str) -> Option<TemporalTimezone> {
        if value == "UTC" {
            return Some(TemporalTimezone::Utc);
        }
        if let Some(offset) = parse_fixed_offset(value) {
            return Some(offset);
        }
        value
            .parse::<chrono_tz::Tz>()
            .ok()
            .map(TemporalTimezone::Iana)
    }

    /// Resolve a naive local datetime to its UTC-equivalent instant under this
    /// zone. Returns `None` for a local time that is ambiguous or does not exist
    /// (e.g. across a DST transition), so such cells fail to parse deterministically.
    fn local_to_utc(self, ndt: NaiveDateTime) -> Option<NaiveDateTime> {
        match self {
            TemporalTimezone::Utc => Some(ndt),
            TemporalTimezone::FixedOffset { seconds_east } => {
                let offset = FixedOffset::east_opt(seconds_east).expect("validated fixed offset");
                Some(offset.from_local_datetime(&ndt).single()?.naive_utc())
            }
            TemporalTimezone::Iana(tz) => Some(tz.from_local_datetime(&ndt).single()?.naive_utc()),
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

/// Parse the contents of a `datetime("…")` / `date("…")` temporal literal to a
/// UTC-equivalent instant in microseconds (spec §10.3), using the same
/// conservative automatic rules as inference. `require_date` truncates the
/// result to midnight for a `date(...)` constructor. Returns `None` for contents
/// the rules do not recognize.
pub fn parse_temporal_literal(text: &str, require_date: bool) -> Option<i64> {
    let parsed = parse_temporal(text)?;
    let instant = if require_date {
        parsed.value.instant.date().and_hms_opt(0, 0, 0)?
    } else {
        parsed.value.instant
    };
    Some(instant.and_utc().timestamp_micros())
}

/// Parse a `Parse(anchor: "…")` date string to a [`NaiveDate`], using the same
/// conservative rules as inference (spec §10.3). Returns `None` for contents the
/// rules do not recognize as a date.
pub fn parse_anchor_date(text: &str) -> Option<NaiveDate> {
    Some(parse_temporal(text.trim())?.value.instant.date())
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
        let utc = policy.timezone.local_to_utc(ndt)?;
        return Some(coerce_temporal(utc, policy.as_type, false));
    }
    // A time-only format (e.g. `%H:%M`) carries no date, so it parses only when
    // an anchor date is supplied; the time is interpreted in the declared zone
    // on that date (spec §10.3).
    if let (Some(anchor), Ok(time)) = (policy.anchor, NaiveTime::parse_from_str(text, pattern)) {
        let utc = policy.timezone.local_to_utc(anchor.and_time(time))?;
        return Some(coerce_temporal(utc, policy.as_type, false));
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

/// Parse a `±HH:MM` fixed-offset timezone string.
fn parse_fixed_offset(value: &str) -> Option<TemporalTimezone> {
    let sign = match value.as_bytes().first().copied()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (hour, minute) = value.get(1..)?.split_once(':')?;
    let hour: i32 = hour.parse().ok()?;
    let minute: i32 = minute.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(TemporalTimezone::FixedOffset {
        seconds_east: sign * (hour * 3600 + minute * 60),
    })
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
