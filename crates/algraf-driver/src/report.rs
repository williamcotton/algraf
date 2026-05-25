//! Centralized diagnostic mapping and preparation reporting (spec §23.4).
//!
//! Before v0.15, parse diagnostics, driver/data load errors, data inference
//! warnings, semantic diagnostics, and render diagnostics were assembled by
//! hand in each CLI command path and again in the LSP backend. This module is
//! the one place those phases are described, mapped, and ordered, so adapters
//! consume a single assembled report instead of re-deriving the wiring.
//!
//! The report type intentionally depends only on `core`, `data`, and the
//! driver's own error types — never on the CLI or LSP crates (spec §23.3).

use std::path::{Path, PathBuf};

use algraf_core::{codes, Diagnostic, DiagnosticCode, Span};
use algraf_data::{DataError, DataWarning};

use crate::error::{DriverError, LoadContext};

/// The pipeline phase a report entry came from. Phases preserve assembly order
/// and let adapters decide how to surface an entry without re-deriving its
/// origin (spec §23.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportPhase {
    /// Lexer/parser diagnostics (spec §12).
    Parse,
    /// Data and schema loading diagnostics (spec §10.2).
    Load,
    /// Name resolution and validation diagnostics (spec §13).
    Semantic,
    /// Render diagnostics, when a caller supplies them (spec §24).
    Render,
}

/// A data inference warning together with the table/source/column context it
/// was produced in. Data warnings often know only a data-column name, so they
/// are kept as structured buckets rather than forced into a source span
/// (spec §10.3); see the v0.15 design decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataWarningEntry {
    /// Whether the warning came from the primary source or a named table.
    pub context: LoadContext,
    /// The file the data was loaded from, when known.
    pub path: Option<PathBuf>,
    /// The underlying data warning, including its optional column name.
    pub warning: DataWarning,
}

impl DataWarningEntry {
    /// The user-facing warning message.
    pub fn message(&self) -> &str {
        &self.warning.message
    }
}

/// One ordered collection of everything observed while preparing a chart:
/// parse, load, and semantic diagnostics plus optional render diagnostics, all
/// kept in deterministic phase order, alongside structured data warnings.
///
/// Diagnostics that carry a meaningful source span live in `diagnostics`. Data
/// warnings, which generally know only a data-column name, live in
/// `data_warnings` so adapters never have to invent a source span for them.
#[derive(Debug, Default, Clone)]
pub struct PreparationReport {
    diagnostics: Vec<(ReportPhase, Diagnostic)>,
    data_warnings: Vec<DataWarningEntry>,
}

impl PreparationReport {
    /// An empty report.
    pub fn new() -> PreparationReport {
        PreparationReport::default()
    }

    /// Append one diagnostic tagged with its phase.
    pub fn push(&mut self, phase: ReportPhase, diagnostic: Diagnostic) {
        self.diagnostics.push((phase, diagnostic));
    }

    /// Append many diagnostics for a phase, preserving their order.
    pub fn extend<I>(&mut self, phase: ReportPhase, diagnostics: I)
    where
        I: IntoIterator<Item = Diagnostic>,
    {
        self.diagnostics
            .extend(diagnostics.into_iter().map(|d| (phase, d)));
    }

    /// Record a data warning with its load context.
    pub fn push_data_warning(&mut self, entry: DataWarningEntry) {
        self.data_warnings.push(entry);
    }

    /// Record every warning carried by a load, tagging each with its context
    /// and source path.
    pub fn push_data_warnings<'a, I>(
        &mut self,
        context: &LoadContext,
        path: Option<&Path>,
        warnings: I,
    ) where
        I: IntoIterator<Item = &'a DataWarning>,
    {
        for warning in warnings {
            self.data_warnings.push(DataWarningEntry {
                context: context.clone(),
                path: path.map(Path::to_path_buf),
                warning: warning.clone(),
            });
        }
    }

    /// The phase-tagged diagnostics in assembly order.
    pub fn entries(&self) -> &[(ReportPhase, Diagnostic)] {
        &self.diagnostics
    }

    /// All recorded data warnings in assembly order.
    pub fn data_warnings(&self) -> &[DataWarningEntry] {
        &self.data_warnings
    }

    /// Whether any data warning was recorded.
    pub fn has_data_warnings(&self) -> bool {
        !self.data_warnings.is_empty()
    }

    /// A flat list of diagnostics in deterministic phase order, dropping the
    /// phase tags. Adapters that already know how to render a `Diagnostic`
    /// slice (CLI human/JSON output, LSP publication) consume this.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics.iter().map(|(_, d)| d.clone()).collect()
    }
}

/// Map a driver/data loading error to a stable `(code, message)` pair. This is
/// the single place CLI and LSP agree on driver-error wording, so missing-file,
/// unreadable-file, malformed CSV/JSON, and geospatial parse messages stay
/// consistent across adapters (spec §26, codes `E1005`–`E1010`, `E1805`).
pub fn driver_error_code_message(err: &DriverError) -> (DiagnosticCode, String) {
    match err {
        DriverError::Data { path, source, .. } => data_error_code_message(path, source),
        DriverError::Usage(message)
        | DriverError::StdinRead(message)
        | DriverError::StdinParse(message) => (codes::E1006, message.clone()),
    }
}

/// Map a driver error to a [`Diagnostic`] anchored at `span`. Callers that have
/// a meaningful source location (such as the data-source path expression) use
/// this; callers without a span keep the `(code, message)` pair instead.
pub fn driver_error_diagnostic(err: &DriverError, span: Span) -> Diagnostic {
    let (code, message) = driver_error_code_message(err);
    Diagnostic::error(code, message, span)
}

/// Map a data loading error to a stable `(code, message)` pair, naming the
/// file it came from (spec §10.2, §26).
pub fn data_error_code_message(path: &Path, err: &DataError) -> (DiagnosticCode, String) {
    match err {
        DataError::Io(io) if io.kind() == std::io::ErrorKind::NotFound => (
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
