//! JSON and NDJSON loading (spec §10.2).
//!
//! A JSON source is an array of row objects (`[{"a": 1}, {"a": 2}]`); an NDJSON
//! source is one row object per line. Both produce the same column-oriented
//! [`DataFrame`](crate::DataFrame) shape as CSV: each value is rendered to its
//! canonical text and run through the shared type-inference pipeline (spec
//! §10.3), so inference behaves identically across formats for equivalent data.
//! As a consequence, JSON does not preserve a distinction between the number
//! `1` and the string `"1"` — both infer as an integer, exactly as in CSV.

use std::io::Read;

use serde_json::{Map, Value};

use crate::csv::{build, LoadResult};
use crate::error::DataError;

/// Fully load a JSON array of row objects (spec §10.2, §10.3).
pub fn read_json<R: Read>(mut reader: R) -> Result<LoadResult, DataError> {
    let mut text = String::new();
    reader.read_to_string(&mut text)?;
    read_json_str(&text)
}

/// Load JSON data from a string.
pub fn read_json_str(input: &str) -> Result<LoadResult, DataError> {
    let value: Value = serde_json::from_str(input)?;
    let Value::Array(rows) = value else {
        return Err(DataError::JsonNotArray);
    };
    let mut objects = Vec::with_capacity(rows.len());
    for (index, row) in rows.into_iter().enumerate() {
        match row {
            Value::Object(map) => objects.push(map),
            _ => return Err(DataError::JsonRowNotObject { index }),
        }
    }
    Ok(build_from_objects(objects))
}

/// Fully load NDJSON: one JSON row object per non-blank line (spec §10.2).
pub fn read_ndjson<R: Read>(mut reader: R) -> Result<LoadResult, DataError> {
    let mut text = String::new();
    reader.read_to_string(&mut text)?;
    read_ndjson_str(&text)
}

/// Load NDJSON data from a string.
pub fn read_ndjson_str(input: &str) -> Result<LoadResult, DataError> {
    let mut objects = Vec::new();
    for (offset, raw) in input.lines().enumerate() {
        let line = offset + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value =
            serde_json::from_str(trimmed).map_err(|source| DataError::NdJson { line, source })?;
        match value {
            Value::Object(map) => objects.push(map),
            _ => return Err(DataError::NdJsonRowNotObject { line }),
        }
    }
    Ok(build_from_objects(objects))
}

/// Assemble row objects into a [`LoadResult`]. Columns are discovered in
/// first-seen key order across rows; a key absent from a row is a missing cell.
fn build_from_objects(objects: Vec<Map<String, Value>>) -> LoadResult {
    let mut names: Vec<String> = Vec::new();
    let mut index_of: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut columns: Vec<Vec<String>> = Vec::new();

    for (row, object) in objects.iter().enumerate() {
        // Every known column gets a (default missing) cell for this row.
        for column in &mut columns {
            column.push(String::new());
        }
        for (key, value) in object {
            let index = *index_of.entry(key.clone()).or_insert_with(|| {
                names.push(key.clone());
                // Backfill the rows that preceded this column's first appearance.
                columns.push(vec![String::new(); row + 1]);
                names.len() - 1
            });
            columns[index][row] = json_cell(value);
        }
    }

    build(names, columns)
}

/// Render a JSON value to the canonical text the inference pipeline expects.
/// `null` becomes the empty (missing) string; nested arrays/objects serialize
/// to compact JSON and infer as strings.
fn json_cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
