//! SVG debug-layout and metadata-comment augmentation.
//!
//! `render --debug-layout` and `render --emit-metadata` post-process the
//! rendered SVG by inserting an `<g>` overlay of layout rectangles and/or a
//! metadata comment just before the closing `</svg>` tag.

use algraf_render::{svg_num, Layout, Rect, Theme};
use algraf_semantics::ChartIr;

pub(crate) fn augment_svg(
    svg: &mut String,
    ir: &ChartIr,
    theme: &Theme,
    layout: &Layout,
    diagnostic_count: usize,
    debug_layout: bool,
    emit_metadata: bool,
) {
    let mut fragment = String::new();
    if emit_metadata {
        fragment.push_str(&format!(
            "<!-- algraf metadata: width={} height={} theme={} spaces={} diagnostics={} -->\n",
            ir.width,
            ir.height,
            theme.name,
            ir.spaces.len(),
            diagnostic_count,
        ));
    }
    if debug_layout {
        fragment.push_str(&debug_layout_svg(layout));
    }
    insert_before_svg_end(svg, &fragment);
}

pub(crate) fn debug_layout_svg(layout: &Layout) -> String {
    let mut out = String::new();
    out.push_str("<g class=\"algraf-debug-layout\" aria-hidden=\"true\">\n");
    out.push_str(&debug_rect("svg", layout.svg, "#d62728"));
    out.push_str(&debug_rect("plot", layout.plot, "#2ca02c"));
    for (index, facet) in layout.facets.iter().enumerate() {
        out.push_str(&debug_rect(
            &format!("facet-strip-{index}"),
            facet.strip,
            "#9467bd",
        ));
        out.push_str(&debug_rect(
            &format!("facet-plot-{index}"),
            facet.plot,
            "#17becf",
        ));
    }
    if let Some(legend) = layout.legend {
        out.push_str(&debug_rect("legend", legend, "#1f77b4"));
    }
    out.push_str("</g>\n");
    out
}

pub(crate) fn debug_rect(name: &str, rect: Rect, stroke: &str) -> String {
    format!(
        "<rect class=\"algraf-debug-{name}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"1\" stroke-dasharray=\"4 3\" />\n",
        svg_num(rect.x),
        svg_num(rect.y),
        svg_num(rect.width),
        svg_num(rect.height),
    )
}

pub(crate) fn insert_before_svg_end(svg: &mut String, fragment: &str) {
    if fragment.is_empty() {
        return;
    }
    if let Some(index) = svg.rfind("</svg>") {
        svg.insert_str(index, fragment);
    } else {
        svg.push_str(fragment);
    }
}
