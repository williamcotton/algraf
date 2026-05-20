//! Data values and temporal representation (spec §10.7, §10.3).

use std::cmp::Ordering;

use chrono::NaiveDateTime;

/// Whether a temporal value carries a time component (spec §10.3, §4143).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TemporalPrecision {
    /// A date with no time-of-day (lifted to `00:00:00` for the instant).
    Date,
    /// A full date and time.
    DateTime,
}

/// A temporal value normalized to a UTC-equivalent instant (spec §10.3).
///
/// Offset-aware inputs are converted to UTC; naive inputs are treated as
/// timezone-free instants (never the local timezone). The original precision is
/// preserved so scales can choose date vs datetime formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateTimeValue {
    /// The UTC-equivalent instant used for ordering and scale mapping.
    pub instant: NaiveDateTime,
    /// Whether the source value was date-only or a full datetime.
    pub precision: TemporalPrecision,
}

impl DateTimeValue {
    pub fn new(instant: NaiveDateTime, precision: TemporalPrecision) -> Self {
        DateTimeValue { instant, precision }
    }
}

/// An owned data value (spec §10.7).
///
/// Ordering is total and deterministic so categorical domains sort stably.
/// Floats use [`f64::total_cmp`]; `NaN` is treated as a normal (missing-like)
/// value for ordering rather than producing undefined comparisons.
#[derive(Debug, Clone)]
pub enum DataValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Temporal(DateTimeValue),
    String(String),
}

impl DataValue {
    pub fn is_null(&self) -> bool {
        matches!(self, DataValue::Null)
    }

    /// A stable rank used to order values of differing variants.
    fn rank(&self) -> u8 {
        match self {
            DataValue::Null => 0,
            DataValue::Bool(_) => 1,
            DataValue::Int(_) => 2,
            DataValue::Float(_) => 3,
            DataValue::Temporal(_) => 4,
            DataValue::String(_) => 5,
        }
    }
}

impl PartialEq for DataValue {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for DataValue {}

impl PartialOrd for DataValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DataValue {
    fn cmp(&self, other: &Self) -> Ordering {
        use DataValue::*;
        match (self, other) {
            (Bool(a), Bool(b)) => a.cmp(b),
            (Int(a), Int(b)) => a.cmp(b),
            (Float(a), Float(b)) => a.total_cmp(b),
            (Temporal(a), Temporal(b)) => a.cmp(b),
            (String(a), String(b)) => a.cmp(b),
            _ => self.rank().cmp(&other.rank()),
        }
    }
}

/// A borrowed reference to a data value (spec §10.5).
///
/// String values borrow from the column; scalar values are copied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataValueRef<'a> {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Temporal(DateTimeValue),
    String(&'a str),
}

impl DataValueRef<'_> {
    pub fn is_null(&self) -> bool {
        matches!(self, DataValueRef::Null)
    }

    /// Copy this reference into an owned [`DataValue`].
    pub fn to_owned(&self) -> DataValue {
        match *self {
            DataValueRef::Null => DataValue::Null,
            DataValueRef::Bool(b) => DataValue::Bool(b),
            DataValueRef::Int(i) => DataValue::Int(i),
            DataValueRef::Float(f) => DataValue::Float(f),
            DataValueRef::Temporal(t) => DataValue::Temporal(t),
            DataValueRef::String(s) => DataValue::String(s.to_string()),
        }
    }
}
