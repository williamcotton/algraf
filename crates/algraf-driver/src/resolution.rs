use std::path::{Path, PathBuf};

use algraf_core::Span;
use algraf_data::Format;
use algraf_syntax::ast::ChartBlock;
use algraf_syntax::{
    chart_data_source, chart_table_sources, document_data_source, SourceExpr, SourceFormat,
    SyntaxNode,
};

use crate::DriverError;

/// Where the Algraf source text came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceInput {
    Stdin,
    Path(PathBuf),
}

impl SourceInput {
    /// A human-facing label for diagnostics.
    pub fn label(&self) -> String {
        match self {
            SourceInput::Stdin => "<stdin>".to_string(),
            SourceInput::Path(path) => path.display().to_string(),
        }
    }

    pub fn is_stdin(&self) -> bool {
        matches!(self, SourceInput::Stdin)
    }
}

/// A resolved source path plus the loader format policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSource {
    pub path: PathBuf,
    /// `None` means select format by extension.
    pub format: Option<Format>,
    /// SQL query for `Sqlite(...)` sources.
    pub query: Option<String>,
    pub span: Option<Span>,
}

/// A resolved chart-scoped named table source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTableSource {
    pub name: String,
    pub path: PathBuf,
    /// `None` means select format by extension.
    pub format: Option<Format>,
    /// SQL query for `Sqlite(...)` sources.
    pub query: Option<String>,
    pub span: Option<Span>,
}

/// Role of a resolved data dependency for one chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataDependencyKind {
    Primary,
    Table { name: String },
}

/// One resolved path dependency for a chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDependency {
    pub kind: DataDependencyKind,
    pub path: PathBuf,
    /// `None` means select format by extension.
    pub format: Option<Format>,
    /// SQL query for `Sqlite(...)` sources.
    pub query: Option<String>,
}

/// A data location after source-relative path resolution and `--data` override
/// handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataLocation {
    Path {
        path: PathBuf,
        /// `None` means select format by extension.
        format: Option<Format>,
    },
    Sqlite {
        path: PathBuf,
        query: String,
    },
    Stdin,
}

/// Internal driver context shared by resolution, loading, and preparation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DriverEnv<'a> {
    pub(crate) source_input: &'a SourceInput,
    pub(crate) base_dir: Option<&'a Path>,
    pub(crate) data_override: Option<&'a str>,
    pub(crate) multi_chart: bool,
}

impl<'a> DriverEnv<'a> {
    pub(crate) fn new(
        source_input: &'a SourceInput,
        base_dir: Option<&'a Path>,
        data_override: Option<&'a str>,
        multi_chart: bool,
    ) -> DriverEnv<'a> {
        DriverEnv {
            source_input,
            base_dir,
            data_override,
            multi_chart,
        }
    }

    pub(crate) fn resolver(self) -> SourceResolver<'a> {
        SourceResolver { env: self }
    }
}

/// Private resolver that owns path precedence and source-expression handling.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceResolver<'a> {
    env: DriverEnv<'a>,
}

impl SourceResolver<'_> {
    pub(crate) fn source_base_dir(&self) -> PathBuf {
        self.env
            .base_dir
            .map(PathBuf::from)
            .or_else(|| match self.env.source_input {
                SourceInput::Path(path) => path.parent().map(PathBuf::from),
                SourceInput::Stdin => Some(PathBuf::from(".")),
            })
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub(crate) fn resolve_path(&self, path: &str) -> PathBuf {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            path
        } else {
            self.source_base_dir().join(path)
        }
    }

    pub(crate) fn resolve_source_expr_path(
        &self,
        source_expr: &SourceExpr,
    ) -> Option<ResolvedSource> {
        match source_expr {
            SourceExpr::Path { path, format, span } => Some(ResolvedSource {
                path: self.resolve_path(path),
                format: data_format(*format),
                query: None,
                span: Some(*span),
            }),
            SourceExpr::Sqlite { path, query, span } => Some(ResolvedSource {
                path: self.resolve_path(path),
                format: None,
                query: Some(query.clone()),
                span: Some(*span),
            }),
            _ => None,
        }
    }

    pub(crate) fn resolve_document_data_path(&self, root: &SyntaxNode) -> Option<ResolvedSource> {
        self.resolve_source_expr_path(&document_data_source(root))
    }

    pub(crate) fn resolve_chart_data_path(&self, chart: &ChartBlock) -> Option<ResolvedSource> {
        self.resolve_source_expr_path(&chart_data_source(chart))
    }

    pub(crate) fn resolve_named_table_sources(
        &self,
        chart: &ChartBlock,
    ) -> Vec<ResolvedTableSource> {
        chart_table_sources(chart)
            .into_iter()
            .filter_map(|(name, source_expr)| {
                self.resolve_source_expr_path(&source_expr)
                    .map(|resolved| ResolvedTableSource {
                        name,
                        path: resolved.path,
                        format: resolved.format,
                        query: resolved.query,
                        span: resolved.span,
                    })
            })
            .collect()
    }

    pub(crate) fn data_location(
        &self,
        source_expr: &SourceExpr,
    ) -> Result<DataLocation, DriverError> {
        if let Some(data) = self.env.data_override {
            if data == "-" {
                if self.env.source_input.is_stdin() {
                    return Err(DriverError::Usage(
                        "cannot read both source and CSV data from stdin".to_string(),
                    ));
                }
                return Ok(DataLocation::Stdin);
            }
            return Ok(DataLocation::Path {
                path: PathBuf::from(data),
                format: None,
            });
        }

        match source_expr {
            SourceExpr::Stdin { .. } => {
                if self.env.source_input.is_stdin() {
                    return Err(DriverError::Usage(
                        "Chart(data: stdin) but source was also read from stdin; use --data"
                            .to_string(),
                    ));
                }
                Ok(DataLocation::Stdin)
            }
            SourceExpr::Path { .. } => {
                let resolved = self
                    .resolve_source_expr_path(source_expr)
                    .expect("path source should resolve");
                Ok(DataLocation::Path {
                    path: resolved.path,
                    format: resolved.format,
                })
            }
            SourceExpr::Sqlite { .. } => {
                let resolved = self
                    .resolve_source_expr_path(source_expr)
                    .expect("sqlite source should resolve");
                Ok(DataLocation::Sqlite {
                    path: resolved.path,
                    query: resolved.query.expect("sqlite source should carry query"),
                })
            }
            SourceExpr::Missing | SourceExpr::Invalid { .. } => Err(DriverError::Usage(
                "chart has no data source; add Chart(data: \"file.csv\")".to_string(),
            )),
        }
    }
}

/// Resolve the base directory for relative data paths.
pub fn source_base_dir(source: &SourceInput, base_dir: Option<&Path>) -> PathBuf {
    DriverEnv::new(source, base_dir, None, false)
        .resolver()
        .source_base_dir()
}

/// Resolve a path string using the source path or `--base-dir`.
pub fn resolve_path(path: &str, source: &SourceInput, base_dir: Option<&Path>) -> PathBuf {
    DriverEnv::new(source, base_dir, None, false)
        .resolver()
        .resolve_path(path)
}

/// Convert a syntax source format into a runtime data loader format.
pub fn source_format_to_data(format: SourceFormat) -> Format {
    match format {
        SourceFormat::GeoJson => Format::GeoJson,
        SourceFormat::Shapefile => Format::Shapefile,
    }
}

fn data_format(format: Option<SourceFormat>) -> Option<Format> {
    format.map(source_format_to_data)
}

/// Resolve a path source expression. Non-path expressions return `None`.
pub fn resolve_source_expr_path(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
) -> Option<ResolvedSource> {
    DriverEnv::new(source, base_dir, None, false)
        .resolver()
        .resolve_source_expr_path(source_expr)
}

/// Resolve the document-level primary data path, without applying a data override.
pub fn resolve_document_data_path(
    root: &SyntaxNode,
    source: &SourceInput,
    base_dir: Option<&Path>,
) -> Option<ResolvedSource> {
    DriverEnv::new(source, base_dir, None, false)
        .resolver()
        .resolve_document_data_path(root)
}

/// Resolve one chart's primary data path, without applying a data override.
pub fn resolve_chart_data_path(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
) -> Option<ResolvedSource> {
    DriverEnv::new(source, base_dir, None, false)
        .resolver()
        .resolve_chart_data_path(chart)
}

/// Resolve every valid named table path in a chart.
pub fn resolve_named_table_sources(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
) -> Vec<ResolvedTableSource> {
    DriverEnv::new(source, base_dir, None, false)
        .resolver()
        .resolve_named_table_sources(chart)
}

/// Apply `--data` override and source-relative path rules.
pub fn data_location(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
) -> Result<DataLocation, DriverError> {
    DriverEnv::new(source, base_dir, data_override, false)
        .resolver()
        .data_location(source_expr)
}

/// A resolved, load-free plan of one chart's data dependencies (spec §10.9).
///
/// Building a plan performs source-expression extraction, `--data` override
/// handling, and source-relative path resolution, but it reads no data bytes.
/// Loading and schema resolution then execute *from* the plan, so callers can
/// inspect what a chart will touch — primary location, named tables, explicit
/// formats, and source spans — before any expensive I/O starts.
#[derive(Debug, Clone)]
pub struct ChartDataPlan {
    /// The chart's primary data source expression (carries its span).
    pub source: SourceExpr,
    /// The resolved primary location, or `None` when the chart declares no
    /// usable primary source.
    pub primary: Option<DataLocation>,
    /// Resolved chart-scoped named table sources, in declaration order.
    pub named_tables: Vec<ResolvedTableSource>,
}

impl ChartDataPlan {
    /// The span of the primary source expression, where the document records one.
    pub fn primary_span(&self) -> Option<Span> {
        self.source.span()
    }

    /// Every path-backed data dependency: primary first, then named tables in
    /// declaration order. Stdin primaries contribute no path dependency.
    pub fn data_dependencies(&self) -> Vec<DataDependency> {
        let primary = self.primary.iter().filter_map(|location| match location {
            DataLocation::Path { path, format } => Some(DataDependency {
                kind: DataDependencyKind::Primary,
                path: path.clone(),
                format: *format,
                query: None,
            }),
            DataLocation::Sqlite { path, query } => Some(DataDependency {
                kind: DataDependencyKind::Primary,
                path: path.clone(),
                format: None,
                query: Some(query.clone()),
            }),
            DataLocation::Stdin => None,
        });
        let tables = self.named_tables.iter().map(|table| DataDependency {
            kind: DataDependencyKind::Table {
                name: table.name.clone(),
            },
            path: table.path.clone(),
            format: table.format,
            query: table.query.clone(),
        });
        primary.chain(tables).collect()
    }
}

/// Build a [`ChartDataPlan`] without loading any data bytes.
pub fn plan_chart_data(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
    multi_chart: bool,
) -> Result<ChartDataPlan, DriverError> {
    resolve_chart_inputs(
        chart,
        DriverEnv::new(source, base_dir, data_override, multi_chart),
    )
}

/// Resolve all path-backed data dependencies for one chart.
pub fn data_dependencies(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
) -> Result<Vec<DataDependency>, DriverError> {
    let env = DriverEnv::new(source, base_dir, data_override, false);
    Ok(resolve_chart_inputs(chart, env)?.data_dependencies())
}

pub(crate) fn resolve_chart_inputs(
    chart: &ChartBlock,
    env: DriverEnv<'_>,
) -> Result<ChartDataPlan, DriverError> {
    let source = chart_data_source(chart);
    if env.multi_chart && (source.is_stdin() || env.data_override == Some("-")) {
        return Err(DriverError::Usage(
            "stdin data cannot be shared across charts; give each chart a file path".to_string(),
        ));
    }

    let resolver = env.resolver();
    let primary = if source.is_missing() || matches!(source, SourceExpr::Invalid { .. }) {
        None
    } else {
        Some(resolver.data_location(&source)?)
    };

    Ok(ChartDataPlan {
        source,
        primary,
        named_tables: resolver.resolve_named_table_sources(chart),
    })
}
