//! CSV loading and schema inference (spec §10.1–10.3).
//!
//! Headers are required (the first row is the header; headerless CSV is
//! rejected). Full loading infers authoritative column types; schema-only
//! reading samples a bounded number of rows for editor use.

use std::io::Read;
use std::path::Path;

use crate::error::{DataError, DataWarning};
use crate::frame::{DataFrame, Table};
use crate::infer::infer_column_with_policy;
use crate::schema::ColumnDef;
use crate::temporal::TemporalParsePolicy;

/// The default number of rows sampled for provisional schema inference.
pub const DEFAULT_SCHEMA_SAMPLE: usize = 100;

/// A fully loaded dataframe together with inference warnings.
#[derive(Debug, Clone)]
pub struct LoadResult {
    pub frame: DataFrame,
    pub warnings: Vec<DataWarning>,
}

/// Raw header and row cells sampled for editor source previews.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleRows {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

fn reader_from<R: Read>(reader: R, delimiter: u8) -> csv::Reader<R> {
    csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(false)
        .delimiter(delimiter)
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

/// Build a [`LoadResult`] from header names and per-column raw string cells,
/// running each column through the shared type-inference pipeline (spec §10.3).
/// Shared by the CSV/TSV and JSON/NDJSON loaders so inference is identical
/// across formats for equivalent data.
pub(crate) fn build(names: Vec<String>, columns: Vec<Vec<String>>) -> LoadResult {
    build_with_temporal_policy(names, columns, None)
}

pub(crate) fn build_with_temporal_policy(
    names: Vec<String>,
    columns: Vec<Vec<String>>,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> LoadResult {
    let mut schema = Vec::with_capacity(names.len());
    let mut data = Vec::with_capacity(names.len());
    let mut warnings = Vec::new();
    for (name, raw) in names.iter().zip(columns) {
        let inferred = infer_column_with_policy(
            name,
            &raw,
            temporal_policy.and_then(|policy| policy.for_column(name)),
        );
        schema.push(inferred.def);
        data.push(inferred.column);
        warnings.extend(inferred.warnings);
    }
    LoadResult {
        frame: DataFrame::new(schema, data),
        warnings,
    }
}

/// Fully load delimited data with authoritative type inference (spec §10.3),
/// using `delimiter` as the field separator (`,` for CSV, `\t` for TSV).
pub fn read_delimited<R: Read>(reader: R, delimiter: u8) -> Result<LoadResult, DataError> {
    read_delimited_with_temporal_policy(reader, delimiter, None)
}

pub fn read_delimited_with_temporal_policy<R: Read>(
    reader: R,
    delimiter: u8,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DataError> {
    let mut reader = reader_from(reader, delimiter);
    let names = headers(&mut reader)?;
    let columns = read_columns(&mut reader, names.len(), None)?;
    Ok(build_with_temporal_policy(names, columns, temporal_policy))
}

/// Fully load CSV data with authoritative type inference (spec §10.3).
pub fn read_csv<R: Read>(reader: R) -> Result<LoadResult, DataError> {
    read_delimited(reader, b',')
}

pub fn read_csv_with_temporal_policy<R: Read>(
    reader: R,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DataError> {
    read_delimited_with_temporal_policy(reader, b',', temporal_policy)
}

/// Fully load tab-separated data (TSV) with authoritative type inference
/// (spec §10.2, §10.3). TSV is CSV with a tab field separator.
pub fn read_tsv<R: Read>(reader: R) -> Result<LoadResult, DataError> {
    read_delimited(reader, b'\t')
}

pub fn read_tsv_with_temporal_policy<R: Read>(
    reader: R,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DataError> {
    read_delimited_with_temporal_policy(reader, b'\t', temporal_policy)
}

/// Load CSV data from a string.
pub fn read_csv_str(input: &str) -> Result<LoadResult, DataError> {
    read_csv(input.as_bytes())
}

pub fn read_csv_str_with_temporal_policy(
    input: &str,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<LoadResult, DataError> {
    read_csv_with_temporal_policy(input.as_bytes(), temporal_policy)
}

/// Load TSV data from a string.
pub fn read_tsv_str(input: &str) -> Result<LoadResult, DataError> {
    read_tsv(input.as_bytes())
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
    read_delimited_schema(reader, b',', sample)
}

/// Infer a provisional schema from delimited data, sampling up to `sample`
/// data rows (spec §10.3).
pub fn read_delimited_schema<R: Read>(
    reader: R,
    delimiter: u8,
    sample: usize,
) -> Result<Vec<ColumnDef>, DataError> {
    read_delimited_schema_with_temporal_policy(reader, delimiter, sample, None)
}

pub fn read_delimited_schema_with_temporal_policy<R: Read>(
    reader: R,
    delimiter: u8,
    sample: usize,
    temporal_policy: Option<&TemporalParsePolicy>,
) -> Result<Vec<ColumnDef>, DataError> {
    let mut reader = reader_from(reader, delimiter);
    let names = headers(&mut reader)?;
    let columns = read_columns(&mut reader, names.len(), Some(sample))?;
    Ok(build_with_temporal_policy(names, columns, temporal_policy)
        .frame
        .schema()
        .to_vec())
}

/// Infer a provisional schema from a string.
pub fn read_csv_schema_str(input: &str, sample: usize) -> Result<Vec<ColumnDef>, DataError> {
    read_csv_schema(input.as_bytes(), sample)
}

/// Sample raw CSV rows for editor previews. Cells are returned before type
/// inference so hover can show the source text authors expect to inspect.
pub fn read_csv_sample_rows<R: Read>(reader: R, sample: usize) -> Result<SampleRows, DataError> {
    read_delimited_sample_rows(reader, b',', sample)
}

/// Sample raw delimited rows for editor previews.
pub fn read_delimited_sample_rows<R: Read>(
    reader: R,
    delimiter: u8,
    sample: usize,
) -> Result<SampleRows, DataError> {
    let mut reader = reader_from(reader, delimiter);
    let headers = headers(&mut reader)?;
    let mut rows = Vec::new();
    for (read, record) in reader.records().enumerate() {
        if read >= sample {
            break;
        }
        let record = record?;
        let mut row = Vec::with_capacity(headers.len());
        for i in 0..headers.len() {
            row.push(record.get(i).unwrap_or("").to_string());
        }
        rows.push(row);
    }
    Ok(SampleRows { headers, rows })
}
