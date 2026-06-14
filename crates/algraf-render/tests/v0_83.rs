//! v0.83.0: typographic control of chart text chrome — weight, style,
//! alignment, and visibility for theme text tokens (spec §17.3, §20.1, §20.8).

use algraf_data::{read_csv_str, Table};
use algraf_render::{render, RenderResult, Theme};
use algraf_semantics::analyze;
use algraf_syntax::parse;

fn render_result(source: &str, csv: &str) -> RenderResult {
    let frame = read_csv_str(csv).expect("csv").frame;
    let parsed = parse(source);
    let analysis = analyze(&parsed.syntax(), frame.schema());
    let ir = analysis.ir.expect("ir");
    let theme = match ir.theme.as_ref() {
        Some(theme_ir) => Theme::from_ir(theme_ir),
        None => Theme::minimal(),
    };
    render(&ir, &frame, &theme, None).expect("render")
}

fn render_svg(source: &str, csv: &str) -> String {
    render_result(source, csv).svg
}

const CSV: &str = "x,y\n1,2\n2,3\n3,5\n4,4\n";

/// The line that carries the given chart-chrome class, if present.
fn class_line<'a>(svg: &'a str, class: &str) -> Option<&'a str> {
    let needle = format!("class=\"{class}\"");
    svg.lines().find(|line| line.contains(&needle))
}

#[test]
fn default_title_is_byte_stable_left_bold_600() {
    let svg = render_svg(
        "Chart(data: \"d.csv\", title: \"Hello\") { Space(x * y) { Point() } }",
        CSV,
    );
    let title = class_line(&svg, "algraf-title").expect("title");
    // Default title keeps the historical weight 600 and emits no text-anchor.
    assert!(title.contains("font-weight=\"600\""), "title: {title}");
    assert!(!title.contains("text-anchor"), "title: {title}");
    assert!(!title.contains("font-style"), "title: {title}");
}

#[test]
fn title_weight_style_align_emit_attributes() {
    let svg = render_svg(
        "Chart(data: \"d.csv\", title: \"Hello\") {\n\
         Theme(name: \"minimal\", plotTitle: Text(weight: \"bold\", style: \"italic\", align: \"center\"))\n\
         Space(x * y) { Point() } }",
        CSV,
    );
    let title = class_line(&svg, "algraf-title").expect("title");
    assert!(title.contains("font-weight=\"bold\""), "title: {title}");
    assert!(title.contains("font-style=\"italic\""), "title: {title}");
    assert!(title.contains("text-anchor=\"middle\""), "title: {title}");
}

#[test]
fn numeric_title_weight_renders_number() {
    let svg = render_svg(
        "Chart(data: \"d.csv\", title: \"Hello\") {\n\
         Theme(name: \"minimal\", plotTitle: Text(weight: 800))\n\
         Space(x * y) { Point() } }",
        CSV,
    );
    let title = class_line(&svg, "algraf-title").expect("title");
    assert!(title.contains("font-weight=\"800\""), "title: {title}");
}

#[test]
fn normal_title_weight_drops_the_attribute() {
    let svg = render_svg(
        "Chart(data: \"d.csv\", title: \"Hello\") {\n\
         Theme(name: \"minimal\", plotTitle: Text(weight: \"normal\"))\n\
         Space(x * y) { Point() } }",
        CSV,
    );
    let title = class_line(&svg, "algraf-title").expect("title");
    assert!(!title.contains("font-weight"), "title: {title}");
}

#[test]
fn caption_left_align_moves_to_left_inset() {
    let svg = render_svg(
        "Chart(data: \"d.csv\", caption: \"note\") {\n\
         Theme(name: \"minimal\", plotCaption: Text(align: \"left\"))\n\
         Space(x * y) { Point() } }",
        CSV,
    );
    let caption = class_line(&svg, "algraf-caption").expect("caption");
    assert!(
        caption.contains("text-anchor=\"start\""),
        "caption: {caption}"
    );
    assert!(caption.contains("x=\"16\""), "caption: {caption}");
}

#[test]
fn default_caption_stays_bottom_right() {
    let svg = render_svg(
        "Chart(data: \"d.csv\", caption: \"note\") { Space(x * y) { Point() } }",
        CSV,
    );
    let caption = class_line(&svg, "algraf-caption").expect("caption");
    assert!(
        caption.contains("text-anchor=\"end\""),
        "caption: {caption}"
    );
}

#[test]
fn hidden_title_and_subtitle_are_not_emitted() {
    let svg = render_svg(
        "Chart(data: \"d.csv\", title: \"Hello\", subtitle: \"Deck\") {\n\
         Theme(name: \"minimal\", plotTitle: Text(hidden: true), plotSubtitle: Text(hidden: true))\n\
         Space(x * y) { Point() } }",
        CSV,
    );
    assert!(class_line(&svg, "algraf-title").is_none(), "{svg}");
    assert!(class_line(&svg, "algraf-subtitle").is_none(), "{svg}");
}

#[test]
fn hidden_caption_and_source_are_not_emitted() {
    let svg = render_svg(
        "Chart(data: \"d.csv\", caption: \"note\", source: \"src\") {\n\
         Theme(name: \"minimal\", plotCaption: Text(hidden: true), plotSource: Text(hidden: true))\n\
         Space(x * y) { Point() } }",
        CSV,
    );
    assert!(class_line(&svg, "algraf-caption").is_none(), "{svg}");
    assert!(class_line(&svg, "algraf-source").is_none(), "{svg}");
}

#[test]
fn hidden_title_reclaims_top_band_for_a_taller_plot() {
    let shown = render_result(
        "Chart(data: \"d.csv\", title: \"Hello\") { Space(x * y) { Point() } }",
        CSV,
    );
    let hidden = render_result(
        "Chart(data: \"d.csv\", title: \"Hello\") {\n\
         Theme(name: \"minimal\", plotTitle: Text(hidden: true))\n\
         Space(x * y) { Point() } }",
        CSV,
    );
    // Reclaiming the title band gives the plot more vertical room.
    assert!(
        hidden.layout.plot.height > shown.layout.plot.height,
        "hidden {} should exceed shown {}",
        hidden.layout.plot.height,
        shown.layout.plot.height
    );
}

#[test]
fn hidden_axis_title_drops_titles_and_widens_plot() {
    let shown = render_result("Chart(data: \"d.csv\") { Space(x * y) { Point() } }", CSV);
    let hidden = render_result(
        "Chart(data: \"d.csv\") {\n\
         Theme(name: \"minimal\", axisTitle: Text(hidden: true))\n\
         Space(x * y) { Point() } }",
        CSV,
    );
    // The default x/y axis titles are the mapped column names.
    assert!(shown.svg.contains(">x</text>") || shown.svg.contains(">y</text>"));
    assert!(!hidden.svg.contains(">x</text>"), "x title still present");
    assert!(!hidden.svg.contains(">y</text>"), "y title still present");
    // Reclaiming the title band leaves the plot at least as wide/tall.
    assert!(hidden.layout.plot.width >= shown.layout.plot.width);
}

#[test]
fn bold_axis_title_emits_font_weight_on_tick_run() {
    let svg = render_svg(
        "Chart(data: \"d.csv\") {\n\
         Theme(name: \"minimal\", axisTitle: Text(weight: \"bold\"))\n\
         Space(x * y) { Point() } }",
        CSV,
    );
    // The x-axis title (column name `x`) is rendered bold.
    assert!(svg.contains("font-weight=\"bold\""), "{svg}");
}
