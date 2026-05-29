use algraf_semantics::{GeometryIr, PolarThetaIr, PropertyKey};

use crate::aes::{color_spec, number_setting};
use crate::sink::{Fill, MarkSink, Paint};
use crate::space::ScaledSpace;

use super::common::{mark_interaction, pos_bound, render_rows, stroke_style, DEFAULT_FILL};
use super::polar::annular_segment_path;
use super::GeometryRenderContext;

pub(super) fn render_rect(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let scales = ctx.scales;
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, 1.0);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    if space.is_polar() {
        render_rect_polar(
            sink,
            geo,
            space,
            table,
            rows,
            &fill,
            &stroke,
            stroke_width,
            alpha,
        );
        return;
    }
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
        sink.begin_mark(mark_interaction(geo, table, row));
        sink.rect(
            x,
            y,
            width,
            height,
            &Paint {
                fill: Fill::Color(color),
                stroke: stroke_style(&stroke, stroke_width, table, row),
                opacity: Some(alpha),
            },
        );
        sink.end_mark();
    }
}

pub(super) fn render_tile(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let scales = ctx.scales;
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, 1.0);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    for row in render_rows(table, rows) {
        // Annular tile (heatmap) in polar: angular band × radial band.
        if let Some(polar) = space.polar() {
            let (Some((center, bw)), Some((r_start, r_w))) = (
                space.polar_angle_band(table, row),
                space.polar_radius_band(table, row),
            ) else {
                continue;
            };
            let color = fill
                .resolve(table, row)
                .unwrap_or_else(|| DEFAULT_FILL.to_string());
            let d = annular_segment_path(
                polar,
                center - bw / 2.0,
                center + bw / 2.0,
                r_start,
                r_start + r_w,
            );
            sink.begin_mark(mark_interaction(geo, table, row));
            sink.path(
                &d,
                &Paint {
                    fill: Fill::Color(color),
                    stroke: stroke_style(&stroke, stroke_width, table, row),
                    opacity: Some(alpha),
                },
            );
            sink.end_mark();
            continue;
        }
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
        sink.begin_mark(mark_interaction(geo, table, row));
        sink.rect(
            cx - bw / 2.0,
            cy - bh / 2.0,
            bw,
            bh,
            &Paint {
                fill: Fill::Color(color),
                stroke: stroke_style(&stroke, stroke_width, table, row),
                opacity: Some(alpha),
            },
        );
        sink.end_mark();
    }
}

/// Render `Rect` cells as annular segments in polar (e.g. circular histogram):
/// the `x` bounds map to angles and the `y` bounds to radii (spec §16.16).
#[allow(clippy::too_many_arguments)]
fn render_rect_polar(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn algraf_data::Table,
    rows: Option<&[usize]>,
    fill: &crate::aes::ColorSpec,
    stroke: &crate::aes::ColorSpec,
    stroke_width: f64,
    alpha: f64,
) {
    let Some(polar) = space.polar() else {
        return;
    };
    let theta_is_x = matches!(polar.theta, PolarThetaIr::X);
    // The angular extent comes from the theta axis's bound properties, the radial
    // extent from the radius axis's.
    let (angle_axis, radius_axis) = if theta_is_x {
        (&space.x, space.y.as_ref())
    } else {
        (space.y.as_ref().unwrap_or(&space.x), Some(&space.x))
    };
    let (angle_min, angle_max) = if theta_is_x {
        (PropertyKey::Xmin, PropertyKey::Xmax)
    } else {
        (PropertyKey::Ymin, PropertyKey::Ymax)
    };
    let (radius_min, radius_max) = if theta_is_x {
        (PropertyKey::Ymin, PropertyKey::Ymax)
    } else {
        (PropertyKey::Xmin, PropertyKey::Xmax)
    };
    for row in render_rows(table, rows) {
        let (Some(a0), Some(a1)) = (
            pos_bound(geo, angle_min, angle_axis, table, row),
            pos_bound(geo, angle_max, angle_axis, table, row),
        ) else {
            continue;
        };
        let (Some(r0), Some(r1)) = (
            radius_axis.and_then(|ax| pos_bound(geo, radius_min, ax, table, row)),
            radius_axis.and_then(|ax| pos_bound(geo, radius_max, ax, table, row)),
        ) else {
            continue;
        };
        if (a1 - a0).abs() <= f64::EPSILON {
            continue;
        }
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        let d = annular_segment_path(polar, a0, a1, r0.min(r1), r0.max(r1));
        sink.begin_mark(mark_interaction(geo, table, row));
        sink.path(
            &d,
            &Paint {
                fill: Fill::Color(color),
                stroke: stroke_style(stroke, stroke_width, table, row),
                opacity: Some(alpha),
            },
        );
        sink.end_mark();
    }
}
