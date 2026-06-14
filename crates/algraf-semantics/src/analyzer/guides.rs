//! Guide declaration analysis (spec §19): legend toggles, axis labels, and grid
//! control, applied as space-local or chart-level overrides.

use algraf_core::{codes, Diagnostic};
use algraf_data::validate_temporal_format;
use algraf_syntax::ast::{Decl, LiteralKind, ValueExpr};
use algraf_syntax::{node_span, unescape_string_literal as string_value};

use algraf_core::Span;

use super::args::DupGuard;
use super::context::{Analyzer, ValueForm};
use crate::ir::{AxisPositionIr, AxisSelectorIr, GridShapeIr, GuideOverridesIr, TemporalFormatIr};

/// Numeric tick-label format strings shared with `Text` (spec §14.16, §19.4).
pub(super) fn is_numeric_format(value: &str) -> bool {
    matches!(
        value,
        ".0f" | ".1f" | ".2f" | "$.2f" | ".0%" | ".1%" | ".2%"
    )
}

impl Analyzer<'_> {
    pub(super) fn guide_decl(&mut self, decl: &Decl, guides: &mut GuideOverridesIr) {
        let mut dup = DupGuard::new(codes::E1002, "Guide argument");
        let mut axis: Option<AxisSelectorIr> = None;
        let mut label: Option<String> = None;
        let mut time_format: Option<TemporalFormatIr> = None;
        let mut tick_label_angle: Option<f64> = None;
        let mut tick_label_rows: Option<usize> = None;
        // `position` and `format` are validated against the resolved axis below.
        let mut position: Option<(String, Span)> = None;
        let mut numeric_format: Option<(String, Span)> = None;
        // `grid` applies per-axis when an `axis:` is named, else globally.
        let mut grid_flag: Option<bool> = None;
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
                        match temporal_format(&value) {
                            Some(format) => time_format = Some(format),
                            None => self.diag(Diagnostic::error(
                                codes::E1907,
                                format!("unknown or invalid temporal format `{value}`"),
                                node_span(lit.syntax()),
                            )),
                        }
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E1204,
                        "`timeFormat` expects a named temporal format or chrono-style format string",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "position" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        position = Some((
                            string_value(&lit.text().unwrap_or_default()),
                            node_span(lit.syntax()),
                        ));
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E1204,
                        "`position` expects a string literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "format" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        numeric_format = Some((
                            string_value(&lit.text().unwrap_or_default()),
                            node_span(lit.syntax()),
                        ));
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E1909,
                        "`format` expects a numeric format string",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "tickLabelAngle" => {
                    if let Some(value) = arg.value() {
                        match ValueForm::of(&value) {
                            ValueForm::Number(n)
                                if n.is_finite() && (-90.0..=90.0).contains(&n) =>
                            {
                                tick_label_angle = Some(n);
                            }
                            _ => self.diag(Diagnostic::error(
                                codes::E1204,
                                "`tickLabelAngle` expects a finite number between -90 and 90",
                                node_span(value.syntax()),
                            )),
                        }
                    }
                }
                "tickLabelRows" => {
                    if let Some(value) = arg.value() {
                        match ValueForm::of(&value) {
                            ValueForm::Number(n) if n.fract() == 0.0 && (1.0..=8.0).contains(&n) => {
                                tick_label_rows = Some(n as usize);
                            }
                            _ => self.diag(Diagnostic::error(
                                codes::E1204,
                                "`tickLabelRows` expects an integer from 1 through 8",
                                node_span(value.syntax()),
                            )),
                        }
                    }
                }
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
                        grid_flag = Some(b);
                    }
                }
                "gridShape" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        match string_value(&lit.text().unwrap_or_default()).as_str() {
                            "circle" => guides.grid_shape = Some(GridShapeIr::Circle),
                            "polygon" => guides.grid_shape = Some(GridShapeIr::Polygon),
                            other => self.diag(Diagnostic::error(
                                codes::E1906,
                                format!(
                                    "`gridShape` must be \"circle\" or \"polygon\", not {other:?}"
                                ),
                                node_span(lit.syntax()),
                            )),
                        }
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E1906,
                        "`gridShape` expects a string literal: \"circle\" or \"polygon\"",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                _ => self.diag(Diagnostic::warning(
                    codes::W2006,
                    format!("unsupported Guide argument `{key}` ignored"),
                    key_span,
                )),
            }
        }
        let has_label = label.is_some();
        let has_time_format = time_format.is_some();
        let has_tick_label_angle = tick_label_angle.is_some();
        let has_tick_label_rows = tick_label_rows.is_some();
        let has_position = position.is_some();
        let has_format = numeric_format.is_some();
        // A bare `Guide(grid: ...)` sets the global grid; `Guide(axis: x, grid:
        // ...)` sets only that axis's grid lines (spec §19).
        match (axis, grid_flag) {
            (Some(AxisSelectorIr::X), Some(b)) => guides.x_grid = Some(b),
            (Some(AxisSelectorIr::Y), Some(b)) => guides.y_grid = Some(b),
            (None, Some(b)) => guides.grid = Some(b),
            _ => {}
        }
        let has_grid = grid_flag.is_some();
        match axis {
            Some(AxisSelectorIr::X) => {
                if let Some(text) = label.take() {
                    guides.x_label = Some(text);
                }
                if let Some(format) = time_format.take() {
                    guides.x_time_format = Some(format);
                }
                if let Some(angle) = tick_label_angle.take() {
                    guides.x_tick_label_angle = Some(angle);
                }
                if let Some(rows) = tick_label_rows.take() {
                    guides.x_tick_label_rows = Some(rows);
                }
                if let Some((value, span)) = position.take() {
                    guides.x_position = self.resolve_axis_position(AxisSelectorIr::X, &value, span);
                }
                if let Some((value, span)) = numeric_format.take() {
                    guides.x_format = self.resolve_numeric_format(&value, span, has_time_format);
                }
            }
            Some(AxisSelectorIr::Y) => {
                if let Some(text) = label.take() {
                    guides.y_label = Some(text);
                }
                if let Some(format) = time_format.take() {
                    guides.y_time_format = Some(format);
                }
                if let Some(angle) = tick_label_angle.take() {
                    guides.y_tick_label_angle = Some(angle);
                }
                if let Some(rows) = tick_label_rows.take() {
                    guides.y_tick_label_rows = Some(rows);
                }
                if let Some((value, span)) = position.take() {
                    guides.y_position = self.resolve_axis_position(AxisSelectorIr::Y, &value, span);
                }
                if let Some((value, span)) = numeric_format.take() {
                    guides.y_format = self.resolve_numeric_format(&value, span, has_time_format);
                }
            }
            None if has_label
                || has_time_format
                || has_tick_label_angle
                || has_tick_label_rows
                || has_position =>
            {
                self.diag(Diagnostic::error(
                    codes::E1204,
                    "`Guide(label: ...)`, `Guide(timeFormat: ...)`, `Guide(tickLabelAngle: ...)`, `Guide(tickLabelRows: ...)`, and `Guide(position: ...)` require `axis: x` or `axis: y`",
                    node_span(decl.syntax()),
                ));
            }
            None if has_format => {
                self.diag(Diagnostic::error(
                    codes::E1909,
                    "`Guide(format: ...)` requires `axis: x` or `axis: y`",
                    node_span(decl.syntax()),
                ));
            }
            None => {}
        }
        if axis.is_some()
            && !has_label
            && !has_time_format
            && !has_tick_label_angle
            && !has_tick_label_rows
            && !has_position
            && !has_format
            && !has_grid
        {
            self.diag(Diagnostic::warning(
                codes::W2006,
                "`Guide(axis: ...)` without `label:`, `timeFormat:`, `position:`, `format:`, `grid:`, `tickLabelAngle:`, or `tickLabelRows:` has no effect",
                node_span(decl.syntax()),
            ));
        }
    }

    /// Validate a `position` string for the named axis (spec §19.2, §19.3).
    /// An invalid value, or one not valid for the axis, emits `E1204` and falls
    /// back to the default side (returns `None`).
    fn resolve_axis_position(
        &mut self,
        axis: AxisSelectorIr,
        value: &str,
        span: Span,
    ) -> Option<AxisPositionIr> {
        let resolved = match (axis, value) {
            (AxisSelectorIr::Y, "left") => Some(AxisPositionIr::Left),
            (AxisSelectorIr::Y, "right") => Some(AxisPositionIr::Right),
            (AxisSelectorIr::X, "top") => Some(AxisPositionIr::Top),
            (AxisSelectorIr::X, "bottom") => Some(AxisPositionIr::Bottom),
            _ => None,
        };
        if resolved.is_none() {
            let allowed = match axis {
                AxisSelectorIr::X => "\"top\" or \"bottom\"",
                AxisSelectorIr::Y => "\"left\" or \"right\"",
            };
            self.diag(Diagnostic::error(
                codes::E1204,
                format!("`position` for this axis must be {allowed}, not {value:?}"),
                span,
            ));
        }
        resolved
    }

    /// Validate a numeric tick-label `format` string (spec §19.4). An unknown
    /// format, or one combined with `timeFormat`, emits `E1909` and is dropped.
    fn resolve_numeric_format(
        &mut self,
        value: &str,
        span: Span,
        has_time_format: bool,
    ) -> Option<String> {
        if has_time_format {
            self.diag(Diagnostic::error(
                codes::E1909,
                "`format` and `timeFormat` cannot be combined on one axis",
                span,
            ));
            return None;
        }
        if !is_numeric_format(value) {
            self.diag(Diagnostic::error(
                codes::E1909,
                format!("unknown numeric axis format `{value}`"),
                span,
            ));
            return None;
        }
        Some(value.to_string())
    }
}

pub(super) fn temporal_format(value: &str) -> Option<TemporalFormatIr> {
    match value {
        "iso-date" => Some(TemporalFormatIr::IsoDate),
        "iso-minute" => Some(TemporalFormatIr::IsoMinute),
        "iso-second" => Some(TemporalFormatIr::IsoSecond),
        "iso-millis" => Some(TemporalFormatIr::IsoMillis),
        "rfc3339" => Some(TemporalFormatIr::Rfc3339),
        "year" => Some(TemporalFormatIr::Year),
        "month" => Some(TemporalFormatIr::Month),
        "month-day" => Some(TemporalFormatIr::MonthDay),
        "time-minute" => Some(TemporalFormatIr::TimeMinute),
        "time-second" => Some(TemporalFormatIr::TimeSecond),
        custom if validate_temporal_format(custom) => {
            Some(TemporalFormatIr::Custom(custom.to_string()))
        }
        _ => None,
    }
}
