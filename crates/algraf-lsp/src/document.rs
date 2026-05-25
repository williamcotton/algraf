use std::io;
use std::path::{Path, PathBuf};

use algraf_core::{codes, Diagnostic as CoreDiagnostic, DiagnosticCode};
use algraf_data::{ColumnDef, DataError, Format};
use algraf_driver::{DriverError, SourceInput};
use algraf_semantics::ChartIr;
use tower_lsp::lsp_types::Url;

/// Cached document state (spec §21.3).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DocumentState {
    pub text: String,
    pub version: i32,
    pub parse: Option<ParseState>,
    pub analysis: Option<AnalysisState>,
    pub primary_schema: Option<Vec<ColumnDef>>,
    pub data_path: Option<PathBuf>,
}

/// Cached parse state (spec §21.3).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ParseState {
    pub diagnostics: Vec<CoreDiagnostic>,
}

/// Cached semantic analysis state (spec §21.3).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AnalysisState {
    pub ir: Option<ChartIr>,
    pub diagnostics: Vec<CoreDiagnostic>,
}

/// Schema cache key: path plus any explicit source-constructor format.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DataSourceKey {
    path: PathBuf,
    format: Option<Format>,
}

impl DataSourceKey {
    pub(crate) fn new(path: PathBuf, format: Option<Format>) -> DataSourceKey {
        DataSourceKey { path, format }
    }
}

/// Cached schema state.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum SchemaState {
    Ready {
        schema: Vec<ColumnDef>,
        /// LSP schema reads are bounded samples; semantic hard errors from
        /// sampled types are avoided by the analyzer's current column-existence
        /// checks and retained here for future type-hint policy.
        provisional: bool,
    },
    Error {
        code: DiagnosticCode,
        message: String,
    },
}

pub(crate) enum SchemaResolution {
    Ready {
        schema: Vec<ColumnDef>,
        path: Option<PathBuf>,
    },
    MissingOrInvalid,
    Unavailable {
        diagnostic: CoreDiagnostic,
    },
}

pub(crate) fn source_input_for_uri(uri: &Url) -> SourceInput {
    SourceInput::Path(
        uri.to_file_path()
            .unwrap_or_else(|_| PathBuf::from("document.ag")),
    )
}

pub(crate) fn schema_error_from_driver(err: &DriverError) -> (DiagnosticCode, String) {
    match err {
        DriverError::Data { path, source, .. } => schema_error(path, source),
        DriverError::Usage(message)
        | DriverError::StdinRead(message)
        | DriverError::StdinParse(message) => (codes::E1006, message.clone()),
    }
}

fn schema_error(path: &Path, err: &DataError) -> (DiagnosticCode, String) {
    match err {
        DataError::Io(io) if io.kind() == io::ErrorKind::NotFound => (
            codes::E1005,
            format!("data file not found: {}", path.display()),
        ),
        DataError::Io(io) => (
            codes::E1006,
            format!("data file could not be read: {}: {io}", path.display()),
        ),
        DataError::Csv(err) => (
            codes::E1006,
            format!("CSV parse error in {}: {err}", path.display()),
        ),
        DataError::MissingHeader => (
            codes::E1007,
            format!("CSV header missing in {}", path.display()),
        ),
        DataError::DuplicateHeader(name) => (
            codes::E1008,
            format!("duplicate CSV column `{name}` in {}", path.display()),
        ),
        DataError::Json(err) => (
            codes::E1009,
            format!("malformed JSON in {}: {err}", path.display()),
        ),
        DataError::NdJson { line, source } => (
            codes::E1009,
            format!(
                "malformed JSON on line {line} in {}: {source}",
                path.display()
            ),
        ),
        DataError::JsonNotArray => (
            codes::E1010,
            format!(
                "JSON data must be an array of row objects in {}",
                path.display()
            ),
        ),
        DataError::JsonRowNotObject { index } => (
            codes::E1010,
            format!("JSON row {index} is not an object in {}", path.display()),
        ),
        DataError::NdJsonRowNotObject { line } => (
            codes::E1010,
            format!("NDJSON line {line} is not an object in {}", path.display()),
        ),
        DataError::Geo(message) => (
            codes::E1805,
            format!("geospatial parse error in {}: {message}", path.display()),
        ),
    }
}
