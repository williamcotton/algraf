//! Shared parsing, source resolution, data loading, and analysis driver.
//!
//! The driver is intentionally non-UI: it does not parse command-line flags,
//! print diagnostics, choose output filenames, rasterize PNGs, or speak LSP.

mod cache;
mod error;
mod io;
mod loading;
mod prepare;
mod report;
mod resolution;
mod variables;

pub use cache::{
    fingerprint_path, resolve_schema_cached, resolve_sqlite_schema_cached,
    resolve_topojson_schema_cached, CachedSchema, DataSourceKey, InMemorySchemaCache,
    NoSchemaCache, SchemaCache, SourceFingerprint,
};
pub use error::{DriverError, LoadContext};
pub use io::{DriverIo, DriverPathMetadata, DriverShapefileBundle, OsDriverIo};
pub use loading::{
    load_data, load_data_with_io, load_named_table_schemas, load_named_table_schemas_with_io,
    load_named_tables, load_named_tables_with_io, load_path, load_path_with_io, load_schema,
    load_schema_path, load_schema_path_with_io, load_schema_with_io, NamedTable, NamedTableSchema,
};
pub use prepare::{
    prepare_chart, prepare_chart_partial, prepare_chart_partial_with_io, prepare_chart_with_io,
    PrepareOptions, PreparedChart, PreparedReport,
};
pub use report::{
    data_error_code_message, driver_error_code_message, driver_error_diagnostic, DataWarningEntry,
    PreparationReport, ReportPhase,
};
pub use resolution::{
    data_dependencies, data_location, plan_chart_data, resolve_chart_data_path,
    resolve_document_data_path, resolve_named_table_sources, resolve_path,
    resolve_source_expr_path, source_base_dir, source_format_to_data, ChartDataPlan,
    DataDependency, DataDependencyKind, DataLocation, ResolvedSource, ResolvedTableSource,
    SourceInput,
};
pub use variables::{expand_variables, parse_variable_assignments};

use algraf_syntax::ast::{ChartBlock, Root};
use algraf_syntax::{
    chart_data_source, chart_table_sources, document_data_source, parse, Parse, SourceExpr,
    SyntaxNode,
};

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

#[cfg(test)]
mod tests {
    use super::*;
    use algraf_core::{codes, Span};
    use algraf_data::{DataFrame, DataType, Format, Table};
    use algraf_syntax::SourceFormat;
    use std::collections::HashMap;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
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

    #[derive(Debug, Default)]
    struct MemoryIo {
        files: HashMap<PathBuf, Vec<u8>>,
        stdin: Vec<u8>,
    }

    impl MemoryIo {
        fn with_file(mut self, path: impl Into<PathBuf>, bytes: impl Into<Vec<u8>>) -> Self {
            self.files.insert(path.into(), bytes.into());
            self
        }

        fn with_stdin(mut self, bytes: impl Into<Vec<u8>>) -> Self {
            self.stdin = bytes.into();
            self
        }
    }

    impl DriverIo for MemoryIo {
        fn read_path(&self, path: &Path) -> io::Result<Vec<u8>> {
            self.files.get(path).cloned().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("missing {}", path.display()),
                )
            })
        }

        fn read_stdin(&self) -> io::Result<Vec<u8>> {
            Ok(self.stdin.clone())
        }

        fn metadata(&self, path: &Path) -> io::Result<DriverPathMetadata> {
            let bytes = self.files.get(path).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("missing {}", path.display()),
                )
            })?;
            Ok(DriverPathMetadata {
                len: bytes.len() as u64,
                modified: None,
            })
        }
    }

    #[derive(Debug)]
    struct ReaderOnlyStdinIo {
        stdin: Vec<u8>,
    }

    impl ReaderOnlyStdinIo {
        fn new(stdin: impl Into<Vec<u8>>) -> Self {
            Self {
                stdin: stdin.into(),
            }
        }
    }

    impl DriverIo for ReaderOnlyStdinIo {
        fn read_path(&self, _path: &Path) -> io::Result<Vec<u8>> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "path reads are not available in this test",
            ))
        }

        fn read_stdin(&self) -> io::Result<Vec<u8>> {
            panic!("caller stdin should be streamed through open_stdin")
        }

        fn open_stdin(&self) -> io::Result<Box<dyn io::Read + '_>> {
            Ok(Box::new(io::Cursor::new(self.stdin.clone())))
        }

        fn metadata(&self, _path: &Path) -> io::Result<DriverPathMetadata> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "metadata is not available in this test",
            ))
        }
    }

    /// Wraps a [`MemoryIo`] and counts `read_path` calls so cache tests can
    /// distinguish a cache hit (no read) from a reload (one read).
    #[derive(Debug)]
    struct CountingIo {
        inner: MemoryIo,
        reads: AtomicUsize,
    }

    impl CountingIo {
        fn new(inner: MemoryIo) -> CountingIo {
            CountingIo {
                inner,
                reads: AtomicUsize::new(0),
            }
        }

        fn reads(&self) -> usize {
            self.reads.load(Ordering::SeqCst)
        }
    }

    impl DriverIo for CountingIo {
        fn read_path(&self, path: &Path) -> io::Result<Vec<u8>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.inner.read_path(path)
        }

        fn read_stdin(&self) -> io::Result<Vec<u8>> {
            self.inner.read_stdin()
        }

        fn metadata(&self, path: &Path) -> io::Result<DriverPathMetadata> {
            self.inner.metadata(path)
        }
    }

    fn frame_signature(frame: &DataFrame) -> (usize, Vec<(String, DataType, bool)>) {
        (
            frame.row_count(),
            frame
                .schema()
                .iter()
                .map(|column| (column.name.clone(), column.dtype, column.nullable))
                .collect(),
        )
    }

    fn sorted_frame_signature(frame: &DataFrame) -> (usize, Vec<(String, DataType, bool)>) {
        let (rows, mut schema) = frame_signature(frame);
        schema.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        (rows, schema)
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
    fn resolves_absolute_paths_without_rebasing() {
        let dir = temp_dir("absolute");
        let absolute = dir.join("data.csv");
        let source = SourceInput::Path(dir.join("nested/chart.ag"));

        let resolved = resolve_path(absolute.to_str().unwrap(), &source, None);

        assert_eq!(resolved, absolute);
    }

    #[test]
    fn resolves_stdin_source_paths_against_current_directory() {
        let resolved = resolve_path("data.csv", &SourceInput::Stdin, None);

        assert_eq!(resolved, PathBuf::from(".").join("data.csv"));
    }

    #[test]
    fn explicit_base_dir_overrides_source_parent() {
        let dir = temp_dir("base-dir");
        let base = dir.join("data-root");
        let source = SourceInput::Path(dir.join("source-root/chart.ag"));

        let resolved = resolve_path("data.csv", &source, Some(&base));

        assert_eq!(resolved, base.join("data.csv"));
    }

    #[test]
    fn data_override_applies_to_primary_without_rebasing() {
        let dir = temp_dir("override");
        let source = SourceInput::Path(dir.join("nested/chart.ag"));
        let expr = SourceExpr::Path {
            path: "declared.csv".to_string(),
            format: None,
            span: Span::new(0, 0),
        };

        let location = data_location(&expr, &source, Some(&dir), Some("override.csv")).unwrap();

        assert_eq!(
            location,
            DataLocation::Path {
                path: PathBuf::from("override.csv"),
                format: None
            }
        );
    }

    #[test]
    fn chart_and_named_table_sources_share_base_resolution() {
        let dir = temp_dir("chart-table-resolution");
        let base = dir.join("base");
        let source = SourceInput::Path(dir.join("source/chart.ag"));
        let chart = parse_chart(
            r#"Chart(data: "primary.csv") {
                Table cities = GeoJson("cities.geojson")
                Space(geom, data: cities) { Geo() }
            }"#,
        );

        let primary = resolve_chart_data_path(&chart, &source, Some(&base)).unwrap();
        let tables = resolve_named_table_sources(&chart, &source, Some(&base));

        assert_eq!(primary.path, base.join("primary.csv"));
        assert_eq!(tables[0].path, base.join("cities.geojson"));
        assert_eq!(tables[0].format, Some(Format::GeoJson));
    }

    #[test]
    fn resolves_sqlite_sources_with_queries() {
        let dir = temp_dir("sqlite-resolution");
        let source = SourceInput::Path(dir.join("charts/chart.ag"));
        let chart = parse_chart(
            r#"Algraf(version: "0.21", features: ["sql"])
            Chart(data: Sqlite("sales.db", "SELECT region FROM sales ORDER BY region")) {
                Table totals = Sqlite("totals.db", "SELECT region, total FROM totals ORDER BY region")
                Space(region * total, data: totals) { Bar(stat: "identity") }
            }"#,
        );

        let primary = resolve_chart_data_path(&chart, &source, None).unwrap();
        let tables = resolve_named_table_sources(&chart, &source, None);

        assert_eq!(primary.path, dir.join("charts/sales.db"));
        assert_eq!(
            primary.query.as_deref(),
            Some("SELECT region FROM sales ORDER BY region")
        );
        assert_eq!(tables[0].path, dir.join("charts/totals.db"));
        assert_eq!(
            tables[0].query.as_deref(),
            Some("SELECT region, total FROM totals ORDER BY region")
        );
    }

    #[test]
    fn document_data_path_uses_same_resolver_as_chart_path() {
        let dir = temp_dir("document-resolution");
        let source = SourceInput::Path(dir.join("chart.ag"));
        let root = parse(r#"Chart(data: "primary.csv") { Space(x * y) { Point() } }"#).syntax();
        let chart = document_charts(&root).remove(0);

        let document = resolve_document_data_path(&root, &source, None).unwrap();
        let chart = resolve_chart_data_path(&chart, &source, None).unwrap();

        assert_eq!(document, chart);
    }

    #[test]
    fn data_dependency_inventory_reports_primary_then_named_paths() {
        let dir = temp_dir("dependencies");
        let base = dir.join("base");
        let source = SourceInput::Path(dir.join("source/chart.ag"));
        let chart = parse_chart(
            r#"Chart(data: "primary.csv") {
                Table cities = GeoJson("cities.geojson")
                Space(x * y) { Point() }
            }"#,
        );

        let deps = data_dependencies(&chart, &source, Some(&base), None).unwrap();

        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].kind, DataDependencyKind::Primary);
        assert_eq!(deps[0].path, base.join("primary.csv"));
        assert_eq!(
            deps[1].kind,
            DataDependencyKind::Table {
                name: "cities".to_string()
            }
        );
        assert_eq!(deps[1].path, base.join("cities.geojson"));
        assert_eq!(deps[1].format, Some(Format::GeoJson));
    }

    #[test]
    fn internal_driver_env_matches_public_resolution_wrappers() {
        let dir = temp_dir("driver-env");
        let base = dir.join("base");
        let source = SourceInput::Path(dir.join("source/chart.ag"));
        let chart = parse_chart(
            r#"Chart(data: "primary.csv") {
                Table cities = "cities.csv"
                Space(x * y) { Point() }
            }"#,
        );
        let env = crate::resolution::DriverEnv::new(&source, Some(&base), None, None, false);

        let internal_primary = env.resolver().resolve_chart_data_path(&chart).unwrap();
        let public_primary = resolve_chart_data_path(&chart, &source, Some(&base)).unwrap();
        let internal_tables = env.resolver().resolve_named_table_sources(&chart);
        let public_tables = resolve_named_table_sources(&chart, &source, Some(&base));

        assert_eq!(internal_primary, public_primary);
        assert_eq!(internal_tables, public_tables);
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
                    data_format_override: None,
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
    fn parse_declaration_applies_before_primary_schema_inference() {
        let dir = temp_dir("parse-policy-primary");
        fs::write(
            dir.join("events.csv"),
            "started,latency\n05/27/2026 2:30 PM,82\n05/27/2026 3:00 PM,91\n",
        )
        .unwrap();
        let chart = parse_chart(
            r#"Chart(data: "events.csv") {
                Parse(column: started, as: "datetime", format: "%m/%d/%Y %I:%M %p", timezone: "UTC")
                Guide(axis: x, timeFormat: "%b %-d %H:%M")
                Space(started * latency) { Line() Point() }
            }"#,
        );
        let source = SourceInput::Path(dir.join("chart.ag"));
        let prepared = prepare_chart(
            &chart,
            PrepareOptions {
                source_input: &source,
                base_dir: None,
                data_override: None,
                data_format_override: None,
                multi_chart: false,
            },
        )
        .unwrap();
        let frame = &prepared.primary.unwrap().frame;
        assert_eq!(
            frame.column_def("started").unwrap().dtype,
            DataType::Temporal
        );
        assert!(
            prepared.analysis.diagnostics.is_empty(),
            "{:?}",
            prepared.analysis.diagnostics
        );
    }

    #[test]
    fn in_memory_io_matches_os_for_single_file_primary_sources() {
        let dir = temp_dir("memory-primary");
        let cases = [
            (
                "data.csv",
                "x,y\n1,2\n",
                r#"Chart(data: "data.csv") { Space(x * y) { Point() } }"#,
            ),
            (
                "data.tsv",
                "x\ty\n1\t2\n",
                r#"Chart(data: "data.tsv") { Space(x * y) { Point() } }"#,
            ),
            (
                "data.json",
                r#"[{"x":1,"y":2}]"#,
                r#"Chart(data: "data.json") { Space(x * y) { Point() } }"#,
            ),
            (
                "data.ndjson",
                "{\"x\":1,\"y\":2}\n",
                r#"Chart(data: "data.ndjson") { Space(x * y) { Point() } }"#,
            ),
            (
                "map.data",
                r#"{
                  "type":"FeatureCollection",
                  "features":[
                    {"type":"Feature","properties":{"name":"a"},
                     "geometry":{"type":"Point","coordinates":[1,2]}}
                  ]
                }"#,
                r#"Chart(data: GeoJson("map.data")) { Space(geom) { Geo() } }"#,
            ),
        ];

        let source = SourceInput::Path(dir.join("chart.ag"));
        let mut memory = MemoryIo::default();
        for (path, bytes, _) in cases {
            fs::write(dir.join(path), bytes).unwrap();
            memory = memory.with_file(dir.join(path), bytes.as_bytes());
        }

        for (_, _, source_text) in cases {
            let chart = parse_chart(source_text);
            let os = prepare_chart(
                &chart,
                PrepareOptions {
                    source_input: &source,
                    base_dir: None,
                    data_override: None,
                    data_format_override: None,
                    multi_chart: false,
                },
            )
            .unwrap()
            .primary
            .unwrap();
            let injected = prepare_chart_with_io(
                &chart,
                PrepareOptions {
                    source_input: &source,
                    base_dir: None,
                    data_override: None,
                    data_format_override: None,
                    multi_chart: false,
                },
                &memory,
            )
            .unwrap()
            .primary
            .unwrap();

            assert_eq!(frame_signature(&injected.frame), frame_signature(&os.frame));
            assert_eq!(injected.warnings, os.warnings);
        }
    }

    #[test]
    fn in_memory_io_loads_named_tables_schemas_stdin_and_data_override() {
        let root = PathBuf::from("/mem");
        let source = SourceInput::Path(root.join("chart.ag"));
        let memory = MemoryIo::default()
            .with_file(root.join("primary.csv"), b"x,y\n1,2\n".as_slice())
            .with_file(
                root.join("cities.tsv"),
                b"long\tlat\tcity\n1\t2\tA\n".as_slice(),
            )
            .with_file(
                root.join("override.json"),
                br#"[{"x":9,"y":10}]"#.as_slice(),
            )
            .with_stdin(b"x,y\n5,6\n".as_slice());
        let chart = parse_chart(
            r#"Chart(data: "primary.csv") {
                Table cities = "cities.tsv"
                Space(long * lat, data: cities) { Point() }
            }"#,
        );

        let prepared = prepare_chart_with_io(
            &chart,
            PrepareOptions {
                source_input: &source,
                base_dir: None,
                data_override: None,
                data_format_override: None,
                multi_chart: false,
            },
            &memory,
        )
        .unwrap();
        assert_eq!(prepared.primary.unwrap().frame.row_count(), 1);
        assert_eq!(prepared.named_tables[0].name, "cities");
        assert!(prepared.named_tables[0].frame.column("city").is_some());

        let schemas = load_named_table_schemas_with_io(&chart, &source, None, 10, &memory).unwrap();
        assert_eq!(schemas[0].schema[0].name, "long");

        let overridden = load_schema_with_io(
            &SourceExpr::Path {
                path: "primary.csv".to_string(),
                format: None,
                span: Span::new(0, 0),
            },
            &source,
            None,
            Some("/mem/override.json"),
            None,
            Some(10),
            &memory,
        )
        .unwrap();
        assert_eq!(overridden[0].name, "x");

        let stdin = load_data_with_io(
            &SourceExpr::Stdin {
                span: Span::new(0, 0),
            },
            &source,
            None,
            None,
            None,
            &memory,
        )
        .unwrap();
        assert_eq!(stdin.frame.row_count(), 1);
    }

    #[test]
    fn table_ref_chart_data_loads_primary_from_named_table() {
        let root = PathBuf::from("/mem/table-ref-primary");
        let source = SourceInput::Path(root.join("chart.ag"));
        let memory = MemoryIo::default()
            .with_file(root.join("some.csv"), b"x,y\n1,2\n".as_slice())
            .with_file(root.join("cities.csv"), b"long,lat\n3,4\n".as_slice());
        let chart = parse_chart(
            r#"Table main = "some.csv"
            Chart(data: main) {
                Table cities = "cities.csv"
                Space(x * y) { Point() }
            }"#,
        );

        let prepared = prepare_chart_with_io(
            &chart,
            PrepareOptions {
                source_input: &source,
                base_dir: None,
                data_override: None,
                data_format_override: None,
                multi_chart: false,
            },
            &memory,
        )
        .unwrap();

        assert_eq!(prepared.primary.unwrap().frame.row_count(), 1);
        assert!(prepared
            .named_tables
            .iter()
            .any(|table| table.name == "main"));
        assert!(prepared
            .named_tables
            .iter()
            .any(|table| table.name == "cities"));
        assert!(
            prepared.analysis.diagnostics.is_empty(),
            "{:?}",
            prepared.analysis.diagnostics
        );
    }

    #[test]
    fn chart_without_args_uses_table_main_as_primary() {
        let root = PathBuf::from("/mem/table-main-default");
        let source = SourceInput::Path(root.join("chart.ag"));
        let memory = MemoryIo::default().with_file(root.join("some.csv"), b"x,y\n1,2\n".as_slice());
        let chart = parse_chart(
            r#"Chart {
                Table main = "some.csv"
                Space(x * y, data: main) { Point() }
            }"#,
        );

        let prepared = prepare_chart_with_io(
            &chart,
            PrepareOptions {
                source_input: &source,
                base_dir: None,
                data_override: None,
                data_format_override: None,
                multi_chart: false,
            },
            &memory,
        )
        .unwrap();

        assert_eq!(prepared.primary.unwrap().frame.row_count(), 1);
        assert!(
            prepared.analysis.diagnostics.is_empty(),
            "{:?}",
            prepared.analysis.diagnostics
        );
    }

    #[test]
    fn caller_input_accepts_explicit_stream_formats_and_input_alias() {
        let source = SourceInput::Inline {
            label: "<eval>".to_string(),
        };
        let cases = [
            (
                r#"Chart(data: input) { Space(x * y) { Point() } }"#,
                br#"[{"x":1,"y":2},{"x":3,"y":4}]"#.as_slice(),
                Format::Json,
                2,
            ),
            (
                r#"Chart(data: stdin) { Space(x * y) { Point() } }"#,
                b"{\"x\":1,\"y\":2}\n{\"x\":3,\"y\":4}\n".as_slice(),
                Format::NdJson,
                2,
            ),
            (
                r#"Chart(data: input) { Space(x * y) { Point() } }"#,
                b"x\ty\n1\t2\n".as_slice(),
                Format::Tsv,
                1,
            ),
        ];

        for (source_text, bytes, format, rows) in cases {
            let chart = parse_chart(source_text);
            let memory = MemoryIo::default().with_stdin(bytes);
            let prepared = prepare_chart_with_io(
                &chart,
                PrepareOptions {
                    source_input: &source,
                    base_dir: None,
                    data_override: None,
                    data_format_override: Some(format),
                    multi_chart: false,
                },
                &memory,
            )
            .unwrap();

            assert_eq!(prepared.primary.unwrap().frame.row_count(), rows);
        }
    }

    #[test]
    fn caller_input_uses_reader_path_for_explicit_and_sniffed_stdin() {
        let source = SourceInput::Inline {
            label: "<eval>".to_string(),
        };
        let chart = parse_chart(r#"Chart(data: input) { Space(x * y) { Point() } }"#);

        for format in [Some(Format::Csv), None] {
            let io = ReaderOnlyStdinIo::new("x,y\n1,2\n3,4\n");
            let prepared = prepare_chart_with_io(
                &chart,
                PrepareOptions {
                    source_input: &source,
                    base_dir: None,
                    data_override: None,
                    data_format_override: format,
                    multi_chart: false,
                },
                &io,
            )
            .unwrap();

            assert_eq!(prepared.primary.unwrap().frame.row_count(), 2);
        }
    }

    #[test]
    fn in_memory_shapefile_bundle_matches_os_loading() {
        let source = SourceInput::Path(PathBuf::from("/mem/chart.ag"));
        let path = PathBuf::from("/mem/tiny.shp");
        let memory = MemoryIo::default()
            .with_file(&path, fs::read(fixture("tiny.shp")).unwrap())
            .with_file("/mem/tiny.dbf", fs::read(fixture("tiny.dbf")).unwrap())
            .with_file("/mem/tiny.shx", fs::read(fixture("tiny.shx")).unwrap())
            .with_file("/mem/tiny.prj", fs::read(fixture("tiny.prj")).unwrap())
            .with_file("/mem/tiny.cpg", fs::read(fixture("tiny.cpg")).unwrap());
        let chart = parse_chart(r#"Chart(data: Shapefile("tiny.shp")) { Space(geom) { Geo() } }"#);

        let os = load_path(
            &fixture("tiny.shp"),
            Some(Format::Shapefile),
            LoadContext::Primary,
        )
        .unwrap();
        let injected = prepare_chart_with_io(
            &chart,
            PrepareOptions {
                source_input: &source,
                base_dir: None,
                data_override: None,
                data_format_override: None,
                multi_chart: false,
            },
            &memory,
        )
        .unwrap()
        .primary
        .unwrap();

        assert_eq!(
            sorted_frame_signature(&injected.frame),
            sorted_frame_signature(&os.frame)
        );
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
                    data_format_override: None,
                    multi_chart: false,
                },
            )
            .unwrap();
            assert!(prepared.primary.unwrap().frame.column("geom").is_some());
        }
    }

    #[test]
    fn loads_topojson_constructor_with_object() {
        let source = SourceInput::Path(PathBuf::from("chart.ag"));
        let chart = parse_chart(&format!(
            r#"Chart(data: TopoJson("{}", object: "regions")) {{ Space(geom) {{ Geo() }} }}"#,
            fixture("tiny.topojson").display()
        ));
        let prepared = prepare_chart(
            &chart,
            PrepareOptions {
                source_input: &source,
                base_dir: None,
                data_override: None,
                data_format_override: None,
                multi_chart: false,
            },
        )
        .unwrap();
        let frame = prepared.primary.unwrap().frame;
        assert_eq!(frame.row_count(), 2);
        assert!(frame.column("geom").is_some());
        assert!(frame.column("population").is_some());
    }

    #[test]
    fn topojson_missing_object_reports_load_error() {
        let source = SourceInput::Path(PathBuf::from("chart.ag"));
        let chart = parse_chart(&format!(
            r#"Chart(data: TopoJson("{}", object: "nope")) {{ Space(geom) {{ Geo() }} }}"#,
            fixture("tiny.topojson").display()
        ));
        let prepared = prepare_chart_partial(&chart, partial_options(&source));
        assert!(prepared.primary.is_none());
        assert!(prepared
            .report
            .entries()
            .iter()
            .any(|(phase, d)| *phase == ReportPhase::Load && d.code == codes::E1805.as_str()));
    }

    #[test]
    fn loads_inferred_and_explicit_schema_formats() {
        let dir = temp_dir("schema-formats");
        fs::write(dir.join("data.csv"), "x,y\n1,2\n").unwrap();

        let csv_schema =
            load_schema_path(&dir.join("data.csv"), None, 10, LoadContext::Primary).unwrap();
        let geo_schema = load_schema_path(
            &fixture("tiny.geojson"),
            Some(Format::GeoJson),
            10,
            LoadContext::Primary,
        )
        .unwrap();

        assert_eq!(csv_schema[0].name, "x");
        assert!(geo_schema.iter().any(|column| column.name == "geom"));
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
                    data_format_override: None,
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
                data_format_override: None,
                multi_chart: true,
            },
        )
        .unwrap_err();
        assert!(matches!(err, DriverError::Usage(message) if message.contains("stdin data")));
    }

    #[test]
    fn partial_preparation_reports_stdin_conflict_without_unknown_columns() {
        let source = SourceInput::Stdin;
        let chart = parse_chart(r#"Chart(data: stdin) { Space(x * y) { Point() } }"#);

        let prepared = prepare_chart_partial(&chart, partial_options(&source));

        assert!(prepared.primary.is_none());
        assert!(prepared
            .report
            .entries()
            .iter()
            .any(|(phase, d)| *phase == ReportPhase::Load && d.code == codes::E1006.as_str()));
        assert!(!prepared
            .report
            .entries()
            .iter()
            .any(|(_, d)| d.code == codes::E1101.as_str()));
    }

    fn partial_options(source: &SourceInput) -> PrepareOptions<'_> {
        PrepareOptions {
            source_input: source,
            base_dir: None,
            data_override: None,
            data_format_override: None,
            multi_chart: false,
        }
    }

    #[test]
    fn partial_preparation_reports_missing_data_without_aborting() {
        let dir = temp_dir("partial-missing");
        let source = SourceInput::Path(dir.join("chart.ag"));
        let chart = parse_chart(r#"Chart(data: "missing.csv") { Space(x * y) { Point() } }"#);

        let prepared = prepare_chart_partial(&chart, partial_options(&source));

        assert!(prepared.primary.is_none());
        let load: Vec<_> = prepared
            .report
            .entries()
            .iter()
            .filter(|(phase, _)| *phase == ReportPhase::Load)
            .collect();
        assert_eq!(load.len(), 1);
        assert_eq!(load[0].1.code, codes::E1005.as_str());
        // Semantic analysis still runs against the empty schema.
        assert!(prepared
            .report
            .entries()
            .iter()
            .any(|(phase, _)| *phase == ReportPhase::Semantic));
    }

    #[test]
    fn partial_preparation_reports_malformed_data() {
        let dir = temp_dir("partial-malformed");
        fs::write(dir.join("bad.csv"), "x,y\n\"unterminated,2\n").unwrap();
        let source = SourceInput::Path(dir.join("chart.ag"));
        let chart = parse_chart(r#"Chart(data: "bad.csv") { Space(x * y) { Point() } }"#);

        let prepared = prepare_chart_partial(&chart, partial_options(&source));

        assert!(prepared.primary.is_none());
        assert!(prepared
            .report
            .entries()
            .iter()
            .any(|(phase, d)| *phase == ReportPhase::Load && d.code == codes::E1006.as_str()));
    }

    #[test]
    fn partial_preparation_surfaces_unknown_column_diagnostics() {
        let dir = temp_dir("partial-unknown");
        fs::write(dir.join("data.csv"), "x,y\n1,2\n").unwrap();
        let source = SourceInput::Path(dir.join("chart.ag"));
        let chart = parse_chart(r#"Chart(data: "data.csv") { Space(missing * y) { Point() } }"#);

        let prepared = prepare_chart_partial(&chart, partial_options(&source));

        assert!(prepared.primary.is_some());
        assert!(
            !prepared.analysis.diagnostics.is_empty(),
            "unknown column should produce a semantic diagnostic"
        );
        assert!(prepared
            .report
            .entries()
            .iter()
            .any(|(phase, _)| *phase == ReportPhase::Semantic));
    }

    #[test]
    fn partial_preparation_reports_named_table_failure_but_keeps_primary() {
        let dir = temp_dir("partial-named-fail");
        fs::write(dir.join("primary.csv"), "x,y\n1,2\n").unwrap();
        let source = SourceInput::Path(dir.join("chart.ag"));
        let chart = parse_chart(
            r#"Chart(data: "primary.csv") {
                Table cities = "missing.csv"
                Space(x * y) { Point() }
            }"#,
        );

        let prepared = prepare_chart_partial(&chart, partial_options(&source));

        assert!(prepared.primary.is_some());
        assert!(prepared.named_tables.is_empty());
        let load: Vec<_> = prepared
            .report
            .entries()
            .iter()
            .filter(|(phase, _)| *phase == ReportPhase::Load)
            .collect();
        assert_eq!(load.len(), 1);
        assert_eq!(load[0].1.code, codes::E1005.as_str());
    }

    #[test]
    fn partial_preparation_collects_data_warnings_with_context() {
        let dir = temp_dir("partial-warnings");
        fs::write(
            dir.join("data.csv"),
            "t\n2020-01-01T00:00:00Z\n2020-01-01T00:00:00\n",
        )
        .unwrap();
        let source = SourceInput::Path(dir.join("chart.ag"));
        let chart = parse_chart(r#"Chart(data: "data.csv") { Space(t) { Point() } }"#);

        let prepared = prepare_chart_partial(&chart, partial_options(&source));

        assert!(prepared.report.has_data_warnings());
        let entry = &prepared.report.data_warnings()[0];
        assert_eq!(entry.context, LoadContext::Primary);
        assert_eq!(entry.path.as_deref(), Some(dir.join("data.csv").as_path()));
        assert_eq!(entry.warning.column.as_deref(), Some("t"));
    }

    #[test]
    fn report_diagnostics_preserve_insertion_order_across_phases() {
        use algraf_core::{Diagnostic, Span};
        let mut report = crate::PreparationReport::new();
        report.extend(
            ReportPhase::Parse,
            [Diagnostic::error(codes::E0001, "parse", Span::new(0, 1))],
        );
        report.push(
            ReportPhase::Load,
            Diagnostic::error(codes::E1005, "load", Span::new(1, 2)),
        );
        report.extend(
            ReportPhase::Semantic,
            [Diagnostic::error(codes::E1001, "semantic", Span::new(2, 3))],
        );
        let observed: Vec<&str> = report.diagnostics().iter().map(|d| d.code).collect();
        assert_eq!(
            observed,
            vec![
                codes::E0001.as_str(),
                codes::E1005.as_str(),
                codes::E1001.as_str()
            ]
        );
    }

    #[test]
    fn central_driver_error_mapping_assigns_stable_codes() {
        use algraf_data::DataError;
        use std::io;
        use std::path::Path;

        let not_found = DataError::Io(io::Error::new(io::ErrorKind::NotFound, "nope"));
        let (code, message) = crate::data_error_code_message(Path::new("a.csv"), &not_found);
        assert_eq!(code, codes::E1005);
        assert!(message.contains("a.csv"));

        let (code, _) =
            crate::data_error_code_message(Path::new("a.json"), &DataError::JsonNotArray);
        assert_eq!(code, codes::E1010);

        let (code, _) = crate::data_error_code_message(
            Path::new("a.shp"),
            &DataError::Geo("bad geometry".to_string()),
        );
        assert_eq!(code, codes::E1805);

        let usage = DriverError::Usage("stdin only".to_string());
        let (code, message) = crate::driver_error_code_message(&usage);
        assert_eq!(code, codes::E1006);
        assert_eq!(message, "stdin only");
    }

    #[test]
    fn data_source_key_normalizes_equivalent_paths() {
        let dotted = DataSourceKey::new("a/./b.csv", None);
        let plain = DataSourceKey::new("a/b.csv", None);
        let parented = DataSourceKey::new("a/c/../b.csv", None);

        assert_eq!(dotted, plain);
        assert_eq!(parented, plain);
        assert_eq!(plain.path(), Path::new("a/b.csv"));
    }

    #[test]
    fn data_source_key_distinguishes_explicit_format() {
        let inferred = DataSourceKey::new("a/data.geojson", None);
        let explicit = DataSourceKey::new("a/data.geojson", Some(Format::GeoJson));

        assert_ne!(inferred, explicit);
        assert_eq!(explicit.format(), Some(Format::GeoJson));
    }

    #[test]
    fn data_source_key_distinguishes_sqlite_queries() {
        let first = DataSourceKey::sqlite("a/sales.db", "SELECT region FROM sales ORDER BY region");
        let second =
            DataSourceKey::sqlite("a/sales.db", "SELECT revenue FROM sales ORDER BY revenue");

        assert_ne!(first, second);
        assert_eq!(
            first.query(),
            Some("SELECT region FROM sales ORDER BY region")
        );
    }

    #[test]
    fn data_source_key_distinguishes_topojson_objects() {
        let regions = DataSourceKey::topojson("a/map.topojson", Some("regions"));
        let counties = DataSourceKey::topojson("a/map.topojson", Some("counties"));
        let sole = DataSourceKey::topojson("a/map.topojson", None);

        assert_ne!(regions, counties);
        assert_ne!(regions, sole);
        assert_eq!(regions.format(), Some(Format::TopoJson));
        assert_eq!(regions.object(), Some("regions"));
    }

    #[test]
    fn schema_cache_resolves_topojson_object_schema() {
        let path = PathBuf::from("/mem/grid.topojson");
        let io = CountingIo::new(MemoryIo::default().with_file(
            &path,
            br#"{
              "type": "Topology",
              "objects": {
                "grid": {
                  "type": "GeometryCollection",
                  "geometries": [
                    {"type": "Point", "coordinates": [0, 0], "properties": {"cell": "A", "value": 10}}
                  ]
                },
                "labels": {
                  "type": "GeometryCollection",
                  "geometries": [
                    {"type": "Point", "coordinates": [1, 1], "properties": {"name": "B"}}
                  ]
                }
              },
              "arcs": []
            }"#
            .as_slice(),
        ));
        let cache = InMemorySchemaCache::new();

        let grid =
            resolve_topojson_schema_cached(&cache, &io, &path, Some("grid"), LoadContext::Primary);
        let labels = resolve_topojson_schema_cached(
            &cache,
            &io,
            &path,
            Some("labels"),
            LoadContext::Primary,
        );
        let cached_grid =
            resolve_topojson_schema_cached(&cache, &io, &path, Some("grid"), LoadContext::Primary);

        let CachedSchema::Ready(grid) = grid else {
            panic!("expected grid schema");
        };
        let CachedSchema::Ready(labels) = labels else {
            panic!("expected labels schema");
        };
        assert!(grid.iter().any(|column| column.name == "value"));
        assert!(!labels.iter().any(|column| column.name == "value"));
        assert!(matches!(cached_grid, CachedSchema::Ready(_)));
        assert_eq!(
            io.reads(),
            2,
            "different TopoJSON objects should use distinct cache entries"
        );
    }

    #[test]
    fn fingerprint_is_none_for_missing_metadata_and_tracks_size() {
        let path = PathBuf::from("/mem/data.csv");
        assert!(fingerprint_path(&MemoryIo::default(), &path).is_none());

        let small = MemoryIo::default().with_file(&path, b"x\n1\n".as_slice());
        let large = MemoryIo::default().with_file(&path, b"x,y\n1,2\n3,4\n".as_slice());
        let small_fp = fingerprint_path(&small, &path).unwrap();
        let large_fp = fingerprint_path(&large, &path).unwrap();

        assert_eq!(small_fp.len, 4);
        assert_ne!(small_fp, large_fp);
    }

    #[test]
    fn chart_data_plan_records_dependencies_without_loading() {
        // No files are written: a plan that loaded bytes would fail here.
        let dir = temp_dir("plan-no-load");
        let source = SourceInput::Path(dir.join("chart.ag"));
        let chart = parse_chart(
            r#"Chart(data: "primary.csv") {
                Table cities = GeoJson("cities.geojson")
                Space(x * y) { Point() }
            }"#,
        );

        let plan = plan_chart_data(&chart, &source, None, None, false).unwrap();

        assert!(matches!(
            &plan.primary,
            Some(DataLocation::Path { path, .. }) if path == &dir.join("primary.csv")
        ));
        assert!(plan.primary_span().is_some());
        let deps = plan.data_dependencies();
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].kind, DataDependencyKind::Primary);
        assert_eq!(
            deps[1].kind,
            DataDependencyKind::Table {
                name: "cities".to_string()
            }
        );
        assert_eq!(deps[1].format, Some(Format::GeoJson));
    }

    #[test]
    fn schema_cache_reuses_unchanged_source() {
        let path = PathBuf::from("/mem/data.csv");
        let io = CountingIo::new(MemoryIo::default().with_file(&path, b"x,y\n1,2\n".as_slice()));
        let cache = InMemorySchemaCache::new();

        let first = resolve_schema_cached(&cache, &io, &path, None, 10, LoadContext::Primary);
        let second = resolve_schema_cached(&cache, &io, &path, None, 10, LoadContext::Primary);

        assert!(matches!(first, CachedSchema::Ready(ref s) if s[0].name == "x"));
        assert!(matches!(second, CachedSchema::Ready(_)));
        assert_eq!(
            io.reads(),
            1,
            "an unchanged source should load exactly once"
        );
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn schema_cache_reloads_changed_source() {
        let path = PathBuf::from("/mem/data.csv");
        let cache = InMemorySchemaCache::new();

        let before =
            CountingIo::new(MemoryIo::default().with_file(&path, b"x,y\n1,2\n".as_slice()));
        let first = resolve_schema_cached(&cache, &before, &path, None, 10, LoadContext::Primary);

        // A different byte length yields a different fingerprint, so the entry
        // must be reloaded rather than served stale.
        let after =
            CountingIo::new(MemoryIo::default().with_file(&path, b"x,y,z\n1,2,3\n".as_slice()));
        let second = resolve_schema_cached(&cache, &after, &path, None, 10, LoadContext::Primary);

        let CachedSchema::Ready(first) = first else {
            panic!("expected a ready schema");
        };
        let CachedSchema::Ready(second) = second else {
            panic!("expected a ready schema");
        };
        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 3, "a changed source should reload");
        assert_eq!(after.reads(), 1);
    }

    #[test]
    fn schema_cache_keeps_error_kinds_distinct_and_never_serves_them_stale() {
        let path = PathBuf::from("/mem/data.csv");
        let cache = InMemorySchemaCache::new();

        // Missing file: metadata is unavailable, so the fingerprint is `None`.
        let missing_io = CountingIo::new(MemoryIo::default());
        let missing =
            resolve_schema_cached(&cache, &missing_io, &path, None, 10, LoadContext::Primary);
        match missing {
            CachedSchema::Error { code, .. } => assert_eq!(code, codes::E1005),
            _ => panic!("a missing file should resolve to an error"),
        }

        // The file later appears: a `None` fingerprint must not serve the stale
        // missing-file error, so the now-present schema is loaded.
        let present_io =
            CountingIo::new(MemoryIo::default().with_file(&path, b"x,y\n1,2\n".as_slice()));
        let present =
            resolve_schema_cached(&cache, &present_io, &path, None, 10, LoadContext::Primary);
        assert!(matches!(present, CachedSchema::Ready(_)));
        assert_eq!(present_io.reads(), 1);
    }

    #[test]
    fn schema_cache_reports_malformed_data_distinctly() {
        let path = PathBuf::from("/mem/bad.csv");
        let io = CountingIo::new(
            MemoryIo::default().with_file(&path, b"x,y\n\"unterminated,2\n".as_slice()),
        );
        let cache = InMemorySchemaCache::new();

        let result = resolve_schema_cached(&cache, &io, &path, None, 10, LoadContext::Primary);
        match result {
            CachedSchema::Error { code, .. } => assert_eq!(code, codes::E1006),
            _ => panic!("malformed CSV should resolve to an error"),
        }
    }

    #[test]
    fn no_schema_cache_always_reloads() {
        let path = PathBuf::from("/mem/data.csv");
        let io = CountingIo::new(MemoryIo::default().with_file(&path, b"x,y\n1,2\n".as_slice()));
        let cache = NoSchemaCache;

        resolve_schema_cached(&cache, &io, &path, None, 10, LoadContext::Primary);
        resolve_schema_cached(&cache, &io, &path, None, 10, LoadContext::Primary);

        assert_eq!(io.reads(), 2, "a no-op cache should reload every time");
    }
}
