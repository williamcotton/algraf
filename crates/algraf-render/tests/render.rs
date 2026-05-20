//! End-to-end render tests: source + CSV to SVG (spec §18, §24, §27.1).

use algraf_data::{read_csv_str, Table};
use algraf_render::{render, RenderResult, Theme};
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
    render(&ir, &frame, &Theme::minimal()).expect("render")
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
fn test_temporal_axis() {
    let svg = render_svg(
        "Chart(data: \"t.csv\") { Space(day * value) { Line() } }",
        "day,value\n2020-01-01,1\n2020-02-01,5\n2020-03-01,3\n",
    );
    assert!(svg.contains(">2020-01-01</text>") || svg.contains("2020-0"));
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
        render(&ir, &frame, &Theme::void()).unwrap().svg
    };
    assert!(svg.contains("algraf-axes"));
    assert!(!void.contains("algraf-axes"));
}
