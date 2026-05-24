//! Shared argument helpers: duplicate-argument detection and typed value
//! extraction (spec §13.9). These collapse the repeated `seen` maps and literal
//! match arms that every declaration parser would otherwise carry, while
//! keeping each call site's diagnostic code, message, and span explicit.

use std::collections::HashMap;

use algraf_core::{Diagnostic, Span};
use algraf_syntax::ast::{AlgebraExpr, Arg, LiteralKind, ValueExpr};
use algraf_syntax::{node_span, unescape_string_literal as string_value};

use super::context::Analyzer;
use crate::ir::AxisSelectorIr;

/// Tracks the keys already seen within one argument list and reports a
/// duplicate with the appropriate code and "first …" related note. Replaces the
/// ad-hoc `seen: HashMap<String, Span>` blocks across the declaration parsers.
pub(super) struct DupGuard {
    seen: HashMap<String, Span>,
    code: &'static str,
    /// The noun used in the message: `duplicate {noun} \`{key}\``.
    noun: &'static str,
    related: &'static str,
}

impl DupGuard {
    /// A guard for ordinary duplicate arguments/settings whose first occurrence
    /// is described as "first defined here".
    pub(super) fn new(code: &'static str, noun: &'static str) -> Self {
        DupGuard {
            seen: HashMap::new(),
            code,
            noun,
            related: "first defined here",
        }
    }

    /// Override the related-note label (e.g. "first declared here" for tables).
    pub(super) fn related(mut self, related: &'static str) -> Self {
        self.related = related;
        self
    }

    /// Record `key` at `span`. If it was already seen, push the duplicate
    /// diagnostic and return `true` so the caller can `continue`.
    pub(super) fn is_duplicate(
        &mut self,
        diagnostics: &mut Vec<Diagnostic>,
        key: &str,
        span: Span,
    ) -> bool {
        if self.already_seen(diagnostics, key, span) {
            return true;
        }
        self.record(key, span);
        false
    }

    /// Like [`is_duplicate`](Self::is_duplicate) but does *not* record the key,
    /// for call sites that run other validation before committing the key (e.g.
    /// the `Table` reserved-name check). Pair with [`record`](Self::record).
    pub(super) fn already_seen(
        &mut self,
        diagnostics: &mut Vec<Diagnostic>,
        key: &str,
        span: Span,
    ) -> bool {
        if let Some(&first) = self.seen.get(key) {
            diagnostics.push(
                Diagnostic::error(self.code, format!("duplicate {} `{key}`", self.noun), span)
                    .with_related(first, self.related),
            );
            return true;
        }
        false
    }

    /// Commit `key` at `span` as seen.
    pub(super) fn record(&mut self, key: &str, span: Span) {
        self.seen.insert(key.to_string(), span);
    }
}

impl Analyzer<'_> {
    /// Extract a string-literal argument value, reporting `code`/`message` at the
    /// value span when present but not a string. Returns `None` (silently) when
    /// the argument has no value.
    pub(super) fn expect_string(
        &mut self,
        arg: &Arg,
        code: &'static str,
        message: impl Into<String>,
    ) -> Option<String> {
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                Some(string_value(&lit.text().unwrap_or_default()))
            }
            Some(value) => {
                self.diag(Diagnostic::error(code, message, node_span(value.syntax())));
                None
            }
            None => None,
        }
    }

    /// Extract a boolean-literal argument value, reporting `code`/`message` at the
    /// value span when present but not a boolean.
    pub(super) fn expect_bool(
        &mut self,
        arg: &Arg,
        code: &'static str,
        message: impl Into<String>,
    ) -> Option<bool> {
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Bool) => {
                Some(lit.text().as_deref() == Some("true"))
            }
            Some(value) => {
                self.diag(Diagnostic::error(code, message, node_span(value.syntax())));
                None
            }
            None => None,
        }
    }

    /// Recognize a `null` literal argument value (used to suppress legends and
    /// axis titles). Reports `code`/`message` at the value span for any other
    /// present value. Returns `true` only when the value is `null`.
    pub(super) fn expect_null_flag(
        &mut self,
        arg: &Arg,
        code: &'static str,
        message: impl Into<String>,
    ) -> bool {
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Null) => true,
            Some(value) => {
                self.diag(Diagnostic::error(code, message, node_span(value.syntax())));
                false
            }
            None => false,
        }
    }

    /// Extract a bare `x` / `y` axis selector, reporting `message` at the value
    /// span otherwise (spec §16.11, §19.4).
    pub(super) fn expect_axis(
        &mut self,
        arg: &Arg,
        message: &'static str,
    ) -> Option<AxisSelectorIr> {
        match arg.value() {
            Some(ValueExpr::Algebra(AlgebraExpr::Name(name))) => {
                match name.name().unwrap_or_default().as_str() {
                    "x" => Some(AxisSelectorIr::X),
                    "y" => Some(AxisSelectorIr::Y),
                    _ => {
                        self.diag(Diagnostic::error(
                            "E1204",
                            message,
                            node_span(name.syntax()),
                        ));
                        None
                    }
                }
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    "E1204",
                    message,
                    node_span(value.syntax()),
                ));
                None
            }
            None => None,
        }
    }
}
