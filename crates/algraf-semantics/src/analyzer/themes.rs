//! Theme declaration analysis (spec §20.1, §20.8): named base themes plus
//! grouped and scalar per-field overrides.

use algraf_core::{codes, Diagnostic, Span};
use algraf_syntax::ast::{Arg, Decl, LiteralKind, ValueExpr};
use algraf_syntax::{node_span, unescape_string_literal as string_value};

use super::args::DupGuard;
use super::context::{Analyzer, ValueForm};
use crate::ir::{
    AxisPositionIr, FontStyleIr, FontWeightIr, LegendPositionIr, TextAlignIr, ThemeIr, ThemeLineIr,
    ThemeOverrides, ThemeRectIr, ThemeTextIr,
};
use crate::registry;
use crate::util::closest;

/// Which axis dimension a theme `axis*Position` token applies to.
#[derive(Clone, Copy)]
enum AxisDim {
    X,
    Y,
}

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
                if let Some(text) = self.theme_text(key, &value) {
                    if let Some(v) = text.size {
                        overrides.font_size = Some(v);
                    }
                    if let Some(v) = &text.fill {
                        overrides.text_color = Some(v.clone());
                    }
                    overrides.axis_text = Some(text);
                }
            }
            "axisTitle" => {
                overrides.axis_title = self.theme_text(key, &value);
            }
            "axisLine" => overrides.axis_line = self.theme_line(key, &value),
            "axisTicks" => overrides.axis_ticks = self.theme_line(key, &value),
            "plotTitle" => {
                overrides.plot_title = self.theme_text(key, &value);
            }
            "plotSubtitle" => {
                overrides.plot_subtitle = self.theme_text(key, &value);
            }
            "plotCaption" => {
                overrides.plot_caption = self.theme_text(key, &value);
            }
            "plotSource" => {
                overrides.plot_source = self.theme_text(key, &value);
            }
            "stripText" => {
                overrides.strip_text = self.theme_text(key, &value);
            }
            "legendTitle" => {
                overrides.legend_title = self.theme_text(key, &value);
            }
            "legendText" => {
                overrides.legend_text = self.theme_text(key, &value);
            }
            "panelBackground" => {
                if let Some(rect) = self.theme_rect(key, &value) {
                    if let Some(fill) = &rect.fill {
                        overrides.plot_background = Some(fill.clone());
                    }
                    overrides.panel_background = Some(rect);
                }
            }
            "gridMajor" => {
                if let Some(line) = self.theme_line(key, &value) {
                    if let Some(v) = &line.stroke {
                        overrides.grid_major_color = Some(v.clone());
                    }
                    if let Some(v) = line.stroke_width {
                        overrides.grid_major_width = Some(v);
                    }
                    overrides.grid_major = Some(line);
                }
            }
            "gridMinor" => overrides.grid_minor = self.theme_line(key, &value),
            "legendPosition" => overrides.legend_position = self.theme_legend_position(key, &value),
            "legendSpacing" => {
                overrides.legend_spacing = self.theme_scalar(key, &value, "a number", as_number)
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
            "axisTickLength" => {
                overrides.axis_tick_length = self.theme_scalar(key, &value, "a number", as_number)
            }
            "background" => {
                overrides.background = self.theme_scalar(key, &value, "a color string", as_str)
            }
            "plotBackground" => match &value {
                ValueExpr::Call(_) => {
                    if let Some(rect) = self.theme_rect(key, &value) {
                        if let Some(fill) = &rect.fill {
                            overrides.plot_background = Some(fill.clone());
                        }
                        overrides.panel_background = Some(rect);
                    }
                }
                _ => {
                    overrides.plot_background =
                        self.theme_scalar(key, &value, "a color string", as_str)
                }
            },
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
            "gridX" => overrides.grid_x = self.theme_scalar(key, &value, "a boolean", as_bool),
            "gridY" => overrides.grid_y = self.theme_scalar(key, &value, "a boolean", as_bool),
            "axes" => overrides.axes = self.theme_scalar(key, &value, "a boolean", as_bool),
            "axisYPosition" => {
                overrides.axis_y_position = self.theme_axis_position(key, &value, AxisDim::Y)
            }
            "axisXPosition" => {
                overrides.axis_x_position = self.theme_axis_position(key, &value, AxisDim::X)
            }
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

    fn theme_text(&mut self, key: &str, value: &ValueExpr) -> Option<ThemeTextIr> {
        let props = self.theme_subcall(key, value, "Text")?;
        let mut out = ThemeTextIr::default();
        for prop in props {
            let Some(name) = prop.key() else { continue };
            let Some(value) = prop.value() else { continue };
            match name.as_str() {
                "fontFamily" => {
                    out.font_family = self.theme_scalar(&name, &value, "a string", as_str);
                }
                "size" => {
                    out.size = self.theme_scalar(&name, &value, "a number", as_number);
                }
                "fill" => {
                    out.fill = self.theme_scalar(&name, &value, "a color string", as_str);
                }
                "weight" => {
                    out.weight = self.theme_font_weight(&name, &value);
                }
                "style" => {
                    out.style = self.theme_font_style(&name, &value);
                }
                "align" => {
                    out.align = self.theme_text_align(&name, &value);
                }
                "hidden" => {
                    out.hidden = self.theme_scalar(&name, &value, "a boolean", as_bool);
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1705,
                    format!("`{key}` Text property `{name}` is not supported"),
                    node_span(prop.syntax()),
                )),
            }
        }
        Some(out)
    }

    /// Resolve a `weight:` token to a font weight (spec §20.8). Accepts the
    /// strings `"normal"`/`"bold"` or an integer in `100`–`900` (multiples of
    /// `100`); anything else emits `E1705`.
    fn theme_font_weight(&mut self, key: &str, value: &ValueExpr) -> Option<FontWeightIr> {
        let expected = "\"normal\", \"bold\", or an integer 100-900 (multiple of 100)";
        match self.substitute_var(ValueForm::of(value)) {
            ValueForm::Str(s) if s == "normal" => Some(FontWeightIr::Normal),
            ValueForm::Str(s) if s == "bold" => Some(FontWeightIr::Bold),
            ValueForm::Number(n)
                if n.fract() == 0.0 && (100.0..=900.0).contains(&n) && (n as u32) % 100 == 0 =>
            {
                Some(FontWeightIr::Numeric(n as u16))
            }
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1705,
                    format!("`{key}` expects {expected}"),
                    node_span(value.syntax()),
                ));
                None
            }
        }
    }

    /// Resolve a `style:` token to a font style (spec §20.8). Accepts
    /// `"normal"`/`"italic"`; anything else emits `E1705`.
    fn theme_font_style(&mut self, key: &str, value: &ValueExpr) -> Option<FontStyleIr> {
        let raw = self.theme_scalar(key, value, "\"normal\" or \"italic\"", as_str)?;
        match raw.as_str() {
            "normal" => Some(FontStyleIr::Normal),
            "italic" => Some(FontStyleIr::Italic),
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1705,
                    format!("`{key}` expects \"normal\" or \"italic\""),
                    node_span(value.syntax()),
                ));
                None
            }
        }
    }

    /// Resolve an `align:` token to a horizontal text alignment (spec §20.8).
    /// `start`/`middle`/`end` are accepted as synonyms of `left`/`center`/
    /// `right`; anything else emits `E1705`.
    fn theme_text_align(&mut self, key: &str, value: &ValueExpr) -> Option<TextAlignIr> {
        let raw = self.theme_scalar(key, value, "\"left\", \"center\", or \"right\"", as_str)?;
        match raw.as_str() {
            "left" | "start" => Some(TextAlignIr::Left),
            "center" | "middle" => Some(TextAlignIr::Center),
            "right" | "end" => Some(TextAlignIr::Right),
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1705,
                    format!("`{key}` expects \"left\", \"center\", or \"right\""),
                    node_span(value.syntax()),
                ));
                None
            }
        }
    }

    fn theme_line(&mut self, key: &str, value: &ValueExpr) -> Option<ThemeLineIr> {
        let props = self.theme_subcall(key, value, "Line")?;
        let mut out = ThemeLineIr::default();
        for prop in props {
            let Some(name) = prop.key() else { continue };
            let Some(value) = prop.value() else { continue };
            match name.as_str() {
                "stroke" => {
                    out.stroke = self.theme_scalar(&name, &value, "a color string", as_str);
                }
                "strokeWidth" => {
                    out.stroke_width = self.theme_scalar(&name, &value, "a number", as_number);
                }
                "dash" => {
                    out.dash = self.theme_line_dash(&name, &value);
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1705,
                    format!("`{key}` Line property `{name}` is not supported"),
                    node_span(prop.syntax()),
                )),
            }
        }
        Some(out)
    }

    fn theme_line_dash(&mut self, key: &str, value: &ValueExpr) -> Option<String> {
        let raw = self.theme_scalar(key, value, "\"solid\", \"dotted\", or \"dashed\"", as_str)?;
        match raw.as_str() {
            "solid" | "dotted" | "dashed" => Some(raw),
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1705,
                    format!("`{key}` expects \"solid\", \"dotted\", or \"dashed\""),
                    node_span(value.syntax()),
                ));
                None
            }
        }
    }

    fn theme_rect(&mut self, key: &str, value: &ValueExpr) -> Option<ThemeRectIr> {
        let props = self.theme_subcall(key, value, "Rect")?;
        let mut out = ThemeRectIr::default();
        for prop in props {
            let Some(name) = prop.key() else { continue };
            let Some(value) = prop.value() else { continue };
            match name.as_str() {
                "fill" => {
                    out.fill = self.theme_scalar(&name, &value, "a color string", as_str);
                }
                "stroke" => {
                    out.stroke = self.theme_scalar(&name, &value, "a color string", as_str);
                }
                "strokeWidth" => {
                    out.stroke_width = self.theme_scalar(&name, &value, "a number", as_number);
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1705,
                    format!("`{key}` Rect property `{name}` is not supported"),
                    node_span(prop.syntax()),
                )),
            }
        }
        Some(out)
    }

    fn theme_legend_position(&mut self, key: &str, value: &ValueExpr) -> Option<LegendPositionIr> {
        let raw = self.theme_scalar(
            key,
            value,
            "one of \"right\", \"bottom\", \"top\", or \"left\"",
            as_str,
        )?;
        match raw.as_str() {
            "right" => Some(LegendPositionIr::Right),
            "bottom" => Some(LegendPositionIr::Bottom),
            "top" => Some(LegendPositionIr::Top),
            "left" => Some(LegendPositionIr::Left),
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1705,
                    format!("`{key}` expects one of \"right\", \"bottom\", \"top\", or \"left\""),
                    node_span(value.syntax()),
                ));
                None
            }
        }
    }

    /// Resolve an `axisYPosition`/`axisXPosition` token to an axis side, scoped
    /// to the axis dimension (spec §20.1). A wrong-axis or unknown value emits
    /// `E1705`.
    fn theme_axis_position(
        &mut self,
        key: &str,
        value: &ValueExpr,
        dim: AxisDim,
    ) -> Option<AxisPositionIr> {
        let (expected, raw) = match dim {
            AxisDim::Y => (
                "\"left\" or \"right\"",
                self.theme_scalar(key, value, "\"left\" or \"right\"", as_str)?,
            ),
            AxisDim::X => (
                "\"top\" or \"bottom\"",
                self.theme_scalar(key, value, "\"top\" or \"bottom\"", as_str)?,
            ),
        };
        let resolved = match (dim, raw.as_str()) {
            (AxisDim::Y, "left") => Some(AxisPositionIr::Left),
            (AxisDim::Y, "right") => Some(AxisPositionIr::Right),
            (AxisDim::X, "top") => Some(AxisPositionIr::Top),
            (AxisDim::X, "bottom") => Some(AxisPositionIr::Bottom),
            _ => None,
        };
        if resolved.is_none() {
            self.diag(Diagnostic::error(
                codes::E1705,
                format!("`{key}` expects {expected}"),
                node_span(value.syntax()),
            ));
        }
        resolved
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
