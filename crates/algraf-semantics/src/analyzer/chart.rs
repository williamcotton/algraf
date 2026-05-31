//! Chart header analysis (spec §13.17 phases 2, 6–8): `Chart(...)` arguments,
//! defaults, the `Layout` declaration, and the chart-body dispatch loop.

use std::collections::HashSet;

use algraf_core::{codes, Diagnostic};
use algraf_syntax::ast::{
    AlgebraExpr, Arg, ChartBlock, ChartItem, Decl, LetDecl, LiteralKind, MapValue, ValueExpr,
};
use algraf_syntax::{
    is_source_constructor, node_span, source_expr_from_arg,
    unescape_string_literal as string_value, SourceExpr, SourceFormat,
};

use super::args::DupGuard;
use super::context::{ActiveTable, Analyzer};
use crate::ir::*;
use crate::registry;

pub(super) const DEFAULT_WIDTH: u32 = 800;
pub(super) const DEFAULT_HEIGHT: u32 = 520;

/// Parsed `Chart(...)` header arguments (spec §13.17 phase 2).
struct ChartArgs {
    data_source: DataSourceIr,
    width: u32,
    height: u32,
    title: Option<String>,
    subtitle: Option<String>,
    caption: Option<String>,
    alt: Option<String>,
    description: Option<String>,
    margin_top: Option<u32>,
    margin_right: Option<u32>,
    margin_bottom: Option<u32>,
    margin_left: Option<u32>,
}

impl Analyzer<'_> {
    // --- Chart (spec §13.17 phases 2, 6–8) ---

    pub(super) fn chart(&mut self, chart: &ChartBlock) -> Option<ChartIr> {
        let ChartArgs {
            data_source,
            width,
            height,
            title,
            subtitle,
            caption,
            alt,
            description,
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
                ChartItem::Parse(_) => {}
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
                ChartItem::Layout(decl) => self.layout_decl(&decl, &mut layout, &primary_table),
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
            alt,
            description,
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

        let mut dup = DupGuard::new(codes::E1002, "Chart argument");
        let mut data_source = None;
        let mut width = DEFAULT_WIDTH;
        let mut height = DEFAULT_HEIGHT;
        let mut title = None;
        let mut subtitle = None;
        let mut caption = None;
        let mut alt = None;
        let mut description = None;
        let mut margin_top = None;
        let mut margin_right = None;
        let mut margin_bottom = None;
        let mut margin_left = None;

        for arg in &args {
            let Some(key) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &key, key_span) {
                continue;
            }

            if !registry::CHART_ARGS.contains(&key.as_str()) {
                self.diag(Diagnostic::error(
                    codes::E1003,
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
                "title" => {
                    title =
                        self.expect_string(arg, codes::E1204, "`title` expects a string literal")
                }
                "subtitle" => {
                    subtitle =
                        self.expect_string(arg, codes::E1204, "`subtitle` expects a string literal")
                }
                "caption" => {
                    caption =
                        self.expect_string(arg, codes::E1204, "`caption` expects a string literal")
                }
                "alt" => {
                    alt = self.expect_string(arg, codes::E1204, "`alt` expects a string literal")
                }
                "description" => {
                    description = self.expect_string(
                        arg,
                        codes::E1204,
                        "`description` expects a string literal",
                    )
                }
                "marginTop" => margin_top = self.arg_u32(arg),
                "marginRight" => margin_right = self.arg_u32(arg),
                "marginBottom" => margin_bottom = self.arg_u32(arg),
                "marginLeft" => margin_left = self.arg_u32(arg),
                _ => {}
            }
        }

        let data_source = data_source.unwrap_or_else(|| {
            self.diag(Diagnostic::error(
                codes::E1001,
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
            alt,
            description,
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
            SourceExpr::Path {
                path,
                format: Some(SourceFormat::Parquet),
                ..
            } => DataSourceIr::Parquet(path),
            SourceExpr::Sqlite { path, query, .. } => DataSourceIr::Sqlite { path, query },
            SourceExpr::TopoJson { path, object, .. } => DataSourceIr::TopoJson { path, object },
            SourceExpr::Stdin { .. } => DataSourceIr::Stdin,
            SourceExpr::Invalid { span } => {
                if let Some(ValueExpr::Call(call)) = arg.value() {
                    if is_source_constructor(&call) {
                        self.diag(Diagnostic::error(
                            codes::E1004,
                            format!(
                                "`{}` source expects string-literal arguments",
                                call.name().unwrap_or_default()
                            ),
                            span,
                        ));
                        return DataSourceIr::Missing;
                    }
                }
                self.diag(Diagnostic::error(
                    codes::E1004,
                    "data source must be a string literal, a \
                     `GeoJson`/`Shapefile`/`Sqlite`/`TopoJson`/`Parquet` source constructor, \
                     or the `stdin` sentinel",
                    span,
                ));
                DataSourceIr::Missing
            }
            SourceExpr::Missing => {
                self.diag(Diagnostic::error(
                    codes::E1004,
                    "data source must be a string literal, a \
                     `GeoJson`/`Shapefile`/`Sqlite`/`TopoJson`/`Parquet` source constructor, \
                     or the `stdin` sentinel",
                    node_span(arg.syntax()),
                ));
                DataSourceIr::Missing
            }
        }
    }

    pub(super) fn arg_u32(&mut self, arg: &Arg) -> Option<u32> {
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::Number) => lit
                .text()
                .and_then(|t| t.parse::<f64>().ok())
                .map(|f| f.max(0.0) as u32),
            _ => None,
        }
    }

    fn layout_decl(&mut self, decl: &Decl, layout: &mut LayoutIr, table: &ActiveTable) {
        let mut dup = DupGuard::new(codes::E1002, "Layout argument");
        for arg in decl.args() {
            let Some(key) = arg.key() else { continue };
            let key_span = node_span(arg.syntax());
            if dup.is_duplicate(&mut self.diagnostics, &key, key_span) {
                continue;
            }

            match key.as_str() {
                "facetColumns" => match self.arg_u32(&arg) {
                    Some(columns) if columns > 0 => layout.facet_columns = Some(columns as usize),
                    _ => self.diag(Diagnostic::error(
                        codes::E1204,
                        "`facetColumns` expects a positive number",
                        key_span,
                    )),
                },
                "facetRows" => {
                    let column =
                        self.layout_column(&arg, table, "`facetRows` expects a column name");
                    if let Some(column) = column {
                        if column.dtype != algraf_data::DataType::Unknown
                            && !column.dtype.is_categorical()
                        {
                            self.diag(
                                Diagnostic::error(
                                    codes::E1303,
                                    format!(
                                        "facet row column `{}` must be categorical",
                                        column.name
                                    ),
                                    column.span,
                                )
                                .with_help(
                                    "use a string, boolean, or pre-binned column for facet rows",
                                ),
                            );
                        }
                        let mut grid = layout.facet_grid.take().unwrap_or(FacetGridIr {
                            rows: None,
                            columns: None,
                        });
                        grid.rows = Some(column);
                        layout.facet_grid = Some(grid);
                    }
                }
                "facetCols" => {
                    let column =
                        self.layout_column(&arg, table, "`facetCols` expects a column name");
                    if let Some(column) = column {
                        if column.dtype != algraf_data::DataType::Unknown
                            && !column.dtype.is_categorical()
                        {
                            self.diag(
                                Diagnostic::error(
                                    codes::E1303,
                                    format!("facet column `{}` must be categorical", column.name),
                                    column.span,
                                )
                                .with_help(
                                    "use a string, boolean, or pre-binned column for facet columns",
                                ),
                            );
                        }
                        let mut grid = layout.facet_grid.take().unwrap_or(FacetGridIr {
                            rows: None,
                            columns: None,
                        });
                        grid.columns = Some(column);
                        layout.facet_grid = Some(grid);
                    }
                }
                "facetScales" => match self
                    .layout_string(&arg, "`facetScales` expects a string literal")
                    .as_deref()
                {
                    Some("fixed") => layout.facet_scales = FacetScaleModeIr::Fixed,
                    Some("free-x") | Some("free_x") => {
                        layout.facet_scales = FacetScaleModeIr::FreeX
                    }
                    Some("free-y") | Some("free_y") => {
                        layout.facet_scales = FacetScaleModeIr::FreeY
                    }
                    Some("free") => layout.facet_scales = FacetScaleModeIr::Free,
                    Some(other) => self.diag(Diagnostic::error(
                        codes::E1204,
                        format!("unknown facet scale mode `{other}`"),
                        key_span,
                    )),
                    None => {}
                },
                "facetLabel" => match self
                    .layout_string(&arg, "`facetLabel` expects a string literal")
                    .as_deref()
                {
                    Some("value") => layout.facet_label = FacetLabelModeIr::Value,
                    Some("name-value") | Some("name_value") => {
                        layout.facet_label = FacetLabelModeIr::NameValue
                    }
                    Some(other) => self.diag(Diagnostic::error(
                        codes::E1204,
                        format!("unknown facet label mode `{other}`"),
                        key_span,
                    )),
                    None => {}
                },
                "facetLabels" => {
                    if let Some(ValueExpr::Map(map)) = arg.value() {
                        if let Some(entries) = self.layout_label_map(&map) {
                            layout.facet_label_map = entries;
                        }
                    } else if let Some(value) = arg.value() {
                        self.diag(Diagnostic::error(
                            codes::E1204,
                            "`facetLabels` expects a string map",
                            node_span(value.syntax()),
                        ));
                    }
                }
                "panelSpacing" => {
                    if let Some(spacing) = self.layout_spacing(&arg) {
                        layout.panel_spacing = Some(spacing);
                    }
                }
                _ => self.diag(Diagnostic::error(
                    codes::E1003,
                    format!("unsupported Layout argument `{key}`"),
                    key_span,
                )),
            }
        }
    }

    fn layout_column(
        &mut self,
        arg: &Arg,
        table: &ActiveTable,
        message: &'static str,
    ) -> Option<ColumnRef> {
        match arg.value() {
            Some(ValueExpr::Algebra(AlgebraExpr::Name(name))) => {
                Some(self.resolve_column(&name, table))
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E1204,
                    message,
                    node_span(value.syntax()),
                ));
                None
            }
            None => None,
        }
    }

    fn layout_string(&mut self, arg: &Arg, message: &'static str) -> Option<String> {
        match arg.value() {
            Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                Some(string_value(&lit.text().unwrap_or_default()))
            }
            Some(value) => {
                self.diag(Diagnostic::error(
                    codes::E1204,
                    message,
                    node_span(value.syntax()),
                ));
                None
            }
            None => None,
        }
    }

    fn layout_label_map(&mut self, map: &MapValue) -> Option<Vec<(String, String)>> {
        let mut entries = Vec::new();
        for entry in map.entries() {
            let key = match entry.key() {
                Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                    string_value(&lit.text().unwrap_or_default())
                }
                other => {
                    let span = other
                        .map(|value| node_span(value.syntax()))
                        .unwrap_or_else(|| node_span(map.syntax()));
                    self.diag(Diagnostic::error(
                        codes::E1204,
                        "`facetLabels` map keys must be string literals",
                        span,
                    ));
                    return None;
                }
            };
            let value = match entry.value() {
                Some(ValueExpr::Literal(lit)) if lit.kind() == Some(LiteralKind::String) => {
                    string_value(&lit.text().unwrap_or_default())
                }
                other => {
                    let span = other
                        .map(|value| node_span(value.syntax()))
                        .unwrap_or_else(|| node_span(map.syntax()));
                    self.diag(Diagnostic::error(
                        codes::E1204,
                        "`facetLabels` map values must be string literals",
                        span,
                    ));
                    return None;
                }
            };
            entries.push((key, value));
        }
        Some(entries)
    }

    fn layout_spacing(&mut self, arg: &Arg) -> Option<PanelSpacingIr> {
        let value = arg.value()?;
        match value {
            ValueExpr::Literal(lit) if lit.kind() == Some(LiteralKind::Number) => {
                let n = lit.text().and_then(|t| t.parse::<f64>().ok())?;
                if n.is_finite() && n >= 0.0 {
                    Some(PanelSpacingIr { x: n, y: n })
                } else {
                    self.diag(Diagnostic::error(
                        codes::E1204,
                        "`panelSpacing` expects a non-negative number or [x, y]",
                        node_span(lit.syntax()),
                    ));
                    None
                }
            }
            ValueExpr::Array(array) => {
                let values = array.values();
                if values.len() != 2 {
                    self.diag(Diagnostic::error(
                        codes::E1204,
                        "`panelSpacing` expects a non-negative number or [x, y]",
                        node_span(array.syntax()),
                    ));
                    return None;
                }
                let mut out = [0.0, 0.0];
                for (index, item) in values.iter().enumerate() {
                    let ValueExpr::Literal(lit) = item else {
                        self.diag(Diagnostic::error(
                            codes::E1204,
                            "`panelSpacing` expects numeric array entries",
                            node_span(item.syntax()),
                        ));
                        return None;
                    };
                    let Some(n) = lit.text().and_then(|t| t.parse::<f64>().ok()) else {
                        self.diag(Diagnostic::error(
                            codes::E1204,
                            "`panelSpacing` expects numeric array entries",
                            node_span(lit.syntax()),
                        ));
                        return None;
                    };
                    if !n.is_finite() || n < 0.0 {
                        self.diag(Diagnostic::error(
                            codes::E1204,
                            "`panelSpacing` expects non-negative numbers",
                            node_span(lit.syntax()),
                        ));
                        return None;
                    }
                    out[index] = n;
                }
                Some(PanelSpacingIr {
                    x: out[0],
                    y: out[1],
                })
            }
            other => {
                self.diag(Diagnostic::error(
                    codes::E1204,
                    "`panelSpacing` expects a non-negative number or [x, y]",
                    node_span(other.syntax()),
                ));
                None
            }
        }
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
