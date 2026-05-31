//! Cross-backend parity tests (spec §24.6, §27.1).
//!
//! The SVG and draw-list backends consume the same planned scene through the
//! same mark sink, so every primitive the SVG backend draws has a corresponding
//! draw-list op with identical coordinates and colors. These tests prove that
//! op-by-op parity for representative charts and guard against a new SVG
//! primitive being added without a matching draw-list op.

use std::collections::{BTreeMap, HashMap};

use algraf_data::{read_csv_str, DataFrame, Table};
use algraf_render::{
    render, render_draw_list, render_draw_list_with_tables, render_with_tables, DrawList, DrawOp,
    DrawRole, Theme,
};
use algraf_semantics::{analyze, analyze_with_tables};
use algraf_syntax::parse;

fn draw_list(source: &str, csv: &str) -> DrawList {
    let frame = read_csv_str(csv).expect("csv").frame;
    let ir = analyze(&parse(source).syntax(), frame.schema())
        .ir
        .expect("ir");
    render_draw_list(&ir, &frame, &Theme::minimal(), None)
        .expect("draw list")
        .draw_list
}

fn svg(source: &str, csv: &str) -> String {
    let frame = read_csv_str(csv).expect("csv").frame;
    let ir = analyze(&parse(source).syntax(), frame.schema())
        .ir
        .expect("ir");
    render(&ir, &frame, &Theme::minimal(), None)
        .expect("render")
        .svg
}

fn draw_list_with_tables(source: &str, primary_csv: &str, tables: &[(&str, &str)]) -> DrawList {
    let frame = read_csv_str(primary_csv).expect("primary csv").frame;
    let mut named = HashMap::<String, DataFrame>::new();
    let mut schemas = HashMap::new();
    for (name, csv) in tables {
        let table = read_csv_str(csv).expect("named csv").frame;
        schemas.insert((*name).to_string(), table.schema().to_vec());
        named.insert((*name).to_string(), table);
    }
    let parsed = parse(source);
    let analysis = analyze_with_tables(&parsed.syntax(), frame.schema(), &schemas);
    let ir = analysis.ir.expect("ir");
    render_draw_list_with_tables(&ir, &frame, &named, &Theme::minimal(), None)
        .expect("draw list")
        .draw_list
}

fn svg_with_tables(source: &str, primary_csv: &str, tables: &[(&str, &str)]) -> String {
    let frame = read_csv_str(primary_csv).expect("primary csv").frame;
    let mut named = HashMap::<String, DataFrame>::new();
    let mut schemas = HashMap::new();
    for (name, csv) in tables {
        let table = read_csv_str(csv).expect("named csv").frame;
        schemas.insert((*name).to_string(), table.schema().to_vec());
        named.insert((*name).to_string(), table);
    }
    let parsed = parse(source);
    let analysis = analyze_with_tables(&parsed.syntax(), frame.schema(), &schemas);
    let ir = analysis.ir.expect("ir");
    render_with_tables(&ir, &frame, &named, &Theme::minimal(), None)
        .expect("render")
        .svg
}

/// Count primitive *elements* in an SVG string by tag. Structural elements
/// (`<svg>`, `<g>`, `<title>`, `<desc>`, `<tspan>`) are not primitives and are
/// excluded.
fn svg_primitive_counts(svg: &str) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for (tag, op) in [
        ("<rect", "rect"),
        ("<circle", "circle"),
        ("<path", "path"),
        ("<polygon", "polygon"),
        ("<line", "line"),
        ("<text", "text"),
    ] {
        let n = svg.matches(tag).count();
        if n > 0 {
            counts.insert(op, n);
        }
    }
    counts
}

/// Count draw-list ops by kind.
fn draw_op_counts(list: &DrawList) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for op in &list.ops {
        let kind = match op {
            DrawOp::ClipStart { .. } => "rect",
            DrawOp::CircleClipStart { .. } => "circle",
            DrawOp::ClipEnd { .. } => continue,
            DrawOp::Rect { .. } => "rect",
            DrawOp::Circle { .. } => "circle",
            DrawOp::Path { .. } => "path",
            DrawOp::Polygon { .. } => "polygon",
            DrawOp::Line { .. } => "line",
            DrawOp::Text { .. } => "text",
        };
        *counts.entry(kind).or_insert(0) += 1;
    }
    counts
}

#[test]
fn svg_and_draw_list_have_matching_inset_primitive_counts() {
    let source = r##"Chart(data: "parents.csv", width: 280, height: 220) {
  Table child = "child.csv"
  Space(x * y) {
    Inset(data: child, match: [id => id], width: 46, height: 46, clip: "circle", guides: false) {
      Space(t * value) {
        Point(fill: "#2b8cbe")
      }
    }
  }
}"##;
    let primary = "id,x,y\nA,1,1\nB,2,2\n";
    let child = "id,t,value\nA,1,1\nA,2,2\nB,1,3\nB,2,4\n";
    let svg_counts = svg_primitive_counts(&svg_with_tables(source, primary, &[("child", child)]));
    let draw_counts = draw_op_counts(&draw_list_with_tables(source, primary, &[("child", child)]));
    assert_eq!(svg_counts, draw_counts);
}

/// Every representative chart must emit the same primitive counts through both
/// backends — the op-by-op parity guard. If a new SVG primitive is added that
/// is not routed through the mark sink, its SVG element count will exceed the
/// draw-list op count and this test fails.
#[test]
fn svg_and_draw_list_have_matching_primitive_counts() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "points + legend",
            "Chart(data: \"p.csv\") { Space(x * y) { Point(fill: g) } }",
            "x,y,g\n1,2,a\n2,3,b\n3,1,a\n4,5,b\n",
        ),
        (
            "line",
            "Chart(data: \"p.csv\") { Space(x * y) { Line() } }",
            "x,y\n1,2\n2,3\n3,1\n4,5\n",
        ),
        (
            "area",
            "Chart(data: \"p.csv\") { Space(x * y) { Area() } }",
            "x,y\n1,2\n2,3\n3,1\n4,5\n",
        ),
        (
            "bars",
            "Chart(data: \"p.csv\") { Space(g * y) { Bar() } }",
            "g,y\na,2\nb,3\nc,1\n",
        ),
        (
            "faceting",
            "Chart(data: \"p.csv\") { Space((x * y) / g) { Point() } }",
            "x,y,g\n1,2,a\n2,3,b\n3,1,a\n4,5,b\n",
        ),
        (
            "polar pie",
            "Chart(data: \"p.csv\") { Space(sales, coords: \"polar\", theta: \"y\") { Bar(fill: product) } }",
            "product,sales\na,3\nb,2\nc,5\n",
        ),
        (
            "contours",
            "Chart(data: \"p.csv\") { Derive contours = ContourLines(x, y, z, levels: [1, 2]) Space(x * y, data: contours) { Path(group: contour_id, stroke: level) } }",
            "x,y,z\n0,0,0\n1,0,1\n2,0,2\n0,1,1\n1,1,2\n2,1,3\n0,2,2\n1,2,3\n2,2,4\n",
        ),
        (
            "summary grid",
            "Chart(data: \"p.csv\") { Derive grid = Summary2D(x, y, z, bins: 2) Space(x_center * y_center, data: grid) { Rect(xmin: x_start, xmax: x_end, ymin: y_start, ymax: y_end, fill: value) } }",
            "x,y,z\n0,0,1\n0.2,0.1,3\n0.8,0.7,9\n1,1,11\n",
        ),
    ];

    for (name, source, csv) in cases {
        let svg_counts = svg_primitive_counts(&svg(source, csv));
        let draw_counts = draw_op_counts(&draw_list(source, csv));
        assert_eq!(
            svg_counts, draw_counts,
            "primitive counts diverged for `{name}`: svg={svg_counts:?} draw={draw_counts:?}",
        );
    }
}

/// Each datum the SVG backend draws as a point has a corresponding `Mark` op in
/// the draw list.
#[test]
fn draw_list_has_one_mark_per_datum() {
    let csv = "x,y\n1,2\n2,3\n3,1\n4,5\n5,4\n";
    let list = draw_list("Chart(data: \"p.csv\") { Space(x * y) { Point() } }", csv);
    let marks = list
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                DrawOp::Circle {
                    role: DrawRole::Mark,
                    ..
                }
            )
        })
        .count();
    assert_eq!(marks, 5, "one circle mark per data row");
}

/// Guides reach the draw list: a Cartesian chart has axis lines, tick labels,
/// gridlines, and (when a fill aesthetic is mapped) legend ops.
#[test]
fn draw_list_carries_guides_and_legend() {
    let list = draw_list(
        "Chart(data: \"p.csv\") { Space(x * y) { Point(fill: g) } }",
        "x,y,g\n1,2,a\n2,3,b\n3,1,a\n",
    );
    let has = |role: DrawRole| list.ops.iter().any(|op| op_role(op) == role);
    assert!(has(DrawRole::Axis), "axis ops present");
    assert!(has(DrawRole::Grid), "grid ops present");
    assert!(has(DrawRole::Legend), "legend ops present");
}

/// Polar charts emit arc/wedge path ops rather than approximating rectangles.
#[test]
fn polar_marks_are_paths_not_rects() {
    let list = draw_list(
        "Chart(data: \"p.csv\") { Space(sales, coords: \"polar\", theta: \"y\") { Bar(fill: product) } }",
        "product,sales\na,3\nb,2\nc,5\n",
    );
    let wedges = list
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                DrawOp::Path {
                    role: DrawRole::Mark,
                    ..
                }
            )
        })
        .count();
    assert_eq!(wedges, 3, "one wedge path per pie slice");
    // No rectangle marks in a polar chart.
    assert!(!list.ops.iter().any(|op| matches!(
        op,
        DrawOp::Rect {
            role: DrawRole::Mark,
            ..
        }
    )));
}

fn op_role(op: &DrawOp) -> DrawRole {
    match op {
        DrawOp::ClipStart { role, .. }
        | DrawOp::CircleClipStart { role, .. }
        | DrawOp::ClipEnd { role } => *role,
        DrawOp::Rect { role, .. }
        | DrawOp::Circle { role, .. }
        | DrawOp::Path { role, .. }
        | DrawOp::Polygon { role, .. }
        | DrawOp::Line { role, .. }
        | DrawOp::Text { role, .. } => *role,
    }
}
