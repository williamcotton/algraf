//! The semantic analyzer (spec §13).
//!
//! `analyze` is pure: it takes a parsed syntax tree plus the primary data
//! source schema and produces IR and diagnostics. Filesystem resolution and
//! schema loading happen at the caller's boundary (spec §23.5); schema errors
//! such as "file not found" are produced there, not here.

use std::collections::HashMap;

use algraf_core::{Diagnostic, Severity, Span};
use algraf_data::{ColumnDef, DataType};
use algraf_syntax::ast::{
    AlgebraBinary, AlgebraExpr, AlgebraName, AlgebraOp, Arg, ChartBlock, ChartItem, Decl,
    DeriveDecl, GeometryCall, LiteralKind, Root, SpaceBlock, SpaceItem, ValueExpr,
};
use algraf_syntax::{parse, SyntaxKind, SyntaxNode};

use crate::ir::*;
use crate::registry::{self, Accept, GeometryDef, PropSpec};
use crate::util::{closest, node_span};

/// The result of semantic analysis.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub ir: Option<ChartIr>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Analyze a parsed tree against a primary data schema (spec §13.17).
pub fn analyze(root: &SyntaxNode, primary_schema: &[ColumnDef]) -> Analysis {
    let mut analyzer = Analyzer::new(primary_schema);
    let ir = Root::cast(root.clone())
        .and_then(|r| r.chart())
        .and_then(|chart| analyzer.chart(&chart));
    Analysis {
        ir,
        diagnostics: analyzer.diagnostics,
    }
}

/// Parse `source` and analyze it, merging parse and semantic diagnostics.
pub fn analyze_source(source: &str, primary_schema: &[ColumnDef]) -> Analysis {
    let parsed = parse(source);
    let mut analysis = analyze(&parsed.syntax(), primary_schema);
    let mut diagnostics = parsed.into_diagnostics();
    diagnostics.append(&mut analysis.diagnostics);
    Analysis {
        ir: analysis.ir,
        diagnostics,
    }
}

const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 520;
const CHART_ARGS: &[&str] = &["data", "width", "height", "title", "subtitle", "caption"];

/// A resolvable table: column name to type, in declared order.
struct ActiveTable {
    columns: Vec<(String, DataType)>,
}

impl ActiveTable {
    fn from_schema(schema: &[ColumnDef]) -> Self {
        ActiveTable {
            columns: schema.iter().map(|c| (c.name.clone(), c.dtype)).collect(),
        }
    }

    fn from_ir(schema: &[ColumnDefIr]) -> Self {
        ActiveTable {
            columns: schema.iter().map(|c| (c.name.clone(), c.dtype)).collect(),
        }
    }

    fn get(&self, name: &str) -> Option<DataType> {
        self.columns
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| *t)
    }

    fn names(&self) -> impl Iterator<Item = &str> {
        self.columns.iter().map(|(n, _)| n.as_str())
    }
}

struct Analyzer<'a> {
    primary: &'a [ColumnDef],
    derived: HashMap<String, Vec<ColumnDefIr>>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Analyzer<'a> {
    fn new(primary: &'a [ColumnDef]) -> Self {
        Analyzer {
            primary,
            derived: HashMap::new(),
            diagnostics: Vec::new(),
        }
    }

    fn diag(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    // --- Chart (spec §13.17 phases 2, 6–8) ---

    fn chart(&mut self, chart: &ChartBlock) -> Option<ChartIr> {
        let (data_source, width, height) = self.chart_args(chart);

        let mut derived_tables = Vec::new();
        let mut layout = LayoutIr::default();
        let mut spaces = Vec::new();
        for item in chart.items() {
            match item {
                ChartItem::Derive(d) => {
                    if let Some(ir) = self.derive(&d) {
                        self.derived
                            .insert(ir.name.clone(), ir.output_schema.clone());
                        derived_tables.push(ir);
                    }
                }
                ChartItem::Space(s) => spaces.push(self.space(&s)),
                ChartItem::Layout(decl) => self.layout_decl(&decl, &mut layout),
                // Scale / Guide / Theme declarations are recorded for render
                // configuration in a later milestone.
                ChartItem::Scale(_)
                | ChartItem::Guide(_)
                | ChartItem::Theme(_)
                | ChartItem::Error(_) => {}
            }
        }

        Some(ChartIr {
            data_source,
            derived_tables,
            layout,
            width,
            height,
            spaces,
        })
    }

    fn chart_args(&mut self, chart: &ChartBlock) -> (DataSourceIr, u32, u32) {
        let span = node_span(chart.syntax());
        let args = chart.args();

        let mut seen: HashMap<String, Span> = HashMap::new();
        let mut data_source = None;
        let mut width = DEFAULT_WIDTH;
        let mut height = DEFAULT_HEIGHT;

        for arg in &args {
            let Some(key) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if let Some(&first) = seen.get(&key) {
                self.diag(
                    Diagnostic::error(
                        "E1002",
                        format!("duplicate Chart argument `{key}`"),
                        key_span,
                    )
                    .with_related(first, "first defined here"),
                );
                continue;
            }
            seen.insert(key.clone(), key_span);

            if !CHART_ARGS.contains(&key.as_str()) {
                self.diag(Diagnostic::error(
                    "E1003",
                    format!("unsupported Chart argument `{key}`"),
                    key_span,
                ));
                continue;
            }

            match key.as_str() {
                "data" => data_source = Some(self.data_source(arg)),
                "width" => {
                    if let Some(n) = self.arg_u32(arg) {
                        width = n;
                    }
                }
                "height" => {
                    if let Some(n) = self.arg_u32(arg) {
                        height = n;
                    }
                }
                _ => {}
            }
        }

        let data_source = data_source.unwrap_or_else(|| {
            self.diag(Diagnostic::error(
                "E1001",
                "Chart requires a `data` argument",
                span,
            ));
            DataSourceIr::Missing
        });

        (data_source, width, height)
    }

    fn data_source(&mut self, arg: &Arg) -> DataSourceIr {
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                DataSourceIr::Path(string_value(&lit.text().unwrap_or_default()))
            }
            Some(ValueExpr::Stdin(_)) => DataSourceIr::Stdin,
            other => {
                let span = other
                    .map(|v| node_span(v.syntax()))
                    .unwrap_or_else(|| node_span(arg.syntax()));
                self.diag(Diagnostic::error(
                    "E1004",
                    "data source must be a string literal or the `stdin` sentinel",
                    span,
                ));
                DataSourceIr::Missing
            }
        }
    }

    fn arg_u32(&mut self, arg: &Arg) -> Option<u32> {
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Number) => lit
                .text()
                .and_then(|t| t.parse::<f64>().ok())
                .map(|f| f.max(0.0) as u32),
            _ => None,
        }
    }

    fn layout_decl(&mut self, decl: &Decl, layout: &mut LayoutIr) {
        let mut seen: HashMap<String, Span> = HashMap::new();
        for arg in decl.args() {
            let Some(key) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if let Some(&first) = seen.get(&key) {
                self.diag(
                    Diagnostic::error(
                        "E1002",
                        format!("duplicate Layout argument `{key}`"),
                        key_span,
                    )
                    .with_related(first, "first defined here"),
                );
                continue;
            }
            seen.insert(key.clone(), key_span);

            match key.as_str() {
                "facetColumns" => match self.arg_u32(&arg) {
                    Some(columns) if columns > 0 => layout.facet_columns = Some(columns as usize),
                    _ => self.diag(Diagnostic::error(
                        "E1204",
                        "`facetColumns` expects a positive number",
                        key_span,
                    )),
                },
                _ => self.diag(Diagnostic::error(
                    "E1003",
                    format!("unsupported Layout argument `{key}`"),
                    key_span,
                )),
            }
        }
    }

    // --- Derive (spec §13.4) ---

    fn derive(&mut self, derive: &DeriveDecl) -> Option<DeriveIr> {
        let span = node_span(derive.syntax());
        let name = derive.name()?;
        if self.derived.contains_key(&name) {
            self.diag(Diagnostic::error(
                "E1104",
                format!("duplicate derived table `{name}`"),
                span,
            ));
            return None;
        }

        let stat = derive.stat()?;
        let stat_name = stat.name().unwrap_or_default();
        let stat_span = node_span(stat.syntax());
        if stat_name != "Bin" {
            self.diag(Diagnostic::error(
                "E1403",
                format!("unknown stat `{stat_name}`; version 0.1 supports `Bin`"),
                stat_span,
            ));
            return None;
        }

        // Bin reads the primary table; its input must be one numeric column.
        let table = ActiveTable::from_schema(self.primary);
        let input = stat.input();
        let input_frame = match &input {
            Some(AlgebraExpr::Name(n)) => {
                let col = self.resolve_column(n, &table);
                match col.dtype {
                    DataType::Temporal => self.diag(Diagnostic::error(
                        "E1405",
                        "temporal binning is not supported in this version",
                        col.span,
                    )),
                    DataType::Integer | DataType::Float | DataType::Unknown => {}
                    _ => self.diag(Diagnostic::error(
                        "E1404",
                        format!("Bin input column `{}` is not numeric", col.name),
                        col.span,
                    )),
                }
                FrameIr::Vector(col)
            }
            _ => {
                self.diag(Diagnostic::error(
                    "E1404",
                    "Bin requires a single numeric column as input",
                    stat_span,
                ));
                FrameIr::Invalid
            }
        };

        let settings = self.collect_settings(&stat.args());
        let output_schema = vec![
            ColumnDefIr {
                name: "bin_start".into(),
                dtype: DataType::Float,
            },
            ColumnDefIr {
                name: "bin_end".into(),
                dtype: DataType::Float,
            },
            ColumnDefIr {
                name: "bin_center".into(),
                dtype: DataType::Float,
            },
            ColumnDefIr {
                name: "count".into(),
                dtype: DataType::Integer,
            },
        ];

        Some(DeriveIr {
            name,
            stat: StatCallIr {
                kind: StatKind::Bin,
                input: input_frame,
                settings,
                span: stat_span,
            },
            output_schema,
            span,
        })
    }

    /// Collect arbitrary `key: literal` settings without strict validation.
    fn collect_settings(&mut self, args: &[Arg]) -> Vec<Setting> {
        let mut settings = Vec::new();
        for arg in args {
            let Some(name) = arg.key() else { continue };
            if let Some(value) = arg.value() {
                if let Some(v) = setting_value(&value) {
                    settings.push(Setting { name, value: v });
                }
            }
        }
        settings
    }

    // --- Space (spec §13.3, §13.17 phases 8–12) ---

    fn space(&mut self, space: &SpaceBlock) -> SpaceIr {
        let span = node_span(space.syntax());
        let (data_ref, table) = self.space_data(space);

        let frame = match space.frame() {
            Some(expr) => {
                let frame = self.build_frame(&expr, &table);
                self.check_cartesian_arity(&frame, node_span(expr.syntax()));
                self.check_facet_variable(&frame);
                frame
            }
            None => FrameIr::Invalid,
        };

        let mut geometries = Vec::new();
        for item in space.items() {
            if let SpaceItem::Geometry(call) = item {
                if let Some(geo) = self.geometry(&call, &frame, &table) {
                    geometries.push(geo);
                }
            }
        }

        SpaceIr {
            data: data_ref,
            frame,
            geometries,
            span,
        }
    }

    fn space_data(&mut self, space: &SpaceBlock) -> (SpaceDataRef, ActiveTable) {
        let data_arg = space
            .args()
            .into_iter()
            .find(|a| a.key().as_deref() == Some("data"));

        if let Some(arg) = data_arg {
            if let Some(ValueExpr::Algebra(AlgebraExpr::Name(name))) = arg.value() {
                let table_name = name.name().unwrap_or_default();
                if let Some(schema) = self.derived.get(&table_name) {
                    return (
                        SpaceDataRef::Derived(table_name),
                        ActiveTable::from_ir(schema),
                    );
                }
                self.diag(Diagnostic::error(
                    "E1103",
                    format!("unknown derived table `{table_name}`"),
                    node_span(name.syntax()),
                ));
            } else if let Some(value) = arg.value() {
                self.diag(Diagnostic::error(
                    "E1103",
                    "space `data` must name a derived table",
                    node_span(value.syntax()),
                ));
            }
        }

        (
            SpaceDataRef::Primary,
            ActiveTable::from_schema(self.primary),
        )
    }

    // --- Algebra frame (spec §8, §13.5) ---

    fn build_frame(&mut self, expr: &AlgebraExpr, table: &ActiveTable) -> FrameIr {
        match expr {
            AlgebraExpr::Name(name) => FrameIr::Vector(self.resolve_column(name, table)),
            AlgebraExpr::Paren(paren) => match paren.inner() {
                Some(inner) => self.build_frame(&inner, table),
                None => FrameIr::Invalid,
            },
            AlgebraExpr::Binary(binary) => self.build_binary(binary, table),
            AlgebraExpr::Error(_) => FrameIr::Invalid,
        }
    }

    fn build_binary(&mut self, binary: &AlgebraBinary, table: &ActiveTable) -> FrameIr {
        let lhs = binary
            .lhs()
            .map(|e| self.build_frame(&e, table))
            .unwrap_or(FrameIr::Invalid);
        let rhs = binary
            .rhs()
            .map(|e| self.build_frame(&e, table))
            .unwrap_or(FrameIr::Invalid);

        match binary.op() {
            Some(AlgebraOp::Cross) => cartesian_push(lhs, rhs),
            Some(AlgebraOp::Nest) => FrameIr::Nested {
                outer: Box::new(lhs),
                inner: Box::new(rhs),
            },
            Some(AlgebraOp::Blend) => {
                if !blend_parenthesized(binary) {
                    self.diag(
                        Diagnostic::error(
                            "E1305",
                            "blend `+` expression must be parenthesized",
                            node_span(binary.syntax()),
                        )
                        .with_help("wrap it in parentheses, e.g. `time * (lower + upper)`"),
                    );
                }
                union_push(lhs, rhs)
            }
            None => FrameIr::Invalid,
        }
    }

    fn resolve_column(&mut self, name: &AlgebraName, table: &ActiveTable) -> ColumnRef {
        let col_name = name.name().unwrap_or_default();
        let span = node_span(name.syntax());
        match table.get(&col_name) {
            Some(dtype) => ColumnRef {
                name: col_name,
                dtype,
                span,
            },
            None => {
                let mut diag =
                    Diagnostic::error("E1101", format!("unknown column `{col_name}`"), span);
                if let Some(suggestion) = closest(&col_name, table.names()) {
                    diag = diag.with_help(format!("did you mean `{suggestion}`?"));
                }
                self.diag(diag);
                ColumnRef {
                    name: col_name,
                    dtype: DataType::Unknown,
                    span,
                }
            }
        }
    }

    /// Reject 3D-or-higher Cartesian spaces (spec §8.3, §13.14).
    fn check_cartesian_arity(&mut self, frame: &FrameIr, span: Span) {
        match frame {
            FrameIr::Cartesian(axes) => {
                if axes.len() > 2 {
                    self.diag(
                        Diagnostic::error("E1306", "3D Cartesian spaces are unsupported", span)
                            .with_help("use nesting to facet, e.g. `(x * y) / z`"),
                    );
                }
                for axis in axes {
                    self.check_cartesian_arity(axis, span);
                }
            }
            FrameIr::Nested { outer, inner } => {
                self.check_cartesian_arity(outer, span);
                self.check_cartesian_arity(inner, span);
            }
            FrameIr::Union(members) => {
                for m in members {
                    self.check_cartesian_arity(m, span);
                }
            }
            FrameIr::Vector(_) | FrameIr::Invalid => {}
        }
    }

    fn check_facet_variable(&mut self, frame: &FrameIr) {
        if let Some(panel) = facet_panel_column(frame) {
            if panel.dtype != DataType::Unknown && !panel.dtype.is_categorical() {
                self.diag(
                    Diagnostic::error(
                        "E1303",
                        format!("facet column `{}` must be categorical", panel.name),
                        panel.span,
                    )
                    .with_help("use a string, boolean, or pre-binned column for facet panels"),
                );
            }
        }
    }

    // --- Geometry (spec §13.6, §13.9–13.13) ---

    fn geometry(
        &mut self,
        call: &GeometryCall,
        frame: &FrameIr,
        table: &ActiveTable,
    ) -> Option<GeometryIr> {
        let span = node_span(call.syntax());
        let name = call.name().unwrap_or_default();

        let def = match registry::geometry(&name) {
            Some(def) => def,
            None => {
                let mut diag =
                    Diagnostic::error("E1201", format!("unknown geometry `{name}`"), span);
                if let Some(suggestion) = closest(&name, registry::geometry_names()) {
                    diag = diag.with_help(format!("did you mean `{suggestion}`?"));
                }
                self.diag(diag);
                return None;
            }
        };

        let args = call.args();
        let mut seen: HashMap<String, Span> = HashMap::new();
        let mut mappings = Vec::new();
        let mut settings = Vec::new();

        for arg in &args {
            let Some(key) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());

            if let Some(&first) = seen.get(&key) {
                self.diag(
                    Diagnostic::error("E1203", format!("duplicate property `{key}`"), key_span)
                        .with_related(first, "first defined here"),
                );
                continue;
            }
            seen.insert(key.clone(), key_span);

            let Some(prop) = def.prop(&key) else {
                self.unknown_property(def, &key, key_span);
                continue;
            };

            match self.check_property(prop, arg, table) {
                PropOutcome::Mapping(column) => mappings.push(AestheticMapping {
                    aesthetic: key,
                    column,
                }),
                PropOutcome::Setting(value) => settings.push(GeometrySetting { name: key, value }),
                PropOutcome::Invalid => {}
            }
        }

        for prop in def.props.iter().filter(|p| p.required) {
            if !seen.contains_key(prop.name) {
                self.diag(Diagnostic::error(
                    "E1205",
                    format!("`{}` requires property `{}`", def.name, prop.name),
                    span,
                ));
            }
        }

        self.bar_dodge_hint(def, frame, &mappings, &settings, span);

        Some(GeometryIr {
            kind: def.kind,
            mappings,
            settings,
            span,
        })
    }

    fn unknown_property(&mut self, def: &GeometryDef, key: &str, span: Span) {
        let mut diag = Diagnostic::error(
            "E1202",
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
        let form = ValueForm::of(&value);

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
                    "E1204",
                    format!("`{}` expects a quoted string value", prop.name),
                    span,
                )
                .with_help(format!("write it as a string, e.g. {written:?}")),
            );
        } else {
            self.diag(Diagnostic::error(
                "E1204",
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
        mappings: &[AestheticMapping],
        settings: &[GeometrySetting],
        span: Span,
    ) {
        if def.kind != GeometryKind::Bar {
            return;
        }
        let has_fill = mappings.iter().any(|m| m.aesthetic == "fill");
        let stacked = settings.iter().any(|s| {
            s.name == "layout" && matches!(&s.value, SettingValue::String(v) if v != "identity")
        });
        // Only hint when the space is a flat Cartesian with no nesting; a
        // frame that already nests is the dodge form the hint would suggest.
        let plain_cartesian = matches!(frame, FrameIr::Cartesian(_)) && !contains_nested(frame);
        if has_fill && plain_cartesian && !stacked {
            self.diag(
                Diagnostic::new(
                    Severity::Hint,
                    "H3001",
                    "use nested algebra for dodged bars",
                    span,
                )
                .with_help("e.g. `Space((x / fill) * y)`, or set `layout: \"stack\"`"),
            );
        }
    }
}

enum PropOutcome {
    Mapping(ColumnRef),
    Setting(SettingValue),
    Invalid,
}

/// A classified property value form.
enum ValueForm {
    Column(AlgebraName),
    ComplexAlgebra,
    Number(f64),
    Str(String),
    Bool(bool),
    Null,
    Array(Option<Vec<f64>>),
    Stdin,
    Error,
}

impl ValueForm {
    fn of(value: &ValueExpr) -> ValueForm {
        match value {
            ValueExpr::Algebra(AlgebraExpr::Name(n)) => ValueForm::Column(n.clone()),
            ValueExpr::Algebra(AlgebraExpr::Error(_)) => ValueForm::Error,
            ValueExpr::Algebra(_) => ValueForm::ComplexAlgebra,
            ValueExpr::Literal(lit) => match lit.kind() {
                Some(LiteralKind::Number) => ValueForm::Number(
                    lit.text()
                        .and_then(|t| t.parse::<f64>().ok())
                        .unwrap_or(0.0),
                ),
                Some(LiteralKind::String) => {
                    ValueForm::Str(string_value(&lit.text().unwrap_or_default()))
                }
                Some(LiteralKind::Bool) => ValueForm::Bool(lit.text().as_deref() == Some("true")),
                Some(LiteralKind::Null) | None => ValueForm::Null,
            },
            ValueExpr::Stdin(_) => ValueForm::Stdin,
            ValueExpr::Array(array) => {
                let mut nums = Vec::new();
                let mut all_numeric = true;
                for item in array.values() {
                    match ValueForm::of(&item) {
                        ValueForm::Number(n) => nums.push(n),
                        _ => all_numeric = false,
                    }
                }
                ValueForm::Array(all_numeric.then_some(nums))
            }
            ValueExpr::Error(_) => ValueForm::Error,
        }
    }

    fn describe(&self) -> &'static str {
        match self {
            ValueForm::Column(_) => "a column mapping",
            ValueForm::ComplexAlgebra => "an algebra expression",
            ValueForm::Number(_) => "a number",
            ValueForm::Str(_) => "a string",
            ValueForm::Bool(_) => "a boolean",
            ValueForm::Null => "null",
            ValueForm::Array(_) => "an array",
            ValueForm::Stdin => "the stdin sentinel",
            ValueForm::Error => "an invalid value",
        }
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

fn setting_value(value: &ValueExpr) -> Option<SettingValue> {
    match ValueForm::of(value) {
        ValueForm::Number(n) => Some(SettingValue::Number(n)),
        ValueForm::Str(s) => Some(SettingValue::String(s)),
        ValueForm::Bool(b) => Some(SettingValue::Bool(b)),
        ValueForm::Null => Some(SettingValue::Null),
        ValueForm::Array(Some(nums)) => Some(SettingValue::NumberArray(nums)),
        _ => None,
    }
}

/// Strip surrounding quotes and resolve escapes in a string literal lexeme.
fn string_value(raw: &str) -> String {
    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Whether a blend `+` node is acceptably parenthesized (spec §8.5).
///
/// A blend node is valid if its parent is a parenthesized expression, or if it
/// is an inner link of a blend chain whose root is parenthesized.
fn blend_parenthesized(binary: &AlgebraBinary) -> bool {
    match binary.syntax().parent() {
        Some(parent) if parent.kind() == SyntaxKind::ALGEBRA_PAREN => true,
        Some(parent) if parent.kind() == SyntaxKind::ALGEBRA_BINARY => {
            AlgebraBinary::cast(parent).and_then(|b| b.op()) == Some(AlgebraOp::Blend)
        }
        _ => false,
    }
}

fn contains_nested(frame: &FrameIr) -> bool {
    match frame {
        FrameIr::Nested { .. } => true,
        FrameIr::Cartesian(members) | FrameIr::Union(members) => {
            members.iter().any(contains_nested)
        }
        FrameIr::Vector(_) | FrameIr::Invalid => false,
    }
}

fn facet_panel_column(frame: &FrameIr) -> Option<&ColumnRef> {
    let FrameIr::Nested { outer, inner } = frame else {
        return None;
    };
    if !matches!(outer.as_ref(), FrameIr::Cartesian(axes) if axes.len() == 2) {
        return None;
    }
    match inner.as_ref() {
        FrameIr::Vector(column) => Some(column),
        _ => None,
    }
}

fn cartesian_push(acc: FrameIr, next: FrameIr) -> FrameIr {
    match acc {
        FrameIr::Cartesian(mut axes) => {
            axes.push(next);
            FrameIr::Cartesian(axes)
        }
        other => FrameIr::Cartesian(vec![other, next]),
    }
}

fn union_push(acc: FrameIr, next: FrameIr) -> FrameIr {
    match acc {
        FrameIr::Union(mut members) => {
            members.push(next);
            FrameIr::Union(members)
        }
        other => FrameIr::Union(vec![other, next]),
    }
}
