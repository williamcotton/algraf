use algraf_semantics::{GeometryIr, PropertyKey};

use crate::aes::{color_spec, number_setting};
use crate::helpers::{number_setting_opt, string_setting};
use crate::svg::{escape_attr, escape_text, num, SvgWriter};

use super::common::{constant_or, emit_svg_line, render_rows, DEFAULT_STROKE};
use super::GeometryRenderContext;

pub(super) fn render_hline(w: &mut SvgWriter, geo: &GeometryIr, ctx: GeometryRenderContext<'_>) {
    let space = ctx.space;
    let plot = ctx.plot;
    let table = ctx.table;
    let theme = ctx.theme;
    let scales = ctx.scales;
    let Some(y) = number_setting_opt(geo, PropertyKey::Y).and_then(|value| space.map_y(value))
    else {
        return;
    };
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let color = constant_or(&stroke, DEFAULT_STROKE);
    let width = number_setting(geo, PropertyKey::StrokeWidth, theme.line_width);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    emit_svg_line(w, plot.x, y, plot.right(), y, &color, width, alpha);
    if let Some(label) = string_setting(geo, PropertyKey::Label) {
        w.line(&format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"end\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
            num(plot.right() - 4.0),
            num(y - 4.0),
            escape_attr(&theme.font_family),
            num(theme.font_size),
            escape_attr(&theme.text_color),
            escape_text(&label),
        ));
    }
}

pub(super) fn render_vline(w: &mut SvgWriter, geo: &GeometryIr, ctx: GeometryRenderContext<'_>) {
    let space = ctx.space;
    let plot = ctx.plot;
    let table = ctx.table;
    let theme = ctx.theme;
    let scales = ctx.scales;
    let Some(x) = number_setting_opt(geo, PropertyKey::X).and_then(|value| space.map_x(value))
    else {
        return;
    };
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let color = constant_or(&stroke, DEFAULT_STROKE);
    let width = number_setting(geo, PropertyKey::StrokeWidth, theme.line_width);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    emit_svg_line(w, x, plot.y, x, plot.bottom(), &color, width, alpha);
    if let Some(label) = string_setting(geo, PropertyKey::Label) {
        w.line(&format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"start\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
            num(x + 4.0),
            num(plot.y + theme.font_size),
            escape_attr(&theme.font_family),
            num(theme.font_size),
            escape_attr(&theme.text_color),
            escape_text(&label),
        ));
    }
}

pub(super) fn render_rug(w: &mut SvgWriter, geo: &GeometryIr, ctx: GeometryRenderContext<'_>) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let plot = ctx.plot;
    let theme = ctx.theme;
    let scales = ctx.scales;
    let sides = string_setting(geo, PropertyKey::Sides).unwrap_or_else(|| "b".to_string());
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let width = number_setting(geo, PropertyKey::StrokeWidth, theme.line_width);
    let alpha = number_setting(geo, PropertyKey::Alpha, 0.55);
    let tick = 6.0;
    for row in render_rows(table, rows) {
        let color = stroke
            .resolve(table, row)
            .unwrap_or_else(|| DEFAULT_STROKE.to_string());
        if sides.contains('b') {
            if let Some(x) = space.resolve_x(table, row) {
                emit_svg_line(
                    w,
                    x,
                    plot.bottom(),
                    x,
                    plot.bottom() - tick,
                    &color,
                    width,
                    alpha,
                );
            }
        }
        if sides.contains('t') {
            if let Some(x) = space.resolve_x(table, row) {
                emit_svg_line(w, x, plot.y, x, plot.y + tick, &color, width, alpha);
            }
        }
        if sides.contains('l') {
            if let Some(y) = space.resolve_y(table, row) {
                emit_svg_line(w, plot.x, y, plot.x + tick, y, &color, width, alpha);
            }
        }
        if sides.contains('r') {
            if let Some(y) = space.resolve_y(table, row) {
                emit_svg_line(
                    w,
                    plot.right(),
                    y,
                    plot.right() - tick,
                    y,
                    &color,
                    width,
                    alpha,
                );
            }
        }
    }
}

/// Render a `Segment` geometry: a straight line between literal endpoints
/// (spec §14.19).
pub(super) fn render_segment(w: &mut SvgWriter, geo: &GeometryIr, ctx: GeometryRenderContext<'_>) {
    let space = ctx.space;
    let table = ctx.table;
    let theme = ctx.theme;
    let scales = ctx.scales;
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let color = constant_or(&stroke, DEFAULT_STROKE);
    let width = number_setting(geo, PropertyKey::StrokeWidth, theme.line_width);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);

    let (Some(x), Some(y), Some(xend), Some(yend)) = (
        number_setting_opt(geo, PropertyKey::X).and_then(|v| space.map_x(v)),
        number_setting_opt(geo, PropertyKey::Y).and_then(|v| space.map_y(v)),
        number_setting_opt(geo, PropertyKey::Xend).and_then(|v| space.map_x(v)),
        number_setting_opt(geo, PropertyKey::Yend).and_then(|v| space.map_y(v)),
    ) else {
        return;
    };

    emit_svg_line(w, x, y, xend, yend, &color, width, alpha);
}
