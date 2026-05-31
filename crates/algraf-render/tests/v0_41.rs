use algraf_data::{read_csv_str, Table};
use algraf_render::{render, render_draw_list, DrawList, DrawOp, DrawRole, Theme};
use algraf_semantics::analyze;
use algraf_syntax::parse;

fn draw_list(source: &str, csv: &str) -> DrawList {
    let frame = read_csv_str(csv).expect("csv").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    render_draw_list(&ir, &frame, &Theme::minimal(), None)
        .expect("draw list")
        .draw_list
}

fn render_metadata_json(source: &str, csv: &str) -> serde_json::Value {
    let frame = read_csv_str(csv).expect("csv").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    let result = render(&ir, &frame, &Theme::minimal(), None).expect("render");
    serde_json::from_str(&result.metadata.to_json()).expect("metadata json")
}

fn plot_rects(list: &DrawList) -> Vec<(f64, f64)> {
    list.ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rect {
                role: DrawRole::PlotArea,
                width,
                height,
                ..
            } => Some((*width, *height)),
            _ => None,
        })
        .collect()
}

fn circles(list: &DrawList) -> Vec<(f64, f64)> {
    list.ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Circle {
                role: DrawRole::Mark,
                cx,
                cy,
                ..
            } => Some((*cx, *cy)),
            _ => None,
        })
        .collect()
}

#[test]
fn visual_zoom_uses_clip_scope_and_sidecar_clip_flags() {
    let source = r#"Chart(data: "p.csv", width: 320, height: 240) {
  Space(x * y, zoomX: [2, 8], zoomY: [2, 8]) {
    Point()
  }
}"#;
    let csv = "x,y\n0,0\n5,5\n10,10\n";
    let list = draw_list(source, csv);
    assert!(list
        .ops
        .iter()
        .any(|op| matches!(op, DrawOp::ClipStart { .. })));
    assert_eq!(circles(&list).len(), 3, "zoom must not filter input rows");

    let metadata = render_metadata_json(source, csv);
    assert_eq!(
        metadata["plots"][0]["axes"]["x"]["domain"],
        serde_json::json!([2, 8])
    );
    assert_eq!(
        metadata["plots"][0]["axes"]["y"]["domain"],
        serde_json::json!([2, 8])
    );
    assert_eq!(
        metadata["plots"][0]["clip_rect"],
        metadata["plots"][0]["plot_rect"]
    );
    assert_eq!(metadata["marks"][0]["clipped"], true);
    assert!(metadata["marks"][1]["clipped"].is_null());
}

#[test]
fn fixed_aspect_shrinks_cartesian_plot_to_preserve_units() {
    let list = draw_list(
        r#"Chart(data: "p.csv", width: 640, height: 280) {
  Scale(axis: x, domain: [0, 10])
  Scale(axis: y, domain: [0, 10])
  Space(x * y, aspect: 1) {
    Point()
  }
}"#,
        "x,y\n2,2\n8,8\n",
    );
    let plots = plot_rects(&list);
    assert_eq!(plots.len(), 1);
    assert!((plots[0].0 - plots[0].1).abs() < 0.001, "{plots:?}");
}

#[test]
fn facet_grid_emits_row_major_panels_empty_slots_and_labels() {
    let list = draw_list(
        r#"Chart(data: "p.csv", width: 520, height: 360) {
  Layout(facetRows: r, facetCols: c, facetLabel: "name-value",
         facetLabels: ["A" => "Alpha"], panelSpacing: [12, 10])
  Space(x * y) {
    Point()
  }
}"#,
        "x,y,r,c\n1,1,A,X\n2,2,A,Y\n3,3,B,X\n",
    );
    assert_eq!(plot_rects(&list).len(), 4);
    let labels = list
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text {
                role: DrawRole::FacetLabel,
                content,
                ..
            } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        vec![
            "r: Alpha\nc: X",
            "r: Alpha\nc: Y",
            "r: B\nc: X",
            "r: B\nc: Y"
        ]
    );
}

#[test]
fn free_facet_scales_are_panel_local_in_metadata() {
    let metadata = render_metadata_json(
        r#"Chart(data: "p.csv", width: 520, height: 280) {
  Layout(facetCols: g, facetScales: "free")
  Space(x * y) {
    Point()
  }
}"#,
        "x,y,g\n1,1,A\n2,2,A\n100,10,B\n200,20,B\n",
    );
    let plots = metadata["plots"].as_array().expect("plots");
    assert_eq!(plots.len(), 2);
    let x0 = plots[0]["axes"]["x"]["domain"].as_array().expect("x0");
    let x1 = plots[1]["axes"]["x"]["domain"].as_array().expect("x1");
    let y0 = plots[0]["axes"]["y"]["domain"].as_array().expect("y0");
    let y1 = plots[1]["axes"]["y"]["domain"].as_array().expect("y1");
    assert!(
        x0[1].as_f64().unwrap() < x1[0].as_f64().unwrap(),
        "{metadata}"
    );
    assert!(
        y0[1].as_f64().unwrap() < y1[0].as_f64().unwrap(),
        "{metadata}"
    );
}

#[test]
fn point_jitter_sugar_matches_explicit_jitter_points() {
    let csv = "x,y,g\n1,1,A\n2,2,B\n";
    let sugar = draw_list(
        r#"Chart(data: "p.csv", width: 320, height: 240) {
  Scale(axis: x, domain: [0, 3])
  Scale(axis: y, domain: [0, 3])
  Space(x * y) {
    Point(fill: g, jitter: [0.2, 0.4])
  }
}"#,
        csv,
    );
    let explicit = draw_list(
        r#"Chart(data: "p.csv", width: 320, height: 240) {
  Scale(axis: x, domain: [0, 3])
  Scale(axis: y, domain: [0, 3])
  Derive jittered = JitterPoints(x, y, width: 0.2, height: 0.4)
  Space(x * y, data: jittered) {
    Point(fill: g)
  }
}"#,
        csv,
    );
    assert_eq!(sugar.to_json(), explicit.to_json());
}

#[test]
fn point_nudge_and_jitter_are_deterministic() {
    let source = r#"Chart(data: "p.csv", width: 320, height: 240) {
  Scale(axis: x, domain: [0, 3])
  Scale(axis: y, domain: [0, 3])
  Space(x * y) {
    Point(jitter: [0.2, 0.2], nudge: [2, -1], nudgeData: [0.1, 0])
  }
}"#;
    let csv = "x,y\n1,1\n2,2\n";
    let first = draw_list(source, csv);
    let second = draw_list(source, csv);
    assert_eq!(first.to_json(), second.to_json());
    assert_ne!(
        circles(&first),
        circles(&draw_list(
            r#"Chart(data: "p.csv", width: 320, height: 240) {
  Scale(axis: x, domain: [0, 3])
  Scale(axis: y, domain: [0, 3])
  Space(x * y) { Point() }
}"#,
            csv,
        ))
    );
}
