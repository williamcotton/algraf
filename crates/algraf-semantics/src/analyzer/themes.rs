//! Theme declaration analysis (spec §20.1, §20.8): named base themes plus
//! grouped and scalar per-field overrides.

use algraf_core::{codes, Diagnostic, Span};
use algraf_syntax::ast::{Arg, Decl, LiteralKind, ValueExpr};
use algraf_syntax::{node_span, unescape_string_literal as string_value};

use super::args::DupGuard;
use super::context::{Analyzer, ValueForm};
use crate::ir::{ThemeIr, ThemeOverrides};
use crate::registry;
use crate::util::closest;

impl Analyzer<'_> {
    pub(super) fn theme_decl(&mut self, decl: &Decl) -> Option<ThemeIr> {
        let mut dup = DupGuard::new(codes::E1002, "Theme argument");
        let mut theme = ThemeIr::default();
        let mut saw_any = false;
        for arg in decl.args() {
            let Some(key) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &key, key_span) {
                continue;
            }
            saw_any = true;

            if key == "name" {
                match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        let name = string_value(&lit.text().unwrap_or_default());
                        if !registry::THEME_NAMES.contains(&name.as_str()) {
                            self.diag(Diagnostic::error(
                                codes::E1204,
                                format!("unknown theme `{name}`"),
                                node_span(lit.syntax()),
                            ));
                        } else {
                            theme.base = Some(name);
                        }
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E1204,
                        "`name` expects a string literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                }
            } else {
                self.theme_override(&key, &arg, key_span, &mut theme.overrides);
            }
        }
        saw_any.then_some(theme)
    }

    /// Apply one `Theme(...)` override argument to the override set (spec §20.8).
    /// Unknown keys emit `E1704`; type/shape mismatches emit `E1705`.
    fn theme_override(
        &mut self,
        key: &str,
        arg: &Arg,
        key_span: Span,
        overrides: &mut ThemeOverrides,
    ) {
        let Some(value) = arg.value() else { return };
        match key {
            // Grouped, geometry-style overrides (spec §20.8).
            "axisText" => {
                if let Some(props) = self.theme_subcall(key, &value, "Text") {
                    if let Some(v) = self.theme_number(&props, "size") {
                        overrides.font_size = Some(v);
                    }
                    if let Some(v) = self.theme_color(&props, "fill") {
                        overrides.text_color = Some(v);
                    }
                }
            }
            "gridMajor" => {
                if let Some(props) = self.theme_subcall(key, &value, "Line") {
                    if let Some(v) = self.theme_color(&props, "stroke") {
                        overrides.grid_major_color = Some(v);
                    }
                    if let Some(v) = self.theme_number(&props, "strokeWidth") {
                        overrides.grid_major_width = Some(v);
                    }
                }
            }
            // Direct scalar overrides.
            "fontFamily" => {
                overrides.font_family = self.theme_scalar(key, &value, "a string", as_str)
            }
            "fontSize" => {
                overrides.font_size = self.theme_scalar(key, &value, "a number", as_number)
            }
            "titleSize" => {
                overrides.title_size = self.theme_scalar(key, &value, "a number", as_number)
            }
            "pointSize" => {
                overrides.point_size = self.theme_scalar(key, &value, "a number", as_number)
            }
            "lineWidth" => {
                overrides.line_width = self.theme_scalar(key, &value, "a number", as_number)
            }
            "background" => {
                overrides.background = self.theme_scalar(key, &value, "a color string", as_str)
            }
            "plotBackground" => {
                overrides.plot_background = self.theme_scalar(key, &value, "a color string", as_str)
            }
            "axisColor" => {
                overrides.axis_color = self.theme_scalar(key, &value, "a color string", as_str)
            }
            "gridColor" => {
                overrides.grid_major_color =
                    self.theme_scalar(key, &value, "a color string", as_str)
            }
            "textColor" => {
                overrides.text_color = self.theme_scalar(key, &value, "a color string", as_str)
            }
            "grid" => overrides.grid = self.theme_scalar(key, &value, "a boolean", as_bool),
            "axes" => overrides.axes = self.theme_scalar(key, &value, "a boolean", as_bool),
            _ => {
                let mut diag = Diagnostic::error(
                    codes::E1704,
                    format!("unknown Theme property `{key}`"),
                    key_span,
                );
                if let Some(suggestion) =
                    closest(key, registry::THEME_OVERRIDE_KEYS.iter().copied())
                {
                    diag = diag.with_help(format!("did you mean `{suggestion}`?"));
                }
                self.diag(diag);
            }
        }
    }

    /// Resolve a grouped override value such as `Text(size: 12, fill: "#333")`
    /// into its argument list, checking the expected call name.
    fn theme_subcall(&mut self, key: &str, value: &ValueExpr, expected: &str) -> Option<Vec<Arg>> {
        match value {
            ValueExpr::Call(call) if call.name().as_deref() == Some(expected) => Some(call.args()),
            other => {
                self.diag(Diagnostic::error(
                    codes::E1705,
                    format!("`{key}` expects a `{expected}(...)` value"),
                    node_span(other.syntax()),
                ));
                None
            }
        }
    }

    fn theme_number(&mut self, args: &[Arg], name: &str) -> Option<f64> {
        let arg = args.iter().find(|a| a.key().as_deref() == Some(name))?;
        self.theme_scalar(name, &arg.value()?, "a number", as_number)
    }

    fn theme_color(&mut self, args: &[Arg], name: &str) -> Option<String> {
        let arg = args.iter().find(|a| a.key().as_deref() == Some(name))?;
        self.theme_scalar(name, &arg.value()?, "a color string", as_str)
    }

    /// Resolve one scalar theme override value (after `let` substitution),
    /// classifying it with `extract`. On mismatch, emit `E1705` describing the
    /// `expected` shape at the value span.
    fn theme_scalar<T>(
        &mut self,
        key: &str,
        value: &ValueExpr,
        expected: &str,
        extract: fn(ValueForm) -> Option<T>,
    ) -> Option<T> {
        match extract(self.substitute_var(ValueForm::of(value))) {
            Some(v) => Some(v),
            None => {
                self.diag(Diagnostic::error(
                    codes::E1705,
                    format!("`{key}` expects {expected}"),
                    node_span(value.syntax()),
                ));
                None
            }
        }
    }
}

fn as_number(form: ValueForm) -> Option<f64> {
    match form {
        ValueForm::Number(n) => Some(n),
        _ => None,
    }
}

fn as_str(form: ValueForm) -> Option<String> {
    match form {
        ValueForm::Str(s) => Some(s),
        _ => None,
    }
}

fn as_bool(form: ValueForm) -> Option<bool> {
    match form {
        ValueForm::Bool(b) => Some(b),
        _ => None,
    }
}
