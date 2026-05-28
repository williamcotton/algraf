//! Guide declaration analysis (spec §19): legend toggles, axis labels, and grid
//! control, applied as space-local or chart-level overrides.

use algraf_core::{codes, Diagnostic};
use algraf_data::validate_temporal_format;
use algraf_syntax::ast::{Decl, LiteralKind, ValueExpr};
use algraf_syntax::{node_span, unescape_string_literal as string_value};

use super::args::DupGuard;
use super::context::{Analyzer, ValueForm};
use crate::ir::{AxisSelectorIr, GridShapeIr, GuideOverridesIr, TemporalFormatIr};

impl Analyzer<'_> {
    pub(super) fn guide_decl(&mut self, decl: &Decl, guides: &mut GuideOverridesIr) {
        let mut dup = DupGuard::new(codes::E1002, "Guide argument");
        let mut axis: Option<AxisSelectorIr> = None;
        let mut label: Option<String> = None;
        let mut time_format: Option<TemporalFormatIr> = None;
        let mut tick_label_angle: Option<f64> = None;
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
            }
            None if has_label || has_time_format || has_tick_label_angle => {
                self.diag(Diagnostic::error(
                    codes::E1204,
                    "`Guide(label: ...)`, `Guide(timeFormat: ...)`, and `Guide(tickLabelAngle: ...)` require `axis: x` or `axis: y`",
                    node_span(decl.syntax()),
                ));
            }
            None => {}
        }
        if axis.is_some() && !has_label && !has_time_format && !has_tick_label_angle {
            self.diag(Diagnostic::warning(
                codes::W2006,
                "`Guide(axis: ...)` without `label:`, `timeFormat:`, or `tickLabelAngle:` has no effect",
                node_span(decl.syntax()),
            ));
        }
    }
}

fn temporal_format(value: &str) -> Option<TemporalFormatIr> {
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
