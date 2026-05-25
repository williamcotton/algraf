//! Guide declaration analysis (spec §19): legend toggles, axis labels, and grid
//! control, applied as space-local or chart-level overrides.

use algraf_core::{codes, Diagnostic};
use algraf_syntax::ast::{Decl, LiteralKind, ValueExpr};
use algraf_syntax::{node_span, unescape_string_literal as string_value};

use super::args::DupGuard;
use super::context::Analyzer;
use crate::ir::{AxisSelectorIr, GuideOverridesIr, TemporalFormatIr};

impl Analyzer<'_> {
    pub(super) fn guide_decl(&mut self, decl: &Decl, guides: &mut GuideOverridesIr) {
        let mut dup = DupGuard::new(codes::E1002, "Guide argument");
        let mut axis: Option<AxisSelectorIr> = None;
        let mut label: Option<String> = None;
        let mut time_format: Option<TemporalFormatIr> = None;
        for arg in decl.args() {
            let Some(key) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &key, key_span) {
                continue;
            }

            match key.as_str() {
                "legend" => {
                    if let Some(b) =
                        self.expect_bool(&arg, codes::E1204, "`legend` expects a boolean literal")
                    {
                        guides.legend = Some(b);
                    }
                }
                "axis" => {
                    if let Some(a) = self.expect_axis(&arg, "`axis` expects bare `x` or `y`") {
                        axis = Some(a);
                    }
                }
                "label" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        label = Some(string_value(&lit.text().unwrap_or_default()));
                    }
                    // `label: null` suppresses the axis title (spec §19.x). An
                    // empty string carries the suppression to the renderer.
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Null) => {
                        label = Some(String::new());
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E1204,
                        "`label` expects a string literal or `null`",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "timeFormat" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        let value = string_value(&lit.text().unwrap_or_default());
                        match value.as_str() {
                            "iso-date" => time_format = Some(TemporalFormatIr::IsoDate),
                            "iso-minute" => time_format = Some(TemporalFormatIr::IsoMinute),
                            _ => self.diag(Diagnostic::error(
                                codes::E1204,
                                format!("unknown temporal format `{value}`"),
                                node_span(lit.syntax()),
                            )),
                        }
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E1204,
                        "`timeFormat` expects \"iso-date\" or \"iso-minute\"",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "fill" => {
                    if self.expect_null_flag(
                        &arg,
                        codes::E1204,
                        "`fill` in `Guide` expects `null` to suppress the legend",
                    ) {
                        guides.fill_legend = Some(false);
                    }
                }
                "stroke" => {
                    if self.expect_null_flag(
                        &arg,
                        codes::E1204,
                        "`stroke` in `Guide` expects `null` to suppress the legend",
                    ) {
                        guides.stroke_legend = Some(false);
                    }
                }
                "grid" => {
                    if let Some(b) =
                        self.expect_bool(&arg, codes::E1204, "`grid` expects a boolean literal")
                    {
                        guides.grid = Some(b);
                    }
                }
                _ => self.diag(Diagnostic::warning(
                    codes::W2006,
                    format!("unsupported Guide argument `{key}` ignored"),
                    key_span,
                )),
            }
        }
        let has_label = label.is_some();
        let has_time_format = time_format.is_some();
        match axis {
            Some(AxisSelectorIr::X) => {
                if let Some(text) = label.take() {
                    guides.x_label = Some(text);
                }
                if let Some(format) = time_format.take() {
                    guides.x_time_format = Some(format);
                }
            }
            Some(AxisSelectorIr::Y) => {
                if let Some(text) = label.take() {
                    guides.y_label = Some(text);
                }
                if let Some(format) = time_format.take() {
                    guides.y_time_format = Some(format);
                }
            }
            None if has_label || has_time_format => {
                self.diag(Diagnostic::error(
                    codes::E1204,
                    "`Guide(label: ...)` and `Guide(timeFormat: ...)` require `axis: x` or `axis: y`",
                    node_span(decl.syntax()),
                ));
            }
            None => {}
        }
        if axis.is_some() && !has_label && !has_time_format {
            self.diag(Diagnostic::warning(
                codes::W2006,
                "`Guide(axis: ...)` without `label:` or `timeFormat:` has no effect",
                node_span(decl.syntax()),
            ));
        }
    }
}
