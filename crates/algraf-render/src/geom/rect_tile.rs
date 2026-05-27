use algraf_semantics::{GeometryIr, PropertyKey};

use crate::aes::{color_spec, number_setting};
use crate::svg::{escape_attr, num, SvgWriter};

use super::common::{pos_bound, render_rows, stroke_attrs, DEFAULT_FILL};
use super::GeometryRenderContext;

pub(super) fn render_rect(w: &mut SvgWriter, geo: &GeometryIr, ctx: GeometryRenderContext<'_>) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let scales = ctx.scales;
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, 1.0);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    for row in render_rows(table, rows) {
        let (Some(xmin), Some(xmax), Some(ymin), Some(ymax)) = (
            pos_bound(geo, PropertyKey::Xmin, &space.x, table, row),
            pos_bound(geo, PropertyKey::Xmax, &space.x, table, row),
            space
                .y
                .as_ref()
                .and_then(|axis| pos_bound(geo, PropertyKey::Ymin, axis, table, row)),
            space
                .y
                .as_ref()
                .and_then(|axis| pos_bound(geo, PropertyKey::Ymax, axis, table, row)),
        ) else {
            continue;
        };
        let x = xmin.min(xmax);
        let y = ymin.min(ymax);
        let width = (xmax - xmin).abs();
        let height = (ymax - ymin).abs();
        if width <= f64::EPSILON || height <= f64::EPSILON {
            continue;
        }
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        w.line(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{} opacity=\"{}\" />",
            num(x),
            num(y),
            num(width),
            num(height),
            escape_attr(&color),
            stroke_attrs(&stroke, stroke_width, table, row),
            num(alpha),
        ));
    }
}

pub(super) fn render_tile(w: &mut SvgWriter, geo: &GeometryIr, ctx: GeometryRenderContext<'_>) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let scales = ctx.scales;
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, 1.0);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    for row in render_rows(table, rows) {
        let (Some(cx), Some(bw), Some(cy), Some(bh)) = (
            space.resolve_x(table, row),
            space.x_bandwidth(table, row),
            space.resolve_y(table, row),
            space.y_bandwidth(table, row),
        ) else {
            continue;
        };
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        w.line(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{} opacity=\"{}\" />",
            num(cx - bw / 2.0),
            num(cy - bh / 2.0),
            num(bw),
            num(bh),
            escape_attr(&color),
            stroke_attrs(&stroke, stroke_width, table, row),
            num(alpha),
        ));
    }
}
