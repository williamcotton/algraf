//! Geometry and property analysis (spec §13.6, §13.9–13.13): geometry
//! recognition, per-property type checking, and the dodged-bar hint.

use std::collections::{HashMap, HashSet};

use algraf_core::{codes, is_url_like, Diagnostic, DiagnosticCode, Severity, Span};
use algraf_data::{parse_temporal_literal, DataType};
use algraf_syntax::ast::{AlgebraExpr, Arg, CallValue, GeometryCall, LiteralKind, ValueExpr};
use algraf_syntax::{node_span, unescape_string_literal};

use super::context::{ActiveTable, Analyzer, StyleFragmentLookup, ValueForm};
use super::frames::contains_nested;
use crate::ir::*;
use crate::registry::{self, Accept, GeometryDef, PropSpec};
use algraf_core::closest;

enum PropOutcome {
    Mapping(ColumnRef),
    Setting(SettingValue),
    Invalid,
}

struct EffectiveArg {
    key: String,
    arg: Arg,
    span: Span,
    from_style: bool,
}

impl Analyzer<'_> {
    // --- Geometry (spec §13.6, §13.9–13.13) ---

    pub(super) fn geometry(
        &mut self,
        call: &GeometryCall,
        frame: &FrameIr,
        coords: &CoordsIr,
        table: &ActiveTable,
    ) -> Option<GeometryIr> {
        let span = node_span(call.syntax());
        let name = call.name().unwrap_or_default();

        let def = match registry::geometry(&name) {
            Some(def) => def,
            None => {
                let mut diag =
                    Diagnostic::error(codes::E1201, format!("unknown geometry `{name}`"), span);
                if let Some(suggestion) = closest(&name, registry::geometry_names()) {
                    diag = diag.with_help(format!("did you mean `{suggestion}`?"));
                }
                self.diag(diag);
                return None;
            }
        };

        let args = self.expand_style_args(&call.args());
        let mut seen: HashSet<String> = HashSet::new();
        let mut seen_for_duplicates: HashMap<String, (Span, bool)> = HashMap::new();
        let mut mappings = Vec::new();
        let mut settings = Vec::new();
        let mut interaction = InteractionIr::default();

        for effective in &args {
            let key = &effective.key;
            let key_span = effective.span;

            if let Some((first, first_from_style)) = seen_for_duplicates.get(key).copied() {
                if !first_from_style && !effective.from_style {
                    self.diag(
                        Diagnostic::error(
                            codes::E1203,
                            format!("duplicate property `{key}`"),
                            key_span,
                        )
                        .with_related(first, "first defined here"),
                    );
                    continue;
                }
            }
            seen_for_duplicates.insert(key.clone(), (key_span, effective.from_style));
            seen.insert(key.clone());

            // Declarative interactions (`tooltip:` / `highlight:`, spec §14.25)
            // carry a distinct value shape and are not in any geometry's
            // `PropSpec` list; lower them directly before the property lookup.
            if registry::INTERACTION_PROPS.contains(&key.as_str()) {
                self.lower_interaction(def, &effective.arg, key, key_span, table, &mut interaction);
                continue;
            }

            let Some(prop) = def.prop(key) else {
                self.unknown_property(def, key, key_span);
                continue;
            };

            mappings.retain(|mapping: &AestheticMapping| mapping.aesthetic != prop.key);
            settings.retain(|setting: &GeometrySetting| setting.name != prop.key);

            match self.check_property(prop, &effective.arg, table) {
                PropOutcome::Mapping(column) => mappings.push(AestheticMapping {
                    aesthetic: prop.key,
                    column,
                    span: key_span,
                }),
                PropOutcome::Setting(value) => settings.push(GeometrySetting {
                    name: prop.key,
                    value,
                    span: key_span,
                }),
                PropOutcome::Invalid => {}
            }
        }

        for prop in def.props.iter().filter(|p| p.required) {
            if !seen.contains(prop.name) {
                self.diag(Diagnostic::error(
                    codes::E1205,
                    format!("`{}` requires property `{}`", def.name, prop.name),
                    span,
                ));
            }
        }

        self.bar_dodge_hint(def, frame, coords, &mappings, &settings, span);
        self.check_area_geometry(def, frame, &mappings, &settings, span);
        self.check_label_geometry(def, frame, span);
        self.check_polar_radius(def, coords, &mappings, span);
        self.check_image_src(def, &mappings, &settings);
        self.check_tile_fraction_settings(def, &mut settings);
        self.check_numeric_format(def, &mappings, &settings);
        self.check_text_time_format(def, &mappings, &mut settings);

        Some(GeometryIr {
            kind: def.kind,
            mappings,
            settings,
            interaction,
            span,
        })
    }

    /// Lower a declarative interaction property (`tooltip:` / `highlight:`,
    /// spec §14.25). Interactions are inert data: columns are validated to
    /// exist, but no callbacks, expressions, or scripts are accepted. Schema
    /// alone is enough to validate them — no data rows are materialized.
    fn lower_interaction(
        &mut self,
        def: &GeometryDef,
        arg: &Arg,
        key: &str,
        key_span: Span,
        table: &ActiveTable,
        interaction: &mut InteractionIr,
    ) {
        if !registry::supports_interaction(def.kind) {
            self.diag(
                Diagnostic::error(
                    codes::E1206,
                    format!("`{key}` is not supported on `{}`", def.name),
                    key_span,
                )
                .with_help("interaction metadata is supported on Point, Bar, Rect, and Tile"),
            );
            return;
        }
        let Some(value) = arg.value() else {
            return;
        };
        match key {
            "tooltip" => interaction.tooltip = self.interaction_columns(&value, table),
            "highlight" => {
                if let Some(column) = self.interaction_key(&value, table) {
                    interaction.highlight = Some(column);
                }
            }
            _ => {}
        }
    }

    /// Lower a declarative event emitter attached to the preceding mark in a
    /// `Space`: `On(event: "click", emit: column)`.
    pub(super) fn lower_event_emitter(
        &mut self,
        call: &GeometryCall,
        target: &mut GeometryIr,
        table: &ActiveTable,
    ) {
        let span = node_span(call.syntax());
        if !registry::supports_interaction(target.kind) {
            self.diag(
                Diagnostic::error(
                    codes::E1913,
                    format!(
                        "`On(...)` is not supported on `{}`",
                        target.kind.display_name()
                    ),
                    span,
                )
                .with_help("event metadata is supported on Point, Image, Bar, Rect, and Tile"),
            );
            return;
        }
        if target.interaction.event.is_some() {
            self.diag(Diagnostic::error(
                codes::E1913,
                "`On(...)` event metadata is already declared for this geometry",
                span,
            ));
            return;
        }

        let args = call.args();
        let mut seen = HashSet::new();
        let mut event = None;
        let mut emit = None;

        for arg in &args {
            let key_span = node_span(arg.syntax());
            let Some(key) = arg.key() else {
                self.diag(Diagnostic::error(
                    codes::E1913,
                    "`On(...)` arguments must be named",
                    key_span,
                ));
                continue;
            };
            if !seen.insert(key.clone()) {
                self.diag(Diagnostic::error(
                    codes::E1913,
                    format!("duplicate `On` argument `{key}`"),
                    key_span,
                ));
                continue;
            }
            match key.as_str() {
                "event" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        let value = unescape_string_literal(&lit.text().unwrap_or_default());
                        if value == "click" {
                            event = Some(value);
                        } else {
                            self.diag(
                                Diagnostic::error(
                                    codes::E1913,
                                    format!("unsupported `On` event {value:?}"),
                                    node_span(lit.syntax()),
                                )
                                .with_help("v0.64 accepts only `event: \"click\"`"),
                            );
                        }
                    }
                    Some(value) => self.diag(
                        Diagnostic::error(
                            codes::E1913,
                            "`On(event:)` expects the string literal \"click\"",
                            node_span(value.syntax()),
                        )
                        .with_help("e.g. `On(event: \"click\", emit: zone)`"),
                    ),
                    None => {}
                },
                "emit" => {
                    if let Some(value) = arg.value() {
                        emit = self.event_emit_column(&value, table);
                    }
                }
                other => self.diag(Diagnostic::error(
                    codes::E1913,
                    format!("unknown `On` argument `{other}`"),
                    key_span,
                )),
            }
        }

        if !seen.contains("event") {
            self.diag(Diagnostic::error(
                codes::E1913,
                "`On(...)` requires `event: \"click\"`",
                span,
            ));
        }
        if !seen.contains("emit") {
            self.diag(Diagnostic::error(
                codes::E1913,
                "`On(...)` requires `emit: column`",
                span,
            ));
        }
        if let (Some(event), Some(emit)) = (event, emit) {
            target.interaction.event = Some(EventEmitIr { event, emit, span });
        }
    }

    /// Resolve a `tooltip:` value to the ordered columns it names: a single
    /// column, or an array of columns. Each must reference an existing column.
    fn interaction_columns(&mut self, value: &ValueExpr, table: &ActiveTable) -> Vec<ColumnRef> {
        match value {
            ValueExpr::Algebra(AlgebraExpr::Name(name)) => vec![self.resolve_column(name, table)],
            ValueExpr::Array(array) => {
                let mut columns = Vec::new();
                for item in array.values() {
                    match &item {
                        ValueExpr::Algebra(AlgebraExpr::Name(name)) => {
                            columns.push(self.resolve_column(name, table))
                        }
                        other => self.diag(Diagnostic::error(
                            codes::E1207,
                            "`tooltip` array entries must be column names",
                            node_span(other.syntax()),
                        )),
                    }
                }
                columns
            }
            other => {
                self.diag(
                    Diagnostic::error(
                        codes::E1207,
                        "`tooltip` expects a column or an array of columns",
                        node_span(other.syntax()),
                    )
                    .with_help("e.g. `tooltip: species` or `tooltip: [species, body_mass]`"),
                );
                Vec::new()
            }
        }
    }

    /// Resolve a `highlight:` value to the grouping column it names: a bare
    /// column or a quoted column name. The column must exist.
    fn interaction_key(&mut self, value: &ValueExpr, table: &ActiveTable) -> Option<ColumnRef> {
        self.column_name_property(
            value,
            table,
            codes::E1207,
            "`highlight` expects a column name",
            "e.g. `highlight: species` or `highlight: \"species\"`",
        )
    }

    fn event_emit_column(&mut self, value: &ValueExpr, table: &ActiveTable) -> Option<ColumnRef> {
        self.column_name_property(
            value,
            table,
            codes::E1913,
            "`On(emit:)` expects a column name",
            "e.g. `On(event: \"click\", emit: zone)`",
        )
    }

    fn column_name_property(
        &mut self,
        value: &ValueExpr,
        table: &ActiveTable,
        wrong_form_code: DiagnosticCode,
        wrong_form_message: &'static str,
        wrong_form_help: &'static str,
    ) -> Option<ColumnRef> {
        match value {
            ValueExpr::Algebra(AlgebraExpr::Name(name)) => Some(self.resolve_column(name, table)),
            ValueExpr::Literal(lit) if lit.kind() == Some(LiteralKind::String) => {
                let name = unescape_string_literal(&lit.text().unwrap_or_default());
                let span = node_span(lit.syntax());
                match table.get(&name) {
                    Some(dtype) => Some(ColumnRef { name, dtype, span }),
                    None => {
                        if table.has_unknown_columns() {
                            return Some(ColumnRef {
                                name,
                                dtype: DataType::Unknown,
                                span,
                            });
                        }
                        self.diag(Diagnostic::error(
                            codes::E1101,
                            format!("unknown column `{name}`"),
                            span,
                        ));
                        Some(ColumnRef {
                            name,
                            dtype: DataType::Unknown,
                            span,
                        })
                    }
                }
            }
            other => {
                self.diag(
                    Diagnostic::error(
                        wrong_form_code,
                        wrong_form_message,
                        node_span(other.syntax()),
                    )
                    .with_help(wrong_form_help),
                );
                None
            }
        }
    }

    fn expand_style_args(&mut self, args: &[Arg]) -> Vec<EffectiveArg> {
        let mut out = Vec::new();
        for arg in args {
            let Some(key) = arg.key() else { continue };
            let span = node_span(arg.syntax());
            if key != "style" {
                out.push(EffectiveArg {
                    key,
                    arg: arg.clone(),
                    span,
                    from_style: false,
                });
                continue;
            }
            let Some(value) = arg.value() else {
                self.diag(Diagnostic::error(
                    codes::E1706,
                    "`style` expects a `Style(...)` fragment",
                    span,
                ));
                continue;
            };
            match self.style_fragment_for_value(&value) {
                StyleFragmentLookup::Found(entries) => {
                    for entry in entries {
                        out.push(EffectiveArg {
                            key: entry.key,
                            arg: entry.arg,
                            span: entry.span,
                            from_style: true,
                        });
                    }
                }
                StyleFragmentLookup::NotStyle => {
                    self.diag(Diagnostic::error(
                        codes::E1706,
                        "`style` expects a `Style(...)` fragment",
                        node_span(value.syntax()),
                    ));
                }
                StyleFragmentLookup::Invalid => {}
            }
        }
        out
    }

    fn unknown_property(&mut self, def: &GeometryDef, key: &str, span: Span) {
        let mut diag = Diagnostic::error(
            codes::E1202,
            format!("unknown property `{key}` on `{}`", def.name),
            span,
        );
        if key.eq_ignore_ascii_case("colour") || key.eq_ignore_ascii_case("color") {
            diag = diag.with_help(
                "choose `fill` or `stroke`; `colour` is not an alias because they differ",
            );
        } else if let Some(suggestion) = closest(key, def.prop_names()) {
            diag = diag.with_help(format!("did you mean `{suggestion}`?"));
        }
        self.diag(diag);
    }

    fn check_property(&mut self, prop: &PropSpec, arg: &Arg, table: &ActiveTable) -> PropOutcome {
        let Some(value) = arg.value() else {
            return PropOutcome::Invalid;
        };
        if matches!(value, ValueExpr::Error(_)) {
            return PropOutcome::Invalid;
        }
        // A `datetime("…")` / `date("…")` temporal literal (spec §7.8, §10.3) is
        // a typed value usable wherever a numeric position is accepted (e.g. a
        // reference-mark `x:`/`y:`); it lowers to a UTC-equivalent instant in
        // microseconds. Used anywhere else it is rejected (`E1018`).
        if let ValueExpr::Call(call) = &value {
            if let Some(require_date) = temporal_literal_kind(call.name().as_deref()) {
                if prop.accepts.contains(&Accept::Number) {
                    return self.temporal_literal_setting(call, require_date);
                }
                self.diag(Diagnostic::error(
                    codes::E1018,
                    format!(
                        "a temporal literal is not allowed for `{}`; it is only valid where a numeric position or domain bound is accepted",
                        prop.name
                    ),
                    node_span(call.syntax()),
                ));
                return PropOutcome::Invalid;
            }
        }

        if let Some((name, span)) = self.bare_let_reference(&value) {
            let can_be_column = prop.accepts.contains(&Accept::Column)
                && (table.has_unknown_columns() || table.get(&name).is_some());
            if !can_be_column {
                self.diag_bare_let_reference(&name, span);
                return PropOutcome::Invalid;
            }
        }

        // Resolve only sigiled `$name` let references in property value
        // positions. Bare identifiers remain column references.
        let form = self.value_form(&value);

        // Color literals written as bare identifiers (e.g. `fill: red`) are a
        // common mistake. If this property accepts a color and the value is a
        // bare identifier that names a known CSS color but no such column
        // exists, emit a hint to quote it (H3002).
        if prop.accepts.contains(&Accept::Color) {
            if let ValueForm::Column(name) = &form {
                let raw = name.name().unwrap_or_default();
                if !name.is_quoted()
                    && !table.has_unknown_columns()
                    && table.get(&raw).is_none()
                    && is_css_color_name(&raw)
                {
                    self.diag(
                        Diagnostic::new(
                            Severity::Hint,
                            codes::H3002,
                            format!("quote literal color name `{raw}` for clarity"),
                            node_span(name.syntax()),
                        )
                        .with_help(format!("write it as a string, e.g. {raw:?}")),
                    );
                }
            }
        }

        for accept in prop.accepts {
            match (accept, &form) {
                (Accept::Column, ValueForm::Column(name)) => {
                    return PropOutcome::Mapping(self.resolve_column(name, table));
                }
                (Accept::Number, ValueForm::Number(n)) => {
                    return PropOutcome::Setting(SettingValue::Number(*n));
                }
                (Accept::Color | Accept::Str, ValueForm::Str(s)) => {
                    return PropOutcome::Setting(SettingValue::String(s.clone()));
                }
                (Accept::Bool, ValueForm::Bool(b)) => {
                    return PropOutcome::Setting(SettingValue::Bool(*b));
                }
                (Accept::Enum(opts), ValueForm::Str(s)) if opts.contains(&s.as_str()) => {
                    return PropOutcome::Setting(SettingValue::String(s.clone()));
                }
                (Accept::NumberArray, ValueForm::Array(Some(nums))) => {
                    return PropOutcome::Setting(SettingValue::NumberArray(nums.clone()));
                }
                _ => {}
            }
        }

        // No accepted form matched: produce a precise type diagnostic.
        let span = node_span(value.syntax());
        let enum_opts = prop.accepts.iter().find_map(|a| match a {
            Accept::Enum(opts) => Some(*opts),
            _ => None,
        });
        if let (Some(opts), ValueForm::Column(name)) = (enum_opts, &form) {
            let written = name.name().unwrap_or_else(|| opts[0].to_string());
            self.diag(
                Diagnostic::error(
                    codes::E1204,
                    format!("`{}` expects a quoted string value", prop.name),
                    span,
                )
                .with_help(format!("write it as a string, e.g. {written:?}")),
            );
        } else {
            self.diag(Diagnostic::error(
                codes::E1204,
                format!(
                    "`{}` expects {}, found {}",
                    prop.name,
                    describe_accepts(prop.accepts),
                    form.describe()
                ),
                span,
            ));
        }
        PropOutcome::Invalid
    }

    /// Suggest nested algebra for dodged bars (hint H3001).
    fn bar_dodge_hint(
        &mut self,
        def: &GeometryDef,
        frame: &FrameIr,
        coords: &CoordsIr,
        mappings: &[AestheticMapping],
        settings: &[GeometrySetting],
        span: Span,
    ) {
        if def.kind != GeometryKind::Bar {
            return;
        }
        // Polar bars (coxcomb/wind rose) stack around the angle; dodging into
        // nested algebra is not the idiom there, so the hint is a false
        // positive under a polar transform.
        if matches!(coords, CoordsIr::Polar { .. }) {
            return;
        }
        let has_fill = mappings.iter().any(|m| m.aesthetic == PropertyKey::Fill);
        let stacked = settings.iter().any(|s| {
            s.name == PropertyKey::Layout
                && matches!(&s.value, SettingValue::String(v) if v != "identity")
        });
        // Only hint when the space is a flat Cartesian with no nesting; a
        // frame that already nests is the dodge form the hint would suggest.
        let plain_cartesian = matches!(frame, FrameIr::Cartesian(_)) && !contains_nested(frame);
        if has_fill && plain_cartesian && !stacked {
            self.diag(
                Diagnostic::new(
                    Severity::Hint,
                    codes::H3001,
                    "use nested algebra for dodged bars",
                    span,
                )
                .with_help("e.g. `Space((x / fill) * y)`, or set `layout: \"stack\"`"),
            );
        }
    }

    /// Validate a `radius:` mapping on a `Bar` (the polar `radial_bar` mode,
    /// spec §16.16). The mapping selects concentric rings, so it requires a polar
    /// space and a categorical column; otherwise emit `E1910`.
    fn check_polar_radius(
        &mut self,
        def: &GeometryDef,
        coords: &CoordsIr,
        mappings: &[AestheticMapping],
        span: Span,
    ) {
        if def.kind != GeometryKind::Bar {
            return;
        }
        let Some(mapping) = mappings.iter().find(|m| m.aesthetic == PropertyKey::Radius) else {
            return;
        };
        if !matches!(coords, CoordsIr::Polar { .. }) {
            self.diag(
                Diagnostic::error(
                    codes::E1910,
                    "`radius:` is only supported on a polar Bar (radial bar chart)",
                    span,
                )
                .with_help("add `coords: \"polar\", theta: \"y\"` to the enclosing Space"),
            );
            return;
        }
        if !mapping.column.dtype.is_categorical() {
            self.diag(Diagnostic::error(
                codes::E1910,
                format!(
                    "polar `radius:` requires a categorical column, but `{}` is not categorical",
                    mapping.column.name
                ),
                mapping.span,
            ));
        }
    }

    /// Validate Area's frame and grouped-layout requirements (v0.61). Area
    /// needs a numeric physical y axis. Stacked/fill layouts need an explicit
    /// grouping column or a categorical color mapping so the renderer can emit
    /// one polygon per series.
    fn check_area_geometry(
        &mut self,
        def: &GeometryDef,
        frame: &FrameIr,
        mappings: &[AestheticMapping],
        settings: &[GeometrySetting],
        span: Span,
    ) {
        if def.kind != GeometryKind::Area {
            return;
        }
        let y_col = frame_axis_index(frame, 1).and_then(vector_column);
        match y_col {
            Some(column)
                if matches!(
                    column.dtype,
                    DataType::Integer | DataType::Float | DataType::Unknown
                ) => {}
            Some(column) => self.diag(Diagnostic::error(
                codes::E1302,
                format!(
                    "Area requires a numeric y column, but `{}` is {:?}",
                    column.name, column.dtype
                ),
                column.span,
            )),
            None => self.diag(Diagnostic::error(
                codes::E1302,
                "Area requires a two-dimensional x * y space",
                span,
            )),
        }

        let grouped_layout = settings.iter().any(|setting| {
            setting.name == PropertyKey::Layout
                && matches!(&setting.value, SettingValue::String(value) if value == "stack" || value == "fill")
        });
        if !grouped_layout {
            return;
        }
        let usable_group = mappings.iter().any(|mapping| match mapping.aesthetic {
            PropertyKey::Group => !mapping.column.dtype.is_geometry(),
            PropertyKey::Fill | PropertyKey::Stroke => {
                mapping.column.dtype == DataType::Unknown || mapping.column.dtype.is_categorical()
            }
            _ => false,
        });
        if !usable_group {
            self.diag(
                Diagnostic::error(
                    codes::E1302,
                    "stacked Area layouts require `group:` or a categorical `fill:`/`stroke:` mapping",
                    span,
                )
                .with_help("e.g. `Area(fill: series, layout: \"stack\")`"),
            );
        }
    }

    /// Label is a terminal series label, so it needs a two-dimensional Cartesian
    /// frame with an x axis whose physical start/end can be compared.
    fn check_label_geometry(&mut self, def: &GeometryDef, frame: &FrameIr, span: Span) {
        if def.kind != GeometryKind::Label {
            return;
        }
        if frame_axis_index(frame, 0).is_none() || frame_axis_index(frame, 1).is_none() {
            self.diag(Diagnostic::error(
                codes::E1302,
                "Label requires a two-dimensional x * y space",
                span,
            ));
        }
    }

    /// Validate the `Image(src: ...)` source surface (spec §14.x). Literal
    /// values must be local file paths; mapped values must come from a stringy
    /// column so render-time asset loading never treats numeric data as paths.
    fn check_image_src(
        &mut self,
        def: &GeometryDef,
        mappings: &[AestheticMapping],
        settings: &[GeometrySetting],
    ) {
        if def.kind != GeometryKind::Image {
            return;
        }
        if let Some(mapping) = mappings.iter().find(|m| m.aesthetic == PropertyKey::Src) {
            if !matches!(
                mapping.column.dtype,
                DataType::String | DataType::Mixed | DataType::Unknown
            ) {
                self.diag(Diagnostic::error(
                    codes::E1204,
                    format!(
                        "`src` expects a string column, but `{}` is {:?}",
                        mapping.column.name, mapping.column.dtype
                    ),
                    mapping.span,
                ));
            }
        }
        if let Some(setting) = settings.iter().find(|s| s.name == PropertyKey::Src) {
            let SettingValue::String(value) = &setting.value else {
                return;
            };
            if is_url_like(value) {
                self.diag(
                    Diagnostic::error(
                        codes::E1204,
                        "`src` expects a local image path; URL-valued image sources are not supported",
                        setting.span,
                    )
                    .with_help("use a chart-relative local path such as \"logos/team.png\""),
                );
            }
        }
    }

    /// `Tile(width:, height:)` are fractions of the resolved x/y band sizes.
    /// Keep invalid literals out of the IR so render-time defaults apply.
    fn check_tile_fraction_settings(
        &mut self,
        def: &GeometryDef,
        settings: &mut Vec<GeometrySetting>,
    ) {
        if def.kind != GeometryKind::Tile {
            return;
        }
        let mut invalid = Vec::new();
        for (index, setting) in settings.iter().enumerate() {
            if !matches!(setting.name, PropertyKey::Width | PropertyKey::Height) {
                continue;
            }
            let SettingValue::Number(value) = setting.value else {
                continue;
            };
            if !value.is_finite() || value <= 0.0 || value > 1.0 {
                self.diag(Diagnostic::error(
                    codes::E1204,
                    format!("`{}` expects a finite number in (0, 1]", setting.name),
                    setting.span,
                ));
                invalid.push(index);
            }
        }
        for index in invalid.into_iter().rev() {
            settings.remove(index);
        }
    }

    /// Lower a `datetime("…")` / `date("…")` temporal literal to a numeric
    /// setting holding the UTC-equivalent instant in microseconds (spec §7.8,
    /// §10.3). Emits `E1017` for the wrong argument shape or unparseable contents.
    fn temporal_literal_setting(&mut self, call: &CallValue, require_date: bool) -> PropOutcome {
        let name = call.name().unwrap_or_default();
        let span = node_span(call.syntax());
        let args = call.args();
        let text = match args.first() {
            Some(arg) if args.len() == 1 && arg.key().is_none() => match arg.value() {
                Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                    Some(unescape_string_literal(&lit.text().unwrap_or_default()))
                }
                _ => None,
            },
            _ => None,
        };
        let Some(text) = text else {
            self.diag(Diagnostic::error(
                codes::E1017,
                format!(
                    "`{name}(...)` expects a single quoted temporal string, e.g. {name}(\"2026-01-01\")"
                ),
                span,
            ));
            return PropOutcome::Invalid;
        };
        match parse_temporal_literal(&text, require_date) {
            Some(micros) => PropOutcome::Setting(SettingValue::Number(micros as f64)),
            None => {
                self.diag(Diagnostic::error(
                    codes::E1017,
                    format!(
                        "{text:?} is not a recognized {} literal",
                        if require_date { "date" } else { "datetime" }
                    ),
                    span,
                ));
                PropOutcome::Invalid
            }
        }
    }

    /// Validate and resolve a `timeFormat:` on a `Text` label (off-axis temporal
    /// formatting, spec §19.4). The format must be a known named or valid custom
    /// format, and `label:` must map a temporal column; otherwise `E1907`. On
    /// success the stored setting is rewritten to the resolved chrono pattern so
    /// the renderer formats without re-parsing.
    fn check_text_time_format(
        &mut self,
        def: &GeometryDef,
        mappings: &[AestheticMapping],
        settings: &mut [GeometrySetting],
    ) {
        if def.kind != GeometryKind::Text {
            return;
        }
        if settings
            .iter()
            .any(|setting| setting.name == PropertyKey::Format)
        {
            return;
        }
        let Some(setting) = settings
            .iter_mut()
            .find(|s| s.name == PropertyKey::TimeFormat)
        else {
            return;
        };
        let SettingValue::String(raw) = &setting.value else {
            return;
        };
        let span = setting.span;
        let Some(format) = super::guides::temporal_format(raw) else {
            self.diag(Diagnostic::error(
                codes::E1907,
                format!("unknown or invalid temporal format `{raw}`"),
                span,
            ));
            return;
        };
        let label_is_temporal = mappings
            .iter()
            .find(|m| m.aesthetic == PropertyKey::Label)
            .is_some_and(|m| m.column.dtype == DataType::Temporal);
        if !label_is_temporal {
            self.diag(
                Diagnostic::error(
                    codes::E1907,
                    "`timeFormat` applies only to a temporal `label:` column",
                    span,
                )
                .with_help("map `label:` to a datetime/date column, or remove `timeFormat`"),
            );
            return;
        }
        setting.value = SettingValue::String(format.chrono_pattern().to_string());
    }

    /// Validate numeric text formatting for `Text` and `Label` (v0.61). The
    /// renderer receives only a known deterministic format string.
    fn check_numeric_format(
        &mut self,
        def: &GeometryDef,
        mappings: &[AestheticMapping],
        settings: &[GeometrySetting],
    ) {
        if !matches!(def.kind, GeometryKind::Text | GeometryKind::Label) {
            return;
        }
        let Some(setting) = settings
            .iter()
            .find(|setting| setting.name == PropertyKey::Format)
        else {
            return;
        };
        let SettingValue::String(raw) = &setting.value else {
            return;
        };
        if !matches!(
            raw.as_str(),
            ".0f" | ".1f" | ".2f" | "$.2f" | ".0%" | ".1%" | ".2%"
        ) {
            self.diag(Diagnostic::error(
                codes::E1908,
                format!("unknown numeric label format `{raw}`"),
                setting.span,
            ));
            return;
        }
        if settings
            .iter()
            .any(|setting| setting.name == PropertyKey::TimeFormat)
        {
            self.diag(Diagnostic::error(
                codes::E1908,
                "`format` and `timeFormat` cannot be used together",
                setting.span,
            ));
            return;
        }
        let Some(label) = mappings
            .iter()
            .find(|mapping| mapping.aesthetic == PropertyKey::Label)
        else {
            self.diag(
                Diagnostic::error(
                    codes::E1908,
                    "`format` applies only to a numeric `label:` column",
                    setting.span,
                )
                .with_help("map `label:` to a numeric column, or remove `format`"),
            );
            return;
        };
        if !matches!(
            label.column.dtype,
            DataType::Integer | DataType::Float | DataType::Unknown
        ) {
            self.diag(Diagnostic::error(
                codes::E1908,
                format!(
                    "`format` requires a numeric label column, but `{}` is {:?}",
                    label.column.name, label.column.dtype
                ),
                label.span,
            ));
        }
    }
}

fn frame_axis_index(frame: &FrameIr, index: usize) -> Option<&FrameIr> {
    match frame {
        FrameIr::Cartesian(axes) => axes.get(index),
        _ if index == 0 => Some(frame),
        _ => None,
    }
}

fn vector_column(frame: &FrameIr) -> Option<&ColumnRef> {
    match frame {
        FrameIr::Vector(column) => Some(column),
        _ => None,
    }
}

/// Classify a value-position call name as a temporal literal constructor:
/// `Some(true)` for `date(...)` (truncated to midnight), `Some(false)` for
/// `datetime(...)`, `None` for any other call (spec §7.8).
fn temporal_literal_kind(name: Option<&str>) -> Option<bool> {
    match name {
        Some("date") => Some(true),
        Some("datetime") => Some(false),
        _ => None,
    }
}

fn describe_accepts(accepts: &[Accept]) -> String {
    let parts: Vec<String> = accepts
        .iter()
        .map(|a| match a {
            Accept::Column => "a column mapping".to_string(),
            Accept::Number => "a number".to_string(),
            Accept::Color => "a color string".to_string(),
            Accept::Str => "a string".to_string(),
            Accept::Bool => "a boolean".to_string(),
            Accept::Enum(opts) => format!("one of {opts:?}"),
            Accept::NumberArray => "an array of numbers".to_string(),
        })
        .collect();
    parts.join(" or ")
}

/// Whether `name` is a commonly used CSS color keyword (for H3002 hints).
fn is_css_color_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "red"
            | "green"
            | "blue"
            | "yellow"
            | "black"
            | "white"
            | "gray"
            | "grey"
            | "orange"
            | "purple"
            | "pink"
            | "brown"
            | "cyan"
            | "magenta"
            | "lime"
            | "navy"
            | "teal"
            | "maroon"
            | "olive"
            | "silver"
            | "gold"
            | "steelblue"
            | "tomato"
            | "salmon"
            | "indigo"
            | "violet"
            | "turquoise"
            | "coral"
            | "crimson"
            | "khaki"
            | "plum"
    )
}

pub(super) fn is_color_literal(value: &str) -> bool {
    is_hex_color(value) || is_rgb_color_function(value) || is_css_color_name(value)
}

fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn is_rgb_color_function(value: &str) -> bool {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    let (body, expects_alpha) = if let Some(body) = lower
        .strip_prefix("rgb(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        (body, false)
    } else if let Some(body) = lower
        .strip_prefix("rgba(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        (body, true)
    } else {
        return false;
    };

    let parts: Vec<&str> = body.split(',').map(str::trim).collect();
    if (!expects_alpha && parts.len() != 3) || (expects_alpha && parts.len() != 4) {
        return false;
    }
    if !parts[..3]
        .iter()
        .all(|part| parse_rgb_channel(part).is_some())
    {
        return false;
    }
    !expects_alpha || parse_alpha_channel(parts[3]).is_some()
}

fn parse_rgb_channel(value: &str) -> Option<u8> {
    value.parse::<u8>().ok()
}

fn parse_alpha_channel(value: &str) -> Option<f64> {
    let alpha = value.parse::<f64>().ok()?;
    (alpha.is_finite() && (0.0..=1.0).contains(&alpha)).then_some(alpha)
}
