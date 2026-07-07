//! Draw-list backend tests: the second output backend consumes the same planned
//! scene as SVG and agrees on canvas size, background, and panel placement
//! (spec §24.6, §27.1). These guard the documented equivalence limits of the
//! v0.24 backend contract.

use std::collections::HashMap;

use algraf_data::{read_csv_str, DataFrame, Table};
use algraf_render::{
    render, render_draw_list, Dash, DrawList, DrawOp, DrawRole, Fill, ImageAsset, ImageAssets,
    RenderOptions, Theme,
};
use algraf_semantics::{analyze, analyze_with_tables};
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
    render_draw_list(
        &ir,
        &frame,
        &Theme::minimal(),
        RenderOptions::default().with_named_tables(&named),
    )
    .expect("draw list")
    .draw_list
}

fn image_assets() -> ImageAssets {
    let mut assets = ImageAssets::new();
    assets.insert(ImageAsset {
        source: "a.png".to_string(),
        href: "data:image/png;base64,AAAA".to_string(),
        intrinsic_width: 2.0,
        intrinsic_height: 1.0,
    });
    assets.insert(ImageAsset {
        source: "b.png".to_string(),
        href: "data:image/png;base64,BBBB".to_string(),
        intrinsic_width: 1.0,
        intrinsic_height: 2.0,
    });
    assets
}

fn draw_list_with_assets(source: &str, csv: &str, assets: &ImageAssets) -> DrawList {
    let frame = read_csv_str(csv).expect("csv").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    render_draw_list(
        &ir,
        &frame,
        &Theme::minimal(),
        RenderOptions::default().with_image_assets(assets),
    )
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
fn draw_list_records_image_marks_and_legend_swatches() {
    let list = draw_list_with_assets(
        "Chart(data: \"p.csv\") { Space(x * y) { Image(src: logo, size: 20) } }",
        "x,y,logo\n1,2,a.png\n2,3,b.png\n",
        &image_assets(),
    );
    let mark_images = list
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Image {
                role: DrawRole::Mark,
                href,
                width,
                height,
                ..
            } => Some((href.as_str(), *width, *height)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(mark_images.len(), 2);
    assert!(mark_images.contains(&("data:image/png;base64,AAAA", 20.0, 10.0)));
    assert!(mark_images.contains(&("data:image/png;base64,BBBB", 10.0, 20.0)));
    assert!(list.ops.iter().any(|op| matches!(
        op,
        DrawOp::Image {
            role: DrawRole::Legend,
            ..
        }
    )));
}

#[test]
fn bottom_legend_is_reflected_in_layout_sidecar_and_draw_list() {
    let source = r##"Chart(data: "p.csv", width: 520, height: 320) {
  Theme(name: "minimal", legendPosition: "bottom")
  Space(x * y) {
    Point(fill: g)
  }
}"##;
    let frame = read_csv_str("x,y,g\n1,2,A\n2,3,B\n3,1,A\n")
        .expect("csv")
        .frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    let theme = Theme::from_ir(ir.theme.as_ref().expect("theme"));

    let result = render(&ir, &frame, &theme, None).expect("render");
    let legend = result.layout.legend.expect("legend");
    assert!(legend.y > result.layout.plot.bottom());
    assert!(
        legend.y >= result.layout.plot.bottom() + 48.0,
        "bottom legend should sit below the x-axis title reserve"
    );

    let metadata: serde_json::Value =
        serde_json::from_str(&result.metadata.to_json()).expect("metadata json");
    assert_eq!(metadata["legend"]["position"], "bottom");
    assert_eq!(metadata["legend"]["rect"]["y"].as_f64().unwrap(), legend.y);

    let list = render_draw_list(&ir, &frame, &theme, None)
        .expect("draw list")
        .draw_list;
    let legend_text_below_plot = list.ops.iter().any(|op| {
        matches!(
            op,
            DrawOp::Text {
                role: DrawRole::Legend,
                y,
                ..
            } if *y > result.layout.plot.bottom()
        )
    });
    assert!(legend_text_below_plot);
}

#[test]
fn draw_list_records_continuous_colorbar_segments() {
    let list = draw_list(
        "Chart(data: \"p.csv\") {
  Scale(fill: value, gradient: \"viridis\", breaks: [1, 5, 9])
  Space(x * y) { Tile(fill: value) }
}",
        "x,y,value\nA,M,1\nB,M,9\n",
    );
    let legend_rects = rects(&list, DrawRole::Legend);
    assert!(
        legend_rects.len() > 20,
        "continuous legend should be emitted as colorbar rect segments"
    );
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
fn draw_list_records_formatted_text_marks() {
    let list = draw_list(
        "Chart(data: \"p.csv\") { Space(x * y) { Text(label: value, format: \".1%\") } }",
        "x,y,value\n1,2,0.125\n",
    );
    assert_eq!(texts(&list, DrawRole::Mark), vec!["12.5%".to_string()]);
}

#[test]
fn draw_list_records_terminal_labels_by_physical_x_endpoint() {
    let end = draw_list(
        "Chart(data: \"p.csv\") { Space(x * y) { Label(label: series, group: series, at: \"end\") } }",
        "x,y,series\n1,2,A\n2,4,A\n1,5,B\n2,3,B\n",
    );
    let end_labels = end
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text {
                role: DrawRole::Mark,
                x,
                content,
                ..
            } => Some((content.clone(), *x)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(end_labels.len(), 2);
    assert!(end_labels.iter().all(|(_, x)| *x > 400.0));

    let start = draw_list(
        "Chart(data: \"p.csv\") { Space(x * y) { Label(label: series, group: series, at: \"start\") } }",
        "x,y,series\n1,2,A\n2,4,A\n1,5,B\n2,3,B\n",
    );
    let start_labels = start
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text {
                role: DrawRole::Mark,
                x,
                content,
                ..
            } => Some((content.clone(), *x)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(start_labels.len(), 2);
    assert!(start_labels.iter().all(|(_, x)| *x < 400.0));
}

#[test]
fn draw_list_records_inert_interaction_metadata() {
    let list = draw_list(
        "Chart(data: \"p.csv\", width: 200, height: 200) { Space(x * y) { Point(tooltip: [g], highlight: \"g\") On(event: \"click\", emit: g) } }",
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
    let event = first.event.as_ref().expect("event");
    assert_eq!(event.event, "click");
    assert_eq!(event.emit_field, "g");
    assert_eq!(event.value.as_deref(), Some("A"));
    // The same metadata is serialized into the JSON, inert.
    let json = list.to_json();
    assert!(
        json.contains("\"interaction\":{\"tooltip\":\"g: A\",\"highlight\":\"A\",\"event\":{\"event\":\"click\",\"emit_field\":\"g\",\"value\":\"A\"}}"),
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

#[test]
fn draw_list_records_glyph_clip_and_nested_metadata() {
    let source = r##"Chart(data: "parents.csv", width: 260, height: 220) {
  Table child = "child.csv"
  Glyph mark(data: child, key: [id]) {
    Space(t * value) {
      Point(tooltip: [label], highlight: "label")
    }
  }
  Space(x * y) {
    mark(width: 44, height: 44, clip: "circle")
  }
}"##;
    let list = draw_list_with_tables(
        source,
        "id,x,y\nA,1,1\nB,2,2\n",
        &[(
            "child",
            "id,t,value,label\nA,1,1,a1\nA,2,2,a2\nB,1,3,b1\nB,2,4,b2\n",
        )],
    );

    assert!(list
        .ops
        .iter()
        .any(|op| matches!(op, DrawOp::CircleClipStart { .. })));
    assert!(list
        .interactions
        .plots
        .iter()
        .any(|plot| plot.id == "p0:i0[0]:s0"));
    assert!(list
        .interactions
        .marks
        .iter()
        .any(|mark| mark.id == "p0:i0[1]:s0:g0:r2" && mark.plot == "p0:i0[1]:s0"));
    assert!(list.to_json().contains("\"op\":\"circleClipStart\""));
}
