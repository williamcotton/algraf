//! Diagnostics (spec §12.15, §13.16).
//!
//! A diagnostic is a machine-readable error or warning carrying a stable code,
//! a severity, a message, a primary source span, optional related spans, and
//! optional help text.

use serde::{Deserialize, Serialize};

use crate::span::Span;

/// Diagnostic severity (spec §13.16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Blocks rendering in CLI render mode.
    Error,
    /// Does not block rendering.
    Warning,
    /// Provides guidance.
    Information,
    /// Editor-only suggestion.
    Hint,
}

/// A secondary span attached to a diagnostic for additional context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedSpan {
    pub span: Span,
    pub message: String,
}

/// A machine-readable diagnostic with source span information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Stable diagnostic code, e.g. `"E0012"` (spec §26).
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<RelatedSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl Diagnostic {
    /// Construct a diagnostic with an explicit severity.
    pub fn new(
        severity: Severity,
        code: &'static str,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Diagnostic {
            code,
            severity,
            message: message.into(),
            span,
            related: Vec::new(),
            help: None,
        }
    }

    /// Construct an error diagnostic.
    pub fn error(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Diagnostic::new(Severity::Error, code, message, span)
    }

    /// Construct a warning diagnostic.
    pub fn warning(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Diagnostic::new(Severity::Warning, code, message, span)
    }

    /// Attach help text.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Attach a related span.
    pub fn with_related(mut self, span: Span, message: impl Into<String>) -> Self {
        self.related.push(RelatedSpan {
            span,
            message: message.into(),
        });
        self
    }
}
