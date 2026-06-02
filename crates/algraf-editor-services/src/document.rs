use std::collections::HashMap;
use std::path::PathBuf;

use algraf_core::Diagnostic as CoreDiagnostic;
use algraf_data::{ColumnDef, Format};
use algraf_driver::SourceInput;
use algraf_semantics::ChartIr;
use lsp_types::Url;

/// Cached document state (spec §21.3).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DocumentState {
    pub text: String,
    pub version: i32,
    pub parse: Option<ParseState>,
    pub analysis: Option<AnalysisState>,
    pub primary_schema: Option<Vec<ColumnDef>>,
    pub table_schemas: HashMap<String, Vec<ColumnDef>>,
    pub source_previews: SourcePreviews,
    pub data_path: Option<PathBuf>,
    pub virtual_files: HashMap<String, VirtualFile>,
    pub has_external_schema_sources: bool,
    pub diagnostics: Vec<CoreDiagnostic>,
}

impl DocumentState {
    pub fn diagnostics(&self) -> &[CoreDiagnostic] {
        &self.diagnostics
    }

    pub fn virtual_file_for_path(&self, path: &std::path::Path) -> Option<&VirtualFile> {
        let name = path.file_name().and_then(|name| name.to_str())?;
        self.virtual_files.get(name)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SourcePreviews {
    pub primary: Option<SourcePreview>,
    pub tables: HashMap<String, SourcePreview>,
}

#[derive(Debug, Clone)]
pub struct SourcePreview {
    pub label: String,
    pub schema: Vec<ColumnDef>,
    pub row_headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct VirtualFile {
    pub uri: Url,
    pub text: String,
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

pub enum SchemaResolution {
    Ready {
        schema: Vec<ColumnDef>,
        path: Option<PathBuf>,
        format: Option<Format>,
    },
    CallerInput,
    MissingOrInvalid,
    Unavailable {
        diagnostic: CoreDiagnostic,
    },
}

pub fn source_input_for_uri(uri: &Url) -> SourceInput {
    SourceInput::Path(uri_to_path(uri).unwrap_or_else(|| PathBuf::from("document.ag")))
}

#[cfg(not(target_arch = "wasm32"))]
fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

#[cfg(target_arch = "wasm32")]
fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    if uri.scheme() == "file" {
        let path = percent_decode_path(uri.path());
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    uri.path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|name| !name.is_empty())
        .map(PathBuf::from)
}

#[cfg(target_arch = "wasm32")]
fn percent_decode_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%' && idx + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[idx + 1]), hex(bytes[idx + 2])) {
                out.push((hi << 4) | lo);
                idx += 3;
                continue;
            }
        }
        out.push(bytes[idx]);
        idx += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| path.to_string())
}

#[cfg(target_arch = "wasm32")]
fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
