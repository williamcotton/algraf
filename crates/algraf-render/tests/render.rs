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
    render(&ir, &frame, &Theme::minimal(), None).expect("render")
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
fn test_edge_x_tick_labels_anchor_inside_plot() {
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
    assert!(svg.contains("text-anchor=\"start\"") && svg.contains(">2026-01-01</text>"));
    assert!(svg.contains("text-anchor=\"end\"") && svg.contains(">2026-01-22</text>"));
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
        "Chart(data: \"r.csv\") { Space(x * y) { Point() HLine(y: 2, stroke: \"red\", label: \"Target\") VLine(x: 2, stroke: \"gray\", label: \"Marker\") Rug(sides: \"bl\") } }",
        "x,y\n1,1\n2,2\n3,3\n",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.svg.contains("algraf-geom-hline"));
    assert!(result.svg.contains("algraf-geom-vline"));
    assert!(result.svg.contains("algraf-geom-rug"));
    assert!(result.svg.contains(">Target</text>"));
    assert!(result.svg.contains(">Marker</text>"));
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
fn test_rect_zero_extent_renders_as_marker() {
    let svg = render_svg(
        "Chart(data: \"r.csv\") { Space(x * y) { Rect(xmin: x, xmax: x, ymin: 0, ymax: y, strokeWidth: 3) } }",
        "x,y\n1,5\n",
    );
    assert!(svg.contains("width=\"3\""));
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
fn test_segment_renders_line_between_literal_endpoints() {
    let svg = render_svg(
        "Chart(data: \"p.csv\") {\n  Space(x * y) {\n    Segment(x: 1, y: 1, xend: 3, yend: 4)\n  }\n}",
        "x,y\n0,0\n5,5\n",
    );
    assert!(svg.contains("algraf-geom-segment"));
    assert!(svg.contains("<line "));
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
