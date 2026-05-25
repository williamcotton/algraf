use std::path::PathBuf;

use algraf_core::Diagnostic as CoreDiagnostic;
use algraf_data::ColumnDef;
use algraf_driver::SourceInput;
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
