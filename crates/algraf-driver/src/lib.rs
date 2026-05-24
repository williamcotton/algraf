//! Shared parsing, source resolution, data loading, and analysis driver.
//!
//! The driver is intentionally non-UI: it does not parse command-line flags,
//! print diagnostics, choose output filenames, rasterize PNGs, or speak LSP.

use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use algraf_core::Span;
use algraf_data::{
    read_csv, read_path, read_path_as, read_schema_path, read_schema_path_as, ColumnDef, DataError,
    DataFrame, DataWarning, Format, LoadResult, Table,
};
use algraf_semantics::{analyze_chart_with_tables, Analysis};
use algraf_syntax::ast::{ChartBlock, Root};
use algraf_syntax::{
    chart_data_source, chart_table_sources, document_data_source, parse, Parse, SourceExpr,
    SourceFormat, SyntaxNode,
};

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
    pub span: Option<Span>,
}

/// A resolved chart-scoped named table source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTableSource {
    pub name: String,
    pub path: PathBuf,
    /// `None` means select format by extension.
    pub format: Option<Format>,
    pub span: Option<Span>,
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
    Stdin,
}

/// One loaded chart-scoped named table.
#[derive(Debug, Clone)]
pub struct NamedTable {
    pub name: String,
    pub path: PathBuf,
    pub frame: DataFrame,
    pub warnings: Vec<DataWarning>,
}

/// One loaded chart-scoped named table schema.
#[derive(Debug, Clone)]
pub struct NamedTableSchema {
    pub name: String,
    pub path: PathBuf,
    pub schema: Vec<ColumnDef>,
}

/// Data loading context used to preserve caller-facing error messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadContext {
    Primary,
    Table { name: String },
}

/// Structured driver errors.
#[derive(Debug)]
pub enum DriverError {
    Usage(String),
    Data {
        context: LoadContext,
        path: PathBuf,
        source: DataError,
    },
    StdinRead(String),
    StdinParse(String),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverError::Usage(message) => f.write_str(message),
            DriverError::Data {
                context,
                path,
                source,
            } => match context {
                LoadContext::Primary => {
                    write!(f, "failed to load data {}: {source}", path.display())
                }
                LoadContext::Table { name } => {
                    write!(
                        f,
                        "failed to load Table `{name}` data {}: {source}",
                        path.display()
                    )
                }
            },
            DriverError::StdinRead(message) | DriverError::StdinParse(message) => {
                f.write_str(message)
            }
        }
    }
}

impl std::error::Error for DriverError {}

/// Prepared chart inputs after loading and semantic analysis.
#[derive(Debug)]
pub struct PreparedChart {
    pub source: SourceExpr,
    pub primary: Option<LoadResult>,
    pub named_tables: Vec<NamedTable>,
    pub analysis: Analysis,
}

impl PreparedChart {
    /// Named-table schemas keyed by declaration name.
    pub fn table_schemas(&self) -> HashMap<String, Vec<ColumnDef>> {
        self.named_tables
            .iter()
            .map(|table| (table.name.clone(), table.frame.schema().to_vec()))
            .collect()
    }

    /// Named-table frames keyed by declaration name.
    pub fn into_named_frames(self) -> HashMap<String, DataFrame> {
        self.named_tables
            .into_iter()
            .map(|table| (table.name, table.frame))
            .collect()
    }
}

/// Options for loading and analyzing one chart.
#[derive(Debug, Clone, Copy)]
pub struct PrepareOptions<'a> {
    pub source_input: &'a SourceInput,
    pub base_dir: Option<&'a Path>,
    pub data_override: Option<&'a str>,
    pub multi_chart: bool,
}

/// Parse source text.
pub fn parse_source(source: &str) -> Parse {
    parse(source)
}

/// Extract the first chart's data source.
pub fn extract_data_source(root: &SyntaxNode) -> SourceExpr {
    document_data_source(root)
}

/// Extract one chart's data source.
pub fn extract_chart_data_source(chart: &ChartBlock) -> SourceExpr {
    chart_data_source(chart)
}

/// Extract table source declarations from one chart.
pub fn extract_chart_tables(chart: &ChartBlock) -> Vec<(String, SourceExpr)> {
    chart_table_sources(chart)
}

/// Every top-level chart block in a parsed document.
pub fn document_charts(root: &SyntaxNode) -> Vec<ChartBlock> {
    Root::cast(root.clone())
        .map(|root| root.charts())
        .unwrap_or_default()
}

/// Resolve the base directory for relative data paths.
pub fn source_base_dir(source: &SourceInput, base_dir: Option<&Path>) -> PathBuf {
    base_dir
        .map(PathBuf::from)
        .or_else(|| match source {
            SourceInput::Path(path) => path.parent().map(PathBuf::from),
            SourceInput::Stdin => Some(PathBuf::from(".")),
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Resolve a path string using the source path or `--base-dir`.
pub fn resolve_path(path: &str, source: &SourceInput, base_dir: Option<&Path>) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        source_base_dir(source, base_dir).join(path)
    }
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
    match source_expr {
        SourceExpr::Path { path, format, span } => Some(ResolvedSource {
            path: resolve_path(path, source, base_dir),
            format: data_format(*format),
            span: Some(*span),
        }),
        _ => None,
    }
}

/// Resolve one chart's primary data path, without applying a data override.
pub fn resolve_chart_data_path(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
) -> Option<ResolvedSource> {
    resolve_source_expr_path(&chart_data_source(chart), source, base_dir)
}

/// Resolve every valid named table path in a chart.
pub fn resolve_named_table_sources(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
) -> Vec<ResolvedTableSource> {
    chart_table_sources(chart)
        .into_iter()
        .filter_map(|(name, source_expr)| {
            resolve_source_expr_path(&source_expr, source, base_dir).map(|resolved| {
                ResolvedTableSource {
                    name,
                    path: resolved.path,
                    format: resolved.format,
                    span: resolved.span,
                }
            })
        })
        .collect()
}

/// Apply `--data` override and source-relative path rules.
pub fn data_location(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
) -> Result<DataLocation, DriverError> {
    if let Some(data) = data_override {
        if data == "-" {
            if source.is_stdin() {
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
            if source.is_stdin() {
                return Err(DriverError::Usage(
                    "Chart(data: stdin) but source was also read from stdin; use --data"
                        .to_string(),
                ));
            }
            Ok(DataLocation::Stdin)
        }
        SourceExpr::Path { .. } => {
            let resolved = resolve_source_expr_path(source_expr, source, base_dir)
                .expect("path source should resolve");
            Ok(DataLocation::Path {
                path: resolved.path,
                format: resolved.format,
            })
        }
        SourceExpr::Missing | SourceExpr::Invalid { .. } => Err(DriverError::Usage(
            "chart has no data source; add Chart(data: \"file.csv\")".to_string(),
        )),
    }
}

/// Load a full data source for a chart.
pub fn load_data(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
) -> Result<LoadResult, DriverError> {
    match data_location(source_expr, source, base_dir, data_override)? {
        DataLocation::Path { path, format } => load_path(&path, format, LoadContext::Primary),
        DataLocation::Stdin => read_stdin_csv(),
    }
}

/// Load a full data source from a path.
pub fn load_path(
    path: &Path,
    format: Option<Format>,
    context: LoadContext,
) -> Result<LoadResult, DriverError> {
    let loaded = match format {
        Some(format) => read_path_as(path, format),
        None => read_path(path),
    };
    loaded.map_err(|source| DriverError::Data {
        context,
        path: path.to_path_buf(),
        source,
    })
}

/// Load only a data schema, optionally sampling rows for delimited formats.
pub fn load_schema(
    source_expr: &SourceExpr,
    source: &SourceInput,
    base_dir: Option<&Path>,
    data_override: Option<&str>,
    sample_size: Option<usize>,
) -> Result<Vec<ColumnDef>, DriverError> {
    let Some(sample_size) = sample_size else {
        return Ok(load_data(source_expr, source, base_dir, data_override)?
            .frame
            .schema()
            .to_vec());
    };

    match data_location(source_expr, source, base_dir, data_override)? {
        DataLocation::Path { path, format } => {
            load_schema_path(&path, format, sample_size, LoadContext::Primary)
        }
        DataLocation::Stdin => {
            let mut bytes = Vec::new();
            std::io::stdin().read_to_end(&mut bytes).map_err(|e| {
                DriverError::StdinRead(format!("failed to read CSV from stdin: {e}"))
            })?;
            algraf_data::read_csv_schema(bytes.as_slice(), sample_size)
                .map_err(|e| DriverError::StdinParse(format!("failed to parse stdin CSV: {e}")))
        }
    }
}

/// Load only a schema from a path.
pub fn load_schema_path(
    path: &Path,
    format: Option<Format>,
    sample_size: usize,
    context: LoadContext,
) -> Result<Vec<ColumnDef>, DriverError> {
    let loaded = match format {
        Some(format) => read_schema_path_as(path, format, sample_size),
        None => read_schema_path(path, sample_size),
    };
    loaded.map_err(|source| DriverError::Data {
        context,
        path: path.to_path_buf(),
        source,
    })
}

/// Load every valid named table in a chart.
pub fn load_named_tables(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
) -> Result<Vec<NamedTable>, DriverError> {
    let mut out = Vec::new();
    for resolved in resolve_named_table_sources(chart, source, base_dir) {
        let loaded = load_path(
            &resolved.path,
            resolved.format,
            LoadContext::Table {
                name: resolved.name.clone(),
            },
        )?;
        out.push(NamedTable {
            name: resolved.name,
            path: resolved.path,
            frame: loaded.frame,
            warnings: loaded.warnings,
        });
    }
    Ok(out)
}

/// Load every valid named table schema in a chart.
pub fn load_named_table_schemas(
    chart: &ChartBlock,
    source: &SourceInput,
    base_dir: Option<&Path>,
    sample_size: usize,
) -> Result<Vec<NamedTableSchema>, DriverError> {
    let mut out = Vec::new();
    for resolved in resolve_named_table_sources(chart, source, base_dir) {
        let schema = load_schema_path(
            &resolved.path,
            resolved.format,
            sample_size,
            LoadContext::Table {
                name: resolved.name.clone(),
            },
        )?;
        out.push(NamedTableSchema {
            name: resolved.name,
            path: resolved.path,
            schema,
        });
    }
    Ok(out)
}

/// Load data and analyze a chart.
pub fn prepare_chart(
    chart: &ChartBlock,
    options: PrepareOptions<'_>,
) -> Result<PreparedChart, DriverError> {
    let source_expr = chart_data_source(chart);
    if options.multi_chart && (source_expr.is_stdin() || options.data_override == Some("-")) {
        return Err(DriverError::Usage(
            "stdin data cannot be shared across charts; give each chart a file path".to_string(),
        ));
    }

    let primary = if source_expr.is_missing() || matches!(source_expr, SourceExpr::Invalid { .. }) {
        None
    } else {
        Some(load_data(
            &source_expr,
            options.source_input,
            options.base_dir,
            options.data_override,
        )?)
    };
    let schema = primary
        .as_ref()
        .map(|loaded| loaded.frame.schema())
        .unwrap_or(&[] as &[ColumnDef]);

    let named_tables = load_named_tables(chart, options.source_input, options.base_dir)?;
    let table_schemas: HashMap<String, Vec<ColumnDef>> = named_tables
        .iter()
        .map(|table| (table.name.clone(), table.frame.schema().to_vec()))
        .collect();
    let analysis = analyze_chart_with_tables(chart, schema, &table_schemas);

    Ok(PreparedChart {
        source: source_expr,
        primary,
        named_tables,
        analysis,
    })
}

fn read_stdin_csv() -> Result<LoadResult, DriverError> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|e| DriverError::StdinRead(format!("failed to read CSV from stdin: {e}")))?;
    read_csv(bytes.as_slice())
        .map_err(|e| DriverError::StdinParse(format!("failed to parse stdin CSV: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(test: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "algraf-driver-{test}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn parse_chart(source: &str) -> ChartBlock {
        Root::cast(parse(source).syntax())
            .and_then(|root| root.chart())
            .unwrap()
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../algraf-data/tests/fixtures")
            .join(name)
    }

    #[test]
    fn extracts_primary_and_named_source_expressions() {
        let chart = parse_chart(
            r#"Chart(data: GeoJson("map.geo")) { Table counties = Shapefile("tiny.shp") }"#,
        );
        assert!(matches!(
            extract_chart_data_source(&chart),
            SourceExpr::Path {
                path,
                format: Some(SourceFormat::GeoJson),
                ..
            } if path == "map.geo"
        ));
        let tables = extract_chart_tables(&chart);
        assert!(matches!(
            &tables[0].1,
            SourceExpr::Path {
                path,
                format: Some(SourceFormat::Shapefile),
                ..
            } if path == "tiny.shp"
        ));
    }

    #[test]
    fn resolves_relative_paths_against_source_file() {
        let dir = temp_dir("resolve");
        let source_path = dir.join("nested/chart.ag");
        fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        let source = SourceInput::Path(source_path);
        let resolved = resolve_path("data.csv", &source, None);
        assert_eq!(resolved, dir.join("nested/data.csv"));
    }

    #[test]
    fn loads_supported_path_formats() {
        let dir = temp_dir("formats");
        fs::write(dir.join("data.csv"), "x,y\n1,2\n").unwrap();
        fs::write(dir.join("data.tsv"), "x\ty\n1\t2\n").unwrap();
        fs::write(dir.join("data.json"), r#"[{"x":1,"y":2}]"#).unwrap();
        fs::write(dir.join("data.ndjson"), "{\"x\":1,\"y\":2}\n").unwrap();

        let source = SourceInput::Path(dir.join("chart.ag"));
        for path in ["data.csv", "data.tsv", "data.json", "data.ndjson"] {
            let chart = parse_chart(&format!(
                r#"Chart(data: "{path}") {{ Space(x * y) {{ Point() }} }}"#
            ));
            let prepared = prepare_chart(
                &chart,
                PrepareOptions {
                    source_input: &source,
                    base_dir: None,
                    data_override: None,
                    multi_chart: false,
                },
            )
            .unwrap();
            assert!(
                prepared.primary.unwrap().frame.column("x").is_some(),
                "{path}"
            );
        }
    }

    #[test]
    fn loads_geojson_and_shapefile_constructors() {
        let source = SourceInput::Path(PathBuf::from("chart.ag"));
        for (constructor, path) in [
            ("GeoJson", fixture("tiny.geojson")),
            ("Shapefile", fixture("tiny.shp")),
        ] {
            let chart = parse_chart(&format!(
                r#"Chart(data: {constructor}("{}")) {{ Space(geom) {{ Geo() }} }}"#,
                path.display()
            ));
            let prepared = prepare_chart(
                &chart,
                PrepareOptions {
                    source_input: &source,
                    base_dir: None,
                    data_override: None,
                    multi_chart: false,
                },
            )
            .unwrap();
            assert!(prepared.primary.unwrap().frame.column("geom").is_some());
        }
    }

    #[test]
    fn loads_named_table_frames_and_schemas() {
        let dir = temp_dir("named");
        fs::write(dir.join("primary.csv"), "x,y\n1,2\n").unwrap();
        fs::write(dir.join("cities.csv"), "long,lat,city\n1,2,A\n").unwrap();
        let source = SourceInput::Path(dir.join("chart.ag"));
        let chart = parse_chart(
            r#"Chart(data: "primary.csv") {
                Table cities = "cities.csv"
                Space(long * lat, data: cities) { Point() }
            }"#,
        );

        let tables = load_named_tables(&chart, &source, None).unwrap();
        assert_eq!(tables[0].name, "cities");
        let schemas = load_named_table_schemas(&chart, &source, None, 10).unwrap();
        assert_eq!(schemas[0].schema[0].name, "long");
    }

    #[test]
    fn prepares_each_file_backed_chart_in_multi_chart_document() {
        let dir = temp_dir("multi-chart");
        fs::write(dir.join("a.csv"), "x,y\n1,2\n").unwrap();
        fs::write(dir.join("b.csv"), "x,y\n3,4\n").unwrap();
        let root = parse(
            r#"Chart(data: "a.csv") { Space(x * y) { Point() } }
Chart(data: "b.csv") { Space(x * y) { Line() } }"#,
        )
        .syntax();
        let charts = document_charts(&root);
        assert_eq!(charts.len(), 2);
        let source = SourceInput::Path(dir.join("chart.ag"));

        for chart in &charts {
            let prepared = prepare_chart(
                chart,
                PrepareOptions {
                    source_input: &source,
                    base_dir: None,
                    data_override: None,
                    multi_chart: true,
                },
            )
            .unwrap();
            assert!(prepared.primary.is_some());
            assert!(prepared.analysis.ir.is_some());
        }
    }

    #[test]
    fn named_geospatial_table_uses_constructor_format() {
        let dir = temp_dir("named-geo");
        fs::write(dir.join("primary.csv"), "x,y\n1,2\n").unwrap();
        let source = SourceInput::Path(dir.join("chart.ag"));
        let geojson = fixture("tiny.geojson");
        let chart = parse_chart(&format!(
            r#"Chart(data: "primary.csv") {{
                Table shapes = GeoJson("{}")
                Space(geom, data: shapes) {{ Geo() }}
            }}"#,
            geojson.display()
        ));

        let tables = load_named_tables(&chart, &source, None).unwrap();
        assert!(tables[0].frame.column("geom").is_some());
    }

    #[test]
    fn reports_missing_and_malformed_data_errors() {
        let dir = temp_dir("errors");
        fs::write(dir.join("bad.csv"), "x,y\n\"unterminated,2\n").unwrap();
        let source = SourceInput::Path(dir.join("chart.ag"));

        let missing = load_data(
            &SourceExpr::Path {
                path: "missing.csv".to_string(),
                format: None,
                span: Span::new(0, 0),
            },
            &source,
            None,
            None,
        )
        .unwrap_err();
        assert!(matches!(missing, DriverError::Data { .. }));

        let malformed = load_data(
            &SourceExpr::Path {
                path: "bad.csv".to_string(),
                format: None,
                span: Span::new(0, 0),
            },
            &source,
            None,
            None,
        )
        .unwrap_err();
        assert!(matches!(malformed, DriverError::Data { .. }));
    }

    #[test]
    fn multi_chart_stdin_data_is_rejected() {
        let chart = parse_chart(r#"Chart(data: stdin) { Space(x * y) { Point() } }"#);
        let err = prepare_chart(
            &chart,
            PrepareOptions {
                source_input: &SourceInput::Path(PathBuf::from("chart.ag")),
                base_dir: None,
                data_override: None,
                multi_chart: true,
            },
        )
        .unwrap_err();
        assert!(matches!(err, DriverError::Usage(message) if message.contains("stdin data")));
    }
}
