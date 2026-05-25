use std::path::PathBuf;

use algraf_core::{Diagnostic as CoreDiagnostic, DiagnosticCode};
use algraf_data::{ColumnDef, Format};
use algraf_driver::{driver_error_code_message, DriverError, SourceInput};
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

/// Map a driver error to the LSP `(code, message)` pair. The actual mapping
/// lives in `algraf-driver` so the CLI and LSP agree on driver-error wording
/// (spec §23.4); this wrapper keeps the call sites in this crate stable.
pub(crate) fn schema_error_from_driver(err: &DriverError) -> (DiagnosticCode, String) {
    driver_error_code_message(err)
}
