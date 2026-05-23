//! Data source format selection and dispatch (spec §10.1, §10.2).
//!
//! A source's format is chosen by its file extension. Loading any format yields
//! the same [`DataFrame`](crate::DataFrame) shape, so downstream parser,
//! semantics, and render code stay format-agnostic (spec §10.5).

use std::io::Read;
use std::path::Path;

use crate::csv::{read_csv, read_delimited_schema, read_tsv, LoadResult};
use crate::error::DataError;
use crate::frame::Table;
use crate::json::{read_json, read_ndjson};
use crate::schema::ColumnDef;

/// A supported tabular data format (spec §10.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Comma-separated values; the default for unrecognized extensions.
    Csv,
    /// Tab-separated values (`.tsv`, `.tab`).
    Tsv,
    /// A JSON array of row objects (`.json`).
    Json,
    /// Newline-delimited JSON: one row object per line (`.ndjson`, `.jsonl`).
    NdJson,
}

impl Format {
    /// Select a format from a path's extension. Unrecognized (or absent)
    /// extensions fall back to [`Format::Csv`] (spec §10.2).
    pub fn from_path(path: &Path) -> Format {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(Format::from_extension)
            .unwrap_or(Format::Csv)
    }

    /// Map a file extension (without the dot, any case) to a format.
    pub fn from_extension(ext: &str) -> Format {
        match ext.to_ascii_lowercase().as_str() {
            "tsv" | "tab" => Format::Tsv,
            "json" => Format::Json,
            "ndjson" | "jsonl" => Format::NdJson,
            _ => Format::Csv,
        }
    }
}

/// Fully load a data source from a path, selecting the format by extension
/// (spec §10.2, §10.3).
pub fn read_path(path: &Path) -> Result<LoadResult, DataError> {
    let format = Format::from_path(path);
    let file = std::fs::File::open(path)?;
    read_format(file, format)
}

/// Fully load a data source from a reader using an explicit format.
pub fn read_format<R: Read>(reader: R, format: Format) -> Result<LoadResult, DataError> {
    match format {
        Format::Csv => read_csv(reader),
        Format::Tsv => read_tsv(reader),
        Format::Json => read_json(reader),
        Format::NdJson => read_ndjson(reader),
    }
}

/// Infer a provisional schema from a path, selecting the format by extension.
/// `sample` bounds the rows read for delimited formats; JSON formats parse the
/// whole document (spec §10.3).
pub fn read_schema_path(path: &Path, sample: usize) -> Result<Vec<ColumnDef>, DataError> {
    let format = Format::from_path(path);
    let file = std::fs::File::open(path)?;
    read_schema_format(file, format, sample)
}

/// Infer a provisional schema from a reader using an explicit format.
pub fn read_schema_format<R: Read>(
    reader: R,
    format: Format,
    sample: usize,
) -> Result<Vec<ColumnDef>, DataError> {
    match format {
        Format::Csv => read_delimited_schema(reader, b',', sample),
        Format::Tsv => read_delimited_schema(reader, b'\t', sample),
        Format::Json => Ok(read_json(reader)?.frame.schema().to_vec()),
        Format::NdJson => Ok(read_ndjson(reader)?.frame.schema().to_vec()),
    }
}
