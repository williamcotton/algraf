//! Draw-list backend tests: the second output backend consumes the same planned
//! scene as SVG and agrees on canvas size, background, and panel placement
//! (spec §24.6, §27.1). These guard the documented equivalence limits of the
//! v0.24 backend contract.

use algraf_data::{read_csv_str, Table};
use algraf_render::{render, render_draw_list, Dash, DrawList, DrawOp, DrawRole, Fill, Theme};
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

fn rects(list: &DrawList, role: DrawRole) -> Vec<(f64, f64, f64, f64, String)> {
    list.ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rect {
                role: r,
                x,
                y,
                width,
                height,
                paint,
                ..
            } if *r == role => {
                let fill = match &paint.fill {
                    Fill::Color(c) => c.clone(),
                    Fill::None => "none".to_string(),
                };
                Some((*x, *y, *width, *height, fill))
            }
            _ => None,
        })
        .collect()
}

fn texts(list: &DrawList, role: DrawRole) -> Vec<String> {
    list.ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text {
                role: r, content, ..
            } if *r == role => Some(content.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn draw_list_matches_canvas_dimensions() {
    let list = draw_list(
        "Chart(data: \"p.csv\") { Space(x * y) { Point() } }",
        "x,y\n1,2\n2,3\n",
    );
    assert_eq!(list.width, 800.0);
    assert_eq!(list.height, 520.0);
}

#[test]
fn draw_list_has_full_canvas_background() {
    let theme = Theme::minimal();
    let list = draw_list(
        "Chart(data: \"p.csv\") { Space(x * y) { Point() } }",
        "x,y\n1,2\n2,3\n",
    );
    let backgrounds = rects(&list, DrawRole::Background);
    assert_eq!(backgrounds.len(), 1);
    let (x, y, w, h, fill) = &backgrounds[0];
    assert_eq!((*x, *y, *w, *h), (0.0, 0.0, 800.0, 520.0));
    assert_eq!(fill, &theme.background);
    // The background is the first op, mirroring SVG document order.
    assert!(matches!(
        list.ops.first(),
        Some(DrawOp::Rect {
            role: DrawRole::Background,
            ..
        })
    ));
}

#[test]
fn draw_list_plot_area_matches_svg_layout() {
    let source = "Chart(data: \"p.csv\") { Space(x * y) { Point() } }";
    let csv = "x,y\n1,2\n2,3\n";
    let frame = read_csv_str(csv).expect("csv").frame;
    let ir = analyze(&parse(source).syntax(), frame.schema())
        .ir
        .expect("ir");

    let svg_layout = render(&ir, &frame, &Theme::minimal(), None)
        .expect("render")
        .layout;
    let list = render_draw_list(&ir, &frame, &Theme::minimal(), None)
        .expect("draw list")
        .draw_list;

    let plots = rects(&list, DrawRole::PlotArea);
    assert_eq!(plots.len(), 1);
    let (x, y, w, h, _) = &plots[0];
    // The draw-list backend and SVG backend agree on plot placement because they
    // consume the same planned layout (spec §24.6).
    assert_eq!(*x, svg_layout.plot.x);
    assert_eq!(*y, svg_layout.plot.y);
    assert_eq!(*w, svg_layout.plot.width);
    assert_eq!(*h, svg_layout.plot.height);
}

#[test]
fn draw_list_carries_chart_text() {
    let list = draw_list(
        "Chart(data: \"p.csv\", title: \"Main\", subtitle: \"Sub\", caption: \"Source\") \
         { Space(x * y) { Point() } }",
        "x,y\n1,2\n2,3\n",
    );
    assert_eq!(texts(&list, DrawRole::Title), vec!["Main".to_string()]);
    assert_eq!(texts(&list, DrawRole::Subtitle), vec!["Sub".to_string()]);
    assert_eq!(texts(&list, DrawRole::Caption), vec!["Source".to_string()]);
}

#[test]
fn draw_list_has_one_panel_per_facet() {
    let list = draw_list(
        "Chart(data: \"p.csv\") { Space((x * y) / g) { Point() } }",
        "x,y,g\n1,2,a\n2,3,b\n3,1,a\n4,5,b\n",
    );
    let plots = rects(&list, DrawRole::PlotArea);
    let strips = rects(&list, DrawRole::FacetStrip);
    let labels = texts(&list, DrawRole::FacetLabel);
    assert_eq!(plots.len(), 2);
    assert_eq!(strips.len(), 2);
    assert_eq!(labels, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn draw_list_json_is_deterministic_and_escapes() {
    let list = draw_list(
        "Chart(data: \"p.csv\", title: \"A \\\"quoted\\\" & <tag>\") { Space(x * y) { Point() } }",
        "x,y\n1,2\n2,3\n",
    );
    let json = list.to_json();
    // Stable, deterministic serialization.
    assert_eq!(json, list.to_json());
    assert!(json.starts_with("{\"width\":800,\"height\":520,\"interactions\":{"));
    // JSON string escaping (distinct from SVG/XML escaping).
    assert!(json.contains("A \\\"quoted\\\" & <tag>"));
    // Parses as valid JSON.
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(parsed["width"], 800);
    assert_eq!(parsed["interactions"]["version"], 1);
    assert_eq!(parsed["ops"][0]["role"], "background");
}

#[test]
fn draw_list_records_path_dash() {
    let list = draw_list(
        "Chart(data: \"p.csv\") { Space(x * y) { Path(dash: \"dashed\") } }",
        "x,y\n1,1\n2,2\n",
    );
    assert!(list.ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Path {
                dash: Some(Dash::Dashed),
                ..
            }
        )
    }));
    assert!(list.to_json().contains("\"strokeDasharray\":\"4 4\""));
}

#[test]
fn draw_list_records_inert_interaction_metadata() {
    let list = draw_list(
        "Chart(data: \"p.csv\", width: 200, height: 200) { Space(x * y) { Point(tooltip: [g], highlight: \"g\") } }",
        "x,y,g\n1,2,A\n3,4,B\n",
    );
    let marks: Vec<_> = list
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Circle {
                role: DrawRole::Mark,
                interaction,
                ..
            } => Some(interaction.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(marks.len(), 2);
    let first = marks[0].as_ref().expect("interaction");
    assert_eq!(first.tooltip.as_deref(), Some("g: A"));
    assert_eq!(first.highlight.as_deref(), Some("A"));
    // The same metadata is serialized into the JSON, inert.
    let json = list.to_json();
    assert!(
        json.contains("\"interaction\":{\"tooltip\":\"g: A\",\"highlight\":\"A\"}"),
        "{json}"
    );
    assert!(json.contains("\"interactions\":{\"version\":1"), "{json}");
    assert_eq!(list.interactions.groups[0].key, "g");
    assert_eq!(list.interactions.groups[0].values, vec!["A", "B"]);
}

#[test]
fn draw_list_interactions_match_svg_sidecar_metadata() {
    let source = "Chart(data: \"p.csv\", width: 200, height: 200) { Space(x * y) { Point(tooltip: [g, y], highlight: \"g\") } }";
    let csv = "x,y,g\n1,2,A\n3,4,B\n";
    let frame = read_csv_str(csv).expect("csv").frame;
    let ir = analyze(&parse(source).syntax(), frame.schema())
        .ir
        .expect("ir");

    let svg_result = render(&ir, &frame, &Theme::minimal(), None).expect("render");
    let draw_result = render_draw_list(&ir, &frame, &Theme::minimal(), None).expect("draw list");

    assert_eq!(
        svg_result.metadata.to_json(),
        draw_result.draw_list.interactions.to_json()
    );
    assert_eq!(draw_result.metadata, draw_result.draw_list.interactions);
}
