//! Scale declaration analysis (spec §16.11–16.13): target selection, domain and
//! range bounds, palettes/gradients, and categorical color/label maps.

use std::collections::HashSet;

use algraf_core::{Diagnostic, Span};
use algraf_data::DataType;
use algraf_syntax::ast::{AlgebraExpr, Decl, LiteralKind, MapValue, ValueExpr};
use algraf_syntax::{node_span, unescape_string_literal as string_value};

use super::args::DupGuard;
use super::context::{ActiveTable, Analyzer, ValueForm};
use super::properties::is_color_literal;
use crate::ir::*;

const PALETTE_NAMES: &[&str] = &["default", "accent"];

/// A parsed `Scale(range: ...)` declaration before the target is known.
enum RangeSpec {
    /// Two numeric output bounds, each possibly inferred (`[0, 30]`).
    Numeric([Option<f64>; 2], Span),
    /// A manual category → color map (`["A" => "burlywood"]`).
    ColorMap(Vec<(String, String)>, Span),
}

impl Analyzer<'_> {
    pub(super) fn scale_decl(&mut self, decl: &Decl, table: &ActiveTable) -> Option<ScaleIr> {
        let span = node_span(decl.syntax());
        let mut dup = DupGuard::new("E1002", "Scale argument");
        let mut target: Option<ScaleTargetIr> = None;
        let mut scale_type = None;
        let mut domain: Option<[Option<f64>; 2]> = None;
        let mut domain_span: Option<Span> = None;
        let mut range: Option<RangeSpec> = None;
        let mut reverse = None;
        let mut integer = None;
        let mut palette = None;
        let mut gradient: Option<Vec<String>> = None;
        let mut gradient_span: Option<Span> = None;
        let mut label_map: Option<Vec<(String, String)>> = None;
        let mut labels_span: Option<Span> = None;
        let mut label = None;

        for arg in decl.args() {
            let Some(key) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &key, key_span) {
                continue;
            }

            match key.as_str() {
                "axis" => {
                    if let Some(axis) = self.expect_axis(&arg, "`axis` expects bare `x` or `y`") {
                        self.set_scale_target(&mut target, ScaleTargetIr::Axis(axis), key_span);
                    }
                }
                "fill" | "stroke" | "size" | "strokeWidth" => match arg.value() {
                    Some(ValueExpr::Algebra(AlgebraExpr::Name(name))) => {
                        let column = self.resolve_column(&name, table);
                        self.set_scale_target(
                            &mut target,
                            ScaleTargetIr::Aesthetic {
                                aesthetic: key,
                                column: Some(column),
                            },
                            key_span,
                        );
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        "E1204",
                        format!("`{key}` in `Scale` expects a column name"),
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "type" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        let value = string_value(&lit.text().unwrap_or_default());
                        match value.as_str() {
                            "linear" => scale_type = Some(ScaleTypeIr::Linear),
                            "log10" => scale_type = Some(ScaleTypeIr::Log10),
                            _ => self.diag(Diagnostic::error(
                                "E1204",
                                format!("unknown scale type `{value}`"),
                                node_span(lit.syntax()),
                            )),
                        }
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        "E1204",
                        "`type` expects a string literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "domain" => {
                    if let Some(value) = arg.value() {
                        domain_span = Some(node_span(value.syntax()));
                        match self.numeric_bounds(&value) {
                            Some(bounds) => domain = Some(bounds),
                            None => self.diag(Diagnostic::error(
                                "E1204",
                                "`domain` expects two numeric values (each may be `null`)",
                                node_span(value.syntax()),
                            )),
                        }
                    }
                }
                "range" => {
                    if let Some(value) = arg.value() {
                        let value_span = node_span(value.syntax());
                        match &value {
                            ValueExpr::Map(map) => {
                                if let Some(entries) = self.color_map_entries(map) {
                                    range = Some(RangeSpec::ColorMap(entries, value_span));
                                }
                            }
                            _ => match self.numeric_bounds(&value) {
                                Some(bounds) => {
                                    range = Some(RangeSpec::Numeric(bounds, value_span))
                                }
                                None => self.diag(Diagnostic::error(
                                    "E1603",
                                    "`range` expects two numeric values or a category map",
                                    value_span,
                                )),
                            },
                        }
                    }
                }
                "labels" => {
                    if let Some(value) = arg.value() {
                        labels_span = Some(node_span(value.syntax()));
                        match &value {
                            ValueExpr::Map(map) => {
                                label_map = self.color_map_entries(map);
                            }
                            _ => self.diag(Diagnostic::error(
                                "E1606",
                                "`labels` expects a category map (e.g. [\"A\" => \"Advance\"])",
                                node_span(value.syntax()),
                            )),
                        }
                    }
                }
                "reverse" => {
                    if let Some(b) =
                        self.expect_bool(&arg, "E1204", "`reverse` expects a boolean literal")
                    {
                        reverse = Some(b);
                    }
                }
                "integer" => {
                    if let Some(b) =
                        self.expect_bool(&arg, "E1204", "`integer` expects a boolean literal")
                    {
                        integer = Some(b);
                    }
                }
                "palette" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        let value = string_value(&lit.text().unwrap_or_default());
                        if PALETTE_NAMES.contains(&value.as_str()) {
                            palette = Some(value);
                        } else {
                            self.diag(Diagnostic::error(
                                "E1204",
                                format!("unknown palette `{value}`"),
                                node_span(lit.syntax()),
                            ));
                        }
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        "E1204",
                        "`palette` expects a string literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "gradient" => {
                    let Some(value) = arg.value() else { continue };
                    gradient_span = Some(node_span(value.syntax()));
                    match ValueForm::of(&value) {
                        ValueForm::StringArray(Some(values))
                            if values.len() >= 2
                                && values.iter().all(|value| is_color_literal(value)) =>
                        {
                            gradient = Some(values);
                        }
                        _ => self.diag(Diagnostic::error(
                            "E1601",
                            "`gradient` expects an array of two or more color strings",
                            node_span(value.syntax()),
                        )),
                    }
                }
                "label" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        label = Some(string_value(&lit.text().unwrap_or_default()));
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        "E1204",
                        "`label` expects a string literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                _ => self.diag(Diagnostic::error(
                    "E1003",
                    format!("unsupported Scale argument `{key}`"),
                    key_span,
                )),
            }
        }

        let Some(target) = target else {
            self.diag(Diagnostic::error(
                "E1204",
                "`Scale` requires `axis`, `fill`, `stroke`, `size`, or `strokeWidth`",
                span,
            ));
            return None;
        };

        // Split a `range:` declaration into its numeric and color-map forms once
        // the target is known, validating the form against the scale kind.
        let mut range_numeric: Option<[Option<f64>; 2]> = None;
        let mut color_map: Option<Vec<(String, String)>> = None;

        match &target {
            ScaleTargetIr::Axis(_) => {
                if palette.is_some() || gradient.is_some() {
                    self.diag(Diagnostic::error(
                        "E1204",
                        "`palette` and `gradient` apply only to fill or stroke scales",
                        span,
                    ));
                }
                if let Some(map) = labels_span {
                    self.diag(Diagnostic::error(
                        "E1606",
                        "`labels` maps apply only to categorical fill or stroke scales",
                        map,
                    ));
                    label_map = None;
                }
                match &range {
                    Some(RangeSpec::Numeric(_, s)) => self.diag(Diagnostic::error(
                        "E1603",
                        "`range` applies only to `size` and `strokeWidth` scales",
                        *s,
                    )),
                    Some(RangeSpec::ColorMap(_, s)) => self.diag(Diagnostic::error(
                        "E1606",
                        "a category map `range` applies only to categorical scales",
                        *s,
                    )),
                    None => {}
                }
            }
            ScaleTargetIr::Aesthetic { aesthetic, column } => {
                let is_color = aesthetic == "fill" || aesthetic == "stroke";
                let numeric_col = column.as_ref().is_some_and(|c| {
                    matches!(
                        c.dtype,
                        DataType::Integer | DataType::Float | DataType::Unknown
                    )
                });

                if scale_type.is_some() || reverse.is_some() || integer.is_some() {
                    self.diag(Diagnostic::error(
                        "E1204",
                        "`type`, `reverse`, and `integer` apply only to axis scales",
                        span,
                    ));
                }

                if is_color {
                    let continuous = numeric_col;
                    if gradient.is_some() && !continuous {
                        self.diag(Diagnostic::error(
                            "E1602",
                            "`gradient` is valid only for continuous fill or stroke mappings",
                            gradient_span.unwrap_or(span),
                        ));
                    }
                    if let Some(s) = domain_span {
                        self.diag(Diagnostic::error(
                            "E1204",
                            "`domain` applies only to axis, `size`, or `strokeWidth` scales",
                            s,
                        ));
                        domain = None;
                    }
                    match &range {
                        Some(RangeSpec::ColorMap(entries, s)) => {
                            if continuous {
                                self.diag(Diagnostic::error(
                                    "E1606",
                                    "a category map `range` applies only to categorical scales",
                                    *s,
                                ));
                            } else {
                                color_map = Some(entries.clone());
                            }
                        }
                        Some(RangeSpec::Numeric(_, s)) => self.diag(Diagnostic::error(
                            "E1603",
                            "a numeric `range` applies only to `size` and `strokeWidth` scales",
                            *s,
                        )),
                        None => {}
                    }
                    // `range` and `labels` key sets must agree (spec §16.13).
                    if let (Some(cm), Some(lm), Some(s)) = (&color_map, &label_map, labels_span) {
                        let ck: HashSet<&str> = cm.iter().map(|(k, _)| k.as_str()).collect();
                        let lk: HashSet<&str> = lm.iter().map(|(k, _)| k.as_str()).collect();
                        if ck != lk {
                            self.diag(Diagnostic::error(
                                "E1604",
                                "`range` and `labels` map keys do not match",
                                s,
                            ));
                        }
                    }
                } else {
                    // size / strokeWidth: a continuous scale over a numeric column.
                    if !numeric_col {
                        let s = column.as_ref().map(|c| c.span).unwrap_or(span);
                        self.diag(Diagnostic::error(
                            "E1607",
                            format!("`{aesthetic}` scale requires a numeric column"),
                            s,
                        ));
                    }
                    if palette.is_some() || gradient.is_some() {
                        self.diag(Diagnostic::error(
                            "E1204",
                            "`palette` and `gradient` apply only to fill or stroke scales",
                            span,
                        ));
                    }
                    if let Some(s) = labels_span {
                        self.diag(Diagnostic::error(
                            "E1606",
                            "`labels` maps apply only to categorical scales",
                            s,
                        ));
                        label_map = None;
                    }
                    match &range {
                        Some(RangeSpec::Numeric(bounds, _)) => range_numeric = Some(*bounds),
                        Some(RangeSpec::ColorMap(_, s)) => self.diag(Diagnostic::error(
                            "E1606",
                            "a category map `range` applies only to categorical scales",
                            *s,
                        )),
                        None => {}
                    }
                }
            }
        }

        Some(ScaleIr {
            target,
            scale_type,
            domain,
            range: range_numeric,
            reverse,
            integer,
            palette,
            gradient,
            color_map,
            label_map,
            label,
            span,
        })
    }

    /// Parse a two-element numeric bounds array where each element may be a
    /// number or `null` (`[0, null]`, spec §16.11). Returns `None` for any other
    /// shape so the caller can emit a targeted diagnostic.
    fn numeric_bounds(&mut self, value: &ValueExpr) -> Option<[Option<f64>; 2]> {
        let ValueExpr::Array(array) = value else {
            return None;
        };
        let elems = array.values();
        if elems.len() != 2 {
            return None;
        }
        let mut out = [None, None];
        for (i, elem) in elems.iter().enumerate() {
            match elem {
                ValueExpr::Literal(lit) => match lit.kind() {
                    Some(LiteralKind::Number) => {
                        let n = lit.text().and_then(|t| t.parse::<f64>().ok())?;
                        if !n.is_finite() {
                            return None;
                        }
                        out[i] = Some(n);
                    }
                    Some(LiteralKind::Null) => out[i] = None,
                    _ => return None,
                },
                _ => return None,
            }
        }
        Some(out)
    }

    /// Read a map literal of string keys to string values (used by a categorical
    /// scale's `range:` and `labels:`, spec §16.13). Emits `E1604` for malformed
    /// entries and returns the entries in source order.
    fn color_map_entries(&mut self, map: &MapValue) -> Option<Vec<(String, String)>> {
        let mut out = Vec::new();
        for entry in map.entries() {
            let key = match entry.key() {
                Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                    string_value(&lit.text().unwrap_or_default())
                }
                other => {
                    let s = other
                        .map(|v| node_span(v.syntax()))
                        .unwrap_or_else(|| node_span(map.syntax()));
                    self.diag(Diagnostic::error(
                        "E1604",
                        "map keys must be string literals",
                        s,
                    ));
                    return None;
                }
            };
            let val = match entry.value() {
                Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                    string_value(&lit.text().unwrap_or_default())
                }
                other => {
                    let s = other
                        .map(|v| node_span(v.syntax()))
                        .unwrap_or_else(|| node_span(map.syntax()));
                    self.diag(Diagnostic::error(
                        "E1604",
                        "map values must be string literals",
                        s,
                    ));
                    return None;
                }
            };
            out.push((key, val));
        }
        Some(out)
    }

    fn set_scale_target(
        &mut self,
        target: &mut Option<ScaleTargetIr>,
        next: ScaleTargetIr,
        span: Span,
    ) {
        if target.is_some() {
            self.diag(Diagnostic::error(
                "E1204",
                "`Scale` accepts only one target",
                span,
            ));
        } else {
            *target = Some(next);
        }
    }
}
