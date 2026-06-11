//! Column type inference (spec §10.3).
//!
//! Inference order is boolean, integer, float, temporal, then categorical
//! string. A column whose non-missing values do not all share one type becomes
//! `Mixed` and is stored as raw strings (preferring categorical, spec §10.3).

use crate::error::DataWarning;
use crate::frame::Column;
use crate::schema::{ColumnDef, DataType};
use crate::temporal::{
    parse_temporal, parse_temporal_explicit, ParseErrorPolicy, ParsedTemporal, TemporalColumnParse,
};

/// Tokens treated as missing in any column (spec §10.3).
const MISSING_TOKENS: &[&str] = &["", "NA", "N/A", "NaN", "null", "NULL"];

fn is_missing(text: &str) -> bool {
    MISSING_TOKENS.contains(&text)
}

fn parse_bool(text: &str) -> Option<bool> {
    if text.eq_ignore_ascii_case("true") {
        Some(true)
    } else if text.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

/// A single classified cell.
enum Cell {
    Missing,
    Bool(bool),
    Int(i64),
    Float(f64),
    Temporal(ParsedTemporal),
    Str,
}

fn classify(text: &str) -> Cell {
    if is_missing(text) {
        return Cell::Missing;
    }
    if let Some(b) = parse_bool(text) {
        return Cell::Bool(b);
    }
    if let Ok(i) = text.parse::<i64>() {
        return Cell::Int(i);
    }
    if let Ok(f) = text.parse::<f64>() {
        if f.is_finite() {
            return Cell::Float(f);
        }
    }
    if let Some(t) = parse_temporal(text) {
        return Cell::Temporal(t);
    }
    Cell::Str
}

fn classify_explicit(text: &str, policy: &TemporalColumnParse) -> Cell {
    if is_missing(text) {
        return Cell::Missing;
    }
    parse_temporal_explicit(text, policy).map_or(Cell::Str, Cell::Temporal)
}

/// The outcome of inferring one column.
pub struct InferredColumn {
    pub def: ColumnDef,
    pub column: Column,
    pub warnings: Vec<DataWarning>,
}

/// Infer a column's type from its raw string cells and build typed storage.
pub fn infer_column(name: &str, raw: &[String]) -> InferredColumn {
    infer_column_with_policy(name, raw, None)
}

/// Infer a column's type, optionally forcing a declared temporal parse policy.
pub fn infer_column_with_policy(
    name: &str,
    raw: &[String],
    temporal_policy: Option<&TemporalColumnParse>,
) -> InferredColumn {
    let mut n_bool = 0;
    let mut n_int = 0;
    let mut n_float = 0;
    let mut n_temporal = 0;
    let mut n_string = 0;
    let mut n_missing = 0;
    let mut saw_offset = false;
    let mut saw_naive = false;
    for text in raw {
        let cell = match temporal_policy {
            Some(policy) => classify_explicit(text, policy),
            None => classify(text),
        };
        match cell {
            Cell::Missing => n_missing += 1,
            Cell::Bool(_) => n_bool += 1,
            Cell::Int(_) => n_int += 1,
            Cell::Float(_) => n_float += 1,
            Cell::Temporal(t) => {
                n_temporal += 1;
                if t.offset_aware {
                    saw_offset = true;
                } else {
                    saw_naive = true;
                }
            }
            Cell::Str => n_string += 1,
        }
    }

    let n_present = raw.len() - n_missing;
    let mut dtype = if temporal_policy.is_some() {
        DataType::Temporal
    } else {
        decide_type(n_present, n_bool, n_int, n_float, n_temporal, n_string)
    };
    // A mostly temporal column does not degrade to a categorical axis because
    // of a few malformed cells: at most 10% stragglers infer as Temporal,
    // coerce to missing, and warn (spec §10.3, v0.75). Mixtures involving
    // numeric or boolean cells keep their existing classification.
    let mostly_temporal = dtype == DataType::Mixed
        && temporal_policy.is_none()
        && n_temporal + n_string == n_present
        && n_temporal > n_string
        && n_string * 10 <= n_present;
    if mostly_temporal {
        dtype = DataType::Temporal;
    }

    let column = build_column(dtype, raw, temporal_policy);
    let nullable = n_missing > 0 || (temporal_policy.is_some() && n_string > 0);
    let examples = raw
        .iter()
        .filter(|s| !is_missing(s))
        .take(3)
        .cloned()
        .collect();

    let mut warnings = Vec::new();
    if dtype == DataType::Temporal && saw_offset && saw_naive {
        warnings.push(DataWarning::for_column(
            name,
            "column mixes naive and offset-aware datetime values",
        ));
    }
    // Per-cell parse failures are surfaced according to the column's declared
    // `onError` policy (spec §10.3): `Warn` (default) emits an aggregated data
    // warning, `Missing` stays silent (failures already coerce to missing), and
    // `Error` marks the warning fatal so the driver blocks rendering.
    let on_error = temporal_policy.map(|p| p.on_error).unwrap_or_default();
    let make_warning = |name: &str, message: String| match on_error {
        ParseErrorPolicy::Error => Some(DataWarning::fatal_for_column(name, message)),
        ParseErrorPolicy::Warn => Some(DataWarning::for_column(name, message)),
        ParseErrorPolicy::Missing => None,
    };
    if temporal_policy.is_some() && n_string > 0 {
        let examples = raw
            .iter()
            .filter(|s| {
                !is_missing(s)
                    && matches!(classify_explicit(s, temporal_policy.unwrap()), Cell::Str)
            })
            .take(3)
            .cloned()
            .collect::<Vec<_>>();
        if let Some(warning) = make_warning(
            name,
            format!(
                "{} non-missing value(s) failed explicit temporal parsing{}",
                n_string,
                if examples.is_empty() {
                    String::new()
                } else {
                    format!("; examples: {}", examples.join(", "))
                }
            ),
        ) {
            warnings.push(warning);
        }
    }
    if temporal_policy.is_some() && n_present > 0 && n_temporal == 0 {
        if let Some(warning) = make_warning(
            name,
            "all non-missing values failed explicit temporal parsing".into(),
        ) {
            warnings.push(warning);
        }
    }
    if mostly_temporal {
        let examples = raw
            .iter()
            .filter(|s| !is_missing(s) && matches!(classify(s.as_str()), Cell::Str))
            .take(3)
            .cloned()
            .collect::<Vec<_>>();
        warnings.push(DataWarning::for_column(
            name,
            format!(
                "{} non-missing value(s) in a mostly temporal column failed temporal parsing and were treated as missing{}",
                n_string,
                if examples.is_empty() {
                    String::new()
                } else {
                    format!("; examples: {}", examples.join(", "))
                }
            ),
        ));
    }

    InferredColumn {
        def: ColumnDef {
            name: name.to_string(),
            dtype,
            nullable,
            examples,
        },
        column,
        warnings,
    }
}

fn decide_type(
    n_present: usize,
    n_bool: usize,
    n_int: usize,
    n_float: usize,
    n_temporal: usize,
    n_string: usize,
) -> DataType {
    if n_present == 0 {
        return DataType::Unknown;
    }
    if n_bool == n_present {
        DataType::Boolean
    } else if n_int == n_present {
        DataType::Integer
    } else if n_int + n_float == n_present {
        // Numeric column with at least one non-integer value.
        DataType::Float
    } else if n_temporal == n_present {
        DataType::Temporal
    } else if n_string == n_present {
        DataType::String
    } else {
        DataType::Mixed
    }
}

fn build_column(
    dtype: DataType,
    raw: &[String],
    temporal_policy: Option<&TemporalColumnParse>,
) -> Column {
    match dtype {
        DataType::Boolean => Column::from_bool_options(
            raw.iter()
                .map(|text| match classify_for_build(text, temporal_policy) {
                    Cell::Bool(b) => Some(b),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Integer => Column::from_int_options(
            raw.iter()
                .map(|text| match classify_for_build(text, temporal_policy) {
                    Cell::Int(i) => Some(i),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Float => Column::from_float_options(
            raw.iter()
                .map(|text| match classify_for_build(text, temporal_policy) {
                    Cell::Int(i) => Some(i as f64),
                    Cell::Float(f) => Some(f),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Temporal => Column::from_temporal_options(
            raw.iter()
                .map(|text| match classify_for_build(text, temporal_policy) {
                    Cell::Temporal(t) => Some(t.value),
                    _ => None,
                })
                .collect(),
        ),
        // Geometry is never inferred from text cells: it is produced directly
        // by the GeoJson/Shapefile loaders (spec §10.11), not this pipeline.
        DataType::Geometry => unreachable!("geometry columns are not inferred from text"),
        // String, Mixed, and Unknown preserve the original strings.
        DataType::String | DataType::Mixed | DataType::Unknown => Column::String(
            raw.iter()
                .map(|s| match classify_for_build(s, temporal_policy) {
                    Cell::Missing => None,
                    _ => Some(s.clone()),
                })
                .collect(),
        ),
    }
}

fn classify_for_build(text: &str, temporal_policy: Option<&TemporalColumnParse>) -> Cell {
    match temporal_policy {
        Some(policy) => classify_explicit(text, policy),
        None => classify(text),
    }
}
