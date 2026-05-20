//! CSV loading and schema inference (spec §10.1–10.3).
//!
//! Headers are required (the first row is the header; headerless CSV is
//! rejected). Full loading infers authoritative column types; schema-only
//! reading samples a bounded number of rows for editor use.

use std::io::Read;
use std::path::Path;

use crate::error::{DataError, DataWarning};
use crate::frame::{DataFrame, Table};
use crate::infer::infer_column;
use crate::schema::ColumnDef;

/// The default number of rows sampled for provisional schema inference.
pub const DEFAULT_SCHEMA_SAMPLE: usize = 100;

/// A fully loaded dataframe together with inference warnings.
#[derive(Debug, Clone)]
pub struct LoadResult {
    pub frame: DataFrame,
    pub warnings: Vec<DataWarning>,
}

fn reader_from<R: Read>(reader: R) -> csv::Reader<R> {
    csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(false)
        .from_reader(reader)
}

/// Validate and return the header names.
fn headers<R: Read>(reader: &mut csv::Reader<R>) -> Result<Vec<String>, DataError> {
    let header = reader.headers()?;
    if header.is_empty() {
        return Err(DataError::MissingHeader);
    }
    let names: Vec<String> = header.iter().map(|s| s.to_string()).collect();

    let mut seen = std::collections::HashSet::new();
    for name in &names {
        if !seen.insert(name.clone()) {
            return Err(DataError::DuplicateHeader(name.clone()));
        }
    }
    Ok(names)
}

/// Read every row, returning per-column raw string cells (in column order).
fn read_columns<R: Read>(
    reader: &mut csv::Reader<R>,
    column_count: usize,
    limit: Option<usize>,
) -> Result<Vec<Vec<String>>, DataError> {
    let mut columns: Vec<Vec<String>> = vec![Vec::new(); column_count];
    for (read, record) in reader.records().enumerate() {
        if limit.is_some_and(|max| read >= max) {
            break;
        }
        let record = record?;
        for (i, column) in columns.iter_mut().enumerate() {
            column.push(record.get(i).unwrap_or("").to_string());
        }
    }
    Ok(columns)
}

fn build(names: Vec<String>, columns: Vec<Vec<String>>) -> LoadResult {
    let mut schema = Vec::with_capacity(names.len());
    let mut data = Vec::with_capacity(names.len());
    let mut warnings = Vec::new();
    for (name, raw) in names.iter().zip(columns) {
        let inferred = infer_column(name, &raw);
        schema.push(inferred.def);
        data.push(inferred.column);
        warnings.extend(inferred.warnings);
    }
    LoadResult {
        frame: DataFrame::new(schema, data),
        warnings,
    }
}

/// Fully load CSV data with authoritative type inference (spec §10.3).
pub fn read_csv<R: Read>(reader: R) -> Result<LoadResult, DataError> {
    let mut reader = reader_from(reader);
    let names = headers(&mut reader)?;
    let columns = read_columns(&mut reader, names.len(), None)?;
    Ok(build(names, columns))
}

/// Load CSV data from a string.
pub fn read_csv_str(input: &str) -> Result<LoadResult, DataError> {
    read_csv(input.as_bytes())
}

/// Load CSV data from a filesystem path.
pub fn read_csv_path(path: &Path) -> Result<LoadResult, DataError> {
    let file = std::fs::File::open(path)?;
    read_csv(file)
}

/// Infer a provisional schema by sampling up to `sample` data rows (spec §10.3).
///
/// Suitable for editor completion and hover, where reading the full file on the
/// hot path is undesirable. Types are provisional; the caller marks them so.
pub fn read_csv_schema<R: Read>(reader: R, sample: usize) -> Result<Vec<ColumnDef>, DataError> {
    let mut reader = reader_from(reader);
    let names = headers(&mut reader)?;
    let columns = read_columns(&mut reader, names.len(), Some(sample))?;
    Ok(build(names, columns).frame.schema().to_vec())
}

/// Infer a provisional schema from a string.
pub fn read_csv_schema_str(input: &str, sample: usize) -> Result<Vec<ColumnDef>, DataError> {
    read_csv_schema(input.as_bytes(), sample)
}
