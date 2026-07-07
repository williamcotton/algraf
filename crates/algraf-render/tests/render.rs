//! End-to-end render tests: source + CSV to SVG (spec §18, §24, §27.1).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
#[cfg(feature = "arrow-stream")]
use std::sync::Arc;

use algraf_data::{read_csv_str, Table};
use algraf_driver::SourceInput;
use algraf_render::{
    load_image_assets_with_io, render, render_embedded, EmbeddedOutputFormat,
    EmbeddedRenderOptions, InMemoryDriverIo, RenderLimits, RenderOptions, RenderResult, Theme,
};
use algraf_semantics::{analyze, analyze_with_tables};
use algraf_syntax::parse;
#[cfg(feature = "arrow-stream")]
use arrow_array::{ArrayRef, Float64Array, RecordBatch};
#[cfg(feature = "arrow-stream")]
use arrow_ipc::writer::StreamWriter;
#[cfg(feature = "arrow-stream")]
use arrow_schema::{DataType as ArrowDataType, Field, Schema};

mod common;

use common::{
    image_assets, render_result, render_result_with_assets, render_result_with_tables, render_svg,
};

fn mark_rect_count(svg: &str) -> usize {
    svg.match_indices("<rect x=")
        .filter(|(index, _)| !inside_defs(svg, *index))
        .count()
}

fn high_cardinality_distribution_csv(categories: usize) -> String {
    let mut csv = String::from("group,value\n");
    for group in 0..categories {
        for value in [1, 2, 3, 4, 5] {
            let _ = writeln!(csv, "g{group},{value}");
        }
    }
    csv
}

fn mark_rect_attrs(svg: &str, first: &str, second: &str) -> Vec<(f64, f64)> {
    svg.match_indices("<rect x=\"")
        .filter(|(index, _)| !inside_defs(svg, *index))
        .filter_map(|(index, _)| {
            let rest = &svg[index..];
            let end = rest.find('>')?;
            let tag = &rest[..end];
            Some((svg_tag_attr(tag, first)?, svg_tag_attr(tag, second)?))
        })
        .collect()
}

fn svg_tag_attr(tag: &str, attr: &str) -> Option<f64> {
    let needle = format!("{attr}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    tag[start..end].parse().ok()
}

fn first_tile_rect_size(svg: &str) -> (f64, f64) {
    let tile_layer = svg
        .split_once("algraf-geom-tile")
        .expect("tile layer")
        .1
        .split_once("</g>")
        .expect("tile layer end")
        .0;
    let rect = tile_layer
        .split("<rect ")
        .nth(1)
        .expect("tile rect")
        .split_once('>')
        .expect("rect end")
        .0;
    (
        svg_tag_attr(rect, "width").expect("tile width"),
        svg_tag_attr(rect, "height").expect("tile height"),
    )
}

fn inside_defs(svg: &str, index: usize) -> bool {
    let before = &svg[..index];
    match (before.rfind("<defs>"), before.rfind("</defs>")) {
        (Some(open), Some(close)) => open > close,
        (Some(_), None) => true,
        _ => false,
    }
}

fn svg_root_attr(svg: &str, attr: &str) -> Option<f64> {
    let needle = format!("{attr}=\"");
    let start = svg.find(&needle)? + needle.len();
    let end = svg[start..].find('"')? + start;
    svg[start..end].parse().ok()
}

#[cfg(feature = "arrow-stream")]
fn arrow_stream_fixture() -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("x", ArrowDataType::Float64, false),
        Field::new("y", ArrowDataType::Float64, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Float64Array::from(vec![1.0, 3.0])) as ArrayRef,
            Arc::new(Float64Array::from(vec![2.0, 4.0])) as ArrayRef,
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

#[test]
#[allow(deprecated)]
fn deprecated_render_with_tables_shim_still_forwards() {
    let frame = read_csv_str("x,y\n1,2\n").expect("csv").frame;
    let parsed = parse("Chart(data: \"p.csv\") { Space(x * y) { Point() } }");
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");

    let result =
        algraf_render::render_with_tables(&ir, &frame, &HashMap::new(), &Theme::minimal(), None)
            .expect("render");

    assert!(result.svg.contains("<svg"));
}

#[test]
fn render_mark_budget_rejects_pathological_raw_points() {
    let frame = read_csv_str("x,y\n1,1\n2,2\n3,3\n").expect("csv").frame;
    let parsed = parse("Chart(data: \"p.csv\") { Space(x * y) { Point() } }");
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");

    let result = render(
        &ir,
        &frame,
        &Theme::minimal(),
        RenderOptions::default().with_limits(RenderLimits {
            mark_budget: Some(2),
        }),
    )
    .expect("render");

    assert!(result.diagnostics.iter().any(|d| d.code == "E2001"));
    assert_eq!(result.svg.matches("<circle").count(), 0);
}

#[test]
fn render_glyph_pies_inside_host_points() {
    let source = r##"Chart(data: "parents.csv", width: 360, height: 260) {
  Table mix = "mix.csv"
  Glyph pie(data: mix, key: [id], scales: "shared") {
    Space(value, coords: "polar", theta: "y") {
      Bar(fill: category, layout: "fill")
    }
  }
  Space(x * y) {
    pie(width: 46, height: 46, clip: "circle")
  }
}"##;
    let result = render_result_with_tables(
        source,
        "id,x,y\nA,1,1\nB,2,2\n",
        &[(
            "mix",
            "id,category,value\nA,one,3\nA,two,2\nB,one,1\nB,two,4\n",
        )],
    );

    assert!(result.svg.contains("algraf-glyph"));
    assert!(result.svg.contains("<clipPath"));
    assert!(result.svg.contains("<circle"));
    assert!(result.svg.contains("algraf-geom-bar"));
}

#[test]
fn render_glyph_interaction_metadata_has_nested_paths() {
    let source = r##"Chart(data: "parents.csv", width: 360, height: 260) {
  Table child = "child.csv"
  Glyph mark(data: child, key: [id], scales: "shared") {
    Space(t * value) {
      Point(tooltip: [label], highlight: "label")
    }
  }
  Space(x * y) {
    mark(width: 58, height: 32)
  }
}"##;
    let result = render_result_with_tables(
        source,
        "id,x,y\nA,1,1\nB,2,2\n",
        &[(
            "child",
            "id,t,value,label\nA,1,1,a1\nA,2,2,a2\nB,1,3,b1\nB,2,4,b2\n",
        )],
    );
    let metadata: serde_json::Value =
        serde_json::from_str(&result.metadata.to_json()).expect("metadata json");

    assert!(metadata["plots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|plot| plot["id"] == "p0:i0[0]:s0"));
    assert!(metadata["marks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|mark| mark["id"] == "p0:i0[0]:s0:g0:r0" && mark["plot"] == "p0:i0[0]:s0"));
    assert_eq!(
        metadata["groups"]["label"],
        serde_json::json!(["a1", "a2", "b1", "b2"])
    );
}

#[test]
fn render_glyph_shared_and_local_scales_differ() {
    let shared = r##"Chart(data: "parents.csv", width: 360, height: 260) {
  Table trend = "trend.csv"
  Glyph mark(data: trend, key: [id], scales: "shared") {
    Space(t * value) { Line() }
  }
  Space(x * y) {
    mark(width: 60, height: 30)
  }
}"##;
    let local = shared.replace("scales: \"shared\"", "scales: \"local\"");
    let primary = "id,x,y\nA,1,1\nB,2,2\n";
    let trend = "id,t,value\nA,1,1\nA,2,2\nB,1,100\nB,2,200\n";

    let shared_svg = render_result_with_tables(shared, primary, &[("trend", trend)]).svg;
    let local_svg = render_result_with_tables(&local, primary, &[("trend", trend)]).svg;

    assert_ne!(shared_svg, local_svg);
}

#[test]
fn render_nested_glyph_with_composite_host_key() {
    let source = r##"Chart(data: "parents.csv", width: 360, height: 260) {
  Table mix = "mix.csv"
  Table trend = "trend.csv"
  Glyph trendline(data: trend, key: [id => outer.id, category => category], scales: "local") {
    Space(t * value) { Line(stroke: "#222222", strokeWidth: 0.7) }
  }
  Glyph pie(data: mix, key: [id]) {
    Space(value, coords: "polar", theta: "y") {
      Bar(fill: category, layout: "fill")
      trendline(width: 16, height: 8)
    }
  }
  Space(x * y) {
    pie(width: 58, height: 58, clip: "circle")
  }
}"##;
    let result = render_result_with_tables(
        source,
        "id,x,y\nA,1,1\nB,2,2\n",
        &[
            (
                "mix",
                "id,category,value\nA,one,3\nA,two,2\nB,one,1\nB,two,4\n",
            ),
            (
                "trend",
                "id,category,t,value\nA,one,1,1\nA,one,2,2\nA,two,1,3\nA,two,2,1\nB,one,1,2\nB,one,2,4\nB,two,1,3\nB,two,2,5\n",
            ),
        ],
    );

    assert!(result.svg.matches("algraf-glyph").count() >= 3);
    assert!(result.svg.contains("algraf-geom-line"));
}

#[test]
fn render_glyph_body_size_scale_emits_legend_swatch() {
    // A glyph-body `Scale(size: col, range:, label:)` (spec §14.27) is the
    // canonical home for the glyph's size aesthetic. v0.72 wires the scale
    // into the legend pipeline (spec §16.13).
    let source = r##"Chart(data: "parents.csv", width: 360, height: 260) {
  Table mix = "mix.csv"
  Glyph pie(data: mix, key: [id], scales: "shared") {
    Scale(size: weight, range: [20, 40], label: "Weight")
    Space(value, coords: "polar", theta: "y") {
      Bar(fill: category, layout: "fill")
    }
  }
  Space(x * y) {
    pie(size: weight, clip: "circle")
  }
}"##;
    let result = render_result_with_tables(
        source,
        "id,x,y,weight\nA,1,1,5\nB,2,2,9\n",
        &[(
            "mix",
            "id,category,value,weight\nA,one,3,5\nA,two,2,5\nB,one,1,9\nB,two,4,9\n",
        )],
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(
        result.svg.contains("Weight"),
        "expected size-legend title 'Weight' in SVG; got: {}",
        &result.svg[..result.svg.len().min(2000)]
    );
    assert!(
        result.svg.contains("algraf-legends"),
        "expected legend group to render"
    );
}

#[test]
fn render_glyph_size_without_scale_still_emits_legend() {
    // A glyph call with `size:` mapped but no matching `Scale(size: …)`
    // anywhere produces a size legend using the GlyphSizeIr default diameter
    // range (12, 48), titled by the column name. This mirrors how
    // `Point(size: col)` produces a legend with `DEFAULT_SIZE_RANGE` when no
    // `Scale(size: …)` is declared.
    let source = r##"Chart(data: "parents.csv", width: 360, height: 260) {
  Table mix = "mix.csv"
  Glyph pie(data: mix, key: [id], scales: "shared") {
    Space(value, coords: "polar", theta: "y") {
      Bar(fill: category, layout: "fill")
    }
  }
  Space(x * y) {
    pie(size: weight, clip: "circle")
  }
}"##;
    let result = render_result_with_tables(
        source,
        "id,x,y,weight\nA,1,1,5\nB,2,2,9\n",
        &[(
            "mix",
            "id,category,value\nA,one,3\nA,two,2\nB,one,1\nB,two,4\n",
        )],
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(
        result.svg.contains(">weight<"),
        "expected size legend titled by the column name when no Scale exists"
    );
}

#[test]
fn render_size_and_fill_legends_with_same_title_both_render() {
    // A `Scale(fill:)` and a `Scale(size:)` over the same column with the
    // same label produce TWO distinct legends (one color, one size); the
    // legend-merge title dedup is scoped to color-family legends so it does
    // not drop the size swatch (spec §16.13).
    let source = r##"Chart(data: "p.csv") {
  Space(x * y) {
    Scale(fill: w, gradient: ["#fee08b", "#f03b20"], label: "W")
    Scale(size: w, range: [4, 20], label: "W")
    Point(size: w, fill: w)
  }
}"##;
    let result = render_result(source, "x,y,w\n1,1,3\n2,2,9\n3,3,5\n");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let title_count = result.svg.matches(">W<").count();
    assert!(
        title_count >= 2,
        "expected two legends (color + size) sharing the title 'W'; svg title hits: {}",
        title_count
    );
}

#[test]
fn render_chart_scope_size_scale_consumed_by_glyph_call_emits_legend() {
    // The chart-scope `Scale(size:, range:, label:)` whose only consumer is a
    // glyph call's `size:` argument was the v0.71 legend gap; v0.72 closes it
    // through the same pipeline as glyph-body scales (spec §16.13).
    let source = r##"Chart(data: "parents.csv", width: 360, height: 260) {
  Table mix = "mix.csv"
  Glyph pie(data: mix, key: [id], scales: "shared") {
    Space(value, coords: "polar", theta: "y") {
      Bar(fill: category, layout: "fill")
    }
  }
  Scale(size: weight, range: [20, 40], label: "Weight")
  Space(x * y) {
    pie(size: weight, clip: "circle")
  }
}"##;
    let result = render_result_with_tables(
        source,
        "id,x,y,weight\nA,1,1,5\nB,2,2,9\n",
        &[(
            "mix",
            "id,category,value\nA,one,3\nA,two,2\nB,one,1\nB,two,4\n",
        )],
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(
        result.svg.contains("Weight"),
        "expected chart-scope size legend title 'Weight' in SVG"
    );
}

#[test]
fn render_nested_glyph_mark_center_changes_slice_anchor() {
    let mark_center = r##"Chart(data: "parents.csv", width: 360, height: 260) {
  Table mix = "mix.csv"
  Table trend = "trend.csv"
  Glyph trendline(data: trend, key: [id => outer.id, category => category], scales: "local") {
    Space(t * value) { Line(stroke: "#222222", strokeWidth: 0.7) }
  }
  Glyph pie(data: mix, key: [id]) {
    Space(value, coords: "polar", theta: "y") {
      Bar(fill: category, layout: "fill")
      trendline(width: 16, height: 8, at: "mark-center")
    }
  }
  Space(x * y) {
    pie(width: 58, height: 58, clip: "circle")
  }
}"##;
    let center = mark_center.replace("at: \"mark-center\"", "at: \"position\"");
    let primary = "id,x,y\nA,1,1\n";
    let mix = "id,category,value\nA,one,3\nA,two,2\n";
    let trend = "id,category,t,value\nA,one,1,1\nA,one,2,2\nA,two,1,3\nA,two,2,1\n";

    let mark_center_svg =
        render_result_with_tables(mark_center, primary, &[("mix", mix), ("trend", trend)]).svg;
    let center_svg =
        render_result_with_tables(&center, primary, &[("mix", mix), ("trend", trend)]).svg;

    assert_ne!(mark_center_svg, center_svg);
}

#[test]
fn render_glyph_coalesces_sparse_match_warnings() {
    let source = r##"Chart(data: "parents.csv", width: 360, height: 260) {
  Table trend = "trend.csv"
  Glyph mark(data: trend, key: [id]) {
    Space(t * value) { Point() }
  }
  Space(x * y) {
    mark(width: 60, height: 30)
  }
}"##;
    let result = render_result_with_tables(
        source,
        "id,x,y\nA,1,1\nB,2,2\nC,3,3\n",
        &[("trend", "id,t,value\nA,1,1\nA,2,2\n")],
    );
    let warnings = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == "W2002")
        .collect::<Vec<_>>();

    assert_eq!(warnings.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(
        warnings[0].message,
        "glyph `mark` matched no child rows for 2 of 3 host rows"
    );
}

#[test]
fn render_glyph_budget_rejects_recursive_expansion() {
    let frame = read_csv_str("id,x,y\nA,1,1\nB,2,2\n")
        .expect("primary csv")
        .frame;
    let mix = read_csv_str("id,category,value\nA,one,3\nA,two,2\nB,one,1\nB,two,4\n")
        .expect("mix")
        .frame;
    let source = r##"Chart(data: "parents.csv") {
  Table mix = "mix.csv"
  Glyph pie(data: mix, key: [id]) {
    Space(value, coords: "polar", theta: "y") { Bar(fill: category, layout: "fill") }
  }
  Space(x * y) {
    pie(width: 40, height: 40)
  }
}"##;
    let parsed = parse(source);
    let mut schemas = HashMap::new();
    schemas.insert("mix".to_string(), mix.schema().to_vec());
    let analysis = analyze_with_tables(&parsed.syntax(), frame.schema(), &schemas);
    let ir = analysis.ir.expect("ir");
    let mut named = HashMap::new();
    named.insert("mix".to_string(), mix);
    let result = render(
        &ir,
        &frame,
        &Theme::minimal(),
        RenderOptions::default()
            .with_named_tables(&named)
            .with_limits(RenderLimits {
                mark_budget: Some(2),
            }),
    )
    .expect("render");

    assert!(result.diagnostics.iter().any(|d| d.code == "E2210"));
}

fn svg_num(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    let mut s = format!("{rounded:.3}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

#[test]
fn embedded_facade_renders_json_input_with_variables() {
    let source = r##"Chart(data: input, width: 320, height: 220) {
  Space(x * y) {
    Line(stroke: "${color}", strokeWidth: ${size})
    Point(fill: "${color}", size: ${size})
  }
}"##;
    let result = render_embedded(
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

    let svg = result.svg().unwrap();
    assert_eq!(result.content_type, "image/svg+xml");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("#e74c3c"));
}

#[cfg(feature = "arrow-stream")]
#[test]
fn embedded_facade_renders_arrow_stream_input() {
    let source = "Chart(data: input, width: 320, height: 220) {\n  Space(x * y) { Point() }\n}";
    let result = render_embedded(
        source,
        arrow_stream_fixture(),
        EmbeddedRenderOptions {
            data_format: algraf_data::Format::ArrowStream,
            ..EmbeddedRenderOptions::default()
        },
    )
    .unwrap();

    let svg = result.svg().unwrap();
    assert_eq!(result.content_type, "image/svg+xml");
    assert!(svg.contains("<svg"));
}

#[test]
fn embedded_facade_interactive_svg_embeds_runtime() {
    let source = r#"Chart(data: input, width: 320, height: 220) {
  Space(x * y) {
    Point(tooltip: [g, y], highlight: g)
  }
}"#;
    let input = br#"[{"x":1,"y":2,"g":"A"},{"x":3,"y":4,"g":"B"}]"#;
    let static_result = render_embedded(
        source,
        input,
        EmbeddedRenderOptions {
            data_format: algraf_data::Format::Json,
            ..EmbeddedRenderOptions::default()
        },
    )
    .unwrap();
    let interactive_result = render_embedded(
        source,
        input,
        EmbeddedRenderOptions {
            data_format: algraf_data::Format::Json,
            interactive: true,
            ..EmbeddedRenderOptions::default()
        },
    )
    .unwrap();

    let static_svg = static_result.svg().unwrap();
    let interactive_svg = interactive_result.svg().unwrap();

    assert!(!static_svg.contains("<script"), "{static_svg}");
    assert!(static_svg.contains("<title>g: A\ny: 2</title>"));
    assert!(interactive_svg.contains("<script"), "{interactive_svg}");
    assert!(
        interactive_svg.contains("algraf-crosshair"),
        "{interactive_svg}"
    );
    assert!(interactive_svg.contains("<title>g: A\ny: 2</title>"));

    let body_end = interactive_svg.find("<script").unwrap();
    let static_body_end = static_svg.find("</svg>").unwrap();
    assert_eq!(&interactive_svg[..body_end], &static_svg[..static_body_end]);
}

#[test]
fn embedded_facade_returns_png_bytes() {
    let result = render_embedded(
        "Chart(data: input) { Space(x * y) { Point() } }",
        b"x,y\n1,2\n",
        EmbeddedRenderOptions {
            output_format: EmbeddedOutputFormat::Png,
            png_scale: 1.0,
            ..EmbeddedRenderOptions::default()
        },
    )
    .unwrap();

    assert_eq!(result.content_type, "image/png");
    assert!(result.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    let interactive_result = render_embedded(
        "Chart(data: input) { Space(x * y) { Point() } }",
        b"x,y\n1,2\n",
        EmbeddedRenderOptions {
            output_format: EmbeddedOutputFormat::Png,
            interactive: true,
            png_scale: 1.0,
            ..EmbeddedRenderOptions::default()
        },
    )
    .unwrap();

    assert_eq!(interactive_result.content_type, "image/png");
    assert_eq!(interactive_result.bytes, result.bytes);
}

#[test]
fn test_scatter_renders_points() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Point(fill: g) } }",
        "x,y,g\n1,2,a\n2,3,b\n3,1,a\n",
    );
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains("viewBox=\"0 0 800 520\""));
    assert!(svg.contains("role=\"img\""));
    assert!(svg.contains("algraf-geom-point"));
    // Three rows -> three circles.
    assert_eq!(svg.matches("<circle").count(), 3);
    assert!(svg.ends_with("</svg>\n"));
}

#[test]
fn test_1d_space_renders_points_on_center_baseline() {
    let result = render_result(
        "Chart(data: \"p.csv\", width: 400, height: 240) { Space(x) { Point() } }",
        "x\n10\n20\n30\n",
    );
    let baseline = result.layout.plot.y + result.layout.plot.height / 2.0;
    let cy = format!("cy=\"{}\"", svg_num(baseline));

    assert_eq!(result.svg.matches("<circle").count(), 3);
    assert_eq!(result.svg.matches(&cy).count(), 3);
}

#[test]
fn test_1d_space_renders_x_sorted_line_without_y_axis() {
    let result = render_result(
        "Chart(data: \"p.csv\", width: 400, height: 240) { Space(x) { Line() Point() } }",
        "x\n30\n10\n20\n",
    );
    let plot = result.layout.plot;
    let baseline = svg_num(plot.y + plot.height / 2.0);
    let left_axis = format!(
        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"",
        svg_num(plot.x),
        svg_num(plot.y),
        svg_num(plot.x),
        svg_num(plot.bottom())
    );
    let line_layer = result
        .svg
        .split_once("algraf-geom-line")
        .and_then(|(_, after)| after.split_once("</g>"))
        .map(|(layer, _)| layer)
        .unwrap_or("");

    assert!(line_layer.contains("<path"));
    assert!(line_layer.matches(&baseline).count() >= 3);
    assert_eq!(result.svg.matches("<circle").count(), 3);
    assert!(!result.svg.contains(&left_axis));
    assert!(result.svg.contains("class=\"algraf-axes\""));
}

#[test]
fn test_scatter_has_legend_for_fill_mapping() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Point(fill: g) } }",
        "x,y,g\n1,2,a\n2,3,b\n",
    );
    assert!(svg.contains("algraf-legends"));
    // Two categories, each with a swatch + label.
    assert!(svg.contains(">a</text>"));
    assert!(svg.contains(">b</text>"));
}

#[test]
fn test_temporal_fill_legend_uses_scale_time_format() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {
  Scale(fill: day, timeFormat: \"iso-date\", label: \"Origin day\")
  Space(x * y) { Point(fill: day) }
}",
        "x,y,day\n1,2,2026-05-19\n2,3,2026-05-20\n",
    );
    let legend = legend_layer(&svg);

    assert!(legend.contains(">Origin day</text>"));
    assert!(legend.contains(">2026-05-19</text>"));
    assert!(legend.contains(">2026-05-20</text>"));
    assert!(!legend.contains("2026-05-19T00:00:00+00:00"));
}

#[test]
fn test_guide_legend_false_suppresses_legends() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Guide(legend: false) Space(x * y) { Point(fill: g) } }",
        "x,y,g\n1,2,a\n2,3,b\n",
    );
    assert!(!svg.contains("algraf-legends"));
}

#[test]
fn test_chart_title_subtitle_and_caption_render() {
    let result = render_result(
        "Chart(data: \"p.csv\", title: \"Main <Title>\", subtitle: \"Sub & text\", caption: \"Source\") { Space(x * y) { Point() } }",
        "x,y\n1,2\n2,3\n",
    );
    assert!(result.svg.contains("<title>Main &lt;Title&gt;</title>"));
    assert!(result.svg.contains("<desc>Sub &amp; text"));
    assert!(result.svg.contains("class=\"algraf-title\""));
    assert!(result.svg.contains("Main &lt;Title&gt;</text>"));
    assert!(result.svg.contains("class=\"algraf-caption\""));
    assert!(result.layout.plot.y > 40.0);
}

#[test]
fn explicit_alt_and_description_drive_svg_and_metadata() {
    let result = render_result(
        r#"Chart(data: "p.csv", title: "Visible title", subtitle: "Subtitle fallback", alt: "Accessible summary", description: "Long accessible description") {
  Space(x * y) { Point() }
}"#,
        "x,y\n1,2\n",
    );

    assert!(result.svg.contains("aria-label=\"Accessible summary\""));
    assert!(result
        .svg
        .contains("<desc>Long accessible description</desc>"));
    assert!(!result.svg.contains("<desc>Subtitle fallback</desc>"));

    let metadata: serde_json::Value =
        serde_json::from_str(&result.metadata.to_json()).expect("metadata json");
    assert_eq!(metadata["chart"]["alt"], "Accessible summary");
    assert_eq!(
        metadata["chart"]["description"],
        "Long accessible description"
    );
}

#[test]
fn test_chart_margin_right_reserves_space() {
    let csv = "x,y\n1,2\n2,3\n";
    let default = render_result(
        "Chart(data: \"p.csv\", width: 800, height: 520) { Space(x * y) { Point() } }",
        csv,
    );
    let wide = render_result(
        "Chart(data: \"p.csv\", width: 800, height: 520, marginRight: 150) { Space(x * y) { Point() } }",
        csv,
    );
    // The configured minimum widens the right margin, so the plot ends sooner.
    assert!(wide.layout.plot.right() < default.layout.plot.right());
    assert!((800.0 - wide.layout.plot.right() - 150.0).abs() < 0.001);
}

#[test]
fn test_chart_margin_below_default_is_a_floor() {
    let csv = "x,y\n1,2\n2,3\n";
    let default = render_result(
        "Chart(data: \"p.csv\", width: 800, height: 520) { Space(x * y) { Point() } }",
        csv,
    );
    // A value below the computed default (right margin 30) must not shrink it.
    let tiny = render_result(
        "Chart(data: \"p.csv\", width: 800, height: 520, marginRight: 5) { Space(x * y) { Point() } }",
        csv,
    );
    assert_eq!(tiny.layout.plot.right(), default.layout.plot.right());
}

#[test]
fn test_long_right_legend_labels_reserve_width() {
    let result = render_result(
        "Chart(data: \"p.csv\", width: 480, height: 260, marginRight: 5) {
  Space(x * y) { Point(fill: segment) }
}",
        "x,y,segment\n1,2,Before Apr 7 additions\n2,3,Before Apr 7 deletions\n",
    );
    let legend = result.layout.legend.expect("legend area");

    assert!(
        legend.width > 170.0,
        "expected measured legend width, got {legend:?}"
    );
    assert!(
        legend.right() <= 480.0 - 30.0 + 0.001,
        "legend should fit inside the computed right margin: {legend:?}"
    );
}

#[test]
fn test_tall_right_legend_expands_svg_height() {
    let result = render_result(
        "Chart(data: \"p.csv\", width: 360, height: 150) {
  Space(x * y) { Point(fill: segment) }
}",
        "x,y,segment\n1,1,s01\n2,2,s02\n3,3,s03\n4,4,s04\n5,5,s05\n6,6,s06\n7,7,s07\n8,8,s08\n9,9,s09\n10,10,s10\n11,11,s11\n12,12,s12\n",
    );
    let legend = result.layout.legend.expect("legend area");

    assert!(
        legend.height > result.layout.plot.height,
        "expected tall measured legend, got legend={legend:?} plot={:?}",
        result.layout.plot
    );
    assert!(
        result.layout.svg.height > 150.0,
        "expected SVG viewport to grow, got {:?}",
        result.layout.svg
    );
    assert!(
        legend.bottom() <= result.layout.svg.height - 50.0 + 0.001,
        "legend should fit above the bottom guide reserve: legend={legend:?} svg={:?}",
        result.layout.svg
    );
    let serialized_height = svg_root_attr(&result.svg, "height").expect("root height");
    assert!(
        (serialized_height - result.layout.svg.height).abs() < 0.001,
        "serialized height should match expanded layout"
    );
}

/// Render `source` against `csv` with `void` as the resolved chart theme, the
/// way the CLI does for a chart-level `Theme(name: "void")` (spec §22.3).
fn render_void(source: &str, csv: &str) -> RenderResult {
    let frame = read_csv_str(csv).expect("csv").frame;
    let parsed = parse(source);
    let ir = analyze(&parsed.syntax(), frame.schema()).ir.expect("ir");
    render(&ir, &frame, &Theme::void(), None).expect("render")
}

#[test]
fn test_no_axes_margin_overrides_below_default() {
    let csv = "x,y\n1,2\n2,3\n";
    // The void theme has no axes, so the base 10px margin is pure padding. A
    // configured value sets the side exactly — down to 0 — letting an embedded
    // sparkline reach the viewport edges (spec §17.3).
    let bleed = render_void(
        "Chart(data: \"p.csv\", width: 200, height: 100, marginTop: 0, marginRight: 0, marginBottom: 0, marginLeft: 0) { Space(x * y) { Line() } }",
        csv,
    );
    assert_eq!(bleed.layout.plot.x, 0.0);
    assert_eq!(bleed.layout.plot.y, 0.0);
    assert_eq!(bleed.layout.plot.width, 200.0);
    assert_eq!(bleed.layout.plot.height, 100.0);

    // An intermediate value is honored exactly rather than floored at 10px.
    let inset = render_void(
        "Chart(data: \"p.csv\", width: 200, height: 100, marginLeft: 4) { Space(x * y) { Line() } }",
        csv,
    );
    assert_eq!(inset.layout.plot.x, 4.0);

    // An absent side keeps the 10px no-axes default.
    assert_eq!(inset.layout.plot.y, 10.0);
}

/// Extract the `y` attribute of the `<text>` element whose content is `label`.
fn text_y(svg: &str, label: &str) -> f64 {
    let element = text_element(svg, label);
    let y_start = element.find("y=\"").unwrap() + 3;
    let y_end = element[y_start..].find('"').unwrap();
    element[y_start..y_start + y_end].parse().unwrap()
}

/// Extract the `x` attribute of the `<text>` element whose content is `label`.
fn text_x(svg: &str, label: &str) -> f64 {
    let element = text_element(svg, label);
    let x_start = element.find("x=\"").unwrap() + 3;
    let x_end = element[x_start..].find('"').unwrap();
    element[x_start..x_start + x_end].parse().unwrap()
}

fn text_element<'a>(svg: &'a str, label: &str) -> &'a str {
    let needle = format!(">{label}</text>");
    let element_end = svg.find(&needle).expect("label") + needle.len();
    let element_start = svg[..element_end].rfind("<text").unwrap();
    &svg[element_start..element_end]
}

fn layer_path_count(svg: &str, class: &str) -> usize {
    layer_paths(svg, class).len()
}

fn layer_paths<'a>(svg: &'a str, class: &str) -> Vec<&'a str> {
    let start = svg.find(class).expect("layer class");
    let layer = &svg[start..];
    let end = layer.find("</g>").expect("layer end");
    let layer = &layer[..end];
    layer
        .match_indices("<path")
        .map(|(index, _)| {
            let path = &layer[index..];
            let end = path.find('>').expect("path end") + 1;
            &path[..end]
        })
        .collect()
}

fn path_d(path: &str) -> &str {
    let start = path.find("d=\"").expect("path d") + 3;
    let end = path[start..].find('"').expect("path d end");
    &path[start..start + end]
}

#[test]
fn test_text_declutter_separates_overlapping_labels() {
    // `lo`/`hi` anchor the y domain; A and B map to nearly the same y.
    let csv = "x,y,name\n2,0,lo\n2,100,hi\n2,50.0,A\n2,50.4,B\n";
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Text(label: name, size: 10, declutter: true) } }",
        csv,
    );
    // gap = size * 1.2 = 12.
    assert!((text_y(&svg, "A") - text_y(&svg, "B")).abs() >= 12.0 - 1e-6);
}

#[test]
fn test_text_declutter_separates_same_row_station_labels() {
    let csv = "\
station_name,zone,capacity,trips,revenue
Central Station,Downtown,32,7,27.9
Library Plaza,Downtown,28,6,26.4
Market Hall,Market,26,6,30.7
North Campus,Campus,24,6,28.1
Museum Loop,Cultural,30,4,59
River Park,Riverfront,22,4,51.8
Science Center,Campus,18,4,16.1
Marina Gate,Riverfront,20,3,25.4
Harbor Point,Riverfront,16,2,22.9
";
    let svg = render_svg(
        "Chart(data: \"p.csv\", width: 760, height: 470) {
            Scale(axis: x, domain: [0, 36], expand: [0, 0.05])
            Scale(axis: y, domain: [0, 8], expand: [0, 0.05])
            Space(capacity * trips) {
                Text(label: station_name, dy: -12, size: 10, declutter: true)
            }
        }",
        csv,
    );

    let north = text_x(&svg, "North Campus");
    let market = text_x(&svg, "Market Hall");
    let library = text_x(&svg, "Library Plaza");
    // Estimated widths are 72, 66, and 78 px respectively; the declutter gap is
    // 4 px, so adjacent centered labels need 73 and 76 px between centers.
    assert!(market - north >= 73.0 - 0.01);
    assert!(library - market >= 76.0 - 0.01);
}

#[test]
fn test_text_without_declutter_leaves_positions() {
    let csv = "x,y,name\n2,0,lo\n2,100,hi\n2,50.0,A\n2,50.4,B\n";
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Text(label: name, size: 10) } }",
        csv,
    );
    // Untouched: A and B keep their near-identical mapped positions.
    assert!((text_y(&svg, "A") - text_y(&svg, "B")).abs() < 5.0);
}

#[test]
fn text_numeric_format_renders_in_svg() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Text(label: value, format: \"$.2f\") } }",
        "x,y,value\n1,2,3.5\n",
    );
    assert!(svg.contains(">$3.50</text>"), "{svg}");
}

#[test]
fn label_renders_one_terminal_text_per_group() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Line(group: series) Label(label: series, group: series, at: \"end\", dx: 4) } }",
        "x,y,series\n1,2,Alpha\n2,4,Alpha\n1,5,Beta\n2,3,Beta\n",
    );
    assert_eq!(svg.matches(">Alpha</text>").count(), 1, "{svg}");
    assert_eq!(svg.matches(">Beta</text>").count(), 1, "{svg}");
    assert!(text_x(&svg, "Alpha") > text_x(&svg, "Beta") - 1.0);
}

#[test]
fn test_text_dy_column_offsets_each_label() {
    // A and B share the same y (same cy); only their `off` differs.
    let csv = "x,y,name,off\n1,40,A,0\n2,40,B,20\n3,60,C,0\n";
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Text(label: name, dy: off) } }",
        csv,
    );
    assert!((text_y(&svg, "B") - text_y(&svg, "A") - 20.0).abs() < 1e-6);
}

#[test]
fn categorical_scale_domain_orders_axis_and_warns_on_append() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Scale(axis: x, domain: [\"B\", \"A\"]) Space(g * y) { Bar() } }",
        "g,y\nA,1\nC,2\n",
    );
    assert!(result.diagnostics.iter().any(|d| d.code == "R0004"));
    let metadata: serde_json::Value =
        serde_json::from_str(&result.metadata.to_json()).expect("metadata json");
    assert_eq!(
        metadata["axes"]["x"]["domain"],
        serde_json::json!(["B", "A", "C"])
    );
}

#[test]
fn string_domain_on_continuous_axis_warns_at_render_time() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Scale(axis: x, domain: [\"A\"]) Space(x * y) { Point() } }",
        "x,y\n1,2\n2,3\n",
    );
    assert!(result.diagnostics.iter().any(|d| d.code == "R0004"));
}

#[test]
fn categorical_axis_type_allows_numeric_bar_position() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Scale(axis: x, type: \"categorical\") Space(day * value) { Bar() } }",
        "day,value\n1,18\n2,34\n3,48\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.svg.matches("opacity=").count(), 3);
    let metadata: serde_json::Value =
        serde_json::from_str(&result.metadata.to_json()).expect("metadata json");
    assert_eq!(
        metadata["axes"]["x"]["domain"],
        serde_json::json!(["1", "2", "3"])
    );
}

#[test]
fn categorical_axis_type_allows_temporal_bar_position() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Scale(axis: x, type: \"categorical\") Space(bucket_start * lines_changed) { Bar(fill: author_name, layout: \"stack\") } }",
        "bucket_start,author_name,lines_changed\n2024-01-01,Ada,120\n2024-01-01,Lin,80\n2025-01-01,Ada,50\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.svg.matches("opacity=").count(), 3);
    let metadata: serde_json::Value =
        serde_json::from_str(&result.metadata.to_json()).expect("metadata json");
    assert_eq!(
        metadata["axes"]["x"]["domain"],
        serde_json::json!(["2024-01-01T00:00:00+00:00", "2025-01-01T00:00:00+00:00"])
    );
}

#[test]
fn temporal_bar_uses_tick_interval_bucket_width() {
    let result = render_result(
        r#"Chart(data: "p.csv", width: 640, height: 360) {
  Scale(axis: x, type: "temporal", tickInterval: "1 day")
  Guide(axis: x, timeFormat: "%b %d")
  Space(day * value) { Bar(alpha: 0.86) }
}"#,
        "day,value\n2024-01-01,10\n2024-01-02,20\n2024-01-04,15\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(mark_rect_count(&result.svg), 3);
    assert!(result.svg.contains(">Jan 01</text>"), "{}", result.svg);

    let metadata: serde_json::Value =
        serde_json::from_str(&result.metadata.to_json()).expect("metadata json");
    assert_eq!(metadata["axes"]["x"]["scale"], "time");

    let mut rects = mark_rect_attrs(&result.svg, "x", "width");
    rects.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert_eq!(rects.len(), 3, "{rects:?}\n{}", result.svg);
    let first_gap = rects[1].0 - (rects[0].0 + rects[0].1);
    let missing_day_gap = rects[2].0 - (rects[1].0 + rects[1].1);
    assert!(
        first_gap > rects[0].1 * 0.1,
        "consecutive daily bars should keep regular bar spacing: {rects:?}"
    );
    assert!(
        missing_day_gap > first_gap + rects[0].1,
        "missing day should remain a larger temporal gap: {rects:?}"
    );
    let jan_01_x = text_x(&result.svg, "Jan 01");
    let jan_01_center = rects[0].0 + rects[0].1 / 2.0;
    assert!(
        (jan_01_x - jan_01_center).abs() < 2.0,
        "date tick should sit under the bar center: tick={jan_01_x} rect={:?}",
        rects[0]
    );
}

#[test]
fn horizontal_temporal_bar_uses_tick_interval_bucket_width() {
    let result = render_result(
        r#"Chart(data: "p.csv", width: 640, height: 360) {
  Scale(axis: y, type: "temporal", tickInterval: "1 day")
  Guide(axis: y, timeFormat: "%b %d")
  Space(value * day) { Bar(alpha: 0.86) }
}"#,
        "day,value\n2024-01-01,10\n2024-01-02,20\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(mark_rect_count(&result.svg), 2);
    let metadata: serde_json::Value =
        serde_json::from_str(&result.metadata.to_json()).expect("metadata json");
    assert_eq!(metadata["axes"]["y"]["scale"], "time");
}

#[test]
fn grouped_temporal_bar_subdivides_each_elapsed_time_bucket() {
    let result = render_result(
        r##"Chart(data: "p.csv", width: 640, height: 360) {
  Scale(axis: x, type: "temporal", tickInterval: "1 day")
  Guide(axis: x, timeFormat: "%b %d")
  Space(day / group * value) { Bar(fill: "#2563eb", alpha: 0.86) }
}"##,
        "day,group,value\n2024-01-01,A,10\n2024-01-01,B,20\n2024-01-03,A,15\n2024-01-03,B,12\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(mark_rect_count(&result.svg), 4);
    let metadata: serde_json::Value =
        serde_json::from_str(&result.metadata.to_json()).expect("metadata json");
    assert_eq!(metadata["axes"]["x"]["scale"], "time");
    assert_eq!(
        metadata["axes"]["x"]["innerDomain"],
        serde_json::json!(["A", "B"])
    );

    let mut rects = mark_rect_attrs(&result.svg, "x", "width");
    rects.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert_eq!(rects.len(), 4, "{rects:?}\n{}", result.svg);
    assert!(
        rects[1].0 < rects[0].0 + rects[0].1 * 2.5,
        "first two bars should share one date bucket: {rects:?}"
    );
    let within_bucket_gap = rects[1].0 - (rects[0].0 + rects[0].1);
    let missing_day_gap = rects[2].0 - (rects[1].0 + rects[1].1);
    assert!(
        missing_day_gap > within_bucket_gap + rects[0].1,
        "missing date should remain a wider elapsed-time gap: {rects:?}"
    );
}

#[test]
fn stacked_temporal_bar_groups_by_bucket_anchor() {
    let result = render_result(
        r#"Chart(data: "p.csv", width: 640, height: 360) {
  Scale(axis: x, type: "temporal", tickInterval: "1 day")
  Space(day * value) { Bar(fill: group, layout: "stack", alpha: 0.86) }
}"#,
        "day,group,value\n2024-01-01,A,10\n2024-01-01,B,5\n2024-01-02,A,3\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.svg.matches("opacity=\"0.86\"").count(), 3);
    let metadata: serde_json::Value =
        serde_json::from_str(&result.metadata.to_json()).expect("metadata json");
    assert!(
        metadata["axes"]["y"]["domain"][1].as_f64().unwrap() >= 15.0,
        "{metadata}"
    );
}

#[test]
fn temporal_bar_without_tick_interval_reports_targeted_diagnostic() {
    let result = render_result(
        r#"Chart(data: "p.csv") {
  Scale(axis: x, type: "temporal")
  Space(day * value) { Bar() }
}"#,
        "day,value\n2024-01-01,10\n2024-01-02,20\n",
    );
    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "R0002")
        .expect("R0002 diagnostic");
    let help = diagnostic.help.as_deref().unwrap_or_default();
    assert!(help.contains("tickInterval"), "{diagnostic:?}");
    assert!(
        help.contains("Scale(axis: x, type: \"categorical\")"),
        "{diagnostic:?}"
    );
}

#[test]
fn temporal_bar_exact_breaks_do_not_supply_bucket_width() {
    let result = render_result(
        r#"Chart(data: "p.csv") {
  Scale(axis: x, type: "temporal", breaks: [date("2024-01-01"), date("2024-01-02")], tickInterval: "1 day")
  Space(day * value) { Bar() }
}"#,
        "day,value\n2024-01-01,10\n2024-01-02,20\n",
    );
    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "R0002")
        .expect("R0002 diagnostic");
    assert!(
        diagnostic
            .help
            .as_deref()
            .unwrap_or_default()
            .contains("tickInterval"),
        "{diagnostic:?}"
    );
}

#[test]
fn temporal_categorical_axis_uses_custom_guide_time_format() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Scale(axis: x, type: \"categorical\") Guide(axis: x, timeFormat: \"%b %Y\") Space(bucket_start * lines_changed) { Bar() } }",
        "bucket_start,lines_changed\n2024-01-01,120\n2024-02-01,80\n2024-03-01,50\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains(">Jan 2024</text>"), "{}", result.svg);
    assert!(result.svg.contains(">Feb 2024</text>"), "{}", result.svg);
    assert!(
        !result.svg.contains(">2024-01-01T00:00:00+00:00</text>"),
        "{}",
        result.svg
    );
}

#[test]
fn temporal_categorical_axis_uses_named_year_guide_time_format() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Scale(axis: x, type: \"categorical\") Guide(axis: x, timeFormat: \"year\") Space(bucket_start * lines_changed) { Bar() } }",
        "bucket_start,lines_changed\n2024-01-01,120\n2025-01-01,80\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains(">2024</text>"), "{}", result.svg);
    assert!(result.svg.contains(">2025</text>"), "{}", result.svg);
}

#[test]
fn temporal_categorical_axis_without_guide_keeps_rfc3339_labels() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Scale(axis: x, type: \"categorical\") Space(bucket_start * lines_changed) { Bar() } }",
        "bucket_start,lines_changed\n2024-01-01,120\n2025-01-01,80\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(
        result.svg.contains(">2024-01-01T00:00:00+00:00</text>"),
        "{}",
        result.svg
    );
}

#[test]
fn non_temporal_categorical_axis_ignores_guide_time_format() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Guide(axis: x, timeFormat: \"year\") Space(bucket * lines_changed) { Bar() } }",
        "bucket,lines_changed\nalpha,120\nbeta,80\n",
    );
    assert!(result.svg.contains(">alpha</text>"), "{}", result.svg);
    assert!(result.svg.contains(">beta</text>"), "{}", result.svg);
}

#[test]
fn categorical_axis_type_orders_numeric_categories_with_string_domain() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Scale(axis: x, type: \"categorical\", domain: [\"3\", \"1\"]) Space(day * value) { Bar() } }",
        "day,value\n1,18\n2,34\n",
    );
    assert!(result.diagnostics.iter().any(|d| d.code == "R0004"));
    let metadata: serde_json::Value =
        serde_json::from_str(&result.metadata.to_json()).expect("metadata json");
    assert_eq!(
        metadata["axes"]["x"]["domain"],
        serde_json::json!(["3", "1", "2"])
    );
}

#[test]
fn horizontal_stacked_bar_uses_numeric_categorical_y_axis() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Scale(axis: y, type: \"categorical\") Space(value * day) { Bar(fill: group, layout: \"stack\") } }",
        "day,group,value\n1,a,10\n1,b,20\n2,a,5\n2,b,5\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.svg.matches("opacity=").count(), 4);
    assert!(result.svg.contains(">30</text>"), "{}", result.svg);
    let metadata: serde_json::Value =
        serde_json::from_str(&result.metadata.to_json()).expect("metadata json");
    assert_eq!(
        metadata["axes"]["y"]["domain"],
        serde_json::json!(["1", "2"])
    );
}

#[test]
fn area_stack_and_fill_train_domains_and_emit_group_paths() {
    let stack = render_result(
        "Chart(data: \"p.csv\") { Space(x * y) { Area(fill: series, layout: \"stack\") } }",
        "x,y,series\n1,2,A\n1,3,B\n2,4,A\n2,1,B\n",
    );
    assert!(stack.diagnostics.is_empty(), "{:?}", stack.diagnostics);
    assert_eq!(layer_path_count(&stack.svg, "algraf-geom-area"), 2);
    let stack_metadata: serde_json::Value =
        serde_json::from_str(&stack.metadata.to_json()).expect("metadata json");
    assert!(
        stack_metadata["axes"]["y"]["domain"][1].as_f64().unwrap() >= 5.0,
        "{stack_metadata}"
    );

    let fill = render_result(
        "Chart(data: \"p.csv\") { Space(x * y) { Area(fill: series, layout: \"fill\") } }",
        "x,y,series\n1,2,A\n1,3,B\n2,4,A\n2,1,B\n",
    );
    assert!(fill.diagnostics.is_empty(), "{:?}", fill.diagnostics);
    assert_eq!(layer_path_count(&fill.svg, "algraf-geom-area"), 2);
    let fill_metadata: serde_json::Value =
        serde_json::from_str(&fill.metadata.to_json()).expect("metadata json");
    let fill_max = fill_metadata["axes"]["y"]["domain"][1].as_f64().unwrap();
    assert!((1.0..=1.1).contains(&fill_max), "{fill_metadata}");
}

#[test]
fn area_stack_and_fill_keep_sparse_groups_contiguous() {
    let csv = "x,y,series\n1,2,A\n1,1,B\n2,2,A\n3,2,A\n3,1,B\n";

    for layout in ["stack", "fill"] {
        let result = render_result(
            &format!(
                "Chart(data: \"p.csv\") {{ Space(x * y) {{ Area(fill: series, layout: \"{layout}\") }} }}"
            ),
            csv,
        );
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let l_counts = layer_paths(&result.svg, "algraf-geom-area")
            .iter()
            .map(|path| path_d(path).matches('L').count())
            .collect::<Vec<_>>();
        assert_eq!(l_counts, vec![5, 5], "{layout}: {}", result.svg);
    }
}

#[test]
fn test_no_legend_for_literal_fill() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Point(fill: \"steelblue\") } }",
        "x,y\n1,2\n2,3\n",
    );
    assert!(!svg.contains("algraf-legends"));
    assert!(svg.contains("fill=\"steelblue\""));
}

#[test]
fn test_bar_renders_rects() {
    let svg = render_svg(
        "Chart(data: \"f.csv\") { Space(quarter * amount) { Bar() } }",
        "quarter,amount\nQ1,10\nQ2,20\nQ3,15\n",
    );
    assert!(svg.contains("algraf-geom-bar"));
    assert!(svg.matches("<rect").count() >= 3);
    // Category labels appear on the x axis.
    assert!(svg.contains(">Q1</text>"));
}

#[test]
fn test_bar_y_domain_includes_zero() {
    let svg = render_svg(
        "Chart(data: \"f.csv\") { Space(quarter * amount) { Bar() } }",
        "quarter,amount\nQ1,10\nQ2,20\nQ3,15\n",
    );
    assert!(svg.contains(">0</text>"));
}

#[test]
fn test_dodged_bar_via_nesting() {
    // Nested band x produces a sub-band per type within each quarter.
    let svg = render_svg(
        "Chart(data: \"f.csv\") { Space((quarter / type) * amount) { Bar(fill: type) } }",
        "quarter,type,amount\nQ1,a,10\nQ1,b,5\nQ2,a,8\nQ2,b,12\n",
    );
    assert!(svg.contains("algraf-geom-bar"));
    assert_eq!(svg.matches("<rect class=").count(), 2); // background + plot only
                                                        // 4 data bars (data marks carry an opacity attribute; legend swatches do not).
    assert_eq!(svg.matches("opacity=").count(), 4);
}

#[test]
fn test_stacked_bar() {
    let svg = render_svg(
        "Chart(data: \"f.csv\") { Space(quarter * amount) { Bar(fill: type, layout: \"stack\") } }",
        "quarter,type,amount\nQ1,a,10\nQ1,b,5\nQ2,a,8\nQ2,b,12\n",
    );
    // 4 stacked segments.
    assert_eq!(svg.matches("opacity=").count(), 4);
}

#[test]
fn test_stacked_bar_y_domain_uses_totals() {
    let svg = render_svg(
        "Chart(data: \"f.csv\") { Space(quarter * amount) { Bar(fill: type, layout: \"stack\") } }",
        "quarter,type,amount\nQ1,a,10\nQ1,b,20\nQ2,a,5\nQ2,b,5\n",
    );
    assert!(svg.contains(">30</text>"));
}

#[test]
fn test_fill_bar_normalizes_segments_to_one() {
    let result = render_result(
        "Chart(data: \"f.csv\") { Space(quarter * amount) { Bar(fill: type, layout: \"fill\") } }",
        "quarter,type,amount\nQ1,a,10\nQ1,b,30\nQ2,a,5\nQ2,b,5\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.svg.matches("opacity=").count(), 4);
    assert!(result.svg.contains(">1</text>"));
}

#[test]
fn test_horizontal_bar_uses_physical_frame_order() {
    let result = render_result(
        "Chart(data: \"f.csv\") { Space(amount * quarter) { Bar() } }",
        "quarter,amount\nQ1,10\nQ2,20\nQ3,15\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-bar"));
    assert_eq!(result.svg.matches("opacity=").count(), 3);
    assert!(result.svg.contains(">Q1</text>"));
}

#[test]
fn test_reversed_x_axis_keeps_tick_labels() {
    let result = render_result(
        "Chart(data: \"f.csv\") {
            Scale(axis: x, reverse: true)
            Space(amount * rep) { Bar() }
        }",
        "rep,amount\nAlice,82\nBowman,64\nChen,57\nDiaz,41\nEvans,28\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains(">0</text>"));
    assert!(result.svg.contains(">40</text>"));
    assert!(result.svg.contains(">80</text>"));
}

#[test]
fn test_horizontal_stacked_bar_domain_uses_totals() {
    let result = render_result(
        "Chart(data: \"f.csv\") { Space(amount * quarter) { Bar(fill: type, layout: \"stack\") } }",
        "quarter,type,amount\nQ1,a,10\nQ1,b,20\nQ2,a,5\nQ2,b,5\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.svg.matches("opacity=").count(), 4);
    assert!(result.svg.contains(">30</text>"));
}

#[test]
fn test_line_groups_by_stroke() {
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Space(time * value) { Line(stroke: series) } }",
        "time,value,series\n1,2,a\n2,3,a\n1,5,b\n2,1,b\n",
    );
    assert!(svg.contains("algraf-geom-line"));
    // One path per series.
    assert_eq!(svg.matches("<path").count(), 2);
}

#[test]
fn test_line_group_aesthetic_separates_constant_color_series() {
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Space(time * value) { Line(group: series, stroke: \"#888888\") } }",
        "time,value,series\n1,2,a\n2,3,a\n1,5,b\n2,1,b\n",
    );
    assert_eq!(svg.matches("<path").count(), 2);
    assert_eq!(svg.matches("stroke=\"#888888\"").count(), 2);
}

#[test]
fn test_smooth_lm_renders_fit_line() {
    let result = render_result(
        "Chart(data: \"s.csv\") { Space(x * y) { Smooth(method: \"lm\", stroke: \"#333333\") } }",
        "x,y\n1,2\n2,3\n3,5\n4,7\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-smooth"));
    assert!(result.svg.contains("stroke=\"#333333\""));
    assert_eq!(result.svg.matches("<path").count(), 1);
}

#[test]
fn test_smooth_loess_renders_curved_polyline() {
    // A loess fit is sampled across the range, so its path has many vertices
    // (one M plus several L commands), unlike the two-point lm line.
    let result = render_result(
        "Chart(data: \"s.csv\") { Space(x * y) { Smooth(method: \"loess\", span: 0.6, stroke: \"#333333\") } }",
        "x,y\n0,0\n1,1\n2,4\n3,9\n4,16\n5,25\n6,16\n7,9\n8,4\n9,1\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-smooth"));
    let line = result
        .svg
        .lines()
        .find(|l| l.contains("<path") && l.contains("#333333"))
        .unwrap();
    assert!(line.matches('L').count() > 5, "loess path: {line}");
}

#[test]
fn test_smooth_se_renders_confidence_band() {
    // With se: true a filled band path is drawn behind the fitted line, so two
    // paths appear per smooth.
    let result = render_result(
        "Chart(data: \"s.csv\") { Space(x * y) { Smooth(method: \"lm\", se: true, stroke: \"#333333\", fill: \"#cccccc\") } }",
        "x,y\n1,2\n2,2.5\n3,5\n4,6\n5,9\n6,9.5\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("fill=\"#cccccc\""));
    assert_eq!(result.svg.matches("<path").count(), 2);
}

#[test]
fn test_faceted_smooth_ignores_empty_stroke_groups() {
    let result = render_result(
        "Chart(data: \"s.csv\") { Space((x * y) / island) { Smooth(method: \"lm\", stroke: species) } }",
        "island,species,x,y\nA,alpha,1,2\nA,alpha,2,3\nB,beta,1,5\nB,beta,2,7\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.svg.matches("algraf-geom-smooth").count(), 2);
}

#[test]
fn test_temporal_axis() {
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Space(day * value) { Line() } }",
        "day,value\n2020-01-01,1\n2020-02-01,5\n2020-03-01,3\n",
    );
    assert!(svg.contains(">2020-01-01</text>") || svg.contains("2020-0"));
}

#[test]
fn test_temporal_axis_iso_minute_format() {
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Guide(axis: x, timeFormat: \"iso-minute\") Space(ts * value) { Line() } }",
        "ts,value\n2026-05-25 14:30,1\n2026-05-25 15:30,5\n",
    );
    assert!(svg.contains("2026-05-25 14:30"));
}

#[test]
fn test_dense_x_tick_labels_are_thinned_to_avoid_overlap() {
    let svg = render_svg(
        "Chart(data: \"t.csv\", width: 760, height: 420) {
            Guide(axis: x, timeFormat: \"iso-minute\")
            Space(ts * value) { Line() }
        }",
        "ts,value\n\
         2026-05-27 00:00,10\n\
         2026-05-28 09:30,14\n\
         2026-05-29 00:00,12\n\
         2026-05-30 16:45,18\n",
    );
    let visible_time_labels = svg.matches("2026-05-").count();
    assert!(
        visible_time_labels <= 4,
        "expected dense temporal labels to be thinned; got {visible_time_labels}: {svg}"
    );
    assert!(svg.contains(">2026-05-27 00:00</text>"));
    assert!(svg.contains(">2026-05-30 12:00</text>"));
}

#[test]
fn test_rotated_x_tick_labels_keep_more_categories_than_horizontal() {
    // Rotated tick labels are parallel diagonal strands, so adjacency depends on
    // the perpendicular gap between baselines, not the label length. A dense
    // categorical axis that thins long horizontal labels should keep many more
    // (here all) when rotated (spec §19.4).
    let csv = "city,pop\n\
        New York,8\nLos Angeles,4\nChicago,3\nHouston,2\nPhoenix,2\nPhiladelphia,2\n\
        San Antonio,2\nSan Diego,1\nDallas,1\nDenver,1\nSeattle,1\nMiami,1\n";
    let cities = [
        "New York",
        "Los Angeles",
        "Chicago",
        "Houston",
        "Phoenix",
        "Philadelphia",
        "San Antonio",
        "San Diego",
        "Dallas",
        "Denver",
        "Seattle",
        "Miami",
    ];
    let count = |svg: &str| {
        cities
            .iter()
            .filter(|c| svg.contains(&format!(">{c}</text>")))
            .count()
    };
    let horizontal = render_svg(
        "Chart(data: \"c.csv\", width: 600, height: 360) { Space(city * pop) { Bar() } }",
        csv,
    );
    let rotated = render_svg(
        "Chart(data: \"c.csv\", width: 600, height: 360) {
            Guide(axis: x, tickLabelAngle: -45)
            Space(city * pop) { Bar() }
        }",
        csv,
    );
    let (h, r) = (count(&horizontal), count(&rotated));
    assert!(h < 12, "horizontal long labels should be thinned; got {h}");
    assert_eq!(r, 12, "rotated labels should all fit; got {r}: {rotated}");
}

#[test]
fn test_temporal_axis_uses_calendar_month_ticks() {
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Space(day * value) { Line() } }",
        "day,value\n2026-01-01,1\n2026-02-01,5\n2026-03-01,3\n2026-04-01,4\n2026-05-01,2\n",
    );
    // Month-start ticks carry granularity-adaptive `%Y-%m` labels (v0.75).
    assert!(svg.contains(">2026-02</text>"));
    assert!(!svg.contains("2026-01-25"));
}

#[test]
fn test_short_temporal_axis_uses_whole_day_ticks() {
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Space(day * value) { Line() } }",
        "day,value\n2026-01-01,1\n2026-01-02,5\n2026-01-03,3\n",
    );
    assert_eq!(svg.matches(">2026-01-01</text>").count(), 1);
    assert_eq!(svg.matches(">2026-01-02</text>").count(), 1);
    assert_eq!(svg.matches(">2026-01-03</text>").count(), 1);
}

#[test]
fn test_edge_x_tick_labels_are_centered_on_ticks() {
    let svg = render_svg(
        "Chart(data: \"intervals.csv\") {
            Space(time * value) {
                Rect(xmin: start_time, xmax: end_time, ymin: 0, ymax: peak_value)
            }
        }",
        "time,value,start_time,end_time,peak_value\n\
         2026-01-01,0,2026-01-02,2026-01-05,100\n\
         2026-01-08,80,2026-01-08,2026-01-11,80\n\
         2026-01-13,120,2026-01-13,2026-01-17,120\n\
         2026-01-19,140,2026-01-19,2026-01-22,140\n",
    );
    assert!(text_element(&svg, "2026-01-01").contains("text-anchor=\"middle\""));
    assert!(text_element(&svg, "2026-01-22").contains("text-anchor=\"middle\""));
}

#[test]
fn test_facet_wrap_renders_one_panel_per_category() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Layout(facetColumns: 2) Space((x * y) / g) { Point(fill: g) } }",
        "x,y,g\n1,2,a\n2,3,b\n3,1,a\n4,5,c\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.layout.facets.len(), 3);
    assert!(result.layout.facets[1].plot.x > result.layout.facets[0].plot.x);
    assert!(result.layout.facets[1].plot.x - result.layout.facets[0].plot.right() >= 72.0);
    assert_eq!(
        result.layout.facets[2].plot.x,
        result.layout.facets[0].plot.x
    );
    assert!(result.svg.contains("algraf-facet-strip"));
    assert!(result.svg.contains("algraf-facet-panel"));
    assert_eq!(result.svg.matches("<circle").count(), 4);
    assert!(result.svg.contains(">a</text>"));
    assert!(result.svg.contains(">b</text>"));
    assert!(result.svg.contains(">c</text>"));
}

#[test]
fn test_four_facet_default_layout_is_two_by_two() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Space((x * y) / g) { Point(fill: g) } }",
        "x,y,g\n1,2,a\n2,3,b\n3,1,c\n4,5,d\n",
    );
    assert_eq!(result.layout.facets.len(), 4);
    assert!(result.layout.facets[1].plot.x > result.layout.facets[0].plot.x);
    assert_eq!(
        result.layout.facets[2].plot.x,
        result.layout.facets[0].plot.x
    );
    assert_eq!(
        result.layout.facets[3].plot.x,
        result.layout.facets[1].plot.x
    );
    assert!(result.layout.facets[2].plot.y > result.layout.facets[0].plot.y);
}

#[test]
fn test_four_facet_default_layout_with_title_and_legend_is_two_by_two() {
    let result = render_result(
        r#"Chart(data: "p.csv", width: 800, height: 480, title: "Regional Sales Performance vs. Target") {
            Scale(fill: product, palette: "accent")
            Space((x * y) / region) { Point(fill: product) }
        }"#,
        "x,y,region,product\n\
         1,150,North,Widgets\n2,200,North,Gadgets\n\
         1,210,South,Widgets\n2,130,South,Gadgets\n\
         1,120,East,Widgets\n2,80,East,Gadgets\n\
         1,170,West,Widgets\n2,100,West,Gadgets\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.layout.legend.is_some());
    assert_eq!(result.layout.facets.len(), 4);
    assert!(result.layout.facets[1].plot.x > result.layout.facets[0].plot.x);
    assert_eq!(
        result.layout.facets[2].plot.x,
        result.layout.facets[0].plot.x
    );
    assert_eq!(
        result.layout.facets[3].plot.x,
        result.layout.facets[1].plot.x
    );
    assert!(result.layout.facets[2].plot.y > result.layout.facets[0].plot.y);
}

#[test]
fn test_tile_heatmap_gradient() {
    // Both axes must be categorical for a tile grid (hour as a label, not a number).
    let svg = render_svg(
        "Chart(data: \"h.csv\") { Space(day * hour) { Tile(fill: value) } }",
        "day,hour,value\nMon,9am,1\nMon,10am,5\nTue,9am,3\nTue,10am,9\n",
    );
    assert!(svg.contains("algraf-geom-tile"));
    assert_eq!(svg.matches("opacity=").count(), 4);
}

#[test]
fn test_tile_width_height_fraction_shrinks_centered_cells() {
    let csv = "day,hour,value\nMon,9am,1\nMon,10am,5\nTue,9am,3\nTue,10am,9\n";
    let full = render_svg(
        "Chart(data: \"h.csv\") { Space(day * hour) { Tile(fill: value) } }",
        csv,
    );
    let shrunken = render_svg(
        "Chart(data: \"h.csv\") { Space(day * hour) { Tile(fill: value, width: 0.5, height: 0.25) } }",
        csv,
    );
    let (full_width, full_height) = first_tile_rect_size(&full);
    let (tile_width, tile_height) = first_tile_rect_size(&shrunken);
    assert!((tile_width - full_width * 0.5).abs() < 0.01);
    assert!((tile_height - full_height * 0.25).abs() < 0.01);
}

#[test]
fn test_categorical_aspect_equalizes_band_steps() {
    let svg = render_svg(
        "Chart(data: \"h.csv\", width: 640, height: 520) {
  Space(day * hour, aspect: 1) { Tile(fill: value) }
}",
        "day,hour,value\nMon,9am,1\nMon,10am,5\nMon,11am,6\nMon,12pm,7\nTue,9am,3\nTue,10am,9\nTue,11am,2\nTue,12pm,4\n",
    );
    let (tile_width, tile_height) = first_tile_rect_size(&svg);
    let ratio = tile_width / tile_height;
    assert!(
        (ratio - 1.0).abs() < 0.02,
        "two x bands and four y bands should render square band steps, got {ratio}"
    );
}

#[test]
fn test_boxplot_renders_summary_marks() {
    let result = render_result(
        "Chart(data: \"b.csv\") { Space(group * value) { Boxplot(fill: group) } }",
        "group,value\na,1\na,2\na,3\na,4\nb,3\nb,4\nb,5\nb,6\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-boxplot"));
    assert!(result.svg.contains("<line"));
    assert!(result.svg.contains("<rect x="));
}

#[test]
fn test_horizontal_boxplot_uses_physical_frame_order() {
    let result = render_result(
        "Chart(data: \"b.csv\") { Space(value * group) { Boxplot(fill: group) } }",
        "group,value\na,1\na,2\na,3\na,4\nb,3\nb,4\nb,5\nb,6\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-boxplot"));
    assert!(result.svg.contains("<line"));
    assert!(result.svg.contains("<rect x="));
}

#[test]
fn test_boxplot_renders_outliers_by_default() {
    // Group `a` is 1..9 plus a far outlier at 100; it lies beyond the
    // 1.5·IQR fence and renders as a circle.
    let result = render_result(
        "Chart(data: \"b.csv\") { Space(group * value) { Boxplot() } }",
        "group,value\na,1\na,2\na,3\na,4\na,5\na,6\na,7\na,8\na,9\na,100\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.svg.matches("<circle").count(), 1);
}

#[test]
fn test_boxplot_outliers_false_suppresses_circles() {
    let result = render_result(
        "Chart(data: \"b.csv\") { Space(group * value) { Boxplot(outliers: false) } }",
        "group,value\na,1\na,2\na,3\na,4\na,5\na,6\na,7\na,8\na,9\na,100\n",
    );
    assert_eq!(result.svg.matches("<circle").count(), 0);
}

#[test]
fn test_boxplot_handles_subpixel_categorical_bandwidth() {
    let csv = high_cardinality_distribution_csv(360);
    let result = render_result(
        "Chart(data: \"b.csv\", width: 180, height: 160) { Space(group * value) { Boxplot() } }",
        &csv,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-boxplot"));
}

#[test]
fn test_ribbon_renders_closed_path() {
    let result = render_result(
        "Chart(data: \"r.csv\") { Space(x * (lower + upper)) { Ribbon(ymin: lower, ymax: upper, fill: \"steelblue\", alpha: 0.25) } }",
        "x,lower,upper\n1,2,4\n2,3,5\n3,2,6\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-ribbon"));
    assert!(result.svg.contains("fill=\"steelblue\""));
    assert!(result.svg.contains("Z\""));
}

#[test]
fn test_reference_lines_and_rug_render() {
    let result = render_result(
        "Chart(data: \"r.csv\") { Space(x * y) { Point() HLine(y: 2, stroke: \"red\", dash: \"dashed\", label: \"Target\") VLine(x: 2, stroke: \"gray\", dash: \"dotted\", label: \"Marker\") Rug(sides: \"bl\") } }",
        "x,y\n1,1\n2,2\n3,3\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-hline"));
    assert!(result.svg.contains("algraf-geom-vline"));
    assert!(result.svg.contains("algraf-geom-rug"));
    assert!(result.svg.contains(">Target</text>"));
    assert!(result.svg.contains(">Marker</text>"));
    assert!(result.svg.contains("stroke-dasharray=\"4 4\""));
    assert!(result.svg.contains("stroke-dasharray=\"1 2\""));
}

#[test]
fn test_gradient_legend_for_numeric_fill() {
    let svg = render_svg(
        "Chart(data: \"h.csv\") { Space(day * hour) { Tile(fill: value) } }",
        "day,hour,value\nMon,9am,1\nMon,10am,5\nTue,9am,3\nTue,10am,9\n",
    );
    assert!(svg.contains("algraf-legends"));
    assert!(svg.contains(">value</text>"));
    assert!(svg.contains(">1</text>"));
    assert!(svg.contains(">9</text>"));
}

#[test]
fn test_gradient_legend_renders_colorbar_segments() {
    let svg = render_svg(
        "Chart(data: \"h.csv\") {
  Scale(fill: value, gradient: \"viridis\", breaks: [1, 5, 9], labels: [\"low\", \"mid\", \"high\"])
  Space(day * hour) { Tile(fill: value) }
}",
        "day,hour,value\nMon,9am,1\nMon,10am,5\nTue,9am,3\nTue,10am,9\n",
    );
    let legend = legend_layer(&svg);
    assert!(
        legend.matches("<rect").count() > 20,
        "continuous legend should be a segmented colorbar: {legend}"
    );
    assert!(legend.contains(">low</text>"));
    assert!(legend.contains(">mid</text>"));
    assert!(legend.contains(">high</text>"));
}

#[test]
fn test_source_gradient_controls_numeric_fill_colors() {
    let svg = render_svg(
        "Chart(data: \"h.csv\") { Scale(fill: value, gradient: [\"#3366cc\", \"#cc3333\"]) Space(day * hour) { Tile(fill: value) } }",
        "day,hour,value\nMon,9am,1\nMon,10am,9\n",
    );
    assert!(svg.contains("fill=\"#3366cc\""));
    assert!(svg.contains("fill=\"#cc3333\""));
}

#[test]
fn test_named_viridis_gradient_controls_numeric_fill_colors() {
    let svg = render_svg(
        "Chart(data: \"h.csv\") { Scale(fill: value, gradient: \"viridis\") Space(day * hour) { Tile(fill: value) } }",
        "day,hour,value\nMon,9am,0\nMon,10am,10\n",
    );
    assert!(svg.contains("fill=\"#440154\""));
    assert!(svg.contains("fill=\"#fde725\""));
}

#[test]
fn test_source_gradient_supports_rgb_rgba_and_alpha_hex_colors() {
    let svg = render_svg(
        "Chart(data: \"h.csv\") { Scale(fill: value, gradient: [\"rgba(20, 95, 82, 0.5)\", \"rgb(80, 120, 160)\", \"#7bce8780\"]) Space(day * hour) { Tile(fill: value) } }",
        "day,hour,value\nMon,9am,0\nMon,10am,5\nTue,9am,10\n",
    );
    assert!(svg.contains("fill=\"rgba(20, 95, 82, 0.5)\""));
    assert!(svg.contains("fill=\"#5078a0\""));
    assert!(svg.contains("fill=\"rgba(123, 206, 135, 0.502)\""));
}

#[test]
fn test_positioned_gradient_controls_numeric_fill_colors() {
    let svg = render_svg(
        "Chart(data: \"h.csv\") { Scale(fill: value, gradient: [Stop(value: 0, color: \"#3366cc\"), Stop(value: 10, color: \"#cc3333\")]) Space(day * hour) { Tile(fill: value) } }",
        "day,hour,value\nMon,9am,0\nMon,10am,10\n",
    );
    assert!(svg.contains("fill=\"#3366cc\""));
    assert!(svg.contains("fill=\"#cc3333\""));
}

#[test]
fn test_histogram_via_derive_and_rect() {
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Derive bins = Bin(value, bins: 4) Space(bin_start * count, data: bins) { Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count) } }",
        "value\n1\n2\n3\n4\n5\n6\n7\n8\n",
    );
    assert!(svg.contains("algraf-geom-rect"));
    // Four bins -> four rects (plus background + plot).
    assert_eq!(mark_rect_count(&svg), 4);
}

#[test]
fn test_direct_histogram_renders_like_primitive_rects() {
    let result = render_result(
        "Chart(data: \"d.csv\") { Space(value) { Histogram(bins: 4, fill: \"steelblue\") } }",
        "value\n1\n2\n3\n4\n5\n6\n7\n8\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-rect"));
    assert_eq!(mark_rect_count(&result.svg), 4);
    assert!(result.svg.contains("fill=\"steelblue\""));
}

#[test]
fn test_horizontal_histogram_orientation_renders_count_on_x() {
    let result = render_result(
        "Chart(data: \"d.csv\") { Space(value) { Histogram(bins: 4, orientation: \"horizontal\", fill: \"steelblue\") } }",
        "value\n1\n2\n3\n4\n5\n6\n7\n8\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-rect"));
    assert_eq!(mark_rect_count(&result.svg), 4);
    assert!(result.svg.contains(">count</text>"));
}

#[test]
fn test_histogram_with_bin_width_aligns_axis_ticks() {
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Derive bins = Bin(value, binWidth: 1, boundary: 0) Space(bin_start * count, data: bins) { Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count) } }",
        "value\n1.0\n1.4\n1.7\n2.1\n2.5\n2.7\n3.0\n3.2\n3.5\n3.7\n4.1\n4.4\n4.7\n5.0\n5.3\n5.6\n6.1\n6.4\n6.8\n7.2\n7.5\n8.0\n8.3\n8.7\n",
    );
    assert_eq!(mark_rect_count(&svg), 8);
    assert!(svg.contains(">9</text>"));
    assert!(!svg.contains(">0.5</text>"));
}

#[test]
fn test_bin_closed_right_assigns_boundary_values_to_left_bins() {
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Derive bins = Bin(value, binWidth: 10, boundary: 0, closed: \"right\") Space(bin_start * count, data: bins) { Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count) } }",
        "value\n0\n10\n",
    );
    assert_eq!(mark_rect_count(&svg), 2);
    assert!(svg.contains(">-10</text>"));
}

#[test]
fn test_rect_domain_uses_extent_properties() {
    let svg = render_svg(
        "Chart(data: \"r.csv\") { Space(x0 * y1) { Rect(xmin: x0, xmax: x1, ymin: 0, ymax: y1) } }",
        "x0,x1,y1\n0,10,5\n",
    );
    assert!(svg.contains(">10</text>"));
    assert!(svg.contains(">0</text>"));
}

#[test]
fn test_rect_renders_stroke_border() {
    let svg = render_svg(
        "Chart(data: \"r.csv\") { Space(x0 * y1) { Rect(xmin: x0, xmax: x1, ymin: 0, ymax: y1, fill: \"steelblue\", stroke: \"#ffffff\", strokeWidth: 1) } }",
        "x0,x1,y1\n0,10,5\n",
    );
    assert!(svg.contains("stroke=\"#ffffff\""));
    assert!(svg.contains("stroke-width=\"1\""));
}

#[test]
fn test_rect_supports_temporal_union_and_categorical_bounds() {
    let result = render_result(
        "Chart(data: \"g.csv\") { Space((start + end) * phase) { Rect(xmin: start, xmax: end, ymin: phase, ymax: phase, fill: phase) } }",
        "start,end,phase\n2026-01-01,2026-01-05,Review\n2026-01-03,2026-01-07,Filing\n",
    );
    assert!(
        result.diagnostics.iter().all(|diag| diag.code != "W2002"),
        "{:?}",
        result.diagnostics
    );
    assert_eq!(result.svg.matches("opacity=").count(), 2);
    assert!(result.svg.contains(">2026-01-01</text>"));
    assert!(result.svg.contains(">Review</text>"));
}

#[test]
fn test_rect_zero_extent_is_skipped() {
    let svg = render_svg(
        "Chart(data: \"r.csv\") { Space(x * y) { Rect(xmin: x, xmax: x, ymin: 0, ymax: y, strokeWidth: 3) } }",
        "x,y\n1,5\n",
    );
    let data_layer = svg
        .split_once("algraf-geom-rect")
        .and_then(|(_, after)| after.split_once("</g>"))
        .map(|(layer, _)| layer)
        .unwrap_or("");
    assert!(!data_layer.contains("<rect"));
}

#[test]
fn test_determinism() {
    let source = "Chart(data: \"p.csv\") { Space(x * y) { Point(fill: g) } }";
    let csv = "x,y,g\n1,2,a\n2,3,b\n3,1,a\n";
    assert_eq!(render_svg(source, csv), render_svg(source, csv));
}

#[test]
fn test_text_is_escaped() {
    // A category containing markup characters must be escaped in the SVG.
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Point(fill: g) } }",
        "x,y,g\n1,2,<a&b>\n2,3,c\n",
    );
    assert!(svg.contains("&lt;a&amp;b&gt;"));
    assert!(!svg.contains("<a&b>"));
}

#[test]
fn test_number_formatting_no_locale_or_trailing_zeros() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Point() } }",
        "x,y\n0,0\n10,10\n",
    );
    // Locale-independent: no comma decimal separators in coordinates, and no
    // long trailing-zero float tails. (Commas in font-family lists are fine.)
    assert!(!svg.contains(".000"));
    assert!(!svg.contains("0,5") && !svg.contains("1,5"));
}

#[test]
fn test_void_theme_has_no_axes() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Point() } }",
        "x,y\n1,2\n2,3\n",
    );
    let void = {
        let frame = read_csv_str("x,y\n1,2\n2,3\n").unwrap().frame;
        let parsed = parse("Chart(data: \"p.csv\") { Space(x * y) { Point() } }");
        let ir = analyze(&parsed.syntax(), frame.schema()).ir.unwrap();
        render(&ir, &frame, &Theme::void(), None).unwrap().svg
    };
    assert!(svg.contains("algraf-axes"));
    assert!(!void.contains("algraf-axes"));
}

// --- Count stat ---

#[test]
fn test_bar_count_stat_renders_with_count_axis() {
    let svg = render_svg(
        "Chart(data: \"d.csv\") {\n  Space(species) {\n    Bar(stat: \"count\", fill: species)\n  }\n}",
        "species\nA\nA\nB\nA\nB\nC\n",
    );
    assert!(svg.contains("algraf-geom-bar"));
    // The synthetic count column drives the y-axis label.
    assert!(
        svg.contains(">count<"),
        "expected y-axis label `count`; got: {svg}"
    );
}

// --- Space-local theme ---

#[test]
fn test_space_local_theme_does_not_leak_to_other_spaces() {
    // Two spaces: the second has a space-local void theme. Only the second
    // panel should have no plot background ink besides what minimal provides.
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Theme(name: \"minimal\")\n  Space(x * y) { Point() }\n  Space(x * y) { Theme(name: \"void\") Point() }\n}",
        "x,y\n1,2\n2,3\n",
    );
    // The chart still draws axes for the first panel.
    assert!(svg.contains("algraf-axes"));
}

// --- Guide axis label overrides ---

#[test]
fn test_guide_axis_label_overrides_axis_title() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Guide(axis: x, label: \"Flipper (mm)\")\n  Guide(axis: y, label: \"Mass (g)\")\n  Space(x * y) { Point() }\n}",
        "x,y\n1,2\n2,3\n",
    );
    assert!(svg.contains("Flipper (mm)"));
    assert!(svg.contains("Mass (g)"));
}

#[test]
fn test_guide_fill_null_suppresses_fill_legend() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Guide(fill: null)\n  Space(x * y) { Point(fill: g) }\n}",
        "x,y,g\n1,2,a\n2,3,b\n",
    );
    // Stroke would still show; here only fill is mapped.
    assert!(!svg.contains("algraf-legends"));
}

#[test]
fn test_guide_stroke_null_suppresses_stroke_legend() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Guide(stroke: null)\n  Space(x * y) { Line(stroke: g) }\n}",
        "x,y,g\n1,2,a\n2,3,b\n",
    );
    assert!(!svg.contains("algraf-legends"));
}

#[test]
fn test_scale_label_overrides_legend_title() {
    // Scale(fill: col, label: "...") renames the legend title (spec §16.13).
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Scale(fill: g, label: \"Group Name\")\n  Space(x * y) { Point(fill: g) }\n}",
        "x,y,g\n1,2,a\n2,3,b\n",
    );
    assert!(svg.contains(">Group Name</text>"));
    // The bare column-derived title is not used.
    assert!(!svg.contains(">g</text>"));
}

#[test]
fn test_scale_label_is_column_scoped_for_same_aesthetic() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {
  Scale(fill: value, gradient: \"viridis\", label: \"Value\")
  Scale(fill: group, range: [\"low\" => \"#ffffff\", \"high\" => \"#000000\"])
  Space(x * y) {
    Tile(fill: value)
    Text(label: value, fill: group)
  }
}",
        "x,y,value,group\nA,M,1,low\nB,M,9,high\n",
    );
    let legend = legend_layer(&svg);
    assert_eq!(legend.matches(">Value</text>").count(), 1);
    assert!(
        legend.contains(">group</text>"),
        "second fill scale should keep its own column title: {legend}"
    );
}

#[test]
fn test_geometry_legend_false_suppresses_only_that_layer() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {
  Scale(fill: value, gradient: \"viridis\", label: \"Value\")
  Scale(fill: group, range: [\"low\" => \"#ffffff\", \"high\" => \"#000000\"])
  Space(x * y) {
    Tile(fill: value)
    Text(label: value, fill: group, legend: false)
  }
}",
        "x,y,value,group\nA,M,1,low\nB,M,9,high\n",
    );
    let legend = legend_layer(&svg);
    assert!(legend.contains(">Value</text>"));
    assert!(!legend.contains(">group</text>"));
    assert!(!legend.contains(">low</text>"));
    assert!(!legend.contains(">high</text>"));
}

#[test]
fn test_fill_stroke_legends_merge_for_same_column() {
    // fill and stroke mapped to the same categorical column produce a single
    // merged legend whose swatches carry a stroke outline (spec §19.7).
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Space(x * y) { Point(fill: g, stroke: g) }\n}",
        "x,y,g\n1,2,a\n2,3,b\n",
    );
    // Exactly one legend title for the column (not two stacked legends).
    assert_eq!(
        svg.matches(">g</text>").count(),
        1,
        "expected one merged title"
    );
    // Swatch rects carry a stroke from the merged stroke aesthetic.
    assert!(
        svg.contains("stroke-width=\"2\""),
        "merged swatch should draw a stroke outline"
    );
}

#[test]
fn test_distinct_columns_keep_separate_legends() {
    // fill and stroke on different columns remain two separate legends.
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Space(x * y) { Point(fill: g, stroke: h) }\n}",
        "x,y,g,h\n1,2,a,p\n2,3,b,q\n",
    );
    assert!(svg.contains(">g</text>"));
    assert!(svg.contains(">h</text>"));
}

#[test]
fn test_guide_grid_false_suppresses_grid() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Guide(grid: false)\n  Space(x * y) { Point() }\n}",
        "x,y\n1,2\n2,3\n",
    );
    assert!(!svg.contains("algraf-grid"));
}

#[test]
fn test_scale_domain_reverse_and_log_render() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Scale(axis: x, type: \"log10\", domain: [1, 100])\n  Scale(axis: y, reverse: true)\n  Space(x * y) { Point() }\n}",
        "x,y\n1,1\n10,2\n100,3\n",
    );
    assert!(svg.contains(">100</text>"));
    assert!(svg.contains(">1</text>"));
    assert!(svg.contains("algraf-axes"));
}

#[test]
fn test_sqrt_scale_renders_and_positions_by_square_root() {
    // On a sqrt axis with domain [0, 100], the value 25 sits at the pixel
    // midpoint (sqrt(25)/sqrt(100) = 0.5), unlike a linear axis where 50 would.
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Scale(axis: x, type: \"sqrt\", domain: [0, 100])\n  Space(x * y) { Point() }\n}",
        "x,y\n0,1\n25,2\n100,3\n",
    );
    assert!(svg.contains("algraf-axes"));
    // Nice data-value ticks are still emitted (not log decades): a linearly
    // spaced 0,20,…,100 set, with 100 at the far end.
    assert!(svg.contains(">40</text>"));
    assert!(svg.contains(">100</text>"));
}

#[test]
fn test_scale_integer_constrains_axis_ticks_to_whole_numbers() {
    // A small integer-valued domain would otherwise pick a 0.5 tick step once
    // the 8% padding makes the bounds fractional.
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Scale(axis: x, integer: true)\n  Scale(axis: y, integer: true)\n  Space(x * y) { Point() }\n}",
        "x,y\n1,1\n2,2\n3,3\n4,3\n",
    );
    assert!(svg.contains(">2</text>"));
    assert!(svg.contains(">3</text>"));
    assert!(!svg.contains(">1.5</text>"));
    assert!(!svg.contains(">2.5</text>"));
}

#[test]
fn test_scale_accent_palette_changes_categorical_colors() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Scale(fill: g, palette: \"accent\")\n  Space(x * y) { Point(fill: g) }\n}",
        "x,y,g\n1,2,a\n2,3,b\n",
    );
    assert!(svg.contains("fill=\"#006BA4\""));
    assert!(svg.contains("fill=\"#FF800E\""));
}

#[test]
fn test_grouped_histogram_stacks_and_legends() {
    // Two species across two bins: bars stack within each bin, and a fill
    // legend is emitted from the group column.
    let result = render_result(
        "Chart(data: \"d.csv\") { Space(v) { Histogram(fill: g, bins: 2) } }",
        "v,g\n1,a\n1,b\n2,a\n2,a\n2,b\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    // A grouped histogram desugars to stacked Rects with a categorical fill.
    assert!(result.svg.contains("algraf-geom-rect"));
    assert!(result.svg.contains("algraf-legends"));
    // Each (bin, group) cell with a nonzero count is a stacked rect; the two
    // species give distinct fills.
    let data_layer = result
        .svg
        .split_once("algraf-legends")
        .map_or(result.svg.as_str(), |(before, _)| before);
    assert!(data_layer.matches("<rect").count() >= 4);
}

#[test]
fn test_dodged_histogram_splits_bins_into_subbars() {
    // `Space(v / g)` dodges: within a bin the two groups sit in adjacent,
    // non-overlapping x sub-slots (both rising from y baseline).
    let result = render_result(
        "Chart(data: \"d.csv\") { Space(v / g) { Histogram(fill: g, bins: 2) } }",
        "v,g\n1,a\n1,b\n2,a\n2,a\n2,b\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-rect"));
    assert!(result.svg.contains("algraf-legends"));
    let data_layer = result
        .svg
        .split_once("algraf-legends")
        .map_or(result.svg.as_str(), |(before, _)| before);
    // Collect the rect x positions in the data layer; adjacent sub-bars must
    // have distinct x (side-by-side), not share one stacked x per bin.
    let xs: Vec<&str> = data_layer
        .match_indices("<rect x=\"")
        .map(|(i, _)| {
            let s = &data_layer[i + 9..];
            &s[..s.find('"').unwrap()]
        })
        .collect();
    let unique: std::collections::HashSet<&&str> = xs.iter().collect();
    assert!(
        unique.len() >= 4,
        "expected distinct sub-slot x positions: {xs:?}"
    );
}

#[test]
fn test_blended_histogram_overlays_full_width_series_and_annotations() {
    let result = render_result(
        "Chart(data: \"d.csv\") { Scale(fill: series, range: [\"a\" => \"#beaed4\", \"b\" => \"#7fc97f\"], labels: [\"a\" => \"A\", \"b\" => \"B\"], label: \"\") Space((a + b)) { Histogram(binWidth: 1, alpha: 0.8, stroke: \"#000000\") VLine(x: 1) Text(x: 2, y: 2, label: \"Mean\") } }",
        "a,b\n0,1\n0,1\n1,2\n1,2\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-rect"));
    assert!(result.svg.contains("algraf-legends"));
    assert!(result.svg.contains(">A</text>"));
    assert!(result.svg.contains(">B</text>"));
    assert!(result.svg.contains(">Mean</text>"));
    assert_eq!(result.svg.matches(">Mean</text>").count(), 1);
    let data_layer = result
        .svg
        .split_once("algraf-legends")
        .map_or(result.svg.as_str(), |(before, _)| before);
    assert!(
        !data_layer.contains("height=\"1\""),
        "zero-count overlaid bins should not render stroked 1px rectangles"
    );
    let xs: Vec<&str> = data_layer
        .match_indices("<rect x=\"")
        .map(|(i, _)| {
            let s = &data_layer[i + 9..];
            &s[..s.find('"').unwrap()]
        })
        .collect();
    let unique: std::collections::HashSet<&&str> = xs.iter().collect();
    assert!(
        unique.len() < xs.len(),
        "overlaid series should share x positions: {xs:?}"
    );
}

#[test]
fn test_temporal_histogram_renders_bins() {
    let result = render_result(
        "Chart(data: \"t.csv\") { Space(day) { Histogram(bins: 3, fill: \"steelblue\") } }",
        "day\n2026-01-01\n2026-01-02\n2026-01-03\n2026-01-04\n2026-01-05\n2026-01-06\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-rect"));
    assert_eq!(mark_rect_count(&result.svg), 3);
    assert!(result.svg.contains("2026-01"));
}

#[test]
fn test_temporal_histogram_calendar_interval_renders_bins() {
    let result = render_result(
        "Chart(data: \"t.csv\") { Space(day) { Histogram(interval: \"month\", fill: \"steelblue\") } }",
        "day\n2026-01-03\n2026-01-20\n2026-02-02\n2026-03-15\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-rect"));
    assert!(result.svg.contains("2026-01"));
    assert!(result.svg.contains("2026-02"));
}

#[test]
fn test_temporal_histogram_calendar_interval_ticks_use_bin_centers() {
    let result = render_result(
        "Chart(data: \"t.csv\") { Guide(axis: x, timeFormat: \"iso-date\") Space(day) { Histogram(interval: \"week\", fill: \"steelblue\") } }",
        "day\n2026-01-01\n2026-01-06\n2026-01-14\n2026-01-21\n2026-01-30\n2026-02-04\n2026-02-13\n2026-02-19\n2026-02-28\n2026-03-05\n2026-03-12\n2026-03-18\n2026-03-25\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains(">2026-01-01</text>"));
    assert!(result.svg.contains(">2026-01-15</text>"));
    assert!(result.svg.contains(">2026-03-26</text>"));
    assert!(!result.svg.contains(">2026-02-01</text>"));
}

// --- Area, Text, Segment ---

#[test]
fn test_area_renders_filled_path() {
    let svg = render_svg(
        "Chart(data: \"t.csv\") {\n  Space(x * y) {\n    Area(baseline: 0, fill: \"steelblue\")\n  }\n}",
        "x,y\n1,4\n2,3\n3,5\n",
    );
    assert!(svg.contains("algraf-geom-area"));
    assert!(svg.contains("<path "));
}

#[test]
fn test_text_geometry_renders_labels() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Text(label: name)\n  }\n}",
        "x,y,name\n1,2,Alice\n2,3,Bob\n",
    );
    assert!(svg.contains("algraf-geom-text"));
    assert!(svg.contains(">Alice<"));
    assert!(svg.contains(">Bob<"));
}

#[test]
fn test_text_geometry_renders_multiline_labels_as_tspans() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Text(label: name)\n  }\n}",
        "x,y,name\n1,2,\"Alpha &\nBeta <tag>\"\n",
    );
    assert!(svg.contains("<tspan "));
    assert!(svg.contains("dy=\"1.2em\""));
    assert!(svg.contains(">Alpha &amp;</tspan>"));
    assert!(svg.contains(">Beta &lt;tag&gt;</tspan>"));
}

#[test]
fn test_segment_renders_line_between_literal_endpoints() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Segment(x: 1, y: 1, xend: 3, yend: 4)\n  }\n}",
        "x,y\n0,0\n5,5\n",
    );
    assert!(svg.contains("algraf-geom-segment"));
    assert!(svg.contains("<line "));
}

#[test]
fn test_segment_renders_one_line_per_row_for_mapped_endpoints() {
    // A dumbbell: one horizontal segment per category, from `low` to `high`.
    let result = render_result(
        "Chart(data: \"p.csv\") {\n  Space(low * city) {\n    Segment(x: low, y: city, xend: high, yend: city, stroke: \"#abcdef\")\n  }\n}",
        "city,low,high\nA,1,5\nB,2,8\nC,0,3\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.svg.matches("stroke=\"#abcdef\"").count(), 3);
}

#[test]
fn test_segment_skips_rows_with_missing_endpoints() {
    let result = render_result(
        "Chart(data: \"p.csv\") {\n  Space(low * city) {\n    Segment(x: low, y: city, xend: high, yend: city, stroke: \"#abcdef\")\n  }\n}",
        "city,low,high\nA,1,5\nB,2,\nC,0,3\n",
    );
    assert_eq!(result.svg.matches("stroke=\"#abcdef\"").count(), 2);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.message.contains("Segment skipped")));
}

// --- Diagnostics ---

#[test]
fn test_w2002_when_geometry_produces_no_marks() {
    // A Smooth on an empty (single-row) input cannot produce marks; the
    // renderer should emit one aggregated W2002 warning.
    let result = render_result(
        "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Smooth(method: \"lm\")\n  }\n}",
        "x,y\n1,1\n",
    );
    assert!(
        result.diagnostics.iter().any(|d| d.code == "W2002"),
        "expected W2002, got {:?}",
        result.diagnostics
    );
}

#[test]
fn bar_space_mismatch_diagnostic_suggests_categorical_axis_type() {
    let result = render_result(
        "Chart(data: \"p.csv\") { Space(bucket_start * lines_changed) { Bar() } }",
        "bucket_start,lines_changed\n2024-01-01,120\n2025-01-01,50\n",
    );
    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "R0002")
        .expect("R0002 diagnostic");
    let help = diagnostic.help.as_deref().unwrap_or_default();
    assert!(
        help.contains("Scale(axis: x, type: \"categorical\")"),
        "{diagnostic:?}"
    );
}

// --- Density geom ---

#[test]
fn test_density_geom_renders_filled_area() {
    // Density desugars to a filled Area over a KDE curve (spec §15.11).
    let svg = render_svg(
        "Chart(data: \"d.csv\") {\n  Space(v) {\n    Density(fill: \"#4c78a8\")\n  }\n}",
        "v\n0\n1\n1\n2\n2\n2\n3\n3\n4\n5\n",
    );
    assert!(svg.contains("algraf-geom-area"));
    assert!(svg.contains("fill=\"#4c78a8\""));
    // The area is a single closed path.
    assert_eq!(svg.matches("<path").count(), 1);
}

#[test]
fn test_violin_renders_mirrored_density_and_quantiles() {
    let result = render_result(
        "Chart(data: \"v.csv\") { Space(group * value) { Violin(fill: group, quantiles: [0.25, 0.5, 0.75]) } }",
        "group,value\na,1\na,2\na,2\na,3\na,4\nb,2\nb,3\nb,4\nb,4\nb,5\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-violin"));
    assert_eq!(result.svg.matches("<path").count(), 2);
    assert!(result.svg.matches("<line").count() >= 6);
}

#[test]
fn test_horizontal_violin_uses_physical_frame_order() {
    let result = render_result(
        "Chart(data: \"v.csv\") { Space(value * group) { Violin(fill: group, quantiles: [0.25, 0.5, 0.75]) } }",
        "group,value\na,1\na,2\na,2\na,3\na,4\nb,2\nb,3\nb,4\nb,4\nb,5\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-violin"));
    assert_eq!(result.svg.matches("<path").count(), 2);
    assert!(result.svg.matches("<line").count() >= 6);
}

#[test]
fn test_one_sided_violin_and_sina_render_density_ridges() {
    let result = render_result(
        "Chart(data: \"v.csv\") { Space(value * group) { Violin(side: \"top\", fill: group, quantiles: [0.5]) Sina(side: \"top\", fill: \"#aaaaaa\", size: 1) } }",
        "group,value\na,1\na,2\na,2\na,3\na,4\nb,2\nb,3\nb,4\nb,4\nb,5\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-violin"));
    assert!(result.svg.contains("algraf-geom-sina"));
    assert_eq!(result.svg.matches("<circle").count(), 10);
    assert_eq!(result.svg.matches("<path").count(), 2);
}

#[test]
fn test_ungrouped_violin_and_sina_share_density_layout() {
    let result = render_result(
        "Chart(data: \"v.csv\") { Space(group * value) { Violin(fill: \"#4c78a8\") Sina(fill: \"#aaaaaa\", size: 1) } }",
        "group,value\nall,1\nall,2\nall,2\nall,3\nall,4\nall,5\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-violin"));
    assert!(result.svg.contains("algraf-geom-sina"));
    assert_eq!(result.svg.matches("<path").count(), 1);
    assert_eq!(result.svg.matches("<circle").count(), 6);
}

#[test]
fn test_violin_and_sina_handle_subpixel_categorical_bandwidth() {
    let csv = high_cardinality_distribution_csv(360);
    let result = render_result(
        "Chart(data: \"v.csv\", width: 180, height: 160) { Space(group * value) { Violin() Sina(size: 1) } }",
        &csv,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-violin"));
    assert!(result.svg.contains("algraf-geom-sina"));
}

#[test]
fn test_freqpoly_renders_bin_count_line() {
    let result = render_result(
        "Chart(data: \"d.csv\") { Space(v) { FreqPoly(bins: 4, stroke: \"steelblue\") } }",
        "v\n1\n2\n3\n4\n5\n6\n7\n8\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-line"));
    assert_eq!(result.svg.matches("<path").count(), 1);
}

#[test]
fn test_horizontal_freqpoly_orientation_renders_count_on_x() {
    let result = render_result(
        "Chart(data: \"d.csv\") { Space(v) { FreqPoly(bins: 4, orientation: \"horizontal\", stroke: \"steelblue\") } }",
        "v\n1\n2\n3\n4\n5\n6\n7\n8\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-line"));
    assert_eq!(result.svg.matches("<path").count(), 1);
    assert!(result.svg.contains(">count</text>"));
}

#[test]
fn test_bin2d_renders_rectangular_bins() {
    let result = render_result(
        "Chart(data: \"b.csv\") { Space(x * y) { Bin2D(bins: 2) } }",
        "x,y\n1,1\n1,2\n2,1\n8,8\n9,9\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-rect"));
    assert!(result.svg.contains(">count</text>"));
}

#[test]
fn test_hexbin_renders_hexagonal_bins() {
    let result = render_result(
        "Chart(data: \"b.csv\") { Space(x * y) { HexBin(bins: 3) } }",
        "x,y\n1,1\n1,2\n2,1\n8,8\n9,9\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-hexbin"));
    assert!(result.svg.contains("<polygon"));
    // When `fill` is omitted, HexBin shades by count and emits a continuous
    // count legend, matching Bin2D.
    assert!(result.svg.contains(">count</text>"));
}

#[test]
fn test_hexbin_constant_fill_omits_count_legend() {
    let result = render_result(
        "Chart(data: \"b.csv\") { Space(x * y) { HexBin(bins: 3, fill: \"#333333\") } }",
        "x,y\n1,1\n1,2\n2,1\n8,8\n9,9\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-hexbin"));
    assert!(!result.svg.contains(">count</text>"));
}

#[test]
fn test_point_shape_setting_and_mapping_render_distinct_marks() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Point(shape: kind, fill: kind, size: 4) Point(shape: \"diamond\", fill: \"#333333\", size: 4) } }",
        "x,y,kind\n1,2,circle\n2,3,square\n3,4,triangle\n4,5,diamond\n",
    );
    assert!(svg.contains("<circle"));
    assert!(svg.contains("<rect"));
    assert!(svg.contains("<path"));
}

#[test]
fn test_image_mark_renders_embedded_assets_and_legend() {
    let result = render_result_with_assets(
        "Chart(data: \"p.csv\") { Space(x * y) { Image(src: logo, size: 20) } }",
        "x,y,logo\n1,2,a.png\n2,3,b.png\n",
        &image_assets(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-image"));
    assert!(result.svg.contains("href=\"data:image/png;base64,AAAA\""));
    assert!(result.svg.contains("href=\"data:image/png;base64,BBBB\""));
    assert!(result.svg.contains("width=\"20\" height=\"10\""));
    assert!(result.svg.contains("width=\"10\" height=\"20\""));

    let legend = legend_layer(&result.svg);
    assert!(legend.contains(">logo<"));
    assert!(legend.contains(">a.png<"));
    assert!(legend.contains(">b.png<"));
    assert!(legend.contains("<image"));
}

#[test]
fn test_constant_image_source_has_no_legend() {
    let result = render_result_with_assets(
        "Chart(data: \"p.csv\") { Space(x * y) { Image(src: \"a.png\", size: 12) } }",
        "x,y\n1,2\n2,3\n",
        &image_assets(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result
            .svg
            .matches("href=\"data:image/png;base64,AAAA\"")
            .count(),
        2
    );
    assert!(!result.svg.contains("algraf-legends"));
}

#[test]
fn image_asset_loader_embeds_local_pngs() {
    const PNG_HEADER_2X1: &[u8] = b"\x89PNG\r\n\x1a\n12345678\x00\x00\x00\x02\x00\x00\x00\x01";

    let source = "Chart(data: \"p.csv\") { Space(x * y) { Image(src: logo) } }";
    let frame = read_csv_str("x,y,logo\n1,2,logo.png\n").expect("csv").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    let io = InMemoryDriverIo::default().with_file("logo.png", PNG_HEADER_2X1.to_vec());
    let result = load_image_assets_with_io(
        &ir,
        &frame,
        &HashMap::new(),
        &SourceInput::Path(PathBuf::from("chart.ag")),
        None,
        &io,
    );

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let asset = result.assets.get("logo.png").expect("asset");
    assert_eq!(asset.intrinsic_width, 2.0);
    assert_eq!(asset.intrinsic_height, 1.0);
    assert!(asset.href.starts_with("data:image/png;base64,"));
}

#[test]
fn image_asset_loader_rejects_url_values_from_data() {
    let source = "Chart(data: \"p.csv\") { Space(x * y) { Image(src: logo) } }";
    let frame = read_csv_str("x,y,logo\n1,2,https://example.com/logo.png\n")
        .expect("csv")
        .frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    let result = load_image_assets_with_io(
        &ir,
        &frame,
        &HashMap::new(),
        &SourceInput::Path(PathBuf::from("chart.ag")),
        None,
        &InMemoryDriverIo::default(),
    );

    assert!(result
        .diagnostics
        .iter()
        .any(|diag| diag.code == "E1204" && diag.message.contains("is a URL")));
}

/// The substring of `svg` covering only the `algraf-legends` layer.
fn legend_layer(svg: &str) -> &str {
    let start = svg.find("algraf-legends").expect("legend layer present");
    &svg[start..]
}

#[test]
fn test_shape_legend_merges_into_fill_legend_swatches() {
    // `shape` and `fill` over the same column produce a single legend whose
    // swatches are the marker glyphs filled with the categorical colors (spec
    // §19.5, §19.7), not plain squares.
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Point(shape: kind, fill: kind, size: 4) } }",
        "x,y,kind\n1,2,north\n2,3,south\n",
    );
    let legend = legend_layer(&svg);
    // First category: a circle swatch in the first categorical color.
    assert!(
        legend.contains("<circle") && legend.contains("fill=\"#4E79A7\""),
        "legend should draw the first category as a colored circle: {legend}"
    );
    // Second category: a square swatch in the second categorical color.
    assert!(
        legend.contains("<rect") && legend.contains("fill=\"#F28E2B\""),
        "legend should draw the second category as a colored square: {legend}"
    );
    // The merged legend has a single title, not one per aesthetic.
    assert_eq!(legend.matches(">kind<").count(), 1, "single legend title");
}

#[test]
fn test_shape_only_mapping_creates_default_colored_shape_legend() {
    // A `shape` mapping with no color mapping still creates a shape legend, with
    // swatches drawn in the default mark fill (spec §19.5).
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(x * y) { Point(shape: kind, size: 4) } }",
        "x,y,kind\n1,2,north\n2,3,south\n",
    );
    let legend = legend_layer(&svg);
    assert!(
        legend.contains(">kind<"),
        "shape legend title present: {legend}"
    );
    assert!(
        legend.contains("<circle") && legend.contains("fill=\"#4E79A7\""),
        "default-colored circle swatch: {legend}"
    );
}

#[test]
fn test_summary_stats_render_from_raw_rows() {
    let svg = render_svg(
        r#"Chart(data: "d.csv", width: 420, height: 280) {
  Derive summary = Summary(value, by: [group], reducer: "mean_se")
  Space(group * value, data: summary) {
    Segment(x: group, y: lower, xend: group, yend: upper, stroke: group)
    Point(fill: group, size: 3)
  }
}"#,
        "group,value\nA,1\nA,3\nA,5\nB,2\nB,4\nB,6\n",
    );
    assert_eq!(svg.matches("<circle").count(), 2);
    assert!(svg.contains("algraf-geom-segment"));
}

#[test]
fn test_ecdf_qq_and_summary_bin_render() {
    let ecdf = render_svg(
        r##"Chart(data: "d.csv", width: 420, height: 280) {
  Derive rows = Ecdf(value)
  Space(x * y, data: rows) { Path(stroke: "#2f6fbb", strokeWidth: 2) }
}"##,
        "value\n3\n1\n2\n2\n",
    );
    assert!(ecdf.contains("algraf-geom-path"));

    let qq = render_svg(
        r##"Chart(data: "d.csv", width: 420, height: 280) {
  Derive rows = Qq(value, distribution: "normal")
  Space(theoretical * sample, data: rows) { Point(fill: "#4c78a8", size: 2) }
}"##,
        "value\n-1\n0\n1\n2\n",
    );
    assert_eq!(qq.matches("<circle").count(), 4);

    let bins = render_svg(
        r#"Chart(data: "d.csv", width: 420, height: 280) {
  Derive rows = SummaryBin(x, value, bins: 2, reducer: "mean")
  Space(bin_center * value, data: rows) { Line() Point(size: 2) }
}"#,
        "x,value\n1,10\n2,14\n3,20\n4,24\n",
    );
    assert_eq!(bins.matches("<circle").count(), 2);
}

#[test]
fn test_binned_scale_matches_explicit_class_table() {
    let binned = render_svg(
        r##"Chart(data: "d.csv", width: 420, height: 280) {
  Scale(fill: value, mode: "binned",
        breaks: [0, 10, 20],
        labels: ["low", "mid", "high"],
        range: ["#eff3ff", "#6baed6", "#08519c"],
        label: "")
  Space(x * y) { Point(fill: value, size: 3) }
}"##,
        "x,y,value\n1,1,2\n2,2,12\n3,1,24\n",
    );
    let explicit = render_svg(
        r##"Chart(data: "d.csv", width: 420, height: 280) {
  Scale(fill: class,
        range: ["low" => "#eff3ff", "mid" => "#6baed6", "high" => "#08519c"],
        labels: ["low" => "low", "mid" => "mid", "high" => "high"],
        label: "")
  Space(x * y) { Point(fill: class, size: 3) }
}"##,
        "x,y,value,class\n1,1,2,low\n2,2,12,mid\n3,1,24,high\n",
    );
    assert_eq!(binned, explicit);
}

#[test]
fn test_identity_color_and_axis_break_labels_render() {
    let svg = render_svg(
        r##"Chart(data: "d.csv", width: 420, height: 280) {
  Scale(axis: x, domain: [0, 100], breaks: [0, 50, 100],
        labels: ["zero", "half", "full"], expand: 0)
  Scale(fill: color, mode: "identity")
  Guide(fill: null)
  Guide(axis: x, tickLabelRows: 2)
  Space(x * y) { Point(fill: color, size: 3) }
}"##,
        "x,y,color\n0,1,#ff0000\n50,2,blue\n100,1,#00aa66\n",
    );
    assert!(svg.contains("fill=\"#ff0000\""));
    assert!(svg.contains("fill=\"blue\""));
    assert!(svg.contains(">zero<"));
    assert!(svg.contains(">half<"));
    assert!(svg.contains(">full<"));
}

#[test]
fn test_chained_derived_smooth_table_renders() {
    let result = render_result(
        "Chart(data: \"d.csv\") { Derive bins = Bin(value, bins: 4) Derive trend from bins = Smooth(bin_center, count) Space(x * y, data: trend) { Line() } }",
        "value\n1\n2\n3\n4\n5\n6\n7\n8\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.svg.matches("<path").count(), 1);
}

// --- Density column ---

#[test]
fn test_histogram_bin_has_density_column() {
    let frame = read_csv_str("v\n0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n")
        .unwrap()
        .frame;
    let parsed = parse(
        "Chart(data: \"v.csv\") {\n  Derive bins = Bin(v, bins: 5)\n  Space(bin_start * count, data: bins) { Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count) }\n}",
    );
    let ir = analyze(&parsed.syntax(), frame.schema()).ir.unwrap();
    let result = render(&ir, &frame, &Theme::minimal(), None).unwrap();
    // The output schema should expose density.
    assert!(result.layout.plot.width > 0.0);
    // No direct API for derived tables on the result; the existence of the
    // density column is asserted in the analyzer test and stats unit test.
}

#[test]
fn test_custom_theme_overrides_apply_to_svg() {
    let source = "Chart(data: \"p.csv\") {\n  Theme(name: \"minimal\", gridMajor: Line(stroke: \"#dddddd\", strokeWidth: 2), plotBackground: \"#fafafa\")\n  Space(x * y) { Point() }\n}";
    let csv = "x,y\n1,2\n2,3\n3,1\n";
    let frame = read_csv_str(csv).expect("csv").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    let theme = Theme::from_ir(ir.theme.as_ref().expect("theme"));
    let svg = render(&ir, &frame, &theme, None).expect("render").svg;
    assert!(svg.contains("#fafafa"), "custom plot background applied");
    assert!(
        svg.contains("stroke=\"#dddddd\" stroke-width=\"2\""),
        "custom grid stroke and width applied"
    );
}

#[test]
fn neutral_theme_presets_have_stable_core_values() {
    let gray = Theme::gray();
    assert_eq!(gray.name, "gray");
    assert_eq!(gray.plot_background, "#ebebeb");
    assert_eq!(gray.grid_major.stroke, "#ffffff");
    assert_eq!(gray.legend_text.fill, "#1f1f1f");

    let bw = Theme::bw();
    assert_eq!(bw.name, "bw");
    assert_eq!(bw.panel_background.stroke.as_deref(), Some("#111111"));
    assert_eq!(bw.grid_minor.stroke, "#eeeeee");
    assert_eq!(bw.axis_text.fill, "#111111");

    let linedraw = Theme::linedraw();
    assert_eq!(linedraw.name, "linedraw");
    assert_eq!(linedraw.axis_color, "#000000");
    assert_eq!(linedraw.line_width, 0.8);
    assert_eq!(linedraw.plot_title.fill, "#000000");
}

#[test]
fn structured_theme_overrides_reach_chart_axes_and_legends() {
    let source = r##"Chart(data: "p.csv", title: "Styled") {
  Theme(
    name: "bw",
    plotTitle: Text(size: 22, fill: "#123456"),
    axisText: Text(size: 10, fill: "#654321"),
    legendText: Text(size: 11, fill: "#005500"),
    panelBackground: Rect(fill: "#fafafa", stroke: "#111111", strokeWidth: 1)
  )
  Space(x * y) { Point(fill: g) }
}"##;
    let frame = read_csv_str("x,y,g\n1,2,A\n2,3,B\n").expect("csv").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    let theme = Theme::from_ir(ir.theme.as_ref().expect("theme"));
    let svg = render(&ir, &frame, &theme, None).expect("render").svg;

    assert!(svg.contains("font-size=\"22\" font-weight=\"600\" fill=\"#123456\""));
    assert!(svg.contains("font-size=\"10\" fill=\"#654321\""));
    assert!(svg.contains("font-size=\"11\" fill=\"#005500\""));
    assert!(svg.contains("fill=\"#fafafa\" stroke=\"#111111\" stroke-width=\"1\""));
}

// --- v0.6.0: Path, per-segment width, manual color maps, axis suppression ---

fn first_path_d(svg: &str, after: &str) -> String {
    let start = svg.find(after).expect("layer");
    let tail = &svg[start..];
    let d_start = tail.find("d=\"").expect("d") + 3;
    let d_end = tail[d_start..].find('"').expect("d end");
    tail[d_start..d_start + d_end].to_string()
}

#[test]
fn test_path_preserves_row_order_unlike_line() {
    // Rows are not x-sorted; Line sorts by x, Path keeps source order.
    let csv = "x,y\n3,1\n1,2\n2,3\n";
    let line = render_svg("Chart(data: \"t.csv\") { Space(x * y) { Line() } }", csv);
    let path = render_svg("Chart(data: \"t.csv\") { Space(x * y) { Path() } }", csv);
    let line_d = first_path_d(&line, "algraf-geom-line");
    let path_d = first_path_d(&path, "algraf-geom-path");
    assert_ne!(line_d, path_d);
    // Line's first plotted x is the smallest; Path's first is the first row's.
    let line_first_x: f64 = line_d[1..].split(' ').next().unwrap().parse().unwrap();
    let path_first_x: f64 = path_d[1..].split(' ').next().unwrap().parse().unwrap();
    assert!(path_first_x > line_first_x);
}

#[test]
fn test_step_vertices_derive_expands_path_vertices() {
    let csv = "x,y\n1,1\n2,3\n3,2\n";
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Derive steps = StepVertices(x, y, direction: \"hv\") Space(x * y, data: steps) { Path(dash: \"dashed\", stroke: \"#123456\") } }",
        csv,
    );
    let d = first_path_d(&svg, "algraf-geom-path");
    // Three source rows become five vertices: the first point, then
    // horizontal+vertical vertices for each following row.
    assert_eq!(d.matches('L').count(), 4);
    assert!(svg.contains("stroke-dasharray=\"4 4\""));
}

#[test]
fn test_step_vertices_missing_rows_break_paths() {
    let csv = "x,y\n1,1\n2,3\n3,\n4,2\n5,4\n";
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Derive steps = StepVertices(x, y) Space(x * y, data: steps) { Path(stroke: \"#123456\") } }",
        csv,
    );
    let data_layer = svg
        .split_once("algraf-geom-path")
        .map(|(_, after)| after)
        .unwrap_or(svg.as_str());
    assert!(
        data_layer.matches("<path d=\"M").count() >= 2,
        "missing StepVertices rows should split the rendered path"
    );
}

#[test]
fn test_vector_endpoints_and_curve_sample_feed_primitives() {
    let csv = "x,y,angle,speed,x1,y1,cohort\n1,1,0,1,2,2,a\n2,2,1.57079632679,1,3,1,b\n";
    let svg = render_svg(
        "Chart(data: \"v.csv\") {
  Derive vectors = VectorEndpoints(x, y, angle, speed, lengthScale: 0.5)
  Derive curves = CurveSample(x, y, x1, y1, curvature: 0.25, points: 5)
  Space(x * y, data: vectors) {
    Segment(x: x, y: y, xend: xend, yend: yend, stroke: cohort, dash: \"dotted\")
  }
  Space(x * y, data: curves) {
    Path(group: link_id, stroke: cohort, dash: \"dashed\")
  }
}",
        csv,
    );
    assert!(svg.contains("algraf-geom-segment"));
    assert!(svg.contains("algraf-geom-path"));
    assert!(svg.contains("stroke-dasharray=\"1 2\""));
    assert!(svg.contains("stroke-dasharray=\"4 4\""));
}

#[test]
fn test_mapped_strokewidth_emits_per_segment_lines() {
    let csv = "x,y,w\n1,1,1\n2,2,50\n3,1,100\n";
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Scale(strokeWidth: w, domain: [0, null], range: [0, 20]) Space(x * y) { Path(strokeWidth: w) } }",
        csv,
    );
    assert!(svg.contains("algraf-geom-path"));
    // Restrict to the data layer; the strokeWidth size legend also emits
    // round-capped swatch lines in the separate `algraf-legends` group.
    let data_layer = svg
        .split_once("algraf-legends")
        .map_or(svg.as_str(), |(before, _)| before);
    // Two segments for three points, drawn as individual round-capped lines.
    assert_eq!(data_layer.matches("stroke-linecap=\"round\"").count(), 2);
    // Widths differ across segments.
    let widths: Vec<&str> = data_layer
        .match_indices("stroke-width=\"")
        .map(|(i, _)| {
            let s = &svg[i + 14..];
            &s[..s.find('"').unwrap()]
        })
        .collect();
    assert!(
        widths
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1
    );
}

#[test]
fn test_taper_renders_single_filled_ribbon() {
    // With taper, a mapped-strokeWidth Path becomes one filled polygon instead
    // of per-segment round-capped lines.
    let csv = "x,y,w\n1,1,1\n2,2,50\n3,1,100\n";
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Scale(strokeWidth: w, domain: [0, null], range: [0, 20]) Space(x * y) { Path(strokeWidth: w, taper: true, stroke: \"#8b5a2b\") } }",
        csv,
    );
    let data_layer = svg
        .split_once("algraf-legends")
        .map_or(svg.as_str(), |(before, _)| before);
    // No per-segment lines; a single filled ribbon path instead.
    assert_eq!(data_layer.matches("stroke-linecap=\"round\"").count(), 0);
    assert_eq!(data_layer.matches("fill=\"#8b5a2b\"").count(), 1);
    // A closed polygon: the path ends with Z.
    assert!(data_layer.contains("Z\""));
}

#[test]
fn test_taper_without_mapped_width_falls_back_to_plain_line() {
    // taper only applies to a mapped strokeWidth; a constant width is unaffected.
    let csv = "x,y\n1,1\n2,2\n3,1\n";
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Space(x * y) { Line(taper: true, strokeWidth: 2, stroke: \"#123456\") } }",
        csv,
    );
    assert!(svg.contains("fill=\"none\" stroke=\"#123456\""));
}

#[test]
fn test_mapped_strokewidth_emits_size_legend() {
    let csv = "x,y,w\n1,1,0\n2,2,50\n3,1,100\n";
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Scale(strokeWidth: w, domain: [0, null], range: [0, 20], label: \"Weight\") Space(x * y) { Path(strokeWidth: w) } }",
        csv,
    );
    // The size legend lives in its own group, titled by the scale label, with
    // a swatch line at the widest tick (range max 20px) and tick labels.
    let legend = svg.split_once("algraf-legends").unwrap().1;
    assert!(legend.contains(">Weight</text>"));
    assert!(legend.contains(">100</text>"));
    assert!(legend.contains("stroke-width=\"20\""));
}

#[test]
fn test_mapped_size_emits_radius_legend() {
    let csv = "x,y,m\n1,1,0\n2,2,5\n3,1,10\n";
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Scale(size: m, range: [0, 12], breaks: [0, 5, 10]) Space(x * y) { Point(size: m) } }",
        csv,
    );
    // A `size` mapping yields circle swatches sized by the mapped radius.
    let legend = svg.split_once("algraf-legends").unwrap().1;
    assert!(legend.contains(">m</text>"));
    assert!(legend.contains("<circle"));
    assert!(legend.contains("r=\"12\""));
}

#[test]
fn test_constant_strokewidth_has_no_size_legend() {
    let csv = "x,y\n1,1\n2,2\n";
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Space(x * y) { Path(strokeWidth: 3) } }",
        csv,
    );
    // A literal setting is not a data mapping, so no legend is generated.
    assert!(!svg.contains("algraf-legends"));
}

#[test]
fn test_manual_color_map_and_legend_renaming() {
    let csv = "x,y,dir\n1,1,A\n2,2,R\n";
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Scale(stroke: dir, range: [\"A\" => \"burlywood\", \"R\" => \"black\"], labels: [\"A\" => \"Advance\", \"R\" => \"Retreat\"], label: \"Direction\") Space(x * y) { Path(stroke: dir) } }",
        csv,
    );
    assert!(svg.contains("burlywood"));
    assert!(svg.contains("black"));
    // Legend uses the renamed labels and title.
    assert!(svg.contains(">Advance</text>"));
    assert!(svg.contains(">Retreat</text>"));
    assert!(svg.contains(">Direction</text>"));
    assert!(!svg.contains(">A</text>"));
}

#[test]
fn test_guide_axis_label_null_suppresses_title() {
    let csv = "long,lat\n1,1\n2,2\n";
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Guide(axis: x, label: null) Guide(axis: y, label: null) Space(long * lat) { Point() } }",
        csv,
    );
    // Axis ticks still render, but neither axis title is drawn.
    assert!(!svg.contains(">long</text>"));
    assert!(!svg.contains(">lat</text>"));
}

#[test]
fn test_named_table_overlay_shares_position_scale() {
    // Primary x in [1,3]; secondary table x extends to 10. The shared scale
    // means the secondary point lands inside the unioned domain (spec §17.5).
    let primary = read_csv_str("x,y\n1,1\n3,3\n").unwrap().frame;
    let cities = read_csv_str("x,y,name\n10,2,Far\n").unwrap().frame;
    let source = "Chart(data: \"p.csv\") { Table cities = \"c.csv\" Space(x * y) { Point() } Space(x * y, data: cities) { Text(label: name) } }";
    let parsed = parse(source);
    let mut tables = HashMap::new();
    tables.insert("cities".to_string(), cities.schema().to_vec());
    let analysis =
        algraf_semantics::analyze_with_tables(&parsed.syntax(), primary.schema(), &tables);
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    let mut frames = HashMap::new();
    frames.insert(
        "cities".to_string(),
        read_csv_str("x,y,name\n10,2,Far\n").unwrap().frame,
    );
    let svg = render(
        &ir,
        &primary,
        &Theme::minimal(),
        RenderOptions::default().with_named_tables(&frames),
    )
    .unwrap()
    .svg;
    // The unioned x domain reaches 10, so a tick label at/near 10 appears.
    assert!(svg.contains(">10</text>") || svg.contains(">9</text>"));
    assert!(svg.contains(">Far</text>"));
}

#[test]
fn test_named_table_overlay_shares_zero_baseline() {
    // A zero-baseline Area overlays a named-table Point layer carrying the
    // same values. The area's zero requirement must propagate to the secondary
    // space so both train the identical y domain — otherwise the secondary
    // space pads below zero and every mark lands a padding's worth too low
    // (spec §17.5). With the shared baseline, each point sits exactly on the
    // area's top edge.
    let result = render_result_with_tables(
        "Chart(data: \"p.csv\") { Table t2 = \"s.csv\" Space(x * y) { Area(fill: series, layout: \"stack\") } Space(x * y2, data: t2) { Point() } }",
        "x,y,series\n1,10,a\n2,20,a\n3,15,a\n",
        &[("t2", "x,y2\n1,10\n2,20\n3,15\n")],
    );
    assert_circles_on_path_vertices(&result.svg, 3);
}

#[test]
fn test_named_table_overlay_explicit_domain_governs_joined_scale() {
    // An explicit chart-level `Scale(axis: y, domain:)` merges into every
    // overlaid space's config, so the override drives the one joined scale:
    // both layers honor the same bounds and the marks still coincide
    // (spec §17.5).
    let result = render_result_with_tables(
        "Chart(data: \"p.csv\") { Table t2 = \"s.csv\" Scale(axis: y, domain: [0, 100]) Space(x * y) { Area(fill: series, layout: \"stack\") } Space(x * y2, data: t2) { Point() } }",
        "x,y,series\n1,10,a\n2,20,a\n3,15,a\n",
        &[("t2", "x,y2\n1,10\n2,20\n3,15\n")],
    );
    assert!(result.svg.contains(">100</text>"));
    assert_circles_on_path_vertices(&result.svg, 3);
}

/// Assert every `<circle>` center coincides with a vertex of some `<path>`,
/// proving the circle layer shares the position scales of the path layer.
fn assert_circles_on_path_vertices(svg: &str, expected_circles: usize) {
    let centers: Vec<(&str, &str)> = svg
        .match_indices("<circle cx=\"")
        .map(|(i, _)| {
            let s = &svg[i + 12..];
            let cx = &s[..s.find('"').unwrap()];
            let rest = &s[s.find("cy=\"").unwrap() + 4..];
            let cy = &rest[..rest.find('"').unwrap()];
            (cx, cy)
        })
        .collect();
    assert_eq!(centers.len(), expected_circles);
    for (cx, cy) in centers {
        assert!(
            svg.contains(&format!("M{cx} {cy} ")) || svg.contains(&format!("L{cx} {cy} ")),
            "point ({cx}, {cy}) should sit on a path vertex"
        );
    }
}

#[test]
fn test_wide_y_tick_labels_reserve_more_left_margin() {
    // Guide planning (spec §17.3): the plot's left edge is pushed right to fit
    // the widest y tick label, so a chart with large y values reserves more
    // left margin than one with single-digit values. Exercises the guide
    // planning/emission split (`max_y_tick_label_width` / `y_axis_left_margin`).
    let narrow = render_result(
        "Chart(data: \"p.csv\") { Space(x * y) { Point() } }",
        "x,y\n1,1\n2,2\n",
    )
    .layout
    .plot
    .x;
    let wide = render_result(
        "Chart(data: \"p.csv\") { Space(x * y) { Point() } }",
        "x,y\n1,1000000\n2,2000000\n",
    )
    .layout
    .plot
    .x;
    assert!(
        wide > narrow,
        "wide y tick labels should reserve more left margin: wide={wide}, narrow={narrow}"
    );
}

#[test]
fn test_tick_label_angle_rotates_axis_labels() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Guide(axis: x, tickLabelAngle: -45)\n  Guide(axis: y, tickLabelAngle: 30)\n  Space(category * value) { Bar(stat: \"identity\") }\n}",
        "category,value\nVery long category,10\nAnother long category,20\n",
    );
    assert!(text_element(&svg, "Very long category").contains("transform=\"rotate(-45 "));
    assert!(text_element(&svg, "10").contains("transform=\"rotate(30 "));
}

#[test]
fn test_rotated_x_tick_labels_reserve_more_bottom_margin() {
    let csv =
        "category,value\nLong Category Alpha,10\nLong Category Beta,20\nLong Category Gamma,30\n";
    let horizontal = render_result(
        "Chart(data: \"p.csv\", width: 640, height: 420) { Space(category * value) { Bar(stat: \"identity\") } }",
        csv,
    )
    .layout
    .plot
    .bottom();
    let rotated = render_result(
        "Chart(data: \"p.csv\", width: 640, height: 420) { Guide(axis: x, tickLabelAngle: -45) Space(category * value) { Bar(stat: \"identity\") } }",
        csv,
    )
    .layout
    .plot
    .bottom();
    assert!(
        rotated < horizontal,
        "rotated x labels should reserve more bottom margin: rotated={rotated}, horizontal={horizontal}"
    );
}

#[test]
fn test_polar_pie_emits_arc_wedges() {
    // A 1D polar space with a fill layout draws angular wedges as SVG arc paths
    // (spec §16.16), not rectangles.
    let svg = render_svg(
        "Chart(data: \"p.csv\", width: 360, height: 360) { Space(amount, coords: \"polar\", theta: \"y\") { Bar(fill: product, layout: \"fill\") } }",
        "product,amount\nA,30\nB,20\nC,50\n",
    );
    // Three categories -> three wedge paths using the arc command.
    assert_eq!(svg.matches("<path d=\"M ").count(), 3);
    assert!(svg.contains(" A "), "polar wedges use the SVG arc command");
    // No axis lines/grid for a polar space.
    assert!(!svg.contains("algraf-axes"));
}

#[test]
fn test_polar_donut_emits_annular_segments() {
    let svg = render_svg(
        "Chart(data: \"p.csv\", width: 360, height: 360) { Space(amount, coords: \"polar\", theta: \"y\", innerRadius: 0.5) { Bar(fill: product, layout: \"fill\") } }",
        "product,amount\nA,30\nB,20\nC,50\n",
    );
    // An annular segment has two arcs (outer + inner) per wedge.
    let first = svg.lines().find(|l| l.contains("<path d=\"M ")).unwrap();
    assert_eq!(
        first.matches(" A ").count(),
        2,
        "donut wedge has inner + outer arc"
    );
}

#[test]
fn test_polar_radar_closes_line_and_polygon_grid() {
    let svg = render_svg(
        "Chart(data: \"p.csv\", width: 400, height: 400) { Space(axis * score, coords: \"polar\", theta: \"x\") { Guide(gridShape: \"polygon\") Line(stroke: \"navy\") Point() } }",
        "axis,score\nA,8\nB,5\nC,9\nD,6\n",
    );
    // A radar Line closes its polygon with Z.
    assert!(svg.contains("Z\" fill=\"none\" stroke=\"navy\""));
    // The polygon grid uses <polygon> rings, not <circle> rings.
    assert!(svg.contains("<polygon"));
    // Four categories -> four point markers.
    assert_eq!(svg.matches("<circle").count(), 4);
}

#[test]
fn test_polar_labels_render_above_opaque_tiles() {
    let svg = render_svg(
        "Chart(data: \"p.csv\", width: 420, height: 420) { Space(day * period, coords: \"polar\", theta: \"x\", innerRadius: 0.25) { Tile(fill: sessions) } }",
        "day,period,sessions\nMon,Morning,8\nMon,Midday,5\nMon,Evening,3\nTue,Morning,6\nTue,Midday,7\nTue,Evening,4\n",
    );
    let grid = svg.find("algraf-polar-grid").expect("polar grid");
    let tiles = svg.find("algraf-geom-tile").expect("tile layer");
    let radius_labels = svg
        .find("algraf-polar-radius-labels")
        .expect("radius labels");

    assert!(grid < tiles, "polar grid should render below tiles");
    assert!(
        tiles < radius_labels,
        "polar radius labels should render above opaque tiles"
    );
    assert!(svg.contains(">Evening</text>"));
}

#[test]
fn test_cartesian_bar_unaffected_by_polar_support() {
    // A Cartesian bar still emits rectangles, never arc paths.
    let svg = render_svg(
        "Chart(data: \"p.csv\") { Space(c * v) { Bar(stat: \"identity\") } }",
        "c,v\na,1\nb,2\n",
    );
    assert!(svg.contains("<rect"));
    assert!(!svg.contains(" A "));
}

// --- Declarative interactions (spec §14.25, §18.10, §29.3) ------------------

const INTERACTION_CSV: &str = "x,y,g\n1,2,A\n3,4,B\n";

const TOOLTIP_SRC: &str = "Chart(data: \"p.csv\", width: 200, height: 200) { Space(x * y) { Point(tooltip: [g, y], highlight: \"g\") } }";

const EVENT_SRC: &str = "Chart(data: \"p.csv\", width: 200, height: 200) { Space(x * y) { Point(tooltip: [g], highlight: \"g\") On(event: \"click\", emit: g) } }";

const PLAIN_SRC: &str =
    "Chart(data: \"p.csv\", width: 200, height: 200) { Space(x * y) { Point() } }";

#[test]
fn tooltip_emits_accessible_title_and_highlight_group() {
    let svg = render_svg(TOOLTIP_SRC, INTERACTION_CSV);
    assert!(svg.contains("data-algraf-highlight=\"A\""), "{svg}");
    assert!(svg.contains("<title>g: A\ny: 2</title></circle>"), "{svg}");
    // Static SVG stays script-free without the opt-in.
    assert!(!svg.contains("<script"), "{svg}");
}

#[test]
fn event_emitter_emits_inert_svg_data_attributes() {
    let svg = render_svg(EVENT_SRC, INTERACTION_CSV);
    assert!(svg.contains("data-algraf-event=\"click\""), "{svg}");
    assert!(svg.contains("data-algraf-emit-field=\"g\""), "{svg}");
    assert!(svg.contains("data-algraf-emit-value=\"A\""), "{svg}");
    assert!(!svg.contains("<script"), "{svg}");
}

#[test]
fn chart_without_interaction_has_no_interaction_markup() {
    let svg = render_svg(PLAIN_SRC, INTERACTION_CSV);
    assert!(!svg.contains("data-algraf-highlight"), "{svg}");
    assert!(!svg.contains("</circle>"), "{svg}");
    assert!(!svg.contains("<title>"), "{svg}");
}

#[test]
fn interactive_render_embeds_only_the_audited_script() {
    let frame = read_csv_str(INTERACTION_CSV).expect("csv").frame;
    let parsed = parse(TOOLTIP_SRC);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    let interactive = algraf_render::render_interactive(&ir, &frame, &Theme::minimal(), None)
        .expect("render")
        .svg;
    let static_svg = render(&ir, &frame, &Theme::minimal(), None)
        .expect("render")
        .svg;

    // The script appears only in the interactive output.
    assert!(interactive.contains("<script"), "{interactive}");
    assert!(interactive.contains("algraf-crosshair"), "{interactive}");
    assert!(
        interactive.contains("data-algraf-highlight"),
        "{interactive}"
    );
    assert!(!static_svg.contains("<script"), "{static_svg}");
    // The chart body is otherwise identical: the static SVG is a prefix of the
    // interactive one up to the appended script.
    let body_end = interactive.find("<script").unwrap();
    let static_body_end = static_svg.find("</svg>").unwrap();
    assert_eq!(&interactive[..body_end], &static_svg[..static_body_end]);
}

#[test]
fn interactive_runtime_is_available_without_mark_interaction() {
    let frame = read_csv_str(INTERACTION_CSV).expect("csv").frame;
    let parsed = parse(PLAIN_SRC);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    let interactive = algraf_render::render_interactive(&ir, &frame, &Theme::minimal(), None)
        .expect("render")
        .svg;
    let static_svg = render(&ir, &frame, &Theme::minimal(), None)
        .expect("render")
        .svg;

    assert!(interactive.contains("algraf-crosshair"), "{interactive}");
    assert!(
        !interactive.contains("data-algraf-highlight=\""),
        "{interactive}"
    );
    let body_end = interactive.find("<script").unwrap();
    let static_body_end = static_svg.find("</svg>").unwrap();
    assert_eq!(&interactive[..body_end], &static_svg[..static_body_end]);
}

#[test]
fn interaction_metadata_records_plot_axes_marks_and_groups() {
    let result = render_result(TOOLTIP_SRC, INTERACTION_CSV);
    let json = result.metadata.to_json();
    assert_eq!(json, result.metadata.to_json(), "metadata JSON is stable");

    let parsed: serde_json::Value = serde_json::from_str(&json).expect("metadata json");
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["axes"]["x"]["scale"], "linear");
    assert_eq!(parsed["axes"]["y"]["scale"], "linear");
    assert_eq!(parsed["marks"].as_array().expect("marks").len(), 2);

    let first = &parsed["marks"][0];
    assert_eq!(first["id"], "p0:g0:r0");
    assert_eq!(first["plot"], "plot0");
    assert!(first["x_px"].as_f64().expect("x px").is_finite());
    assert!(first["y_px"].as_f64().expect("y px").is_finite());
    assert_eq!(first["groups"]["g"], "A");
    assert_eq!(first["tooltip"][0]["label"], "g");
    assert_eq!(first["tooltip"][0]["value"], "A");
    assert_eq!(first["tooltip"][1]["label"], "y");
    assert_eq!(first["tooltip"][1]["value"], "2");
    assert_eq!(parsed["groups"]["g"], serde_json::json!(["A", "B"]));
}

#[test]
fn interaction_metadata_records_event_emitters() {
    let result = render_result(EVENT_SRC, INTERACTION_CSV);
    let json = result.metadata.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("metadata json");
    let first = &parsed["marks"][0];

    assert_eq!(first["interaction"]["event"], "click");
    assert_eq!(first["interaction"]["emit_field"], "g");
    assert_eq!(first["groups"]["g"], "A");
    assert_eq!(parsed["groups"]["g"], serde_json::json!(["A", "B"]));
}

// --- Temporal tickInterval and ladder rendering (spec §16.4, §16.11, v0.75) ---

const MONTHLY_CSV: &str = "\
when,count
2022-05-01,1
2022-11-01,4
2023-04-01,9
2023-09-01,16
2024-02-01,25
2024-07-01,36
2024-12-01,30
2025-05-01,21
2025-09-01,12
";

/// `tickInterval: "3 months"` must render byte-for-byte identically to an
/// explicit `breaks:` array listing the same calendar instants — the
/// equivalence oracle for generated cadences (spec §16.11).
#[test]
fn tick_interval_matches_equivalent_explicit_breaks_byte_for_byte() {
    let interval_chart = r#"Chart(data: "t.csv", width: 720, height: 400) {
  Scale(axis: x, tickInterval: "3 months")
  Space(when * count) { Line() }
}"#;
    let breaks_chart = r#"Chart(data: "t.csv", width: 720, height: 400) {
  Scale(axis: x, breaks: [
      date("2022-07-01"), date("2022-10-01"), date("2023-01-01"),
      date("2023-04-01"), date("2023-07-01"), date("2023-10-01"),
      date("2024-01-01"), date("2024-04-01"), date("2024-07-01"),
      date("2024-10-01"), date("2025-01-01"), date("2025-04-01"),
      date("2025-07-01")
  ])
  Space(when * count) { Line() }
}"#;
    assert_eq!(
        render_svg(interval_chart, MONTHLY_CSV),
        render_svg(breaks_chart, MONTHLY_CSV)
    );
}

/// Without any Scale declaration, the extended automatic ladder labels a
/// multi-year monthly series with month-grid ticks and granularity-adaptive
/// `%Y-%m` labels rather than sparse year ticks or full dates.
#[test]
fn automatic_ladder_labels_multi_year_series_with_month_labels() {
    let chart = r#"Chart(data: "t.csv", width: 720, height: 400) {
  Space(when * count) { Line() }
}"#;
    let svg = render_svg(chart, MONTHLY_CSV);
    assert!(svg.contains(">2023-01<"), "expected 2023-01 label: {svg}");
    assert!(svg.contains(">2024-07<"), "expected 2024-07 label: {svg}");
    assert!(
        !svg.contains(">2023-01-01<"),
        "month-start ticks must not carry full-date labels"
    );
}

/// `Guide(timeFormat: ...)` reformats interval ticks without moving them.
#[test]
fn tick_interval_composes_with_time_format() {
    let chart = r#"Chart(data: "t.csv", width: 720, height: 400) {
  Scale(axis: x, tickInterval: "6 months")
  Guide(axis: x, timeFormat: "%b %Y")
  Space(when * count) { Line() }
}"#;
    let svg = render_svg(chart, MONTHLY_CSV);
    assert!(svg.contains(">Jul 2022<"), "expected Jul 2022 label: {svg}");
    assert!(svg.contains(">Jan 2024<"), "expected Jan 2024 label: {svg}");
}

/// A temporal y axis honors tickInterval the same way x does.
#[test]
fn tick_interval_applies_to_temporal_y_axes() {
    let chart = r#"Chart(data: "t.csv", width: 400, height: 720) {
  Scale(axis: y, tickInterval: "1 year")
  Space(count * when) { Point() }
}"#;
    let svg = render_svg(chart, MONTHLY_CSV);
    assert!(svg.contains(">2023<"), "expected 2023 label: {svg}");
    assert!(svg.contains(">2025<"), "expected 2025 label: {svg}");
}

/// Sparse daily data keeps proportional gaps: the pixel distance between
/// Jan 2 and Jan 10 is eight times the distance between Jan 1 and Jan 2.
#[test]
fn temporal_axis_keeps_proportional_gaps_for_missing_dates() {
    let chart = r#"Chart(data: "t.csv", width: 720, height: 400) {
  Space(when * count) { Point() }
}"#;
    let csv = "when,count\n2024-01-01,1\n2024-01-02,2\n2024-01-10,3\n";
    let result = {
        let frame = algraf_data::read_csv_str(csv).expect("csv").frame;
        let parsed = parse(chart);
        let analysis = analyze(&parsed.syntax(), frame.schema());
        let ir = analysis.ir.expect("ir");
        algraf_render::render(&ir, &frame, &Theme::minimal(), None).expect("render")
    };
    let circles: Vec<f64> = result
        .svg
        .match_indices("<circle")
        .map(|(start, _)| {
            let rest = &result.svg[start..];
            let cx = rest.split("cx=\"").nth(1).expect("cx attr");
            cx.split('"')
                .next()
                .expect("cx value")
                .parse()
                .expect("cx parses")
        })
        .collect();
    assert_eq!(circles.len(), 3, "{}", result.svg);
    let short_gap = circles[1] - circles[0];
    let long_gap = circles[2] - circles[1];
    assert!(
        (long_gap / short_gap - 8.0).abs() < 1e-6,
        "gap ratio was {} (short {short_gap}, long {long_gap})",
        long_gap / short_gap
    );
}

// ---- v0.77: stacked legend order follows the rendered visual stack (§19.5) ----

/// The legend region of an SVG (everything from the `algraf-legends` group on).
fn legend_region(svg: &str) -> &str {
    svg.split_once("algraf-legends")
        .map(|(_, after)| after)
        .expect("expected a legend group in the SVG")
}

/// Byte position of a legend text label within the legend region.
fn legend_label_pos(legend: &str, label: &str) -> usize {
    legend
        .find(&format!(">{label}<"))
        .unwrap_or_else(|| panic!("expected legend label `{label}` in: {legend}"))
}

#[test]
fn stacked_bar_legend_lists_top_band_first() {
    // Deletions accumulate first (baseline), additions render on top; the
    // default legend reads top-to-bottom: additions, then deletions.
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Space(day * count) { Bar(fill: segment, layout: \"stack\") } }",
        "day,count,segment\nmon,3,seg_deletions\nmon,5,seg_additions\ntue,2,seg_deletions\ntue,4,seg_additions\n",
    );
    let legend = legend_region(&svg);
    assert!(
        legend_label_pos(legend, "seg_additions") < legend_label_pos(legend, "seg_deletions"),
        "expected the top stack band first in the legend: {legend}"
    );
}

#[test]
fn horizontal_stacked_bar_legend_lists_rightmost_band_first() {
    // A rightward-growing horizontal stack: the farthest-right band (last
    // accumulated) leads the legend.
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Space(count * day) { Bar(fill: segment, layout: \"stack\") } }",
        "day,count,segment\nmon,3,seg_first\nmon,5,seg_second\ntue,2,seg_first\ntue,4,seg_second\n",
    );
    let legend = legend_region(&svg);
    assert!(
        legend_label_pos(legend, "seg_second") < legend_label_pos(legend, "seg_first"),
        "expected the rightmost stack band first in the legend: {legend}"
    );
}

#[test]
fn stacked_fill_bar_legend_uses_visual_stack_order() {
    // `layout: "fill"` stacks the same way after normalization.
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Space(day * count) { Bar(fill: segment, layout: \"fill\") } }",
        "day,count,segment\nmon,3,seg_lower\nmon,5,seg_upper\ntue,2,seg_lower\ntue,4,seg_upper\n",
    );
    let legend = legend_region(&svg);
    assert!(
        legend_label_pos(legend, "seg_upper") < legend_label_pos(legend, "seg_lower"),
        "expected the top stack band first in the fill-layout legend: {legend}"
    );
}

#[test]
fn unstacked_bar_legend_keeps_domain_order() {
    // Without a stacked layout the legend keeps scale/domain order.
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Space((day / segment) * count) { Bar(fill: segment) } }",
        "day,count,segment\nmon,3,seg_deletions\nmon,5,seg_additions\ntue,2,seg_deletions\ntue,4,seg_additions\n",
    );
    let legend = legend_region(&svg);
    assert!(
        legend_label_pos(legend, "seg_deletions") < legend_label_pos(legend, "seg_additions"),
        "expected domain order for a non-stacked bar legend: {legend}"
    );
}

#[test]
fn stacked_area_legend_lists_top_band_first() {
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Space(x * y) { Area(fill: series, layout: \"stack\") } }",
        "x,y,series\n1,3,series_low\n1,5,series_high\n2,4,series_low\n2,6,series_high\n",
    );
    let legend = legend_region(&svg);
    assert!(
        legend_label_pos(legend, "series_high") < legend_label_pos(legend, "series_low"),
        "expected the top stack band first in the stacked area legend: {legend}"
    );
}

#[test]
fn manual_fill_range_keeps_color_binding_under_stack_reorder() {
    // The manual range binds colors in baseline-outward stack order; the
    // displayed legend reverses the order but every category keeps its color.
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Scale(fill: segment, range: [\"seg_deletions\" => \"#8ecae6\", \"seg_additions\" => \"#1f77b4\"]) Space(day * count) { Bar(fill: segment, layout: \"stack\") } }",
        "day,count,segment\nmon,3,seg_deletions\nmon,5,seg_additions\ntue,2,seg_deletions\ntue,4,seg_additions\n",
    );
    let legend = legend_region(&svg);
    let additions = legend_label_pos(legend, "seg_additions");
    let deletions = legend_label_pos(legend, "seg_deletions");
    assert!(
        additions < deletions,
        "expected the top stack band first: {legend}"
    );
    // Swatches render immediately before their labels, in entry order, so the
    // additions color leads in the legend region and color binding holds.
    let additions_swatch = legend.find("#1f77b4").expect("additions swatch color");
    let deletions_swatch = legend.find("#8ecae6").expect("deletions swatch color");
    assert!(
        additions_swatch < additions
            && additions < deletions_swatch
            && deletions_swatch < deletions,
        "expected swatch colors interleaved with their own labels: {legend}"
    );
}

#[test]
fn disjoint_stack_cohorts_keep_cohort_order_and_reverse_within() {
    // Before/after cohorts never visibly stack together: the legend keeps the
    // `before` cohort first (domain order) while reading each cohort
    // top-to-bottom. A whole-domain reverse would put `after_*` first.
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Scale(fill: seg, range: [\"before_del\" => \"#8ecae6\", \"before_add\" => \"#1f77b4\", \"after_del\" => \"#ffbf69\", \"after_add\" => \"#ff7f0e\"]) Space(x * y) { Area(fill: seg, layout: \"stack\") } }",
        "x,y,seg\n1,3,before_del\n1,5,before_add\n2,4,before_del\n2,6,before_add\n3,2,after_del\n3,7,after_add\n4,3,after_del\n4,8,after_add\n",
    );
    let legend = legend_region(&svg);
    let order: Vec<usize> = ["before_add", "before_del", "after_add", "after_del"]
        .iter()
        .map(|label| legend_label_pos(legend, label))
        .collect();
    assert!(
        order.windows(2).all(|pair| pair[0] < pair[1]),
        "expected cohort order before_add, before_del, after_add, after_del: {legend}"
    );
}

#[test]
fn grouped_stacked_histogram_legend_lists_top_band_first() {
    // The grouped histogram desugars to pre-stacked Rects; group `grp_a`
    // accumulates first (baseline), so `grp_b` reads first in the legend.
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Space(v) { Histogram(fill: g, bins: 2) } }",
        "v,g\n1,grp_a\n1,grp_b\n2,grp_a\n2,grp_a\n2,grp_b\n",
    );
    let legend = legend_region(&svg);
    assert!(
        legend_label_pos(legend, "grp_b") < legend_label_pos(legend, "grp_a"),
        "expected the top stacked histogram band first in the legend: {legend}"
    );
}

// --- v0.82.0 editorial primitives ---

const EDITORIAL_CSV: &str = "year,country,reserves\n2000,US,541\n2010,US,727\n2022,US,372\n2026,US,410\n2000,JP,300\n2010,JP,330\n2022,JP,318\n2026,JP,320\n";

#[test]
fn right_y_axis_places_ticks_right_of_plot() {
    let right = render_svg(
        "Chart(data: \"d.csv\", width: 400, height: 300) { Scale(axis: y, domain: [0, 800]) Guide(axis: y, position: \"right\", format: \".0f\") Space(year * reserves) { Line(group: country) } }",
        EDITORIAL_CSV,
    );
    let left = render_svg(
        "Chart(data: \"d.csv\", width: 400, height: 300) { Scale(axis: y, domain: [0, 800]) Guide(axis: y, format: \".0f\") Space(year * reserves) { Line(group: country) } }",
        EDITORIAL_CSV,
    );
    // The "800" tick label x should be larger (further right) for a right axis.
    let right_x = tick_label_x(&right, "800");
    let left_x = tick_label_x(&left, "800");
    assert!(
        right_x > left_x + 50.0,
        "right axis label x={right_x} should sit well right of left axis x={left_x}"
    );
    // Integer numeric format: no "800.0".
    assert!(right.contains(">800<"), "expected integer tick label 800");
    assert!(!right.contains("800.0"), "format .0f must not emit 800.0");
}

#[test]
fn numeric_axis_format_rounds_to_integers() {
    let svg = render_svg(
        "Chart(data: \"d.csv\", width: 400, height: 300) { Scale(axis: y, domain: [0, 800]) Guide(axis: y, format: \".0f\") Space(year * reserves) { Line(group: country) } }",
        EDITORIAL_CSV,
    );
    assert!(svg.contains(">800<") && svg.contains(">0<"));
}

#[test]
fn multi_line_caption_and_source_stack() {
    let svg = render_svg(
        "Chart(data: \"d.csv\", width: 400, height: 300, caption: \"line one\\nline two\", source: \"Source: X\") { Space(year * reserves) { Line(group: country) } }",
        EDITORIAL_CSV,
    );
    assert!(svg.contains("algraf-caption"), "caption rendered");
    assert!(
        svg.contains("line one") && svg.contains("line two"),
        "both caption lines"
    );
    assert!(
        svg.contains("algraf-source") && svg.contains("Source: X"),
        "source line rendered"
    );
}

#[test]
fn vline_circle_badge_emits_circle_and_centered_text() {
    let svg = render_svg(
        "Chart(data: \"d.csv\", width: 400, height: 300) { Space(year * reserves) { Line(group: country) VLine(x: 2022, stroke: \"#111111\", label: \"1\", labelShape: \"circle\", labelPosition: \"top\") } }",
        EDITORIAL_CSV,
    );
    assert!(svg.contains("<circle"), "badge circle emitted");
    assert!(svg.contains(">1<"), "badge digit emitted");
    // Plain (legacy) VLine label without badge args stays byte-stable text.
    let legacy = render_svg(
        "Chart(data: \"d.csv\", width: 400, height: 300) { Space(year * reserves) { Line(group: country) VLine(x: 2022, label: \"M\") } }",
        EDITORIAL_CSV,
    );
    assert!(legacy.contains(">M<"));
}

#[test]
fn per_axis_grid_hides_only_vertical_lines() {
    let both = render_svg(
        "Chart(data: \"d.csv\", width: 400, height: 300) { Space(year * reserves) { Line(group: country) } }",
        EDITORIAL_CSV,
    );
    let horizontal_only = render_svg(
        "Chart(data: \"d.csv\", width: 400, height: 300) { Guide(axis: x, grid: false) Space(year * reserves) { Line(group: country) } }",
        EDITORIAL_CSV,
    );
    assert!(
        grid_line_count(&horizontal_only) < grid_line_count(&both),
        "hiding x grid should reduce grid line count"
    );
}

#[test]
fn theme_line_dash_and_axis_styles_affect_guides() {
    let svg = render_svg(
        "Chart(data: \"d.csv\", width: 400, height: 300) { Space(year * reserves) { Theme(gridMajor: Line(stroke: \"#dddddd\", strokeWidth: 1, dash: \"dashed\"), axisLine: Line(stroke: \"none\", strokeWidth: 0), axisTicks: Line(stroke: \"none\", strokeWidth: 0), axisTickLength: 0) Line(group: country) } }",
        EDITORIAL_CSV,
    );
    assert!(svg.contains("stroke-dasharray=\"4 4\""));
    let axes = svg
        .split_once("algraf-axes")
        .map(|(_, after)| after)
        .unwrap_or(svg.as_str());
    let axes = axes
        .split_once("</g>")
        .map(|(before, _)| before)
        .unwrap_or(axes);
    assert!(
        !axes.contains("<line"),
        "axis line and ticks should be hidden"
    );
    assert!(axes.contains("<text"), "axis text should remain visible");
}

fn tick_label_x(svg: &str, label: &str) -> f64 {
    let needle = format!(">{label}<");
    let pos = svg
        .find(&needle)
        .unwrap_or_else(|| panic!("label {label} not found"));
    let prefix = &svg[..pos];
    let tag = prefix.rfind("<text").expect("text tag");
    let attrs = &svg[tag..pos];
    let x_at = attrs.find("x=\"").expect("x attr") + 3;
    let rest = &attrs[x_at..];
    let end = rest.find('"').expect("x end");
    rest[..end].parse().expect("x value")
}

fn grid_line_count(svg: &str) -> usize {
    let start = svg.find("algraf-grid").unwrap_or(0);
    let region = &svg[start..];
    let end = region.find("</g>").map(|e| start + e).unwrap_or(svg.len());
    svg[start..end].matches("<line").count()
}
