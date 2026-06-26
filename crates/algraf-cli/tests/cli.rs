use std::fs;
use std::io::Cursor;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use algraf_render::{render_embedded, EmbeddedRenderOptions};
use arrow_array::{ArrayRef, Float64Array, Int64Array, RecordBatch, StringArray};
use arrow_ipc::writer::StreamWriter;
use arrow_schema::{DataType as ArrowDataType, Field, Schema};
use parquet::arrow::ArrowWriter;

const LANGUAGE_TEMPLATE: &str = include_str!("../templates/ALGRAF_LANGUAGE.md");
const TOOLING_TEMPLATE: &str = include_str!("../templates/ALGRAF_TOOLING.md");
const FULL_TEMPLATE: &str = include_str!("../templates/ALGRAF_LANG.md");

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_algraf")
}

fn composed_reference_template() -> String {
    format!("{LANGUAGE_TEMPLATE}\n{TOOLING_TEMPLATE}")
}

fn temp_dir(test: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("algraf-cli-{test}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let data = dir.join("data.csv");
    let chart = dir.join("chart.ag");
    fs::write(&data, "x,y,group\n1,2,a\n3,4,b\n").unwrap();
    fs::write(
        &chart,
        "Chart(data: \"data.csv\") {\n  Space(x * y) {\n    Point(fill: group)\n  }\n}\n",
    )
    .unwrap();
    (chart, data)
}

fn write_parquet_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let data = dir.join("points.parquet");
    let chart = dir.join("parquet.ag");
    let schema = Arc::new(Schema::new(vec![
        Field::new("x", ArrowDataType::Float64, false),
        Field::new("y", ArrowDataType::Float64, false),
        Field::new("group", ArrowDataType::Utf8, true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Float64Array::from(vec![1.0, 2.0, 3.0, 4.0])) as ArrayRef,
            Arc::new(Float64Array::from(vec![3.0, 5.0, 4.0, 7.0])) as ArrayRef,
            Arc::new(StringArray::from(vec![
                Some("a"),
                Some("b"),
                Some("a"),
                Some("b"),
            ])) as ArrayRef,
        ],
    )
    .unwrap();
    let file = fs::File::create(&data).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
    fs::write(
        &chart,
        "Chart(data: Parquet(\"points.parquet\")) {\n  Space(x * y) {\n    Point(fill: group)\n  }\n}\n",
    )
    .unwrap();
    (chart, data)
}

fn arrow_stream_fixture() -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("x", ArrowDataType::Float64, false),
        Field::new("y", ArrowDataType::Float64, false),
        Field::new("group", ArrowDataType::Utf8, true),
        Field::new("count", ArrowDataType::Int64, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Float64Array::from(vec![1.0, 2.0, 3.0, 4.0])) as ArrayRef,
            Arc::new(Float64Array::from(vec![3.0, 5.0, 4.0, 7.0])) as ArrayRef,
            Arc::new(StringArray::from(vec![
                Some("a"),
                Some("b"),
                Some("a"),
                Some("b"),
            ])) as ArrayRef,
            Arc::new(Int64Array::from(vec![1, 2, 3, 4])) as ArrayRef,
        ],
    )
    .unwrap();
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &schema).unwrap();
    writer.write(&batch).unwrap();
    writer.finish().unwrap();
    drop(writer);
    bytes
}

fn data_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../algraf-data/tests/fixtures")
        .join(name)
}

#[test]
fn render_writes_svg_to_stdout() {
    let dir = temp_dir("render");
    let (chart, _) = write_fixture(&dir);

    let output = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("<svg"));
}

#[test]
fn schema_and_render_accept_native_parquet_sources() {
    let dir = temp_dir("parquet-source");
    let (chart, _) = write_parquet_fixture(&dir);

    let schema = Command::new(bin())
        .arg("schema")
        .arg(&chart)
        .arg("--json")
        .output()
        .unwrap();
    assert!(schema.status.success(), "stderr: {}", stderr(&schema));
    let schema_json = stdout(&schema);
    assert!(schema_json.contains("\"name\": \"x\""));
    assert!(schema_json.contains("\"type\": \"float\""));

    let rendered = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .output()
        .unwrap();
    assert!(rendered.status.success(), "stderr: {}", stderr(&rendered));
    assert!(stdout(&rendered).contains("<svg"));
}

#[test]
fn render_interactive_embeds_script_only_with_flag() {
    let dir = temp_dir("render-interactive");
    let data = dir.join("data.csv");
    let chart = dir.join("chart.ag");
    fs::write(&data, "x,y,group\n1,2,a\n3,4,b\n").unwrap();
    fs::write(
        &chart,
        "Chart(data: \"data.csv\") {\n  Space(x * y) {\n    Point(tooltip: [group], highlight: \"group\")\n  }\n}\n",
    )
    .unwrap();

    let plain = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .output()
        .unwrap();
    assert!(plain.status.success(), "stderr: {}", stderr(&plain));
    let plain_svg = stdout(&plain);
    // Static affordances are present without the opt-in; the script is not.
    assert!(plain_svg.contains("data-algraf-highlight=\"a\""));
    assert!(!plain_svg.contains("<script"));

    let interactive = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .arg("--interactive")
        .output()
        .unwrap();
    assert!(
        interactive.status.success(),
        "stderr: {}",
        stderr(&interactive)
    );
    assert!(stdout(&interactive).contains("<script"));
}

#[test]
fn render_draw_list_writes_json_to_stdout() {
    let dir = temp_dir("render-draw-list");
    let (chart, _) = write_fixture(&dir);

    let output = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .arg("--format")
        .arg("draw-list")
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    // A draw list, not SVG.
    assert!(!out.contains("<svg"));
    assert!(out.starts_with("{\"width\":"));
    assert!(out.contains("\"interactions\":{\"version\":1"));
    assert!(out.contains("\"role\":\"background\""));
    assert!(out.contains("\"role\":\"plot-area\""));
}

#[test]
fn render_metadata_writes_interaction_sidecar() {
    let dir = temp_dir("render-metadata");
    let data = dir.join("data.csv");
    let chart = dir.join("chart.ag");
    let svg = dir.join("chart.svg");
    let metadata = dir.join("chart.meta.json");
    fs::write(&data, "x,y,group\n1,2,a\n3,4,b\n").unwrap();
    fs::write(
        &chart,
        "Chart(data: \"data.csv\") {\n  Space(x * y) {\n    Point(tooltip: [group, y], highlight: \"group\")\n    On(event: \"click\", emit: group)\n  }\n}\n",
    )
    .unwrap();

    let output = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .arg("--output")
        .arg(&svg)
        .arg("--metadata")
        .arg(&metadata)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).is_empty());
    assert!(fs::read_to_string(svg).unwrap().contains("<svg"));

    let sidecar = fs::read_to_string(metadata).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&sidecar).unwrap();
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["marks"][0]["tooltip"][0]["label"], "group");
    assert_eq!(parsed["marks"][0]["groups"]["group"], "a");
    assert_eq!(parsed["marks"][0]["interaction"]["event"], "click");
    assert_eq!(parsed["marks"][0]["interaction"]["emit_field"], "group");
    assert_eq!(parsed["groups"]["group"], serde_json::json!(["a", "b"]));
}

#[test]
fn render_svg_json_format_derives_sidecar_path() {
    let dir = temp_dir("render-svg-json");
    let (chart, _) = write_fixture(&dir);
    let base = dir.join("chart");

    let output = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .arg("--format")
        .arg("svg+json")
        .arg("--output")
        .arg(&base)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(dir.join("chart.svg").exists());
    let metadata = dir.join("chart.meta.json");
    assert!(metadata.exists());
    assert!(fs::read_to_string(metadata)
        .unwrap()
        .starts_with("{\"version\":1,\"plot_rect\":"));
}

#[test]
fn render_raster_writes_png_from_scene_model() {
    let dir = temp_dir("render-raster");
    let (chart, _) = write_fixture(&dir);
    let out_path = dir.join("out.png");

    let output = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .arg("--format")
        .arg("raster")
        .arg("--output")
        .arg(&out_path)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let bytes = fs::read(&out_path).unwrap();
    // PNG magic number.
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn render_debug_flags_add_deterministic_svg_content() {
    let dir = temp_dir("render-debug");
    let (chart, _) = write_fixture(&dir);

    let output = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .arg("--emit-metadata")
        .arg("--debug-layout")
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("<!-- algraf metadata:"));
    assert!(out.contains("class=\"algraf-debug-layout\""));
}

#[test]
fn render_writes_png_when_output_extension_is_png() {
    let dir = temp_dir("render-png");
    let (chart, _) = write_fixture(&dir);
    let png = dir.join("chart.png");

    let output = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .arg("--output")
        .arg(&png)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).is_empty());
    let bytes = fs::read(png).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let (width, height, pixel_dims) = read_png_info(&bytes);
    assert_eq!((width, height), (1600, 1040));
    assert_eq!(pixel_dims.unwrap().xppu, pixels_per_meter(192));
}

#[test]
fn render_png_scale_and_dpi_are_configurable() {
    let dir = temp_dir("render-png-scale");
    let (chart, _) = write_fixture(&dir);
    let png = dir.join("chart.png");

    let output = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .arg("--output")
        .arg(&png)
        .arg("--png-scale")
        .arg("1")
        .arg("--png-dpi")
        .arg("300")
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let bytes = fs::read(png).unwrap();
    let (width, height, pixel_dims) = read_png_info(&bytes);
    assert_eq!((width, height), (800, 520));
    assert_eq!(pixel_dims.unwrap().xppu, pixels_per_meter(300));
}

#[test]
fn committed_examples_check_and_render() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out_dir = temp_dir("examples-render");
    let examples = [
        "scatter",
        "line",
        "grouped_bar",
        "stacked_bar",
        "fill_bar",
        "heatmap",
        "histogram",
        "histogram_direct",
        "facet",
        "connected_scatter",
        "barcode",
        "floating",
        "smooth",
        "boxplot",
        "ribbon",
        "violin",
        "freqpoly",
        "derived_chain",
        "gradient",
        "group_line",
        "shapes",
        "penguin_channels",
        "bin2d",
        "hexbin",
        "reference",
    ];

    for name in examples {
        let chart = repo.join("examples").join(format!("{name}.ag"));
        let check = Command::new(bin())
            .arg("check")
            .arg(&chart)
            .output()
            .unwrap();
        assert!(
            check.status.success(),
            "{name} check stderr: {}",
            stderr(&check)
        );

        let svg = out_dir.join(format!("{name}.svg"));
        let render = Command::new(bin())
            .arg("render")
            .arg(&chart)
            .arg("--output")
            .arg(&svg)
            .output()
            .unwrap();
        assert!(
            render.status.success(),
            "{name} render stderr: {}",
            stderr(&render)
        );
        assert!(fs::read_to_string(svg).unwrap().contains("<svg"));

        let png = out_dir.join(format!("{name}.png"));
        let render_png = Command::new(bin())
            .arg("render")
            .arg(&chart)
            .arg("--output")
            .arg(&png)
            .output()
            .unwrap();
        assert!(
            render_png.status.success(),
            "{name} PNG render stderr: {}",
            stderr(&render_png)
        );
        assert!(fs::read(png).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}

#[test]
fn schema_reads_source_from_stdin_once() {
    let dir = temp_dir("schema-stdin");
    let _ = write_fixture(&dir);
    let source = "Chart(data: \"data.csv\") {\n  Space(x * y) { Point() }\n}\n";

    let mut child = Command::new(bin())
        .arg("schema")
        .arg("-")
        .arg("--json")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("\"name\": \"x\""), "stdout: {out}");
}

#[test]
fn missing_chart_data_is_a_diagnostic() {
    let dir = temp_dir("missing-data");
    let chart = dir.join("chart.ag");
    fs::write(&chart, "Chart() {}\n").unwrap();

    let output = Command::new(bin())
        .arg("check")
        .arg(&chart)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("E1001"));
}

#[test]
fn data_flag_does_not_replace_missing_chart_data() {
    let dir = temp_dir("data-override-missing");
    let chart = dir.join("chart.ag");
    let data = dir.join("data.csv");
    fs::write(&chart, "Chart() {}\n").unwrap();
    fs::write(&data, "x,y\n1,2\n").unwrap();

    let output = Command::new(bin())
        .arg("check")
        .arg(&chart)
        .arg("--data")
        .arg(&data)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("E1001"));
}

#[test]
fn source_and_caller_data_cannot_both_read_from_stdin() {
    let source = "Chart(data: \"ignored.csv\") {\n  Space(x * y) { Point() }\n}\n";
    let mut child = Command::new(bin())
        .arg("render")
        .arg("-")
        .arg("--data")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("cannot read both source and caller-provided data from stdin"));
}

#[test]
fn check_stdin_source_conflict_does_not_report_unknown_columns() {
    let source = "Chart(data: stdin) {\n  Space(x * y) { Point() }\n}\n";
    let mut child = Command::new(bin())
        .arg("check")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let err = stderr(&output);

    assert_eq!(output.status.code(), Some(1), "stderr: {err}");
    assert!(err.contains("Chart(data: stdin) but source was also read from stdin"));
    assert!(!err.contains("unknown column"), "stderr: {err}");
}

#[test]
fn render_eval_uses_stdin_for_json_input_and_variables() {
    let source = r##"Chart(data: input, width: 320, height: 220) {
  Space(x * y) {
    Line(stroke: "${color}", strokeWidth: ${size})
    Point(fill: "${color}", size: ${size})
  }
}
"##;
    let mut child = Command::new(bin())
        .arg("render")
        .arg("--eval")
        .arg(source)
        .arg("--data")
        .arg("-")
        .arg("--data-format")
        .arg("json")
        .arg("--var")
        .arg("color=#e74c3c")
        .arg("--var")
        .arg("size=3")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(br#"[{"x":1,"y":2},{"x":3,"y":4}]"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("<svg"));
    assert!(out.contains("#e74c3c"));

    let facade = render_embedded(
        source,
        br#"[{"x":1,"y":2},{"x":3,"y":4}]"#,
        EmbeddedRenderOptions {
            data_format: algraf_data::Format::Json,
            variables: [
                ("color".to_string(), "#e74c3c".to_string()),
                ("size".to_string(), "3".to_string()),
            ]
            .into_iter()
            .collect(),
            ..EmbeddedRenderOptions::default()
        },
    )
    .unwrap();
    assert_eq!(out, facade.svg().unwrap());
}

#[test]
fn source_placeholders_require_cli_variables() {
    let source = "Chart(data: \"p.csv\") {\n  Space(x * y) { Point(stroke: ${color}) }\n}\n";
    let output = Command::new(bin())
        .arg("ast")
        .arg("--eval")
        .arg(source)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    let err = stderr(&output);

    assert_eq!(output.status.code(), Some(2), "stderr: {err}");
    assert!(
        err.contains("undefined variable \"color\""),
        "stderr: {err}"
    );
}

#[test]
fn render_eval_accepts_arrow_stream_stdin_explicit_and_sniffed() {
    let source = "Chart(data: input) {\n  Space(x * y) {\n    Point(fill: group)\n  }\n}\n";
    for args in [
        vec!["--data", "-", "--data-format", "arrow-stream"],
        vec!["--data", "-"],
    ] {
        let mut child = Command::new(bin())
            .arg("render")
            .arg("--eval")
            .arg(source)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&arrow_stream_fixture())
            .unwrap();
        let output = child.wait_with_output().unwrap();

        assert!(output.status.success(), "stderr: {}", stderr(&output));
        assert!(stdout(&output).contains("<svg"));
    }
}

#[test]
fn schema_accepts_arrow_stream_stdin_and_alias() {
    let source = "Chart(data: input) {\n  Space(x * y) { Point() }\n}\n";
    let mut child = Command::new(bin())
        .arg("schema")
        .arg("--eval")
        .arg(source)
        .arg("--json")
        .arg("--data")
        .arg("-")
        .arg("--data-format")
        .arg("arrow")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(&arrow_stream_fixture())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("\"name\": \"x\""), "stdout: {out}");
    assert!(out.contains("\"type\": \"float\""), "stdout: {out}");
    assert!(out.contains("\"name\": \"count\""), "stdout: {out}");
    assert!(out.contains("\"type\": \"integer\""), "stdout: {out}");
}

#[test]
fn sniffed_arrow_file_is_a_registered_diagnostic() {
    let source = "Chart(data: input) {\n  Space(x * y) { Point() }\n}\n";
    let mut child = Command::new(bin())
        .arg("check")
        .arg("--eval")
        .arg(source)
        .arg("--data")
        .arg("-")
        .arg("--json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"ARROW1\0\0")
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("\"code\": \"E1022\""), "stdout: {out}");
    assert!(stderr(&output).is_empty(), "stderr: {}", stderr(&output));
}

#[test]
fn eval_conflicts_with_positional_source() {
    let dir = temp_dir("eval-conflict");
    let (chart, _) = write_fixture(&dir);

    let output = Command::new(bin())
        .arg("check")
        .arg(&chart)
        .arg("--eval")
        .arg("Chart(data: input) {}")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("cannot be used with"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn bad_data_format_is_a_usage_error() {
    let output = Command::new(bin())
        .arg("schema")
        .arg("--eval")
        .arg("Chart(data: input) {}")
        .arg("--data")
        .arg("-")
        .arg("--data-format")
        .arg("yaml")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("invalid value"));
}

#[test]
fn check_loads_named_geojson_table_constructor() {
    let dir = temp_dir("named-geojson-table");
    let data = dir.join("data.csv");
    let chart = dir.join("chart.ag");
    fs::write(&data, "x,y\n1,2\n").unwrap();
    let geojson = data_fixture("tiny.geojson");
    fs::write(
        &chart,
        format!(
            "Chart(data: \"data.csv\") {{\n  Table shapes = GeoJson(\"{}\")\n  Space(geom, data: shapes) {{ Geo() }}\n}}\n",
            geojson.display()
        ),
    )
    .unwrap();

    let output = Command::new(bin())
        .arg("check")
        .arg(&chart)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
}

#[test]
fn check_emits_data_warning_in_human_output_but_not_json() {
    let dir = temp_dir("data-warning");
    let data = dir.join("data.csv");
    let chart = dir.join("chart.ag");
    // A column mixing naive and offset-aware datetimes raises a data inference
    // warning (spec §10.3). The warning has no source span, so it prints as a
    // plain `warning:` line in human output and is omitted from JSON.
    fs::write(&data, "t\n2020-01-01T00:00:00Z\n2020-01-01T00:00:00\n").unwrap();
    fs::write(
        &chart,
        "Chart(data: \"data.csv\") {\n  Space(t) {\n    Point()\n  }\n}\n",
    )
    .unwrap();

    let human = Command::new(bin())
        .arg("check")
        .arg(&chart)
        .output()
        .unwrap();
    assert!(human.status.success(), "stderr: {}", stderr(&human));
    assert!(
        stderr(&human).contains("warning: column mixes naive and offset-aware datetime values"),
        "stderr: {}",
        stderr(&human)
    );

    let json = Command::new(bin())
        .arg("check")
        .arg(&chart)
        .arg("--json")
        .output()
        .unwrap();
    assert!(json.status.success(), "stderr: {}", stderr(&json));
    assert!(
        !stdout(&json).contains("naive and offset-aware"),
        "data warnings must not appear in JSON diagnostics: {}",
        stdout(&json)
    );

    // --strict promotes the data warning to a failure.
    let strict = Command::new(bin())
        .arg("check")
        .arg(&chart)
        .arg("--strict")
        .output()
        .unwrap();
    assert!(!strict.status.success());
}

#[test]
fn init_codex_creates_language_reference_and_agents_file() {
    let dir = temp_dir("init-codex");

    let output = Command::new(bin())
        .arg("init")
        .arg("--codex")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let lang = fs::read_to_string(dir.join("ALGRAF_LANG.md")).unwrap();
    assert_eq!(lang, composed_reference_template());
    assert!(lang.contains("# Algraf Language Reference"));
    assert!(lang.contains("Algraf is not JavaScript, Python"));
    let agents = fs::read_to_string(dir.join("AGENTS.md")).unwrap();
    assert!(agents.contains("ALGRAF_LANG.md"));
    assert!(!dir.join("CLAUDE.md").exists());
}

#[test]
fn language_reference_templates_compose_full_reference_without_tooling_drift() {
    assert_eq!(FULL_TEMPLATE, composed_reference_template());
    assert!(LANGUAGE_TEMPLATE.contains("## Complete Geometry Property Reference"));
    assert!(!LANGUAGE_TEMPLATE.contains("## CLI Commands"));
    assert!(!LANGUAGE_TEMPLATE.contains("## Project Agent Setup"));
    assert!(TOOLING_TEMPLATE.contains("## CLI Commands"));
    assert!(TOOLING_TEMPLATE.contains("## Project Agent Setup"));
    assert!(FULL_TEMPLATE.contains("## Complete Geometry Property Reference"));
    assert!(FULL_TEMPLATE.contains("## CLI Commands"));
    assert!(FULL_TEMPLATE.contains("## Project Agent Setup"));
}

#[test]
fn init_claude_and_agy_share_language_reference() {
    let dir = temp_dir("init-claude-agy");

    let output = Command::new(bin())
        .arg("init")
        .arg("--claude")
        .arg("--agy")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(dir.join("ALGRAF_LANG.md").exists());
    assert!(fs::read_to_string(dir.join("AGENTS.md"))
        .unwrap()
        .contains("ALGRAF_LANG.md"));
    assert!(fs::read_to_string(dir.join("CLAUDE.md"))
        .unwrap()
        .contains("ALGRAF_LANG.md"));
}

#[test]
fn init_appends_to_existing_agents_file_without_overwriting() {
    let dir = temp_dir("init-existing-agents");
    let agents_path = dir.join("AGENTS.md");
    fs::write(&agents_path, "# Existing\n\nKeep this line.\n").unwrap();

    let output = Command::new(bin())
        .arg("init")
        .arg("--agy")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let agents = fs::read_to_string(agents_path).unwrap();
    assert!(agents.contains("Keep this line."));
    assert!(agents.contains("ALGRAF_LANG.md"));
}

#[test]
fn init_refuses_to_overwrite_existing_language_reference() {
    let dir = temp_dir("init-existing-lang");
    fs::write(dir.join("ALGRAF_LANG.md"), "custom\n").unwrap();

    let output = Command::new(bin())
        .arg("init")
        .arg("--codex")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("refusing to overwrite"),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(
        fs::read_to_string(dir.join("ALGRAF_LANG.md")).unwrap(),
        "custom\n"
    );
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn read_png_info(bytes: &[u8]) -> (u32, u32, Option<png::PixelDimensions>) {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let reader = decoder.read_info().unwrap();
    let info = reader.info();
    (info.width, info.height, info.pixel_dims)
}

fn pixels_per_meter(dpi: u32) -> u32 {
    (f64::from(dpi) / 0.0254).round() as u32
}

#[test]
fn render_multi_chart_writes_one_file_per_chart() {
    let dir = temp_dir("render-multi");
    let data = dir.join("data.csv");
    let chart = dir.join("chart.ag");
    fs::write(&data, "x,y,group\n1,2,a\n3,4,b\n").unwrap();
    fs::write(
        &chart,
        "Chart(data: \"data.csv\") {\n  Space(x * y) { Point() }\n}\nChart(data: \"data.csv\") {\n  Space(x * y) { Line() }\n}\n",
    )
    .unwrap();
    let out = dir.join("out.svg");

    let output = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .arg("--output")
        .arg(&out)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(dir.join("out-1.svg").exists(), "first chart output");
    assert!(dir.join("out-2.svg").exists(), "second chart output");
    assert!(!out.exists(), "no verbatim file when multiple charts");
}

#[test]
fn render_multi_chart_to_stdout_is_a_usage_error() {
    let dir = temp_dir("render-multi-stdout");
    let data = dir.join("data.csv");
    let chart = dir.join("chart.ag");
    fs::write(&data, "x,y\n1,2\n3,4\n").unwrap();
    fs::write(
        &chart,
        "Chart(data: \"data.csv\") {\n  Space(x * y) { Point() }\n}\nChart(data: \"data.csv\") {\n  Space(x * y) { Line() }\n}\n",
    )
    .unwrap();

    let output = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("2 charts"),
        "stderr: {}",
        stderr(&output)
    );
}
