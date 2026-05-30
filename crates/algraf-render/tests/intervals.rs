//! Interval construction tests (spec §15.15, §24.6).

use algraf_data::{read_csv_str, Table};
use algraf_render::{render, render_draw_list, render_raster, DrawOp, DrawRole, Theme};
use algraf_semantics::analyze;
use algraf_syntax::parse;

struct RenderBytes {
    svg: String,
    draw_list: String,
    raster: Vec<u8>,
    metadata: String,
}

fn render_bytes(source: &str, csv: &str) -> RenderBytes {
    let frame = read_csv_str(csv).expect("csv").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    assert!(
        analysis.diagnostics.is_empty(),
        "analysis diagnostics: {:#?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");

    let svg = render(&ir, &frame, &Theme::void(), None).expect("svg");
    assert!(
        svg.diagnostics.is_empty(),
        "svg diagnostics: {:#?}",
        svg.diagnostics
    );
    let draw = render_draw_list(&ir, &frame, &Theme::void(), None).expect("draw-list");
    assert!(
        draw.diagnostics.is_empty(),
        "draw diagnostics: {:#?}",
        draw.diagnostics
    );
    let raster = render_raster(&ir, &frame, &Theme::void(), None, 1.0).expect("raster");
    assert!(
        raster.diagnostics.is_empty(),
        "raster diagnostics: {:#?}",
        raster.diagnostics
    );

    RenderBytes {
        svg: svg.svg,
        draw_list: draw.draw_list.to_json(),
        raster: raster.image.data().to_vec(),
        metadata: svg.metadata.to_json(),
    }
}

fn assert_same_outputs(sugar: &str, explicit: &str, csv: &str) {
    let sugar = render_bytes(sugar, csv);
    let explicit = render_bytes(explicit, csv);
    assert_eq!(sugar.svg, explicit.svg, "SVG output differs");
    assert_eq!(
        sugar.draw_list, explicit.draw_list,
        "draw-list output differs"
    );
    assert_eq!(sugar.raster, explicit.raster, "raster output differs");
    assert_eq!(sugar.metadata, explicit.metadata, "metadata output differs");
}

#[test]
fn error_bar_sugar_shares_position_scale_with_point_layer() {
    let csv =
        "dose,estimate,lower,upper\n1,4.2,3.5,4.9\n2,5.1,4.4,5.8\n3,6.4,5.7,7.2\n5,7.6,6.8,8.4\n";
    let source = r##"Chart(data: "d.csv", width: 720, height: 440) {
  Space(dose * estimate) {
    ErrorBar(ymin: lower, ymax: upper, capWidth: 0.35, stroke: "#333333", strokeWidth: 1.2)
    Point(fill: "#1f77b4", stroke: "#333333", size: 4)
  }
}"##;
    let frame = read_csv_str(csv).expect("csv").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    assert!(
        analysis.diagnostics.is_empty(),
        "analysis diagnostics: {:#?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    let draw = render_draw_list(&ir, &frame, &Theme::void(), None).expect("draw-list");
    assert!(
        draw.diagnostics.is_empty(),
        "draw diagnostics: {:#?}",
        draw.diagnostics
    );

    let mut stem_xs = Vec::new();
    let mut point_xs = Vec::new();
    for op in draw.draw_list.ops {
        match op {
            DrawOp::Line {
                role: DrawRole::Mark,
                x1,
                x2,
                ..
            } if (x1 - x2).abs() < 0.001 => stem_xs.push(x1),
            DrawOp::Circle {
                role: DrawRole::Mark,
                cx,
                ..
            } => point_xs.push(cx),
            _ => {}
        }
    }

    assert_eq!(stem_xs.len(), point_xs.len());
    for point_x in point_xs {
        assert!(
            stem_xs
                .iter()
                .any(|stem_x| (stem_x - point_x).abs() < 0.001),
            "point x {point_x} did not align with any error-bar stem: {stem_xs:?}"
        );
    }
}

#[test]
fn error_bar_sugar_matches_explicit_interval_segments() {
    let csv = "x,mid,low,high\n1,3,2,4\n2,5,4,7\n3,4,3,6\n";
    let sugar = r##"Chart(data: "d.csv", width: 320, height: 220) {
  Space(x * mid) {
    ErrorBar(ymin: low, ymax: high, capWidth: 0.4, stroke: "#333333", strokeWidth: 1.2)
  }
}"##;
    let explicit = r##"Chart(data: "d.csv", width: 320, height: 220) {
  Derive whiskers = IntervalSegments(x, low, high, orientation: "vertical", capWidth: 0.4)
  Space(x * y, data: whiskers) {
    Segment(x: x, y: y, xend: xend, yend: yend, stroke: "#333333", strokeWidth: 1.2)
  }
}"##;
    assert_same_outputs(sugar, explicit, csv);
}

#[test]
fn horizontal_line_range_sugar_matches_explicit_interval_segments() {
    let csv = "metric,mid,low,high\nA,10,7,12\nB,15,11,18\nC,13,10,16\n";
    let sugar = r##"Chart(data: "d.csv", width: 320, height: 220) {
  Space(mid * metric) {
    LineRange(xmin: low, xmax: high, orientation: "horizontal", stroke: "#2f6fbb", strokeWidth: 1.4)
  }
}"##;
    let explicit = r##"Chart(data: "d.csv", width: 320, height: 220) {
  Derive ranges = IntervalSegments(metric, low, high, orientation: "horizontal")
  Space(x * y, data: ranges) {
    Segment(x: x, y: y, xend: xend, yend: yend, stroke: "#2f6fbb", strokeWidth: 1.4)
  }
}"##;
    assert_same_outputs(sugar, explicit, csv);
}

#[test]
fn point_range_sugar_matches_explicit_segment_and_point_layers() {
    let csv = "x,mid,low,high\n1,3,2,4\n2,5,4,7\n3,4,3,6\n";
    let sugar = r##"Chart(data: "d.csv", width: 320, height: 220) {
  Space(x * mid) {
    PointRange(ymin: low, ymax: high, stroke: "#333333", strokeWidth: 1.1, fill: "#ffffff", size: 3)
  }
}"##;
    let explicit = r##"Chart(data: "d.csv", width: 320, height: 220) {
  Derive ranges = IntervalSegments(x, low, high, orientation: "vertical")
  Space(x * y, data: ranges) {
    Segment(x: x, y: y, xend: xend, yend: yend, stroke: "#333333", strokeWidth: 1.1)
  }
  Space(x * mid) {
    Point(fill: "#ffffff", stroke: "#333333", size: 3)
  }
}"##;
    assert_same_outputs(sugar, explicit, csv);
}

#[test]
fn cross_bar_sugar_matches_explicit_rect_and_middle_layers() {
    let csv = "x,mid,low,high\n1,3,2,4\n2,5,4,7\n3,4,3,6\n";
    let sugar = r##"Chart(data: "d.csv", width: 320, height: 220) {
  Space(x * mid) {
    CrossBar(ymin: low, ymax: high, width: 0.6, fill: "#8ecae6", stroke: "#222222", strokeWidth: 1)
  }
}"##;
    let explicit = r##"Chart(data: "d.csv", width: 320, height: 220) {
  Derive boxes = IntervalRects(x, low, high, orientation: "vertical", width: 0.6)
  Derive middles = IntervalMiddles(x, mid, orientation: "vertical", width: 0.6)
  Space((xmin + xmax) * (ymin + ymax), data: boxes) {
    Rect(xmin: xmin, xmax: xmax, ymin: ymin, ymax: ymax, fill: "#8ecae6", stroke: "#222222", strokeWidth: 1)
  }
  Space(x * y, data: middles) {
    Segment(x: x, y: y, xend: xend, yend: yend, stroke: "#222222", strokeWidth: 1)
  }
}"##;
    assert_same_outputs(sugar, explicit, csv);
}
