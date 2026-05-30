//! Stat determinism regression tests (spec §27).

use algraf_data::{read_csv_str, Table};
use algraf_render::{render, Theme};
use algraf_semantics::analyze;
use algraf_syntax::parse;

fn render_svg(source: &str, csv: &str) -> String {
    let frame = read_csv_str(csv).expect("csv").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    assert!(
        analysis.diagnostics.is_empty(),
        "analysis diagnostics: {:#?}",
        analysis.diagnostics
    );
    let ir = analysis.ir.expect("ir");
    let result = render(&ir, &frame, &Theme::minimal(), None).expect("render");
    assert!(
        result.diagnostics.is_empty(),
        "render diagnostics: {:#?}",
        result.diagnostics
    );
    result.svg
}

fn assert_stat_is_row_order_independent(source: &str, csv: &str, shuffled_csv: &str) {
    let first = render_svg(source, csv);
    assert_eq!(first, render_svg(source, csv));
    assert_eq!(first, render_svg(source, shuffled_csv));
}

#[test]
fn histogram_bin_output_is_row_order_independent() {
    let source = r##"Chart(data: "d.csv") {
  Theme(name: "minimal")
  Space(value) { Histogram(bins: 4, fill: "#4c78a8") }
}"##;
    assert_stat_is_row_order_independent(
        source,
        "value\n0\n1\n2\n3\n4\n5\n6\n7\n",
        "value\n7\n2\n5\n0\n6\n1\n4\n3\n",
    );
}

#[test]
fn temporal_calendar_bin_output_is_row_order_independent() {
    let source = r##"Chart(data: "d.csv") {
  Parse(column: time, as: "datetime")
  Theme(name: "minimal")
  Guide(axis: x, timeFormat: "iso-date")
  Space(time) { Histogram(interval: "day", fill: "#4c78a8") }
}"##;
    assert_stat_is_row_order_independent(
        source,
        "time\n2026-01-01T00:00:00Z\n2026-01-01T12:00:00Z\n2026-01-02T00:00:00Z\n2026-01-03T00:00:00Z\n",
        "time\n2026-01-03T00:00:00Z\n2026-01-01T12:00:00Z\n2026-01-02T00:00:00Z\n2026-01-01T00:00:00Z\n",
    );
}

#[test]
fn count_output_is_row_order_independent() {
    let source = r##"Chart(data: "d.csv") {
  Theme(name: "minimal")
  Space(category) { Bar(stat: "count", fill: "#4c78a8") }
}"##;
    assert_stat_is_row_order_independent(
        source,
        "category\na\nb\na\nc\nb\n",
        "category\na\nb\nc\nb\na\n",
    );
}

#[test]
fn bin2d_output_is_row_order_independent() {
    let source = r##"Chart(data: "d.csv") {
  Theme(name: "minimal")
  Space(x * y) { Bin2D(bins: 3) }
}"##;
    assert_stat_is_row_order_independent(
        source,
        "x,y\n0,0\n1,1\n2,2\n3,3\n4,4\n5,5\n",
        "x,y\n5,5\n1,1\n4,4\n0,0\n3,3\n2,2\n",
    );
}

#[test]
fn hexbin_output_is_row_order_independent() {
    let source = r##"Chart(data: "d.csv") {
  Theme(name: "minimal")
  Space(x * y) { HexBin(bins: 4) }
}"##;
    assert_stat_is_row_order_independent(
        source,
        "x,y\n0,0\n1,1\n2,2\n3,3\n4,4\n5,5\n",
        "x,y\n4,4\n0,0\n5,5\n2,2\n1,1\n3,3\n",
    );
}

#[test]
fn density_output_is_row_order_independent() {
    let source = r##"Chart(data: "d.csv") {
  Theme(name: "minimal")
  Space(value) { Density(n: 32, fill: "#4c78a8") }
}"##;
    assert_stat_is_row_order_independent(
        source,
        "value\n0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n",
        "value\n9\n1\n7\n3\n5\n0\n8\n2\n6\n4\n",
    );
}

#[test]
fn smooth_output_is_row_order_independent() {
    let source = r##"Chart(data: "d.csv") {
  Theme(name: "minimal")
  Space(x * y) { Smooth(method: "loess", span: 0.75, stroke: "#333333") }
}"##;
    assert_stat_is_row_order_independent(
        source,
        "x,y\n0,0\n1,1\n2,4\n3,9\n4,16\n5,25\n6,36\n",
        "x,y\n4,16\n0,0\n6,36\n2,4\n5,25\n1,1\n3,9\n",
    );
}

#[test]
fn boxplot_quantiles_are_row_order_independent() {
    let source = r##"Chart(data: "d.csv") {
  Theme(name: "minimal")
  Space(group * value) { Boxplot(fill: "#4c78a8") }
}"##;
    assert_stat_is_row_order_independent(
        source,
        "group,value\na,1\na,2\na,3\na,20\nb,4\nb,5\nb,6\nb,30\n",
        "group,value\na,20\nb,30\na,2\nb,4\nb,5\na,1\nb,6\na,3\n",
    );
}
