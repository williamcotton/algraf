//! End-to-end render tests: source + CSV to SVG (spec §18, §24, §27.1).

use algraf_data::{read_csv_str, Table};
use algraf_render::{
    render, render_embedded, EmbeddedOutputFormat, EmbeddedRenderOptions, RenderResult, Theme,
};
use algraf_semantics::analyze;
use algraf_syntax::parse;

/// Parse + analyze + render `source` against `csv`, returning the SVG.
fn render_svg(source: &str, csv: &str) -> String {
    render_result(source, csv).svg
}

fn render_result(source: &str, csv: &str) -> RenderResult {
    let frame = read_csv_str(csv).expect("csv").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    render(&ir, &frame, &Theme::minimal(), None).expect("render")
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
    Line(stroke: "$color", strokeWidth: $size)
    Point(fill: "$color", size: $size)
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
        "Chart(data: \"p.csv\", width: 400, height: 240) { Space(x) { Line(); Point() } }",
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

fn text_element<'a>(svg: &'a str, label: &str) -> &'a str {
    let needle = format!(">{label}</text>");
    let element_end = svg.find(&needle).expect("label") + needle.len();
    let element_start = svg[..element_end].rfind("<text").unwrap();
    &svg[element_start..element_end]
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
    assert!(svg.contains(">2026-02-01</text>"));
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
fn test_source_gradient_controls_numeric_fill_colors() {
    let svg = render_svg(
        "Chart(data: \"h.csv\") { Scale(fill: value, gradient: [\"#3366cc\", \"#cc3333\"]) Space(day * hour) { Tile(fill: value) } }",
        "day,hour,value\nMon,9am,1\nMon,10am,9\n",
    );
    assert!(svg.contains("fill=\"#3366cc\""));
    assert!(svg.contains("fill=\"#cc3333\""));
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
    assert_eq!(svg.matches("<rect x=").count(), 4);
}

#[test]
fn test_direct_histogram_renders_like_primitive_rects() {
    let result = render_result(
        "Chart(data: \"d.csv\") { Space(value) { Histogram(bins: 4, fill: \"steelblue\") } }",
        "value\n1\n2\n3\n4\n5\n6\n7\n8\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-rect"));
    assert_eq!(result.svg.matches("<rect x=").count(), 4);
    assert!(result.svg.contains("fill=\"steelblue\""));
}

#[test]
fn test_histogram_with_bin_width_aligns_axis_ticks() {
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Derive bins = Bin(value, binWidth: 1, boundary: 0) Space(bin_start * count, data: bins) { Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count) } }",
        "value\n1.0\n1.4\n1.7\n2.1\n2.5\n2.7\n3.0\n3.2\n3.5\n3.7\n4.1\n4.4\n4.7\n5.0\n5.3\n5.6\n6.1\n6.4\n6.8\n7.2\n7.5\n8.0\n8.3\n8.7\n",
    );
    assert_eq!(svg.matches("<rect x=").count(), 8);
    assert!(svg.contains(">9</text>"));
    assert!(!svg.contains(">0.5</text>"));
}

#[test]
fn test_bin_closed_right_assigns_boundary_values_to_left_bins() {
    let svg = render_svg(
        "Chart(data: \"d.csv\") { Derive bins = Bin(value, binWidth: 10, boundary: 0, closed: \"right\") Space(bin_start * count, data: bins) { Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count) } }",
        "value\n0\n10\n",
    );
    assert_eq!(svg.matches("<rect x=").count(), 2);
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
        "Chart(data: \"p.csv\") {\n  Theme(name: \"minimal\")\n  Space(x * y) { Point() }\n  Space(x * y) { Theme(name: \"void\"); Point() }\n}",
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
    assert_eq!(result.svg.matches("<rect x=").count(), 3);
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
fn test_chained_derived_smooth_table_renders() {
    let result = render_result(
        "Chart(data: \"d.csv\") { Derive bins = Bin(value, bins: 4) Derive trend = Smooth(bin_center, count) Space(x * y, data: trend) { Line() } }",
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

// --- v0.6.0: Path, per-segment width, manual color maps, axis suppression ---

use algraf_render::render_with_tables;
use std::collections::HashMap;

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
        "Chart(data: \"t.csv\") { Scale(size: m, range: [0, 12]) Space(x * y) { Point(size: m) } }",
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
    let svg = render_with_tables(&ir, &primary, &frames, &Theme::minimal(), None)
        .unwrap()
        .svg;
    // The unioned x domain reaches 10, so a tick label at/near 10 appears.
    assert!(svg.contains(">10</text>") || svg.contains(">9</text>"));
    assert!(svg.contains(">Far</text>"));
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
