//! Scale declaration analysis (spec §16.11–16.13): target selection, domain and
//! range bounds, palettes/gradients, and categorical color/label maps.

use std::collections::HashSet;

use algraf_core::{codes, Diagnostic, Span};
use algraf_data::{parse_temporal_literal, DataType};
use algraf_syntax::ast::{AlgebraExpr, CallValue, Decl, LiteralKind, MapValue, ValueExpr};
use algraf_syntax::{node_span, unescape_string_literal as string_value};

use super::args::DupGuard;
use super::context::{ActiveTable, Analyzer, ValueForm};
use super::properties::is_color_literal;
use crate::ir::*;
use crate::registry;

/// A parsed `Scale(range: ...)` declaration before the target is known.
enum RangeSpec {
    /// Two numeric output bounds, each possibly inferred (`[0, 30]`).
    Numeric([Option<f64>; 2], Span),
    /// A manual category → color map (`["A" => "burlywood"]`).
    ColorMap(Vec<(String, String)>, Span),
    /// Ordered color stops for a binned color scale.
    ColorArray(Vec<String>, Span),
}

fn range_span(range: &RangeSpec) -> Span {
    match range {
        RangeSpec::Numeric(_, span)
        | RangeSpec::ColorMap(_, span)
        | RangeSpec::ColorArray(_, span) => *span,
    }
}

impl Analyzer<'_> {
    pub(super) fn scale_decl(&mut self, decl: &Decl, table: &ActiveTable) -> Option<ScaleIr> {
        let span = node_span(decl.syntax());
        let mut dup = DupGuard::new(codes::E1002, "Scale argument");
        let mut target: Option<ScaleTargetIr> = None;
        let mut scale_type = None;
        let mut mode = None;
        let mut domain: Option<[Option<f64>; 2]> = None;
        let mut categorical_domain: Option<Vec<String>> = None;
        let mut domain_span: Option<Span> = None;
        let mut breaks: Option<Vec<f64>> = None;
        let mut breaks_span: Option<Span> = None;
        let mut break_labels: Option<Vec<String>> = None;
        let mut break_labels_span: Option<Span> = None;
        let mut expansion = None;
        let mut range: Option<RangeSpec> = None;
        let mut reverse = None;
        let mut integer = None;
        let mut palette = None;
        let mut gradient: Option<GradientIr> = None;
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
                key if registry::SCALE_AESTHETIC_TARGETS.contains(&key) => match arg.value() {
                    Some(ValueExpr::Algebra(AlgebraExpr::Name(name))) => {
                        let column = if name.name().as_deref() == Some("series")
                            && !table.has_unknown_columns()
                            && table.get("series").is_none()
                        {
                            ColumnRef {
                                name: "series".into(),
                                dtype: DataType::String,
                                span: name
                                    .ident_span()
                                    .unwrap_or_else(|| node_span(name.syntax())),
                            }
                        } else {
                            self.resolve_column(&name, table)
                        };
                        self.set_scale_target(
                            &mut target,
                            ScaleTargetIr::Aesthetic {
                                aesthetic: key.to_string(),
                                column: Some(column),
                            },
                            key_span,
                        );
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E1204,
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
                            "sqrt" => scale_type = Some(ScaleTypeIr::Sqrt),
                            "categorical" => scale_type = Some(ScaleTypeIr::Categorical),
                            _ => self.diag(Diagnostic::error(
                                codes::E1204,
                                format!("unknown scale type `{value}`"),
                                node_span(lit.syntax()),
                            )),
                        }
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E1204,
                        "`type` expects a string literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "mode" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        let value = string_value(&lit.text().unwrap_or_default());
                        match value.as_str() {
                            "binned" => mode = Some(ScaleModeIr::Binned),
                            "identity" => mode = Some(ScaleModeIr::Identity),
                            _ => self.diag(Diagnostic::error(
                                codes::E1204,
                                format!("unknown scale mode `{value}`"),
                                node_span(lit.syntax()),
                            )),
                        }
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E1204,
                        "`mode` expects a string literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "domain" => {
                    if let Some(value) = arg.value() {
                        domain_span = Some(node_span(value.syntax()));
                        match self.numeric_bounds(&value) {
                            Some(bounds) => domain = Some(bounds),
                            None => match self.categorical_domain(&value) {
                                Some(values) => categorical_domain = Some(values),
                                None => self.diag(Diagnostic::error(
                                    codes::E1204,
                                    "`domain` expects [min, max] numeric bounds or a non-empty string array",
                                    node_span(value.syntax()),
                                )),
                            },
                        }
                    }
                }
                "breaks" => {
                    if let Some(value) = arg.value() {
                        breaks_span = Some(node_span(value.syntax()));
                        breaks = self.break_values(&value);
                    }
                }
                "expand" | "expansion" => {
                    if let Some(value) = arg.value() {
                        expansion = self.scale_expansion(&value);
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
                                None => match self.color_array(&value) {
                                    Some(colors) => {
                                        range = Some(RangeSpec::ColorArray(colors, value_span))
                                    }
                                    None => self.diag(Diagnostic::error(
                                        codes::E1603,
                                        "`range` expects two numeric values, a color array, or a category map",
                                        value_span,
                                    )),
                                },
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
                            _ => match ValueForm::of(&value) {
                                ValueForm::StringArray(Some(values)) => {
                                    break_labels_span = Some(node_span(value.syntax()));
                                    break_labels = Some(values);
                                }
                                _ => self.diag(Diagnostic::error(
                                    codes::E1606,
                                    "`labels` expects a string array or category map",
                                    node_span(value.syntax()),
                                )),
                            },
                        }
                    }
                }
                "reverse" => {
                    if let Some(b) =
                        self.expect_bool(&arg, codes::E1204, "`reverse` expects a boolean literal")
                    {
                        reverse = Some(b);
                    }
                }
                "integer" => {
                    if let Some(b) =
                        self.expect_bool(&arg, codes::E1204, "`integer` expects a boolean literal")
                    {
                        integer = Some(b);
                    }
                }
                "palette" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        let value = string_value(&lit.text().unwrap_or_default());
                        if registry::PALETTE_NAMES.contains(&value.as_str()) {
                            palette = Some(value);
                        } else {
                            self.diag(Diagnostic::error(
                                codes::E1204,
                                format!("unknown palette `{value}`"),
                                node_span(lit.syntax()),
                            ));
                        }
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E1204,
                        "`palette` expects a string literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "gradient" => {
                    let Some(value) = arg.value() else { continue };
                    gradient_span = Some(node_span(value.syntax()));
                    gradient = self.gradient_value(&value);
                }
                "label" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        label = Some(string_value(&lit.text().unwrap_or_default()));
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        codes::E1204,
                        "`label` expects a string literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                _ => self.diag(Diagnostic::error(
                    codes::E1003,
                    format!("unsupported Scale argument `{key}`"),
                    key_span,
                )),
            }
        }

        let Some(target) = target else {
            self.diag(Diagnostic::error(
                codes::E1204,
                "`Scale` requires `axis`, `fill`, `stroke`, `size`, or `strokeWidth`",
                span,
            ));
            return None;
        };

        // Split a `range:` declaration into its numeric and color-map forms once
        // the target is known, validating the form against the scale kind.
        let mut range_numeric: Option<[Option<f64>; 2]> = None;
        let mut color_map: Option<Vec<(String, String)>> = None;
        let mut color_range: Option<Vec<String>> = None;

        match &target {
            ScaleTargetIr::Axis(_) => {
                if palette.is_some() || gradient.is_some() || mode.is_some() {
                    self.diag(Diagnostic::error(
                        codes::E1204,
                        "`palette`, `gradient`, and `mode` apply only to aesthetic scales",
                        span,
                    ));
                }
                if let Some(map) = labels_span {
                    if label_map.is_some() {
                        self.diag(Diagnostic::error(
                            codes::E1606,
                            "`labels` maps apply only to categorical fill or stroke scales",
                            map,
                        ));
                        label_map = None;
                    }
                }
                match &range {
                    Some(RangeSpec::Numeric(_, s)) => self.diag(Diagnostic::error(
                        codes::E1603,
                        "`range` applies only to `size` and `strokeWidth` scales",
                        *s,
                    )),
                    Some(RangeSpec::ColorMap(_, s)) => self.diag(Diagnostic::error(
                        codes::E1606,
                        "a category map `range` applies only to categorical scales",
                        *s,
                    )),
                    Some(RangeSpec::ColorArray(_, s)) => self.diag(Diagnostic::error(
                        codes::E1606,
                        "a color-array `range` applies only to binned color scales",
                        *s,
                    )),
                    None => {}
                }
                if scale_type == Some(ScaleTypeIr::Categorical) {
                    if let Some(s) = domain_span.filter(|_| domain.is_some()) {
                        self.diag(Diagnostic::error(
                            codes::E1204,
                            "`type: \"categorical\"` cannot use a numeric `domain`; use a string-array domain",
                            s,
                        ));
                    }
                    if let Some(s) = breaks_span {
                        self.diag(Diagnostic::error(
                            codes::E1204,
                            "`breaks` applies only to continuous or temporal axis scales",
                            s,
                        ));
                    }
                    if integer.is_some() {
                        self.diag(Diagnostic::error(
                            codes::E1204,
                            "`integer` applies only to continuous axis scales",
                            span,
                        ));
                    }
                }
            }
            ScaleTargetIr::Aesthetic { aesthetic, column } => {
                if categorical_domain.is_some() {
                    self.diag(Diagnostic::error(
                        codes::E1606,
                        "string-array `domain` applies only to position axes",
                        domain_span.unwrap_or(span),
                    ));
                    categorical_domain = None;
                }
                let is_color = aesthetic == "fill" || aesthetic == "stroke";
                let numeric_col = column.as_ref().is_some_and(|c| {
                    matches!(
                        c.dtype,
                        DataType::Integer | DataType::Float | DataType::Unknown
                    )
                });

                if scale_type.is_some() || reverse.is_some() || integer.is_some() {
                    self.diag(Diagnostic::error(
                        codes::E1204,
                        "`type`, `reverse`, and `integer` apply only to axis scales",
                        span,
                    ));
                }

                if is_color {
                    let continuous = numeric_col;
                    if mode == Some(ScaleModeIr::Binned) && !continuous {
                        self.diag(Diagnostic::error(
                            codes::E1602,
                            "`mode: \"binned\"` requires a numeric fill or stroke column",
                            span,
                        ));
                    }
                    if mode == Some(ScaleModeIr::Identity)
                        && column.as_ref().is_some_and(|c| {
                            matches!(
                                c.dtype,
                                DataType::Integer
                                    | DataType::Float
                                    | DataType::Temporal
                                    | DataType::Geometry
                            )
                        })
                    {
                        let s = column.as_ref().map(|c| c.span).unwrap_or(span);
                        self.diag(Diagnostic::error(
                            codes::E1602,
                            "`mode: \"identity\"` requires a string-like color column",
                            s,
                        ));
                    }
                    if gradient.is_some() && !continuous {
                        self.diag(Diagnostic::error(
                            codes::E1602,
                            "`gradient` is valid only for continuous fill or stroke mappings",
                            gradient_span.unwrap_or(span),
                        ));
                    }
                    if mode == Some(ScaleModeIr::Identity)
                        && (gradient.is_some()
                            || palette.is_some()
                            || breaks.is_some()
                            || range.is_some()
                            || label_map.is_some()
                            || break_labels.is_some()
                            || domain.is_some())
                    {
                        self.diag(Diagnostic::error(
                            codes::E1204,
                            "`mode: \"identity\"` cannot be combined with scale mapping controls",
                            span,
                        ));
                    }
                    if let (Some(GradientIr::Positioned(stops)), Some([Some(a), Some(b)])) =
                        (&gradient, domain)
                    {
                        let lo = a.min(b);
                        let hi = a.max(b);
                        for stop in stops {
                            if stop.value < lo || stop.value > hi {
                                self.diag(Diagnostic::error(
                                    codes::E1601,
                                    format!(
                                        "gradient stop value {} is outside explicit domain [{lo}, {hi}]",
                                        stop.value
                                    ),
                                    gradient_span.unwrap_or(span),
                                ));
                            }
                        }
                    }
                    match &range {
                        Some(RangeSpec::ColorMap(entries, s)) => {
                            if continuous || mode == Some(ScaleModeIr::Binned) {
                                self.diag(Diagnostic::error(
                                    codes::E1606,
                                    "a category map `range` applies only to categorical scales",
                                    *s,
                                ));
                            } else {
                                color_map = Some(entries.clone());
                            }
                        }
                        Some(RangeSpec::ColorArray(colors, s)) => {
                            if mode == Some(ScaleModeIr::Binned) {
                                color_range = Some(colors.clone());
                            } else {
                                self.diag(Diagnostic::error(
                                    codes::E1606,
                                    "a color-array `range` applies only to binned color scales",
                                    *s,
                                ));
                            }
                        }
                        Some(RangeSpec::Numeric(_, s)) => self.diag(Diagnostic::error(
                            codes::E1603,
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
                                codes::E1604,
                                "`range` and `labels` map keys do not match",
                                s,
                            ));
                        }
                    }
                    if label_map.is_some() && (continuous || mode == Some(ScaleModeIr::Binned)) {
                        if let Some(s) = labels_span {
                            self.diag(Diagnostic::error(
                                codes::E1606,
                                "`labels` maps apply only to categorical scales",
                                s,
                            ));
                        }
                        label_map = None;
                    }
                } else {
                    // size / strokeWidth: a continuous scale over a numeric column.
                    if !numeric_col {
                        let s = column.as_ref().map(|c| c.span).unwrap_or(span);
                        self.diag(Diagnostic::error(
                            codes::E1607,
                            format!("`{aesthetic}` scale requires a numeric column"),
                            s,
                        ));
                    }
                    if palette.is_some() || gradient.is_some() {
                        self.diag(Diagnostic::error(
                            codes::E1204,
                            "`palette` and `gradient` apply only to fill or stroke scales",
                            span,
                        ));
                    }
                    if mode.is_some() {
                        self.diag(Diagnostic::error(
                            codes::E1204,
                            "`mode` applies only to fill or stroke scales",
                            span,
                        ));
                    }
                    if let Some(s) = labels_span {
                        if label_map.is_some() {
                            self.diag(Diagnostic::error(
                                codes::E1606,
                                "`labels` maps apply only to categorical scales",
                                s,
                            ));
                            label_map = None;
                        }
                    }
                    match &range {
                        Some(RangeSpec::Numeric(bounds, _)) => range_numeric = Some(*bounds),
                        Some(RangeSpec::ColorMap(_, s)) => self.diag(Diagnostic::error(
                            codes::E1606,
                            "a category map `range` applies only to categorical scales",
                            *s,
                        )),
                        Some(RangeSpec::ColorArray(_, s)) => self.diag(Diagnostic::error(
                            codes::E1606,
                            "a color-array `range` applies only to binned color scales",
                            *s,
                        )),
                        None => {}
                    }
                }
            }
        }
        if let (Some(labels), Some(breaks), Some(s)) =
            (&break_labels, &breaks, break_labels_span.or(labels_span))
        {
            if labels.len() != breaks.len() {
                self.diag(Diagnostic::error(
                    codes::E1604,
                    "`labels` length must match `breaks` length",
                    s,
                ));
            }
        }
        if break_labels.is_some() && breaks.is_none() {
            self.diag(Diagnostic::error(
                codes::E1604,
                "`labels` arrays require `breaks`",
                break_labels_span.unwrap_or(span),
            ));
        }
        if mode == Some(ScaleModeIr::Binned) {
            if let (Some(colors), Some(breaks), Some(s)) =
                (&color_range, &breaks, range.as_ref().map(range_span))
            {
                if colors.len() != breaks.len() {
                    self.diag(Diagnostic::error(
                        codes::E1604,
                        "binned scale `range` length must match `breaks` length",
                        s,
                    ));
                }
            }
            if breaks.as_ref().is_some_and(|values| values.is_empty()) {
                self.diag(Diagnostic::error(
                    codes::E1604,
                    "`breaks` must not be empty for a binned scale",
                    breaks_span.unwrap_or(span),
                ));
            }
        }

        Some(ScaleIr {
            target,
            scale_type,
            mode,
            domain,
            categorical_domain,
            breaks,
            break_labels,
            expansion,
            range: range_numeric,
            color_range,
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
                // A `datetime("…")` / `date("…")` temporal literal is a valid
                // domain bound for a temporal axis (spec §7.8, §16.11); it lowers
                // to a UTC-equivalent instant in microseconds.
                ValueExpr::Call(call) => {
                    out[i] = Some(self.temporal_literal_bound(call)? as f64);
                }
                _ => return None,
            }
        }
        Some(out)
    }

    /// Parse an explicit categorical position-axis domain. Values are retained
    /// in source order; duplicates and empty arrays are authoring errors because
    /// they would make trained band domains ambiguous.
    fn categorical_domain(&mut self, value: &ValueExpr) -> Option<Vec<String>> {
        let ValueForm::StringArray(Some(values)) = ValueForm::of(value) else {
            return None;
        };
        if values.is_empty() {
            self.diag(Diagnostic::error(
                codes::E1604,
                "categorical `domain` must not be empty",
                node_span(value.syntax()),
            ));
            return None;
        }
        let mut seen = HashSet::new();
        for category in &values {
            if !seen.insert(category.clone()) {
                self.diag(Diagnostic::error(
                    codes::E1604,
                    format!("duplicate category `{category}` in explicit domain"),
                    node_span(value.syntax()),
                ));
                return None;
            }
        }
        Some(values)
    }

    fn break_values(&mut self, value: &ValueExpr) -> Option<Vec<f64>> {
        let ValueExpr::Array(array) = value else {
            self.diag(Diagnostic::error(
                codes::E1204,
                "`breaks` expects an array of numbers or temporal literals",
                node_span(value.syntax()),
            ));
            return None;
        };
        let mut out = Vec::new();
        for elem in array.values() {
            match &elem {
                ValueExpr::Literal(lit) if lit.kind() == Some(LiteralKind::Number) => {
                    let Some(n) = lit.text().and_then(|t| t.parse::<f64>().ok()) else {
                        self.diag(Diagnostic::error(
                            codes::E1204,
                            "`breaks` values must be finite numbers",
                            node_span(elem.syntax()),
                        ));
                        return None;
                    };
                    if !n.is_finite() {
                        self.diag(Diagnostic::error(
                            codes::E1204,
                            "`breaks` values must be finite numbers",
                            node_span(elem.syntax()),
                        ));
                        return None;
                    }
                    out.push(n);
                }
                ValueExpr::Call(call) => out.push(self.temporal_literal_bound(call)? as f64),
                _ => {
                    self.diag(Diagnostic::error(
                        codes::E1204,
                        "`breaks` values must be numbers or temporal literals",
                        node_span(elem.syntax()),
                    ));
                    return None;
                }
            }
        }
        if !out.windows(2).all(|pair| pair[0] < pair[1]) {
            self.diag(Diagnostic::error(
                codes::E1604,
                "`breaks` values must be strictly increasing",
                node_span(value.syntax()),
            ));
            return None;
        }
        Some(out)
    }

    fn scale_expansion(&mut self, value: &ValueExpr) -> Option<ScaleExpansionIr> {
        match ValueForm::of(value) {
            ValueForm::Number(n) if n.is_finite() && n >= 0.0 => {
                Some(ScaleExpansionIr { mult: n, add: 0.0 })
            }
            ValueForm::Array(Some(values))
                if values.len() == 2 && values.iter().all(|n| n.is_finite() && *n >= 0.0) =>
            {
                Some(ScaleExpansionIr {
                    mult: values[0],
                    add: values[1],
                })
            }
            _ => {
                self.diag(Diagnostic::error(
                    codes::E1204,
                    "`expand` expects a non-negative number or [mult, add]",
                    node_span(value.syntax()),
                ));
                None
            }
        }
    }

    /// Parse a `datetime("…")` / `date("…")` domain bound to microseconds,
    /// emitting `E1017` for an unrecognized constructor or contents (spec §10.3).
    fn temporal_literal_bound(&mut self, call: &CallValue) -> Option<i64> {
        let name = call.name().unwrap_or_default();
        let require_date = match name.as_str() {
            "date" => true,
            "datetime" => false,
            _ => return None,
        };
        let span = node_span(call.syntax());
        let args = call.args();
        let text = match args.first() {
            Some(arg) if args.len() == 1 && arg.key().is_none() => match arg.value() {
                Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                    string_value(&lit.text().unwrap_or_default())
                }
                _ => return self.reject_temporal_bound(&name, span),
            },
            _ => return self.reject_temporal_bound(&name, span),
        };
        match parse_temporal_literal(&text, require_date) {
            Some(micros) => Some(micros),
            None => {
                self.diag(Diagnostic::error(
                    codes::E1017,
                    format!(
                        "{text:?} is not a recognized {} literal",
                        if require_date { "date" } else { "datetime" }
                    ),
                    span,
                ));
                None
            }
        }
    }

    fn reject_temporal_bound(&mut self, name: &str, span: Span) -> Option<i64> {
        self.diag(Diagnostic::error(
            codes::E1017,
            format!("`{name}(...)` expects a single quoted temporal string"),
            span,
        ));
        None
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
                        codes::E1604,
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
                        codes::E1604,
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

    fn color_array(&mut self, value: &ValueExpr) -> Option<Vec<String>> {
        let ValueForm::StringArray(Some(values)) = ValueForm::of(value) else {
            return None;
        };
        if values.is_empty() {
            self.diag(Diagnostic::error(
                codes::E1603,
                "a binned color `range` must contain at least one color",
                node_span(value.syntax()),
            ));
            return None;
        }
        for color in &values {
            if !is_color_literal(color) {
                self.diag(Diagnostic::error(
                    codes::E1604,
                    format!("invalid range color `{color}`"),
                    node_span(value.syntax()),
                ));
                return None;
            }
        }
        Some(values)
    }

    fn gradient_value(&mut self, value: &ValueExpr) -> Option<GradientIr> {
        let ValueExpr::Array(array) = value else {
            self.diag(Diagnostic::error(
                codes::E1601,
                "`gradient` expects an array of two or more color strings or Stop(...) values",
                node_span(value.syntax()),
            ));
            return None;
        };
        let values = array.values();
        if values.len() < 2 {
            self.diag(Diagnostic::error(
                codes::E1601,
                "`gradient` requires at least two stops",
                node_span(value.syntax()),
            ));
            return None;
        }

        let has_string = values.iter().any(|item| {
            matches!(item, ValueExpr::Literal(lit) if lit.kind() == Some(LiteralKind::String))
        });
        let has_stop = values.iter().any(
            |item| matches!(item, ValueExpr::Call(call) if call.name().as_deref() == Some("Stop")),
        );
        if has_string && has_stop {
            self.diag(Diagnostic::error(
                codes::E1601,
                "`gradient` cannot mix color strings and Stop(...) values",
                node_span(value.syntax()),
            ));
            return None;
        }

        if has_string {
            let mut colors = Vec::new();
            for item in values {
                match item {
                    ValueExpr::Literal(lit) if lit.kind() == Some(LiteralKind::String) => {
                        let color = string_value(&lit.text().unwrap_or_default());
                        if is_color_literal(&color) {
                            colors.push(color);
                        } else {
                            self.diag(Diagnostic::error(
                                codes::E1601,
                                format!("invalid gradient color `{color}`"),
                                node_span(lit.syntax()),
                            ));
                            return None;
                        }
                    }
                    other => {
                        self.diag(Diagnostic::error(
                            codes::E1601,
                            "`gradient` string form accepts only color strings",
                            node_span(other.syntax()),
                        ));
                        return None;
                    }
                }
            }
            return Some(GradientIr::Even(colors));
        }

        let mut stops = Vec::new();
        for item in values {
            match item {
                ValueExpr::Call(call) if call.name().as_deref() == Some("Stop") => {
                    if let Some(stop) = self.gradient_stop(&call) {
                        stops.push(stop);
                    } else {
                        return None;
                    }
                }
                other => {
                    self.diag(Diagnostic::error(
                        codes::E1601,
                        "`gradient` positioned form accepts only Stop(...) values",
                        node_span(other.syntax()),
                    ));
                    return None;
                }
            }
        }
        for pair in stops.windows(2) {
            if pair[0].value >= pair[1].value {
                self.diag(Diagnostic::error(
                    codes::E1601,
                    "gradient stop values must be strictly increasing",
                    node_span(value.syntax()),
                ));
                return None;
            }
        }
        Some(GradientIr::Positioned(stops))
    }

    fn gradient_stop(&mut self, call: &CallValue) -> Option<GradientStopIr> {
        let mut value = None;
        let mut color = None;
        let mut seen = HashSet::new();
        let mut ok = true;
        for arg in call.args() {
            let span = node_span(arg.syntax());
            let Some(key) = arg.key() else {
                self.diag(Diagnostic::error(
                    codes::E1601,
                    "`Stop(...)` arguments must be named",
                    span,
                ));
                ok = false;
                continue;
            };
            if !seen.insert(key.clone()) {
                self.diag(Diagnostic::error(
                    codes::E1601,
                    format!("duplicate Stop argument `{key}`"),
                    span,
                ));
                ok = false;
                continue;
            }
            match key.as_str() {
                "value" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Number) => {
                        let n = lit
                            .text()
                            .and_then(|text| text.parse::<f64>().ok())
                            .unwrap_or(f64::NAN);
                        if n.is_finite() {
                            value = Some(n);
                        } else {
                            self.diag(Diagnostic::error(
                                codes::E1601,
                                "`Stop(value:)` expects a finite number",
                                node_span(lit.syntax()),
                            ));
                            ok = false;
                        }
                    }
                    Some(other) => {
                        self.diag(Diagnostic::error(
                            codes::E1601,
                            "`Stop(value:)` expects a finite number",
                            node_span(other.syntax()),
                        ));
                        ok = false;
                    }
                    None => ok = false,
                },
                "color" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        let c = string_value(&lit.text().unwrap_or_default());
                        if is_color_literal(&c) {
                            color = Some(c);
                        } else {
                            self.diag(Diagnostic::error(
                                codes::E1601,
                                format!("invalid gradient color `{c}`"),
                                node_span(lit.syntax()),
                            ));
                            ok = false;
                        }
                    }
                    Some(other) => {
                        self.diag(Diagnostic::error(
                            codes::E1601,
                            "`Stop(color:)` expects a color string",
                            node_span(other.syntax()),
                        ));
                        ok = false;
                    }
                    None => ok = false,
                },
                _ => {
                    self.diag(Diagnostic::error(
                        codes::E1601,
                        format!("unknown Stop argument `{key}`"),
                        span,
                    ));
                    ok = false;
                }
            }
        }
        if value.is_none() {
            self.diag(Diagnostic::error(
                codes::E1601,
                "`Stop(...)` requires `value:`",
                node_span(call.syntax()),
            ));
            ok = false;
        }
        if color.is_none() {
            self.diag(Diagnostic::error(
                codes::E1601,
                "`Stop(...)` requires `color:`",
                node_span(call.syntax()),
            ));
            ok = false;
        }
        ok.then(|| GradientStopIr {
            value: value.unwrap_or(0.0),
            color: color.unwrap_or_default(),
        })
    }

    fn set_scale_target(
        &mut self,
        target: &mut Option<ScaleTargetIr>,
        next: ScaleTargetIr,
        span: Span,
    ) {
        if target.is_some() {
            self.diag(Diagnostic::error(
                codes::E1204,
                "`Scale` accepts only one target",
                span,
            ));
        } else {
            *target = Some(next);
        }
    }
}
