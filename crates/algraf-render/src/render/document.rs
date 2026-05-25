use algraf_core::Diagnostic;
use algraf_semantics::ChartIr;

use crate::aes::Legend;
use crate::guide;
use crate::layout::Layout;
use crate::svg::{escape_attr, escape_text, num, SvgAttr, SvgWriter};
use crate::theme::Theme;

use super::panels::{panel_slots, Panel};

pub(super) fn emit_document(
    ir: &ChartIr,
    layout: &Layout,
    legends: &[Legend],
    panels: &[Panel<'_>],
    theme: &Theme,
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    let width = ir.width as f64;
    let height = ir.height as f64;

    let mut w = SvgWriter::new();
    let aria = ir
        .title
        .as_ref()
        .map(|title| format!(" aria-label=\"{}\"", escape_attr(title)))
        .unwrap_or_default();
    w.line(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" \
         viewBox=\"0 0 {} {}\" role=\"img\"{}>",
        num(width),
        num(height),
        num(width),
        num(height),
        aria,
    ));
    if let Some(title) = &ir.title {
        w.text_element("title", &[], title);
    }
    if let Some(desc) = chart_desc(ir) {
        w.text_element("desc", &[], &desc);
    }

    // Background.
    emit_rect_fill(
        &mut w,
        0.0,
        0.0,
        width,
        height,
        &theme.background,
        "algraf-background",
    );
    render_chart_text(&mut w, ir, width, height, layout, theme);

    let slots = panel_slots(layout, panels);
    for slot in &slots {
        let class = if slot.facet_index.is_some() {
            "algraf-plot-area algraf-facet-panel"
        } else {
            "algraf-plot-area"
        };
        emit_rect_fill(
            &mut w,
            slot.plot.x,
            slot.plot.y,
            slot.plot.width,
            slot.plot.height,
            &theme.plot_background,
            class,
        );
    }

    for slot in &slots {
        if let (Some(strip), Some(panel)) = (slot.strip, slot.panel) {
            guide::render_facet_label(&mut w, slot.label.unwrap_or_default(), strip, &panel.theme);
        }
    }
    for slot in &slots {
        if let Some(panel) = slot.panel {
            if panel.guides.grid && !panel.scaled.is_spatial() {
                guide::render_grid(&mut w, &panel.scaled, panel.plot, &panel.theme);
            }
        }
    }

    // Data layers in source order (spec §18.3).
    for panel in panels {
        for geo in panel.geometries {
            crate::geom::render(
                &mut w,
                geo,
                crate::geom::GeometryRenderContext {
                    space: &panel.scaled,
                    table: panel.table,
                    rows: panel.rows.as_deref(),
                    plot: panel.plot,
                    theme: &panel.theme,
                    scales: &panel.scales,
                },
                diagnostics,
            );
        }
    }

    // Axes and legends.
    for slot in &slots {
        if let Some(panel) = slot.panel {
            // Spatial spaces have no lat/lon axes (spec §16.15).
            if panel.theme.axes && !panel.scaled.is_spatial() {
                guide::render_axes(
                    &mut w,
                    &panel.scaled,
                    panel.plot,
                    &panel.theme,
                    panel.guides.x_label.as_deref(),
                    panel.guides.y_label.as_deref(),
                );
            }
        }
    }
    if let Some(area) = layout.legend {
        guide::render_legends(&mut w, legends, area, theme);
    }

    w.line("</svg>");
    w.finish()
}

fn chart_desc(ir: &ChartIr) -> Option<String> {
    match (&ir.subtitle, &ir.caption) {
        (Some(subtitle), Some(caption)) => Some(format!("{subtitle}\n{caption}")),
        (Some(subtitle), None) => Some(subtitle.clone()),
        (None, Some(caption)) => Some(caption.clone()),
        (None, None) => None,
    }
}

fn render_chart_text(
    w: &mut SvgWriter,
    ir: &ChartIr,
    width: f64,
    height: f64,
    layout: &Layout,
    theme: &Theme,
) {
    let x = layout.plot.x;
    let mut y = 24.0;
    if let Some(title) = &ir.title {
        w.line(&format!(
            "<text class=\"algraf-title\" x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"600\" fill=\"{}\">{}</text>",
            num(x),
            num(y),
            escape_attr(&theme.font_family),
            num(theme.title_size),
            escape_attr(&theme.text_color),
            escape_text(title),
        ));
        y += theme.title_size + 8.0;
    }
    if let Some(subtitle) = &ir.subtitle {
        w.line(&format!(
            "<text class=\"algraf-subtitle\" x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
            num(x),
            num(y),
            escape_attr(&theme.font_family),
            num(theme.font_size),
            escape_attr(&theme.text_color),
            escape_text(subtitle),
        ));
    }
    if let Some(caption) = &ir.caption {
        w.line(&format!(
            "<text class=\"algraf-caption\" x=\"{}\" y=\"{}\" text-anchor=\"end\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
            num(width - 16.0),
            num(height - 12.0),
            escape_attr(&theme.font_family),
            num(theme.font_size),
            escape_attr(&theme.text_color),
            escape_text(caption),
        ));
    }
}

fn emit_rect_fill(
    w: &mut SvgWriter,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    color: &str,
    class: &str,
) {
    w.empty_element(
        "rect",
        &[
            SvgAttr::new("class", class),
            SvgAttr::number("x", x),
            SvgAttr::number("y", y),
            SvgAttr::number("width", width),
            SvgAttr::number("height", height),
            SvgAttr::new("fill", color),
        ],
    );
}
