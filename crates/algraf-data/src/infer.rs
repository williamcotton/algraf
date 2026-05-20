//! Column type inference (spec §10.3).
//!
//! Inference order is boolean, integer, float, temporal, then categorical
//! string. A column whose non-missing values do not all share one type becomes
//! `Mixed` and is stored as raw strings (preferring categorical, spec §10.3).

use crate::error::DataWarning;
use crate::frame::Column;
use crate::schema::{ColumnDef, DataType};
use crate::temporal::{parse_temporal, ParsedTemporal};
use crate::value::DateTimeValue;

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

/// The outcome of inferring one column.
pub struct InferredColumn {
    pub def: ColumnDef,
    pub column: Column,
    pub warnings: Vec<DataWarning>,
}

/// Infer a column's type from its raw string cells and build typed storage.
pub fn infer_column(name: &str, raw: &[String]) -> InferredColumn {
    let cells: Vec<Cell> = raw.iter().map(|s| classify(s)).collect();

    let mut n_bool = 0;
    let mut n_int = 0;
    let mut n_float = 0;
    let mut n_temporal = 0;
    let mut n_string = 0;
    let mut n_missing = 0;
    let mut saw_offset = false;
    let mut saw_naive = false;
    for cell in &cells {
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

    let n_present = cells.len() - n_missing;
    let dtype = decide_type(n_present, n_bool, n_int, n_float, n_temporal, n_string);

    let column = build_column(dtype, &cells, raw);
    let nullable = n_missing > 0;
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

fn build_column(dtype: DataType, cells: &[Cell], raw: &[String]) -> Column {
    match dtype {
        DataType::Boolean => Column::Bool(
            cells
                .iter()
                .map(|c| match c {
                    Cell::Bool(b) => Some(*b),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Integer => Column::Int(
            cells
                .iter()
                .map(|c| match c {
                    Cell::Int(i) => Some(*i),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Float => Column::Float(
            cells
                .iter()
                .map(|c| match c {
                    Cell::Int(i) => Some(*i as f64),
                    Cell::Float(f) => Some(*f),
                    _ => None,
                })
                .collect(),
        ),
        DataType::Temporal => Column::Temporal(
            cells
                .iter()
                .map(|c| match c {
                    Cell::Temporal(t) => Some(t.value),
                    _ => None,
                })
                .collect::<Vec<Option<DateTimeValue>>>(),
        ),
        // String, Mixed, and Unknown preserve the original strings.
        DataType::String | DataType::Mixed | DataType::Unknown => Column::String(
            cells
                .iter()
                .zip(raw)
                .map(|(c, s)| match c {
                    Cell::Missing => None,
                    _ => Some(s.clone()),
                })
                .collect(),
        ),
    }
}
