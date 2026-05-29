use algraf_core::Diagnostic;
use algraf_semantics::ChartIr;

use crate::guide;
use crate::layout::Layout;
use crate::sink::{MarkSink, SvgSink};
use crate::svg::{escape_attr, escape_text, num, SvgAttr, SvgWriter};
use crate::theme::Theme;

use super::backend::RenderScene;
use super::panels::{panel_slots, Panel, PanelSlot};

pub(super) fn emit_document(scene: &RenderScene<'_>, diagnostics: &mut Vec<Diagnostic>) -> String {
    let RenderScene {
        ir,
        layout,
        legends,
        panels,
        theme,
    } = *scene;
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

    // The chart body and guides are emitted through the shared mark sink so the
    // SVG and draw-list backends agree on coordinates and colors (spec §24.6).
    {
        let mut sink = SvgSink::new(&mut w);
        for slot in &slots {
            if let (Some(strip), Some(panel)) = (slot.strip, slot.panel) {
                guide::render_facet_label(
                    &mut sink,
                    slot.label.unwrap_or_default(),
                    strip,
                    &panel.theme,
                );
            }
        }
        paint_grid(&mut sink, &slots);
        paint_geometries(&mut sink, panels, diagnostics);
        paint_axes_and_legends(&mut sink, &slots, legends, layout, theme);
    }

    w.line("</svg>");
    w.finish()
}

/// Draw gridlines behind the data marks, for every panel (spec §17.6, §16.16).
/// Shared by the SVG and draw-list backends.
pub(super) fn paint_grid(sink: &mut dyn MarkSink, slots: &[PanelSlot<'_>]) {
    for slot in slots {
        if let Some(panel) = slot.panel {
            if panel.scaled.is_polar() {
                guide::render_polar_grid(sink, &panel.scaled, &panel.guides, &panel.theme);
            } else if panel.guides.grid && !panel.scaled.is_spatial() {
                guide::render_grid(sink, &panel.scaled, panel.plot, &panel.theme);
            }
        }
    }
}

/// Draw the per-datum geometry marks of every layer in source order (spec
/// §18.3). Shared by the SVG and draw-list backends.
pub(super) fn paint_geometries(
    sink: &mut dyn MarkSink,
    panels: &[Panel<'_>],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for panel in panels {
        for geo in panel.geometries {
            crate::geom::render(
                sink,
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
}

/// Draw axes (or polar labels) and legends above the data marks. Shared by the
/// SVG and draw-list backends.
pub(super) fn paint_axes_and_legends(
    sink: &mut dyn MarkSink,
    slots: &[PanelSlot<'_>],
    legends: &[crate::aes::Legend],
    layout: &Layout,
    theme: &Theme,
) {
    for slot in slots {
        if let Some(panel) = slot.panel {
            // Spatial spaces have no lat/lon axes (spec §16.15); polar spaces use
            // ring/spoke guides instead of Cartesian axes (§16.16).
            if panel.scaled.is_polar() {
                guide::render_polar_labels(sink, &panel.scaled, &panel.guides, &panel.theme);
            } else if panel.theme.axes && !panel.scaled.is_spatial() {
                guide::render_axes(
                    sink,
                    &panel.scaled,
                    panel.plot,
                    &panel.theme,
                    guide::AxisRenderOptions {
                        x_label_override: panel.guides.x_label.as_deref(),
                        y_label_override: panel.guides.y_label.as_deref(),
                        x_time_format: panel.guides.x_time_format.as_ref(),
                        y_time_format: panel.guides.y_time_format.as_ref(),
                        x_tick_label_angle: panel.guides.x_tick_label_angle,
                        y_tick_label_angle: panel.guides.y_tick_label_angle,
                    },
                );
            }
        }
    }
    if let Some(area) = layout.legend {
        guide::render_legends(sink, legends, area, theme);
    }
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
