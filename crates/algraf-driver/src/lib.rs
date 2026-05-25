//! Shared parsing, source resolution, data loading, and analysis driver.
//!
//! The driver is intentionally non-UI: it does not parse command-line flags,
//! print diagnostics, choose output filenames, rasterize PNGs, or speak LSP.

mod error;
mod loading;
mod prepare;
mod resolution;

pub use error::{DriverError, LoadContext};
pub use loading::{
    load_data, load_named_table_schemas, load_named_tables, load_path, load_schema,
    load_schema_path, NamedTable, NamedTableSchema,
};
pub use prepare::{prepare_chart, PrepareOptions, PreparedChart};
pub use resolution::{
    data_location, resolve_chart_data_path, resolve_document_data_path,
    resolve_named_table_sources, resolve_path, resolve_source_expr_path, source_base_dir,
    source_format_to_data, DataLocation, ResolvedSource, ResolvedTableSource, SourceInput,
};

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
    use algraf_core::Span;
    use algraf_data::Format;
    use algraf_syntax::SourceFormat;
    use std::fs;
    use std::path::{Path, PathBuf};
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
        let env = crate::resolution::DriverEnv::new(&source, Some(&base), None, false);

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
