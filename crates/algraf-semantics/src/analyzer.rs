//! The semantic analyzer (spec §13).
//!
//! `analyze` is pure: it takes a parsed syntax tree plus the primary data
//! source schema and produces IR and diagnostics. Filesystem resolution and
//! schema loading happen at the caller's boundary (spec §23.5); schema errors
//! such as "file not found" are produced there, not here.

use std::collections::{HashMap, HashSet};

use algraf_core::{Diagnostic, Severity, Span};
use algraf_data::{ColumnDef, DataType};
use algraf_syntax::ast::{
    AlgebraBinary, AlgebraExpr, AlgebraName, AlgebraOp, Arg, ChartBlock, ChartItem, Decl,
    DeriveDecl, GeometryCall, LetDecl, LiteralKind, MapValue, Root, SpaceBlock, SpaceItem,
    TableDecl, ValueExpr,
};
use algraf_syntax::{
    is_source_constructor, node_span, parse, source_constructor, source_expr_from_arg,
    source_expr_from_value, unescape_string_literal as string_value, SourceExpr, SourceFormat,
    SyntaxKind, SyntaxNode,
};

use crate::ir::*;
use crate::registry::{self, Accept, GeometryDef, PropSpec};
use crate::util::closest;

/// The result of semantic analysis.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub ir: Option<ChartIr>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Analyze a parsed tree against a primary data schema (spec §13.17).
///
/// This analyzes the document's first chart block. Multi-chart documents
/// (spec §7.1) resolve each chart against its own data source, so the caller
/// (the CLI) drives per-chart analysis with [`analyze_chart`].
pub fn analyze(root: &SyntaxNode, primary_schema: &[ColumnDef]) -> Analysis {
    analyze_with_tables(root, primary_schema, &HashMap::new())
}

/// Analyze a parsed tree against a primary schema plus named-table schemas
/// (spec §10.x). `table_schemas` maps each `Table name = "..."` declaration's
/// name to its loaded CSV schema; the caller loads them at the I/O boundary.
pub fn analyze_with_tables(
    root: &SyntaxNode,
    primary_schema: &[ColumnDef],
    table_schemas: &HashMap<String, Vec<ColumnDef>>,
) -> Analysis {
    let mut analyzer = Analyzer::new(primary_schema, table_schemas);
    let ir = Root::cast(root.clone())
        .and_then(|r| r.chart())
        .and_then(|chart| analyzer.chart(&chart));
    Analysis {
        ir,
        diagnostics: analyzer.diagnostics,
    }
}

/// Analyze a single chart block against its primary data schema (spec §7.1).
/// Used to analyze each chart of a multi-chart document independently.
pub fn analyze_chart(chart: &ChartBlock, primary_schema: &[ColumnDef]) -> Analysis {
    analyze_chart_with_tables(chart, primary_schema, &HashMap::new())
}

/// Analyze a single chart block against a primary schema plus named-table
/// schemas (spec §10.x).
pub fn analyze_chart_with_tables(
    chart: &ChartBlock,
    primary_schema: &[ColumnDef],
    table_schemas: &HashMap<String, Vec<ColumnDef>>,
) -> Analysis {
    let mut analyzer = Analyzer::new(primary_schema, table_schemas);
    let ir = analyzer.chart(chart);
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
const CHART_ARGS: &[&str] = &[
    "data",
    "width",
    "height",
    "title",
    "subtitle",
    "caption",
    "marginTop",
    "marginRight",
    "marginBottom",
    "marginLeft",
];
const THEME_NAMES: &[&str] = &["minimal", "classic", "light", "dark", "void"];
/// Recognized `Theme(...)` override keys (spec §20.8); used for diagnostics and
/// did-you-mean suggestions.
const THEME_OVERRIDE_KEYS: &[&str] = &[
    "axisText",
    "gridMajor",
    "fontFamily",
    "fontSize",
    "titleSize",
    "pointSize",
    "lineWidth",
    "background",
    "plotBackground",
    "axisColor",
    "gridColor",
    "textColor",
    "grid",
    "axes",
];
const PALETTE_NAMES: &[&str] = &["default", "accent"];

/// Parsed `Chart(...)` header arguments (spec §13.17 phase 2).
struct ChartArgs {
    data_source: DataSourceIr,
    width: u32,
    height: u32,
    title: Option<String>,
    subtitle: Option<String>,
    caption: Option<String>,
    margin_top: Option<u32>,
    margin_right: Option<u32>,
    margin_bottom: Option<u32>,
    margin_left: Option<u32>,
}

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

    fn merged(primary: &[ColumnDef], derived: &[&[ColumnDefIr]]) -> Self {
        let mut columns: Vec<(String, DataType)> =
            primary.iter().map(|c| (c.name.clone(), c.dtype)).collect();
        for schema in derived {
            for column in *schema {
                if !columns.iter().any(|(name, _)| name == &column.name) {
                    columns.push((column.name.clone(), column.dtype));
                }
            }
        }
        ActiveTable { columns }
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

/// A resolved constant value bound by a `let` declaration (spec §9.6).
#[derive(Clone)]
struct LetVar {
    value: ConstValue,
}

/// The constant value forms a `let` binding may hold (spec §7.10).
#[derive(Clone)]
enum ConstValue {
    Number(f64),
    Str(String),
    Bool(bool),
    Null,
    NumberArray(Vec<f64>),
    StringArray(Vec<String>),
}

impl ConstValue {
    /// Re-express the bound constant as a property [`ValueForm`] for type
    /// checking at the use site (spec §13.9).
    fn to_form(&self) -> ValueForm {
        match self {
            ConstValue::Number(n) => ValueForm::Number(*n),
            ConstValue::Str(s) => ValueForm::Str(s.clone()),
            ConstValue::Bool(b) => ValueForm::Bool(*b),
            ConstValue::Null => ValueForm::Null,
            ConstValue::NumberArray(v) => ValueForm::Array(Some(v.clone())),
            ConstValue::StringArray(v) => ValueForm::StringArray(Some(v.clone())),
        }
    }
}

struct Analyzer<'a> {
    primary: &'a [ColumnDef],
    /// Schemas of chart-scoped named tables, keyed by declaration name.
    table_schemas: &'a HashMap<String, Vec<ColumnDef>>,
    /// Names of declared `Table`s that resolved (used by `space_data`).
    table_names: HashSet<String>,
    derived: HashMap<String, Vec<ColumnDefIr>>,
    reserved_derived_names: HashSet<String>,
    /// Chart-scope `let` bindings, visible in every space (spec §9.6).
    chart_vars: HashMap<String, LetVar>,
    /// Space-scope `let` bindings for the space under analysis; these shadow
    /// chart-scope bindings of the same name (spec §9.6).
    space_vars: HashMap<String, LetVar>,
    synthetic_counter: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Analyzer<'a> {
    fn new(primary: &'a [ColumnDef], table_schemas: &'a HashMap<String, Vec<ColumnDef>>) -> Self {
        Analyzer {
            primary,
            table_schemas,
            table_names: HashSet::new(),
            derived: HashMap::new(),
            reserved_derived_names: HashSet::new(),
            chart_vars: HashMap::new(),
            space_vars: HashMap::new(),
            synthetic_counter: 0,
            diagnostics: Vec::new(),
        }
    }

    fn diag(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    // --- Chart (spec §13.17 phases 2, 6–8) ---

    fn chart(&mut self, chart: &ChartBlock) -> Option<ChartIr> {
        let ChartArgs {
            data_source,
            width,
            height,
            title,
            subtitle,
            caption,
            margin_top,
            margin_right,
            margin_bottom,
            margin_left,
        } = self.chart_args(chart);
        self.reserved_derived_names = chart_derived_names(chart);

        // Resolve named `Table` declarations up front so spaces and column
        // references can bind to them regardless of declaration order (spec
        // §10.x).
        let tables = self.resolve_tables(chart);

        // Collect chart-scope `let` bindings up front so they resolve regardless
        // of declaration order within the chart body (spec §9.6).
        let chart_lets: Vec<LetDecl> = chart
            .items()
            .into_iter()
            .filter_map(|item| match item {
                ChartItem::Let(decl) => Some(decl),
                _ => None,
            })
            .collect();
        self.chart_vars = self.collect_let_decls(&chart_lets);

        let mut derived_tables = self.resolve_chart_derives(chart);
        for ir in &derived_tables {
            self.derived
                .insert(ir.name.clone(), ir.output_schema.clone());
        }
        let mut layout = LayoutIr::default();
        let mut guides = GuideIr::default();
        let mut scales = Vec::new();
        let mut theme: Option<ThemeIr> = None;
        let mut spaces = Vec::new();
        let primary_table = ActiveTable::from_schema(self.primary);
        for item in chart.items() {
            match item {
                ChartItem::Derive(_) => {}
                ChartItem::Table(_) => {}
                ChartItem::Let(_) => {}
                ChartItem::Space(s) => {
                    let analysis = self.space(&s);
                    for ir in analysis.derived {
                        self.derived
                            .insert(ir.name.clone(), ir.output_schema.clone());
                        derived_tables.push(ir);
                    }
                    spaces.extend(analysis.spaces);
                }
                ChartItem::Layout(decl) => self.layout_decl(&decl, &mut layout),
                ChartItem::Guide(decl) => {
                    let mut overrides = GuideOverridesIr::default();
                    self.guide_decl(&decl, &mut overrides);
                    guides = guides.with_overrides(&overrides);
                }
                ChartItem::Theme(decl) => {
                    if let Some(t) = self.theme_decl(&decl) {
                        theme = Some(t);
                    }
                }
                ChartItem::Scale(decl) => {
                    if let Some(scale) = self.scale_decl(&decl, &primary_table) {
                        scales.push(scale);
                    }
                }
                ChartItem::Error(_) => {}
            }
        }

        Some(ChartIr {
            data_source,
            tables,
            derived_tables,
            layout,
            guides,
            scales,
            theme,
            title,
            subtitle,
            caption,
            width,
            height,
            margin_top,
            margin_right,
            margin_bottom,
            margin_left,
            spaces,
        })
    }

    fn chart_args(&mut self, chart: &ChartBlock) -> ChartArgs {
        let span = node_span(chart.syntax());
        let args = chart.args();

        let mut seen: HashMap<String, Span> = HashMap::new();
        let mut data_source = None;
        let mut width = DEFAULT_WIDTH;
        let mut height = DEFAULT_HEIGHT;
        let mut title = None;
        let mut subtitle = None;
        let mut caption = None;
        let mut margin_top = None;
        let mut margin_right = None;
        let mut margin_bottom = None;
        let mut margin_left = None;

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
                "title" => title = self.arg_string(arg, "title"),
                "subtitle" => subtitle = self.arg_string(arg, "subtitle"),
                "caption" => caption = self.arg_string(arg, "caption"),
                "marginTop" => margin_top = self.arg_u32(arg),
                "marginRight" => margin_right = self.arg_u32(arg),
                "marginBottom" => margin_bottom = self.arg_u32(arg),
                "marginLeft" => margin_left = self.arg_u32(arg),
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

        ChartArgs {
            data_source,
            width,
            height,
            title,
            subtitle,
            caption,
            margin_top,
            margin_right,
            margin_bottom,
            margin_left,
        }
    }

    fn data_source(&mut self, arg: &Arg) -> DataSourceIr {
        match source_expr_from_arg(arg, true) {
            SourceExpr::Path {
                path, format: None, ..
            } => DataSourceIr::Path(path),
            SourceExpr::Path {
                path,
                format: Some(SourceFormat::GeoJson),
                ..
            } => DataSourceIr::GeoJson(path),
            SourceExpr::Path {
                path,
                format: Some(SourceFormat::Shapefile),
                ..
            } => DataSourceIr::Shapefile(path),
            SourceExpr::Stdin { .. } => DataSourceIr::Stdin,
            SourceExpr::Invalid { span } => {
                if let Some(ValueExpr::Call(call)) = arg.value() {
                    if is_source_constructor(&call) {
                        self.diag(Diagnostic::error(
                            "E1004",
                            format!(
                                "`{}` source expects a string-literal path",
                                call.name().unwrap_or_default()
                            ),
                            span,
                        ));
                        return DataSourceIr::Missing;
                    }
                }
                self.diag(Diagnostic::error(
                    "E1004",
                    "data source must be a string literal, a `GeoJson`/`Shapefile` \
                     source constructor, or the `stdin` sentinel",
                    span,
                ));
                DataSourceIr::Missing
            }
            SourceExpr::Missing => {
                self.diag(Diagnostic::error(
                    "E1004",
                    "data source must be a string literal, a `GeoJson`/`Shapefile` \
                     source constructor, or the `stdin` sentinel",
                    node_span(arg.syntax()),
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

    fn arg_string(&mut self, arg: &Arg, name: &str) -> Option<String> {
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                Some(string_value(&lit.text().unwrap_or_default()))
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    "E1204",
                    format!("`{name}` expects a string literal"),
                    node_span(value.syntax()),
                ));
                None
            }
            None => None,
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

    fn guide_decl(&mut self, decl: &Decl, guides: &mut GuideOverridesIr) {
        let mut seen: HashMap<String, Span> = HashMap::new();
        let mut axis: Option<AxisSelectorIr> = None;
        let mut label: Option<String> = None;
        for arg in decl.args() {
            let Some(key) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if let Some(&first) = seen.get(&key) {
                self.diag(
                    Diagnostic::error(
                        "E1002",
                        format!("duplicate Guide argument `{key}`"),
                        key_span,
                    )
                    .with_related(first, "first defined here"),
                );
                continue;
            }
            seen.insert(key.clone(), key_span);

            match key.as_str() {
                "legend" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Bool) => {
                        guides.legend = Some(lit.text().as_deref() == Some("true"));
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        "E1204",
                        "`legend` expects a boolean literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "axis" => match arg.value() {
                    Some(ValueExpr::Algebra(AlgebraExpr::Name(name))) => {
                        let raw = name.name().unwrap_or_default();
                        match raw.as_str() {
                            "x" => axis = Some(AxisSelectorIr::X),
                            "y" => axis = Some(AxisSelectorIr::Y),
                            _ => self.diag(Diagnostic::error(
                                "E1204",
                                "`axis` expects bare `x` or `y`",
                                node_span(name.syntax()),
                            )),
                        }
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        "E1204",
                        "`axis` expects bare `x` or `y`",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
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
                        "E1204",
                        "`label` expects a string literal or `null`",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "fill" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Null) => {
                        guides.fill_legend = Some(false);
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        "E1204",
                        "`fill` in `Guide` expects `null` to suppress the legend",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "stroke" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Null) => {
                        guides.stroke_legend = Some(false);
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        "E1204",
                        "`stroke` in `Guide` expects `null` to suppress the legend",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "grid" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Bool) => {
                        guides.grid = Some(lit.text().as_deref() == Some("true"));
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        "E1204",
                        "`grid` expects a boolean literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                _ => self.diag(Diagnostic::warning(
                    "W2006",
                    format!("unsupported Guide argument `{key}` ignored"),
                    key_span,
                )),
            }
        }
        match (axis, label) {
            (Some(AxisSelectorIr::X), Some(text)) => guides.x_label = Some(text),
            (Some(AxisSelectorIr::Y), Some(text)) => guides.y_label = Some(text),
            (Some(_), None) => self.diag(Diagnostic::warning(
                "W2006",
                "`Guide(axis: ...)` without `label:` has no effect",
                node_span(decl.syntax()),
            )),
            (None, Some(_)) => self.diag(Diagnostic::error(
                "E1204",
                "`Guide(label: ...)` requires `axis: x` or `axis: y`",
                node_span(decl.syntax()),
            )),
            (None, None) => {}
        }
    }

    fn scale_decl(&mut self, decl: &Decl, table: &ActiveTable) -> Option<ScaleIr> {
        let span = node_span(decl.syntax());
        let mut seen: HashMap<String, Span> = HashMap::new();
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
            if let Some(&first) = seen.get(&key) {
                self.diag(
                    Diagnostic::error(
                        "E1002",
                        format!("duplicate Scale argument `{key}`"),
                        key_span,
                    )
                    .with_related(first, "first defined here"),
                );
                continue;
            }
            seen.insert(key.clone(), key_span);

            match key.as_str() {
                "axis" => {
                    if let Some(axis) = self.axis_selector(&arg, "`axis` expects bare `x` or `y`") {
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
                "reverse" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Bool) => {
                        reverse = Some(lit.text().as_deref() == Some("true"));
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        "E1204",
                        "`reverse` expects a boolean literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
                "integer" => match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Bool) => {
                        integer = Some(lit.text().as_deref() == Some("true"));
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        "E1204",
                        "`integer` expects a boolean literal",
                        node_span(value.syntax()),
                    )),
                    None => {}
                },
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

    fn axis_selector(&mut self, arg: &Arg, message: &'static str) -> Option<AxisSelectorIr> {
        match arg.value() {
            Some(ValueExpr::Algebra(AlgebraExpr::Name(name))) => {
                let raw = name.name().unwrap_or_default();
                match raw.as_str() {
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

    fn theme_decl(&mut self, decl: &Decl) -> Option<ThemeIr> {
        let mut seen: HashMap<String, Span> = HashMap::new();
        let mut theme = ThemeIr::default();
        let mut saw_any = false;
        for arg in decl.args() {
            let Some(key) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if let Some(&first) = seen.get(&key) {
                self.diag(
                    Diagnostic::error(
                        "E1002",
                        format!("duplicate Theme argument `{key}`"),
                        key_span,
                    )
                    .with_related(first, "first defined here"),
                );
                continue;
            }
            seen.insert(key.clone(), key_span);
            saw_any = true;

            if key == "name" {
                match arg.value() {
                    Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                        let name = string_value(&lit.text().unwrap_or_default());
                        if !THEME_NAMES.contains(&name.as_str()) {
                            self.diag(Diagnostic::error(
                                "E1204",
                                format!("unknown theme `{name}`"),
                                node_span(lit.syntax()),
                            ));
                        } else {
                            theme.base = Some(name);
                        }
                    }
                    Some(value) => self.diag(Diagnostic::error(
                        "E1204",
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
            "fontFamily" => overrides.font_family = self.theme_scalar_string(key, &value),
            "fontSize" => overrides.font_size = self.theme_scalar_number(key, &value),
            "titleSize" => overrides.title_size = self.theme_scalar_number(key, &value),
            "pointSize" => overrides.point_size = self.theme_scalar_number(key, &value),
            "lineWidth" => overrides.line_width = self.theme_scalar_number(key, &value),
            "background" => overrides.background = self.theme_scalar_color(key, &value),
            "plotBackground" => overrides.plot_background = self.theme_scalar_color(key, &value),
            "axisColor" => overrides.axis_color = self.theme_scalar_color(key, &value),
            "gridColor" => overrides.grid_major_color = self.theme_scalar_color(key, &value),
            "textColor" => overrides.text_color = self.theme_scalar_color(key, &value),
            "grid" => overrides.grid = self.theme_scalar_bool(key, &value),
            "axes" => overrides.axes = self.theme_scalar_bool(key, &value),
            _ => {
                let mut diag =
                    Diagnostic::error("E1704", format!("unknown Theme property `{key}`"), key_span);
                if let Some(suggestion) = closest(key, THEME_OVERRIDE_KEYS.iter().copied()) {
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
                    "E1705",
                    format!("`{key}` expects a `{expected}(...)` value"),
                    node_span(other.syntax()),
                ));
                None
            }
        }
    }

    fn theme_number(&mut self, args: &[Arg], name: &str) -> Option<f64> {
        let arg = args.iter().find(|a| a.key().as_deref() == Some(name))?;
        self.theme_scalar_number(name, &arg.value()?)
    }

    fn theme_color(&mut self, args: &[Arg], name: &str) -> Option<String> {
        let arg = args.iter().find(|a| a.key().as_deref() == Some(name))?;
        self.theme_scalar_color(name, &arg.value()?)
    }

    fn theme_scalar_number(&mut self, key: &str, value: &ValueExpr) -> Option<f64> {
        match self.substitute_var(ValueForm::of(value)) {
            ValueForm::Number(n) => Some(n),
            _ => {
                self.theme_value_error(key, "a number", value);
                None
            }
        }
    }

    fn theme_scalar_string(&mut self, key: &str, value: &ValueExpr) -> Option<String> {
        match self.substitute_var(ValueForm::of(value)) {
            ValueForm::Str(s) => Some(s),
            _ => {
                self.theme_value_error(key, "a string", value);
                None
            }
        }
    }

    fn theme_scalar_color(&mut self, key: &str, value: &ValueExpr) -> Option<String> {
        match self.substitute_var(ValueForm::of(value)) {
            ValueForm::Str(s) => Some(s),
            _ => {
                self.theme_value_error(key, "a color string", value);
                None
            }
        }
    }

    fn theme_scalar_bool(&mut self, key: &str, value: &ValueExpr) -> Option<bool> {
        match self.substitute_var(ValueForm::of(value)) {
            ValueForm::Bool(b) => Some(b),
            _ => {
                self.theme_value_error(key, "a boolean", value);
                None
            }
        }
    }

    fn theme_value_error(&mut self, key: &str, expected: &str, value: &ValueExpr) {
        self.diag(Diagnostic::error(
            "E1705",
            format!("`{key}` expects {expected}"),
            node_span(value.syntax()),
        ));
    }

    // --- Let bindings (spec §7.10, §9.6) ---

    /// Evaluate a list of `let` declarations in one scope into a name→value map,
    /// reporting duplicate bindings (E1702) and non-constant values (E1701).
    fn collect_let_decls(&mut self, decls: &[LetDecl]) -> HashMap<String, LetVar> {
        let mut vars: HashMap<String, LetVar> = HashMap::new();
        let mut spans: HashMap<String, Span> = HashMap::new();
        for decl in decls {
            let Some(name) = decl.name() else { continue };
            let name_span = decl.name_span().unwrap_or_else(|| node_span(decl.syntax()));
            if let Some(&first) = spans.get(&name) {
                self.diag(
                    Diagnostic::error(
                        "E1702",
                        format!("duplicate `let` binding `{name}`"),
                        name_span,
                    )
                    .with_related(first, "first bound here"),
                );
                continue;
            }
            spans.insert(name.clone(), name_span);
            if let Some(value) = self.eval_let_value(decl) {
                vars.insert(name, LetVar { value });
            }
        }
        vars
    }

    /// Resolve a `let` binding's value to a constant, or emit E1701. Variables
    /// hold constant values only in this version (spec §7.10): column mappings,
    /// algebra, and references to other variables are rejected.
    fn eval_let_value(&mut self, decl: &LetDecl) -> Option<ConstValue> {
        let value = decl.value()?;
        let span = node_span(value.syntax());
        match ValueForm::of(&value) {
            ValueForm::Number(n) => Some(ConstValue::Number(n)),
            ValueForm::Str(s) => Some(ConstValue::Str(s)),
            ValueForm::Bool(b) => Some(ConstValue::Bool(b)),
            ValueForm::Null => Some(ConstValue::Null),
            ValueForm::Array(Some(v)) => Some(ConstValue::NumberArray(v)),
            ValueForm::StringArray(Some(v)) => Some(ConstValue::StringArray(v)),
            _ => {
                self.diag(
                    Diagnostic::error(
                        "E1701",
                        "`let` binding value must be a constant literal or array",
                        span,
                    )
                    .with_help(
                        "variables hold constants such as \"#3366cc\", 0.4, true, or [1, 2]",
                    ),
                );
                None
            }
        }
    }

    /// Substitute an in-scope `let` binding for a bare-identifier property
    /// value. Space-scope bindings shadow chart-scope ones; quoted identifiers
    /// are always column references and are never substituted (spec §9.6).
    fn substitute_var(&self, form: ValueForm) -> ValueForm {
        if let ValueForm::Column(name) = &form {
            if !name.is_quoted() {
                if let Some(var_name) = name.name() {
                    if let Some(var) = self
                        .space_vars
                        .get(&var_name)
                        .or_else(|| self.chart_vars.get(&var_name))
                    {
                        return var.value.to_form();
                    }
                }
            }
        }
        form
    }

    // --- Named tables (spec §10.x) ---

    /// Resolve `Table name = "path.csv"` declarations into IR, registering each
    /// resolved name for later `data:` binding. Reports duplicate names (E1105)
    /// and names that conflict with a derived table (E1108). A missing/unreadable
    /// file is the caller's concern (E1106/E1107).
    fn resolve_tables(&mut self, chart: &ChartBlock) -> Vec<TableDeclIr> {
        let mut out = Vec::new();
        let mut seen: HashMap<String, Span> = HashMap::new();
        for item in chart.items() {
            let ChartItem::Table(decl) = item else {
                continue;
            };
            let Some(name) = decl.name() else { continue };
            let name_span = decl.name_span().unwrap_or_else(|| node_span(decl.syntax()));
            if let Some(&first) = seen.get(&name) {
                self.diag(
                    Diagnostic::error(
                        "E1105",
                        format!("duplicate `Table` name `{name}`"),
                        name_span,
                    )
                    .with_related(first, "first declared here"),
                );
                continue;
            }
            if self.reserved_derived_names.contains(&name) {
                self.diag(Diagnostic::error(
                    "E1108",
                    format!("`Table` name `{name}` conflicts with a derived table"),
                    name_span,
                ));
                continue;
            }
            seen.insert(name.clone(), name_span);

            let Some(path) = self.table_source_path(&decl) else {
                continue;
            };
            self.table_names.insert(name.clone());
            out.push(TableDeclIr {
                name,
                path,
                span: node_span(decl.syntax()),
            });
        }
        out
    }

    /// The path from a `Table` declaration's source expression.
    fn table_source_path(&mut self, decl: &TableDecl) -> Option<String> {
        match source_expr_from_value(decl.source(), false) {
            SourceExpr::Path { path, .. } => Some(path),
            SourceExpr::Invalid { span } => {
                if let Some(ValueExpr::Call(call)) = decl.source() {
                    if is_source_constructor(&call) && source_constructor(&call).is_none() {
                        self.diag(Diagnostic::error(
                            "E1004",
                            format!(
                                "`{}` source expects a string-literal path",
                                call.name().unwrap_or_default()
                            ),
                            span,
                        ));
                        return None;
                    }
                }
                self.diag(Diagnostic::error(
                    "E1004",
                    "`Table` source must be a string-literal path or a \
                     `GeoJson`/`Shapefile` source constructor",
                    span,
                ));
                None
            }
            SourceExpr::Stdin { span } => {
                self.diag(Diagnostic::error(
                    "E1004",
                    "`Table` source must be a string-literal path or a \
                     `GeoJson`/`Shapefile` source constructor",
                    span,
                ));
                None
            }
            SourceExpr::Missing => None,
        }
    }

    fn table_active(&self, name: &str) -> ActiveTable {
        match self.table_schemas.get(name) {
            Some(schema) => ActiveTable::from_schema(schema),
            None => ActiveTable {
                columns: Vec::new(),
            },
        }
    }

    // --- Derive (spec §13.4) ---

    fn resolve_chart_derives(&mut self, chart: &ChartBlock) -> Vec<DeriveIr> {
        let primary_table = ActiveTable::from_schema(self.primary);
        let mut decls = Vec::new();
        let mut seen_names: HashMap<String, Span> = HashMap::new();

        for item in chart.items() {
            let ChartItem::Derive(derive) = item else {
                continue;
            };
            let span = node_span(derive.syntax());
            let Some(name) = derive.name() else { continue };
            if let Some(&first) = seen_names.get(&name) {
                self.diag(
                    Diagnostic::error("E1104", format!("duplicate derived table `{name}`"), span)
                        .with_related(first, "first defined here"),
                );
                continue;
            }
            seen_names.insert(name.clone(), span);
            decls.push((name, derive));
        }

        let mut producer_by_column: HashMap<String, usize> = HashMap::new();
        for (index, (_, derive)) in decls.iter().enumerate() {
            for output in derive_output_names(derive) {
                producer_by_column.entry(output).or_insert(index);
            }
        }

        let mut deps: Vec<HashSet<usize>> = vec![HashSet::new(); decls.len()];
        for (index, (_, derive)) in decls.iter().enumerate() {
            for input in derive_input_names(derive) {
                if primary_table.get(&input).is_some() {
                    continue;
                }
                if let Some(&producer) = producer_by_column.get(&input) {
                    deps[index].insert(producer);
                }
            }
        }

        let mut resolved = HashSet::new();
        let mut pending: HashSet<usize> = (0..decls.len()).collect();
        let mut out = Vec::new();
        let mut schemas: HashMap<usize, Vec<ColumnDefIr>> = HashMap::new();

        while !pending.is_empty() {
            let mut ready: Vec<usize> = pending
                .iter()
                .copied()
                .filter(|index| deps[*index].iter().all(|dep| resolved.contains(dep)))
                .collect();
            ready.sort_unstable();

            if ready.is_empty() {
                for index in pending.iter().copied() {
                    let (_, derive) = &decls[index];
                    self.diag(Diagnostic::error(
                        "E1501",
                        "cycle between derived table declarations",
                        node_span(derive.syntax()),
                    ));
                }
                break;
            }

            for index in ready {
                pending.remove(&index);
                let mut upstream: Vec<usize> = deps[index].iter().copied().collect();
                upstream.sort_unstable();
                let data = if upstream.is_empty() {
                    SpaceDataRef::Primary
                } else if upstream.len() == 1 {
                    SpaceDataRef::Derived(decls[upstream[0]].0.clone())
                } else {
                    self.diag(Diagnostic::error(
                        "E1404",
                        "derived stat inputs must come from one upstream table",
                        node_span(decls[index].1.syntax()),
                    ));
                    SpaceDataRef::Derived(decls[upstream[0]].0.clone())
                };
                let upstream_schemas: Vec<&[ColumnDefIr]> = upstream
                    .iter()
                    .filter_map(|dep| schemas.get(dep).map(Vec::as_slice))
                    .collect();
                let table = ActiveTable::merged(self.primary, &upstream_schemas);
                if let Some(ir) = self.derive(&decls[index].1, &table, data) {
                    schemas.insert(index, ir.output_schema.clone());
                    resolved.insert(index);
                    out.push(ir);
                }
            }
        }

        out
    }

    fn derive(
        &mut self,
        derive: &DeriveDecl,
        table: &ActiveTable,
        data: SpaceDataRef,
    ) -> Option<DeriveIr> {
        let span = node_span(derive.syntax());
        let name = derive.name()?;

        let stat = derive.stat()?;
        let stat_name = stat.name().unwrap_or_default();
        let stat_span = node_span(stat.syntax());
        let kind = match stat_name.as_str() {
            "Bin" => StatKind::Bin,
            "Smooth" => StatKind::Smooth,
            "Bin2D" => StatKind::Bin2D,
            "HexBin" => StatKind::HexBin,
            _ => {
                self.diag(Diagnostic::error(
                    "E1403",
                    format!("unknown stat `{stat_name}`; supported stats are `Bin`, `Smooth`, `Bin2D`, and `HexBin`"),
                    stat_span,
                ));
                return None;
            }
        };

        let inputs = stat.inputs();
        let (input_frame, settings, output_schema) = match kind {
            StatKind::Bin => {
                let input_frame = self.single_stat_input(&inputs, table, stat_span, "Bin")?;
                if let FrameIr::Vector(col) = &input_frame {
                    match col.dtype {
                        DataType::Temporal
                        | DataType::Integer
                        | DataType::Float
                        | DataType::Unknown => {}
                        _ => self.diag(Diagnostic::error(
                            "E1404",
                            format!("Bin input column `{}` is not numeric or temporal", col.name),
                            col.span,
                        )),
                    }
                }
                let settings = self.collect_bin_settings(&stat.args(), stat_span);
                let output_schema = match &input_frame {
                    FrameIr::Vector(column) => bin_output_schema(column.dtype),
                    _ => bin_output_schema(DataType::Float),
                };
                (input_frame, settings, output_schema)
            }
            StatKind::Smooth => {
                let input_frame = self.two_stat_inputs(&inputs, table, stat_span, "Smooth")?;
                if let FrameIr::Cartesian(columns) = &input_frame {
                    for frame in columns {
                        if let FrameIr::Vector(col) = frame {
                            if !matches!(
                                col.dtype,
                                DataType::Integer | DataType::Float | DataType::Unknown
                            ) {
                                self.diag(Diagnostic::error(
                                    "E1404",
                                    format!("Smooth input column `{}` is not numeric", col.name),
                                    col.span,
                                ));
                            }
                        }
                    }
                }
                (
                    input_frame,
                    self.collect_smooth_settings(&stat.args(), stat_span),
                    smooth_output_schema(),
                )
            }
            StatKind::Bin2D | StatKind::HexBin => {
                let label = if kind == StatKind::Bin2D {
                    "Bin2D"
                } else {
                    "HexBin"
                };
                let input_frame = self.two_stat_inputs(&inputs, table, stat_span, label)?;
                if let FrameIr::Cartesian(columns) = &input_frame {
                    for frame in columns {
                        if let FrameIr::Vector(col) = frame {
                            if !matches!(
                                col.dtype,
                                DataType::Integer | DataType::Float | DataType::Unknown
                            ) {
                                self.diag(Diagnostic::error(
                                    "E1404",
                                    format!("{label} input column `{}` is not numeric", col.name),
                                    col.span,
                                ));
                            }
                        }
                    }
                }
                let output_schema = if kind == StatKind::Bin2D {
                    bin2d_output_schema()
                } else {
                    hexbin_output_schema()
                };
                (
                    input_frame,
                    self.collect_bin2d_settings(&stat.args(), stat_span, label),
                    output_schema,
                )
            }
            _ => {
                self.diag(Diagnostic::error(
                    "E1403",
                    format!("unsupported stat `{stat_name}`"),
                    stat_span,
                ));
                return None;
            }
        };

        Some(DeriveIr {
            name,
            data,
            stat: StatCallIr {
                kind,
                input: input_frame,
                settings,
                span: stat_span,
            },
            output_schema,
            span,
        })
    }

    fn single_stat_input(
        &mut self,
        inputs: &[AlgebraExpr],
        table: &ActiveTable,
        stat_span: Span,
        stat_name: &str,
    ) -> Option<FrameIr> {
        if inputs.len() != 1 {
            self.diag(Diagnostic::error(
                "E1404",
                format!("{stat_name} requires exactly one input column"),
                stat_span,
            ));
            return None;
        }
        match &inputs[0] {
            AlgebraExpr::Name(n) => Some(FrameIr::Vector(self.resolve_column(n, table))),
            _ => {
                self.diag(Diagnostic::error(
                    "E1404",
                    format!("{stat_name} requires a column input"),
                    stat_span,
                ));
                Some(FrameIr::Invalid)
            }
        }
    }

    fn two_stat_inputs(
        &mut self,
        inputs: &[AlgebraExpr],
        table: &ActiveTable,
        stat_span: Span,
        stat_name: &str,
    ) -> Option<FrameIr> {
        if inputs.len() != 2 {
            self.diag(Diagnostic::error(
                "E1404",
                format!("{stat_name} requires exactly two input columns"),
                stat_span,
            ));
            return None;
        }
        let mut frames = Vec::new();
        for input in inputs {
            match input {
                AlgebraExpr::Name(n) => frames.push(FrameIr::Vector(self.resolve_column(n, table))),
                _ => {
                    self.diag(Diagnostic::error(
                        "E1404",
                        format!("{stat_name} requires column inputs"),
                        stat_span,
                    ));
                    frames.push(FrameIr::Invalid);
                }
            }
        }
        Some(FrameIr::Cartesian(frames))
    }

    fn collect_smooth_settings(&mut self, args: &[Arg], stat_span: Span) -> Vec<Setting> {
        let mut settings = Vec::new();
        let mut seen: HashMap<String, Span> = HashMap::new();
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if let Some(&first) = seen.get(&name) {
                self.diag(
                    Diagnostic::error(
                        "E1404",
                        format!("duplicate Smooth setting `{name}`"),
                        key_span,
                    )
                    .with_related(first, "first defined here"),
                );
                continue;
            }
            seen.insert(name.clone(), key_span);
            match name.as_str() {
                "method" => {
                    let Some(value) = arg.value() else { continue };
                    match ValueForm::of(&value) {
                        ValueForm::Str(s) if s == "lm" => settings.push(Setting {
                            name,
                            value: SettingValue::String(s),
                        }),
                        _ => self.diag(Diagnostic::error(
                            "E1404",
                            "`method` expects \"lm\"",
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    "E1404",
                    format!("unknown Smooth setting `{name}`"),
                    key_span,
                )),
            }
        }
        if settings.is_empty() {
            settings.push(Setting {
                name: "method".into(),
                value: SettingValue::String("lm".into()),
            });
        }
        let _ = stat_span;
        settings
    }

    fn collect_bin2d_settings(
        &mut self,
        args: &[Arg],
        stat_span: Span,
        stat_name: &str,
    ) -> Vec<Setting> {
        let mut settings = Vec::new();
        let mut seen: HashMap<String, Span> = HashMap::new();
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if let Some(&first) = seen.get(&name) {
                self.diag(
                    Diagnostic::error(
                        "E1404",
                        format!("duplicate {stat_name} setting `{name}`"),
                        key_span,
                    )
                    .with_related(first, "first defined here"),
                );
                continue;
            }
            seen.insert(name.clone(), key_span);
            match name.as_str() {
                "bins" => {
                    let Some(value) = arg.value() else { continue };
                    match ValueForm::of(&value) {
                        ValueForm::Number(n) if n.is_finite() && n >= 1.0 => {
                            settings.push(Setting {
                                name,
                                value: SettingValue::Number(n),
                            });
                        }
                        _ => self.diag(Diagnostic::error(
                            "E1404",
                            "`bins` must be at least 1",
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    "E1404",
                    format!("unknown {stat_name} setting `{name}`"),
                    key_span,
                )),
            }
        }
        let _ = stat_span;
        settings
    }

    fn collect_bin_settings(&mut self, args: &[Arg], stat_span: Span) -> Vec<Setting> {
        let mut settings = Vec::new();
        let mut seen: HashMap<String, Span> = HashMap::new();
        for arg in args {
            let Some(name) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());

            if let Some(&first) = seen.get(&name) {
                self.diag(
                    Diagnostic::error("E1404", format!("duplicate Bin setting `{name}`"), key_span)
                        .with_related(first, "first defined here"),
                );
                continue;
            }
            seen.insert(name.clone(), key_span);

            match name.as_str() {
                "bins" | "binWidth" | "boundary" => {
                    let Some(value) = arg.value() else {
                        continue;
                    };
                    match ValueForm::of(&value) {
                        ValueForm::Number(n) if n.is_finite() => {
                            if name == "bins" && n < 1.0 {
                                self.diag(Diagnostic::error(
                                    "E1404",
                                    "`bins` must be at least 1",
                                    node_span(value.syntax()),
                                ));
                            } else if name == "binWidth" && n <= 0.0 {
                                self.diag(Diagnostic::error(
                                    "E1404",
                                    "`binWidth` must be greater than 0",
                                    node_span(value.syntax()),
                                ));
                            } else {
                                settings.push(Setting {
                                    name,
                                    value: SettingValue::Number(n),
                                });
                            }
                        }
                        form => self.diag(Diagnostic::error(
                            "E1404",
                            format!(
                                "`{name}` expects a finite number, found {}",
                                form.describe()
                            ),
                            node_span(value.syntax()),
                        )),
                    }
                }
                "closed" => {
                    let Some(value) = arg.value() else {
                        continue;
                    };
                    match ValueForm::of(&value) {
                        ValueForm::Str(s) if s == "left" || s == "right" => {
                            settings.push(Setting {
                                name,
                                value: SettingValue::String(s),
                            });
                        }
                        ValueForm::Column(column) => {
                            let written = column.name().unwrap_or_else(|| "left".to_string());
                            self.diag(
                                Diagnostic::error(
                                    "E1404",
                                    "`closed` expects a quoted string value",
                                    node_span(value.syntax()),
                                )
                                .with_help(format!("write it as a string, e.g. {written:?}")),
                            );
                        }
                        form => self.diag(Diagnostic::error(
                            "E1404",
                            format!(
                                "`closed` expects one of [\"left\", \"right\"], found {}",
                                form.describe()
                            ),
                            node_span(value.syntax()),
                        )),
                    }
                }
                _ => self.diag(Diagnostic::error(
                    "E1404",
                    format!("unknown Bin setting `{name}`"),
                    key_span,
                )),
            }
        }
        self.check_bin_setting_conflicts(&settings, stat_span);
        settings
    }

    fn check_bin_setting_conflicts(&mut self, settings: &[Setting], span: Span) {
        let has_bins = settings.iter().any(|setting| setting.name == "bins");
        let has_bin_width = settings.iter().any(|setting| setting.name == "binWidth");
        if has_bins && has_bin_width {
            self.diag(Diagnostic::error(
                "E1404",
                "`bins` and `binWidth` must not both be provided",
                span,
            ));
        }
    }

    // --- Space (spec §13.3, §13.17 phases 8–12) ---

    fn space(&mut self, space: &SpaceBlock) -> SpaceAnalysis {
        let span = node_span(space.syntax());
        let (data_ref, table) = self.space_data(space);

        // Collect space-scope `let` bindings; these shadow chart-scope bindings
        // of the same name for the duration of this space (spec §9.6).
        let space_lets: Vec<LetDecl> = space
            .items()
            .into_iter()
            .filter_map(|item| match item {
                SpaceItem::Let(decl) => Some(decl),
                _ => None,
            })
            .collect();
        self.space_vars = self.collect_let_decls(&space_lets);

        let frame = match space.frame() {
            Some(expr) => {
                let frame = self.build_frame(&expr, &table);
                self.check_cartesian_arity(&frame, node_span(expr.syntax()));
                self.check_facet_variable(&frame);
                self.check_temporal_nesting(&frame);
                frame
            }
            None => FrameIr::Invalid,
        };
        let projection = self.space_projection(space);

        let mut geometries = Vec::new();
        let mut histograms = Vec::new();
        let mut freq_polys = Vec::new();
        let mut bin2ds = Vec::new();
        let mut densities = Vec::new();
        let mut count_bars = Vec::new();
        let mut theme: Option<ThemeIr> = None;
        let mut guides = GuideOverridesIr::default();
        let mut scales = Vec::new();
        let mut saw_geometry = false;
        for item in space.items() {
            match item {
                SpaceItem::Geometry(call) => {
                    saw_geometry = true;
                    if let Some(geo) = self.geometry(&call, &frame, &table) {
                        if geo.kind == GeometryKind::Histogram {
                            histograms.push(geo);
                        } else if geo.kind == GeometryKind::FreqPoly {
                            freq_polys.push(geo);
                        } else if geo.kind == GeometryKind::Bin2D {
                            bin2ds.push(geo);
                        } else if geo.kind == GeometryKind::Density {
                            densities.push(geo);
                        } else if geo.kind == GeometryKind::Bar && has_count_stat(&geo) {
                            count_bars.push(geo);
                        } else {
                            geometries.push(geo);
                        }
                    }
                }
                SpaceItem::Theme(decl) => {
                    if let Some(t) = self.theme_decl(&decl) {
                        theme = Some(t);
                    }
                }
                SpaceItem::Scale(decl) => {
                    if let Some(scale) = self.scale_decl(&decl, &table) {
                        scales.push(scale);
                    }
                }
                SpaceItem::Guide(decl) => self.guide_decl(&decl, &mut guides),
                SpaceItem::Let(_) => {}
                SpaceItem::Error(_) => {}
            }
        }
        if !saw_geometry {
            self.diag(Diagnostic::warning("W2001", "empty Space block", span));
        }
        self.check_spatial_geometries(&geometries, &frame);

        let mut analysis = SpaceAnalysis::default();
        for histogram in histograms {
            if let Some((derive, histogram_space)) = self.desugar_histogram(
                &histogram,
                &frame,
                theme.clone(),
                guides.clone(),
                scales.clone(),
            ) {
                analysis.derived.push(derive);
                analysis.spaces.push(histogram_space);
            }
        }
        for freq_poly in freq_polys {
            if let Some((derive, freq_space)) = self.desugar_freq_poly(
                &freq_poly,
                &frame,
                theme.clone(),
                guides.clone(),
                scales.clone(),
            ) {
                analysis.derived.push(derive);
                analysis.spaces.push(freq_space);
            }
        }
        for bin2d in bin2ds {
            if let Some((derive, bin2d_space)) = self.desugar_bin2d(
                &bin2d,
                &frame,
                theme.clone(),
                guides.clone(),
                scales.clone(),
            ) {
                analysis.derived.push(derive);
                analysis.spaces.push(bin2d_space);
            }
        }
        for density in densities {
            if let Some((derive, density_space)) = self.desugar_density(
                &density,
                &frame,
                theme.clone(),
                guides.clone(),
                scales.clone(),
            ) {
                analysis.derived.push(derive);
                analysis.spaces.push(density_space);
            }
        }
        for bar in count_bars {
            if let Some((derive, count_space)) = self.desugar_count_bar(
                &bar,
                &frame,
                &data_ref,
                theme.clone(),
                guides.clone(),
                scales.clone(),
            ) {
                analysis.derived.push(derive);
                analysis.spaces.push(count_space);
            }
        }
        if !geometries.is_empty() || analysis.spaces.is_empty() {
            analysis.spaces.push(SpaceIr {
                data: data_ref,
                frame,
                geometries,
                guides,
                scales,
                theme,
                projection,
                span,
            });
        }
        // Space-scope bindings do not leak into sibling spaces (spec §9.6).
        self.space_vars = HashMap::new();
        analysis
    }

    fn desugar_histogram(
        &mut self,
        histogram: &GeometryIr,
        frame: &FrameIr,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(DeriveIr, SpaceIr)> {
        let FrameIr::Vector(input) = frame else {
            self.diag(Diagnostic::error(
                "E1302",
                "Histogram requires a single numeric vector space",
                histogram.span,
            ));
            return None;
        };

        match input.dtype {
            DataType::Temporal | DataType::Integer | DataType::Float | DataType::Unknown => {}
            _ => {
                self.diag(Diagnostic::error(
                    "E1404",
                    format!(
                        "Histogram input column `{}` is not numeric or temporal",
                        input.name
                    ),
                    input.span,
                ));
                return None;
            }
        }

        let name = self.next_histogram_name();
        let settings = self.histogram_bin_settings(histogram);
        let output_schema = bin_output_schema(input.dtype);
        let derive = DeriveIr {
            name: name.clone(),
            data: SpaceDataRef::Primary,
            stat: StatCallIr {
                kind: StatKind::Bin,
                input: FrameIr::Vector(input.clone()),
                settings,
                span: histogram.span,
            },
            output_schema,
            span: histogram.span,
        };

        let boundary_dtype = bin_boundary_dtype(input.dtype);
        let bin_start = synthetic_column("bin_start", boundary_dtype, histogram.span);
        let bin_end = synthetic_column("bin_end", boundary_dtype, histogram.span);
        let count = synthetic_column("count", DataType::Integer, histogram.span);
        let rect = GeometryIr {
            kind: GeometryKind::Rect,
            mappings: vec![
                AestheticMapping {
                    aesthetic: "xmin".into(),
                    column: bin_start.clone(),
                },
                AestheticMapping {
                    aesthetic: "xmax".into(),
                    column: bin_end,
                },
                AestheticMapping {
                    aesthetic: "ymax".into(),
                    column: count.clone(),
                },
            ],
            settings: histogram_rect_settings(histogram),
            span: histogram.span,
        };
        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![FrameIr::Vector(bin_start), FrameIr::Vector(count)]),
            geometries: vec![rect],
            guides,
            scales,
            theme,
            projection: None,
            span: histogram.span,
        };
        Some((derive, space))
    }

    fn desugar_freq_poly(
        &mut self,
        freq_poly: &GeometryIr,
        frame: &FrameIr,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(DeriveIr, SpaceIr)> {
        let FrameIr::Vector(input) = frame else {
            self.diag(Diagnostic::error(
                "E1302",
                "FreqPoly requires a single numeric vector space",
                freq_poly.span,
            ));
            return None;
        };
        match input.dtype {
            DataType::Temporal | DataType::Integer | DataType::Float | DataType::Unknown => {}
            _ => {
                self.diag(Diagnostic::error(
                    "E1404",
                    format!(
                        "FreqPoly input column `{}` is not numeric or temporal",
                        input.name
                    ),
                    input.span,
                ));
                return None;
            }
        }

        let name = self.next_freq_poly_name();
        let settings = self.histogram_bin_settings(freq_poly);
        let output_schema = bin_output_schema(input.dtype);
        let derive = DeriveIr {
            name: name.clone(),
            data: SpaceDataRef::Primary,
            stat: StatCallIr {
                kind: StatKind::Bin,
                input: FrameIr::Vector(input.clone()),
                settings,
                span: freq_poly.span,
            },
            output_schema,
            span: freq_poly.span,
        };

        let boundary_dtype = bin_boundary_dtype(input.dtype);
        let bin_center = synthetic_column("bin_center", boundary_dtype, freq_poly.span);
        let count = synthetic_column("count", DataType::Integer, freq_poly.span);
        let line = GeometryIr {
            kind: GeometryKind::Line,
            mappings: Vec::new(),
            settings: line_settings_from(freq_poly),
            span: freq_poly.span,
        };
        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![FrameIr::Vector(bin_center), FrameIr::Vector(count)]),
            geometries: vec![line],
            guides,
            scales,
            theme,
            projection: None,
            span: freq_poly.span,
        };
        Some((derive, space))
    }

    fn desugar_bin2d(
        &mut self,
        bin2d: &GeometryIr,
        frame: &FrameIr,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(DeriveIr, SpaceIr)> {
        let FrameIr::Cartesian(axes) = frame else {
            self.diag(Diagnostic::error(
                "E1302",
                "Bin2D requires a two-dimensional continuous space",
                bin2d.span,
            ));
            return None;
        };
        let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) = (axes.first(), axes.get(1))
        else {
            self.diag(Diagnostic::error(
                "E1302",
                "Bin2D requires two vector dimensions",
                bin2d.span,
            ));
            return None;
        };
        for col in [x, y] {
            if !matches!(
                col.dtype,
                DataType::Integer | DataType::Float | DataType::Unknown
            ) {
                self.diag(Diagnostic::error(
                    "E1404",
                    format!("Bin2D input column `{}` is not numeric", col.name),
                    col.span,
                ));
                return None;
            }
        }

        let name = self.next_bin2d_name();
        let derive = DeriveIr {
            name: name.clone(),
            data: SpaceDataRef::Primary,
            stat: StatCallIr {
                kind: StatKind::Bin2D,
                input: FrameIr::Cartesian(vec![
                    FrameIr::Vector(x.clone()),
                    FrameIr::Vector(y.clone()),
                ]),
                settings: self.bin2d_geom_settings(bin2d),
                span: bin2d.span,
            },
            output_schema: bin2d_output_schema(),
            span: bin2d.span,
        };

        let x_start = synthetic_column("x_start", DataType::Float, bin2d.span);
        let x_end = synthetic_column("x_end", DataType::Float, bin2d.span);
        let y_start = synthetic_column("y_start", DataType::Float, bin2d.span);
        let y_end = synthetic_column("y_end", DataType::Float, bin2d.span);
        let count = synthetic_column("count", DataType::Integer, bin2d.span);
        let mut mappings = vec![
            AestheticMapping {
                aesthetic: "xmin".into(),
                column: x_start.clone(),
            },
            AestheticMapping {
                aesthetic: "xmax".into(),
                column: x_end.clone(),
            },
            AestheticMapping {
                aesthetic: "ymin".into(),
                column: y_start.clone(),
            },
            AestheticMapping {
                aesthetic: "ymax".into(),
                column: y_end.clone(),
            },
        ];
        if !bin2d.settings.iter().any(|setting| setting.name == "fill") {
            mappings.push(AestheticMapping {
                aesthetic: "fill".into(),
                column: count,
            });
        }
        let rect = GeometryIr {
            kind: GeometryKind::Rect,
            mappings,
            settings: bin2d_rect_settings(bin2d),
            span: bin2d.span,
        };
        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![
                FrameIr::Union(vec![FrameIr::Vector(x_start), FrameIr::Vector(x_end)]),
                FrameIr::Union(vec![FrameIr::Vector(y_start), FrameIr::Vector(y_end)]),
            ]),
            geometries: vec![rect],
            guides,
            scales,
            theme,
            projection: None,
            span: bin2d.span,
        };
        Some((derive, space))
    }

    /// Desugar `Density()` over a 1D numeric vector space into a kernel-density
    /// derived table and a 2D `Area` space (spec §15.11). The KDE produces
    /// `density_x` and `density` columns; the area is drawn from the curve down
    /// to a zero baseline, mirroring how `Histogram` desugars to `Rect`.
    fn desugar_density(
        &mut self,
        density: &GeometryIr,
        frame: &FrameIr,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(DeriveIr, SpaceIr)> {
        let FrameIr::Vector(input) = frame else {
            self.diag(Diagnostic::error(
                "E1302",
                "Density requires a single numeric vector space",
                density.span,
            ));
            return None;
        };

        match input.dtype {
            DataType::Integer | DataType::Float | DataType::Unknown => {}
            _ => {
                self.diag(Diagnostic::error(
                    "E1404",
                    format!("Density input column `{}` is not numeric", input.name),
                    input.span,
                ));
                return None;
            }
        }

        let name = self.next_density_name();
        let settings = self.density_settings(density);
        let output_schema = vec![
            ColumnDefIr {
                name: "density_x".into(),
                dtype: DataType::Float,
            },
            ColumnDefIr {
                name: "density".into(),
                dtype: DataType::Float,
            },
        ];
        let derive = DeriveIr {
            name: name.clone(),
            data: SpaceDataRef::Primary,
            stat: StatCallIr {
                kind: StatKind::Density,
                input: FrameIr::Vector(input.clone()),
                settings,
                span: density.span,
            },
            output_schema,
            span: density.span,
        };

        let density_x = synthetic_column("density_x", DataType::Float, density.span);
        let density_y = synthetic_column("density", DataType::Float, density.span);
        let area = GeometryIr {
            kind: GeometryKind::Area,
            mappings: Vec::new(),
            settings: density_area_settings(density),
            span: density.span,
        };
        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![FrameIr::Vector(density_x), FrameIr::Vector(density_y)]),
            geometries: vec![area],
            guides,
            scales,
            theme,
            projection: None,
            span: density.span,
        };
        Some((derive, space))
    }

    fn density_settings(&mut self, density: &GeometryIr) -> Vec<Setting> {
        let settings: Vec<Setting> = density
            .settings
            .iter()
            .filter(|setting| matches!(setting.name.as_str(), "bandwidth" | "n"))
            .map(|setting| Setting {
                name: setting.name.clone(),
                value: setting.value.clone(),
            })
            .collect();

        if settings.iter().any(|setting| {
            setting.name == "bandwidth"
                && !matches!(setting.value, SettingValue::Number(value) if value > 0.0)
        }) {
            self.diag(Diagnostic::error(
                "E1404",
                "`bandwidth` must be greater than 0",
                density.span,
            ));
        }
        if settings.iter().any(|setting| {
            setting.name == "n"
                && !matches!(setting.value, SettingValue::Number(value) if value >= 2.0)
        }) {
            self.diag(Diagnostic::error(
                "E1404",
                "`n` must be at least 2",
                density.span,
            ));
        }
        settings
    }

    fn next_density_name(&mut self) -> String {
        loop {
            let name = format!("__density_{}", self.synthetic_counter);
            self.synthetic_counter += 1;
            if !self.derived.contains_key(&name) && !self.reserved_derived_names.contains(&name) {
                return name;
            }
        }
    }

    /// Desugar `Bar(stat: "count")` over a 1D categorical space into a Count
    /// derived table and a 2D `Bar` space (spec §15.5).
    fn desugar_count_bar(
        &mut self,
        bar: &GeometryIr,
        frame: &FrameIr,
        data_ref: &SpaceDataRef,
        theme: Option<ThemeIr>,
        guides: GuideOverridesIr,
        scales: Vec<ScaleIr>,
    ) -> Option<(DeriveIr, SpaceIr)> {
        // Find the categorical group column(s). For 0.1, support 1D categorical
        // space (`Space(category)`) and nested 1D (`Space(outer / inner)`).
        let group_cols: Vec<&ColumnRef> = match frame {
            FrameIr::Vector(column) => vec![column],
            FrameIr::Nested { outer, inner } => match (outer.as_ref(), inner.as_ref()) {
                (FrameIr::Vector(o), FrameIr::Vector(i)) => vec![o, i],
                _ => {
                    self.diag(Diagnostic::error(
                        "E1302",
                        "Bar(stat: \"count\") requires a 1D categorical space",
                        bar.span,
                    ));
                    return None;
                }
            },
            _ => {
                self.diag(Diagnostic::error(
                    "E1302",
                    "Bar(stat: \"count\") requires a 1D categorical space",
                    bar.span,
                ));
                return None;
            }
        };

        // Only desugar when reading the primary table; counts over derived
        // tables are not meaningful in 0.1.
        if !matches!(data_ref, SpaceDataRef::Primary) {
            self.diag(Diagnostic::error(
                "E1302",
                "Bar(stat: \"count\") must read from the primary table",
                bar.span,
            ));
            return None;
        }

        let name = self.next_count_name();

        // The Count derived schema: group columns (as-is) + a `count` integer.
        let mut output_schema: Vec<ColumnDefIr> = group_cols
            .iter()
            .map(|c| ColumnDefIr {
                name: c.name.clone(),
                dtype: c.dtype,
            })
            .collect();
        output_schema.push(ColumnDefIr {
            name: "count".into(),
            dtype: DataType::Integer,
        });

        // The stat input frame is just the categorical key(s).
        let stat_input = if group_cols.len() == 1 {
            FrameIr::Vector((*group_cols[0]).clone())
        } else {
            FrameIr::Nested {
                outer: Box::new(FrameIr::Vector((*group_cols[0]).clone())),
                inner: Box::new(FrameIr::Vector((*group_cols[1]).clone())),
            }
        };

        let derive = DeriveIr {
            name: name.clone(),
            data: SpaceDataRef::Primary,
            stat: StatCallIr {
                kind: StatKind::Count,
                input: stat_input,
                settings: Vec::new(),
                span: bar.span,
            },
            output_schema,
            span: bar.span,
        };

        // The derived-table-backed space mirrors the input keys on x and uses
        // `count` for y.
        let count_col = synthetic_column("count", DataType::Integer, bar.span);
        let x_frame = if group_cols.len() == 1 {
            FrameIr::Vector(synthetic_column(
                &group_cols[0].name,
                group_cols[0].dtype,
                bar.span,
            ))
        } else {
            FrameIr::Nested {
                outer: Box::new(FrameIr::Vector(synthetic_column(
                    &group_cols[0].name,
                    group_cols[0].dtype,
                    bar.span,
                ))),
                inner: Box::new(FrameIr::Vector(synthetic_column(
                    &group_cols[1].name,
                    group_cols[1].dtype,
                    bar.span,
                ))),
            }
        };

        // Preserve mappings/settings from the original Bar (e.g. fill, alpha).
        // The y resolution comes from the derived `count` column via the
        // synthetic Cartesian frame; no explicit `y` mapping is needed.
        let mappings = bar.mappings.clone();
        let settings = bar
            .settings
            .iter()
            .filter(|s| s.name != "stat")
            .cloned()
            .collect();

        let bar_ir = GeometryIr {
            kind: GeometryKind::Bar,
            mappings,
            settings,
            span: bar.span,
        };

        let space = SpaceIr {
            data: SpaceDataRef::Derived(name),
            frame: FrameIr::Cartesian(vec![x_frame, FrameIr::Vector(count_col)]),
            geometries: vec![bar_ir],
            guides,
            scales,
            theme,
            projection: None,
            span: bar.span,
        };
        Some((derive, space))
    }

    fn next_count_name(&mut self) -> String {
        loop {
            let name = format!("__count_{}", self.synthetic_counter);
            self.synthetic_counter += 1;
            if !self.derived.contains_key(&name) && !self.reserved_derived_names.contains(&name) {
                return name;
            }
        }
    }

    fn histogram_bin_settings(&mut self, histogram: &GeometryIr) -> Vec<Setting> {
        let settings: Vec<Setting> = histogram
            .settings
            .iter()
            .filter(|setting| {
                matches!(
                    setting.name.as_str(),
                    "bins" | "binWidth" | "boundary" | "closed"
                )
            })
            .map(|setting| Setting {
                name: setting.name.clone(),
                value: setting.value.clone(),
            })
            .collect();

        if settings.iter().any(|setting| {
            setting.name == "bins"
                && !matches!(setting.value, SettingValue::Number(value) if value >= 1.0)
        }) {
            self.diag(Diagnostic::error(
                "E1404",
                "`bins` must be at least 1",
                histogram.span,
            ));
        }
        if settings.iter().any(|setting| {
            setting.name == "binWidth"
                && !matches!(setting.value, SettingValue::Number(value) if value > 0.0)
        }) {
            self.diag(Diagnostic::error(
                "E1404",
                "`binWidth` must be greater than 0",
                histogram.span,
            ));
        }
        self.check_bin_setting_conflicts(&settings, histogram.span);
        settings
    }

    fn next_histogram_name(&mut self) -> String {
        loop {
            let name = format!("__histogram_{}", self.synthetic_counter);
            self.synthetic_counter += 1;
            if !self.derived.contains_key(&name) && !self.reserved_derived_names.contains(&name) {
                return name;
            }
        }
    }

    fn next_freq_poly_name(&mut self) -> String {
        loop {
            let name = format!("__freqpoly_{}", self.synthetic_counter);
            self.synthetic_counter += 1;
            if !self.derived.contains_key(&name) && !self.reserved_derived_names.contains(&name) {
                return name;
            }
        }
    }

    fn next_bin2d_name(&mut self) -> String {
        loop {
            let name = format!("__bin2d_{}", self.synthetic_counter);
            self.synthetic_counter += 1;
            if !self.derived.contains_key(&name) && !self.reserved_derived_names.contains(&name) {
                return name;
            }
        }
    }

    fn bin2d_geom_settings(&mut self, bin2d: &GeometryIr) -> Vec<Setting> {
        bin2d
            .settings
            .iter()
            .filter(|setting| setting.name == "bins")
            .map(|setting| Setting {
                name: setting.name.clone(),
                value: setting.value.clone(),
            })
            .collect()
    }

    /// Read the optional `projection:` argument of a space as a string literal
    /// (spec §16.14). The string's validity (alias or PROJ form) is checked at
    /// render time, where the projection registry lives (`E1802`).
    fn space_projection(&mut self, space: &SpaceBlock) -> Option<String> {
        let arg = space
            .args()
            .into_iter()
            .find(|a| a.key().as_deref() == Some("projection"))?;
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                Some(string_value(&lit.text().unwrap_or_default()))
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    "E1802",
                    "`projection` expects a string literal (an alias or a `+proj=…` string)",
                    node_span(value.syntax()),
                ));
                None
            }
            None => None,
        }
    }

    /// Validate `Geo` marks against their space frame (spec §16.14, §14.x). A
    /// `Geo` mark requires a spatial space: its frame must be a single geometry
    /// column. A single non-geometry column is `E1801`; a planar (multi-axis)
    /// frame is `E1804`.
    fn check_spatial_geometries(&mut self, geometries: &[GeometryIr], frame: &FrameIr) {
        for geo in geometries.iter().filter(|g| g.kind == GeometryKind::Geo) {
            match frame {
                FrameIr::Vector(col) if col.dtype == DataType::Geometry => {}
                // The column was already reported as unknown (E1101); avoid a
                // confusing second diagnostic.
                FrameIr::Vector(col) if col.dtype == DataType::Unknown => {}
                FrameIr::Vector(_) => self.diag(Diagnostic::error(
                    "E1801",
                    "a spatial space requires a geometry column; \
                     `Geo` must be used in a `Space(geom)` over a geometry column",
                    geo.span,
                )),
                FrameIr::Invalid => {}
                _ => self.diag(Diagnostic::error(
                    "E1804",
                    "`Geo` mark requires a spatial space (a `Space` over a geometry column), \
                     not a planar Cartesian space",
                    geo.span,
                )),
            }
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
                if self.table_names.contains(&table_name) {
                    let table = self.table_active(&table_name);
                    return (SpaceDataRef::Table(table_name), table);
                }
                self.diag(Diagnostic::error(
                    "E1103",
                    format!("unknown table `{table_name}`"),
                    node_span(name.syntax()),
                ));
            } else if let Some(value) = arg.value() {
                self.diag(Diagnostic::error(
                    "E1103",
                    "space `data` must name a derived or declared table",
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
        let span = name
            .ident_span()
            .unwrap_or_else(|| node_span(name.syntax()));
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

    fn check_temporal_nesting(&mut self, frame: &FrameIr) {
        match frame {
            FrameIr::Nested { outer, inner } => {
                if direct_temporal_vector(outer) || direct_temporal_vector(inner) {
                    self.diag(
                        Diagnostic::warning(
                            "W2008",
                            "high-cardinality temporal nesting may create excessive bands or panels",
                            temporal_nesting_span(outer)
                                .or_else(|| temporal_nesting_span(inner))
                                .unwrap_or(Span::new(0, 0)),
                        )
                        .with_help(
                            "precompute a coarser period column such as day, week, month, or year",
                        ),
                    );
                }
                self.check_temporal_nesting(outer);
                self.check_temporal_nesting(inner);
            }
            FrameIr::Cartesian(axes) | FrameIr::Union(axes) => {
                for axis in axes {
                    self.check_temporal_nesting(axis);
                }
            }
            FrameIr::Vector(_) | FrameIr::Invalid => {}
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
        // Resolve `let` variables in property value positions before type
        // checking, so a bound constant is checked as its value (spec §9.6).
        let form = self.substitute_var(ValueForm::of(&value));

        // Color literals written as bare identifiers (e.g. `fill: red`) are a
        // common mistake. If this property accepts a color and the value is a
        // bare identifier that names a known CSS color but no such column
        // exists, emit a hint to quote it (H3002).
        if prop.accepts.contains(&Accept::Color) {
            if let ValueForm::Column(name) = &form {
                let raw = name.name().unwrap_or_default();
                if !name.is_quoted() && table.get(&raw).is_none() && is_css_color_name(&raw) {
                    self.diag(
                        Diagnostic::new(
                            Severity::Hint,
                            "H3002",
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

/// A parsed `Scale(range: ...)` declaration before the target is known.
enum RangeSpec {
    /// Two numeric output bounds, each possibly inferred (`[0, 30]`).
    Numeric([Option<f64>; 2], Span),
    /// A manual category → color map (`["A" => "burlywood"]`).
    ColorMap(Vec<(String, String)>, Span),
}

#[derive(Default)]
struct SpaceAnalysis {
    derived: Vec<DeriveIr>,
    spaces: Vec<SpaceIr>,
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
    StringArray(Option<Vec<String>>),
    Stdin,
    /// A nested call value such as `Text(size: 12)` (spec §20.8); only valid in
    /// theme override positions, handled directly there.
    Call,
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
                let mut strings = Vec::new();
                let mut all_numeric = true;
                let mut all_strings = true;
                for item in array.values() {
                    match ValueForm::of(&item) {
                        ValueForm::Number(n) => nums.push(n),
                        _ => all_numeric = false,
                    }
                    match ValueForm::of(&item) {
                        ValueForm::Str(s) => strings.push(s),
                        _ => all_strings = false,
                    }
                }
                if all_strings {
                    ValueForm::StringArray(Some(strings))
                } else {
                    ValueForm::Array(all_numeric.then_some(nums))
                }
            }
            // Map literals are valid only in `Scale(range:/labels:)` positions,
            // handled directly there; elsewhere they are an invalid value.
            ValueExpr::Map(_) => ValueForm::Error,
            ValueExpr::Call(_) => ValueForm::Call,
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
            ValueForm::Array(_) | ValueForm::StringArray(_) => "an array",
            ValueForm::Stdin => "the stdin sentinel",
            ValueForm::Call => "a nested call",
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

fn is_color_literal(value: &str) -> bool {
    is_hex_color(value) || is_css_color_name(value)
}

fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 6) && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn has_count_stat(geo: &GeometryIr) -> bool {
    geo.settings.iter().any(|setting| {
        setting.name == "stat" && matches!(&setting.value, SettingValue::String(v) if v == "count")
    })
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

fn direct_temporal_vector(frame: &FrameIr) -> bool {
    matches!(frame, FrameIr::Vector(column) if column.dtype == DataType::Temporal)
}

fn temporal_nesting_span(frame: &FrameIr) -> Option<Span> {
    match frame {
        FrameIr::Vector(column) if column.dtype == DataType::Temporal => Some(column.span),
        _ => None,
    }
}

fn chart_derived_names(chart: &ChartBlock) -> HashSet<String> {
    chart
        .items()
        .into_iter()
        .filter_map(|item| match item {
            ChartItem::Derive(derive) => derive.name(),
            _ => None,
        })
        .collect()
}

fn derive_input_names(derive: &DeriveDecl) -> Vec<String> {
    derive
        .stat()
        .map(|stat| {
            stat.inputs()
                .into_iter()
                .filter_map(|input| match input {
                    AlgebraExpr::Name(name) => name.name(),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn derive_output_names(derive: &DeriveDecl) -> Vec<String> {
    let Some(stat) = derive.stat() else {
        return Vec::new();
    };
    match stat.name().unwrap_or_default().as_str() {
        "Bin" => bin_output_schema(DataType::Float)
            .into_iter()
            .map(|column| column.name)
            .collect(),
        "Smooth" => smooth_output_schema()
            .into_iter()
            .map(|column| column.name)
            .collect(),
        "Bin2D" => bin2d_output_schema()
            .into_iter()
            .map(|column| column.name)
            .collect(),
        "HexBin" => hexbin_output_schema()
            .into_iter()
            .map(|column| column.name)
            .collect(),
        _ => Vec::new(),
    }
}

fn bin_output_schema(input_dtype: DataType) -> Vec<ColumnDefIr> {
    let boundary_dtype = bin_boundary_dtype(input_dtype);
    vec![
        ColumnDefIr {
            name: "bin_start".into(),
            dtype: boundary_dtype,
        },
        ColumnDefIr {
            name: "bin_end".into(),
            dtype: boundary_dtype,
        },
        ColumnDefIr {
            name: "bin_center".into(),
            dtype: boundary_dtype,
        },
        ColumnDefIr {
            name: "count".into(),
            dtype: DataType::Integer,
        },
        ColumnDefIr {
            name: "density".into(),
            dtype: DataType::Float,
        },
    ]
}

fn smooth_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: DataType::Float,
        },
    ]
}

fn bin2d_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "x_start".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "x_end".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "x_center".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y_start".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y_end".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y_center".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "count".into(),
            dtype: DataType::Integer,
        },
        ColumnDefIr {
            name: "density".into(),
            dtype: DataType::Float,
        },
    ]
}

fn hexbin_output_schema() -> Vec<ColumnDefIr> {
    vec![
        ColumnDefIr {
            name: "x".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "y".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "radius".into(),
            dtype: DataType::Float,
        },
        ColumnDefIr {
            name: "count".into(),
            dtype: DataType::Integer,
        },
        ColumnDefIr {
            name: "density".into(),
            dtype: DataType::Float,
        },
    ]
}

fn bin_boundary_dtype(input_dtype: DataType) -> DataType {
    if input_dtype == DataType::Temporal {
        DataType::Temporal
    } else {
        DataType::Float
    }
}

fn synthetic_column(name: &str, dtype: DataType, span: Span) -> ColumnRef {
    ColumnRef {
        name: name.into(),
        dtype,
        span,
    }
}

fn histogram_rect_settings(histogram: &GeometryIr) -> Vec<GeometrySetting> {
    let mut settings = vec![GeometrySetting {
        name: "ymin".into(),
        value: SettingValue::Number(0.0),
    }];
    settings.extend(
        histogram
            .settings
            .iter()
            .filter(|setting| {
                matches!(
                    setting.name.as_str(),
                    "fill" | "stroke" | "strokeWidth" | "alpha"
                )
            })
            .cloned(),
    );
    settings
}

fn line_settings_from(geometry: &GeometryIr) -> Vec<GeometrySetting> {
    geometry
        .settings
        .iter()
        .filter(|setting| matches!(setting.name.as_str(), "stroke" | "strokeWidth" | "alpha"))
        .cloned()
        .collect()
}

fn bin2d_rect_settings(bin2d: &GeometryIr) -> Vec<GeometrySetting> {
    bin2d
        .settings
        .iter()
        .filter(|setting| {
            matches!(
                setting.name.as_str(),
                "fill" | "stroke" | "strokeWidth" | "alpha"
            )
        })
        .cloned()
        .collect()
}

/// Pass the visual settings of a `Density` geometry through to the `Area` it
/// desugars into. The KDE curve is filled to a zero baseline.
fn density_area_settings(density: &GeometryIr) -> Vec<GeometrySetting> {
    let mut settings = vec![GeometrySetting {
        name: "baseline".into(),
        value: SettingValue::Number(0.0),
    }];
    settings.extend(
        density
            .settings
            .iter()
            .filter(|setting| {
                matches!(
                    setting.name.as_str(),
                    "fill" | "stroke" | "strokeWidth" | "alpha"
                )
            })
            .cloned(),
    );
    settings
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
