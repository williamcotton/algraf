use algraf_core::{codes, Diagnostic};
use algraf_data::Table;
use algraf_semantics::{GeometryIr, PropertyKey};

use crate::aes::{color_spec, number_setting};
use crate::helpers::{number_setting_opt, string_setting};
use crate::render::TextAnchor;
use crate::sink::{MarkSink, TextRun};

use super::common::{
    any_mapped, constant_or, emit_svg_line, emit_svg_line_with_dash, pos_center, render_rows,
    DEFAULT_STROKE,
};
use super::GeometryRenderContext;

pub(super) fn render_hline(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
) {
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
    let dash = string_setting(geo, PropertyKey::Dash);
    emit_svg_line_with_dash(
        sink,
        plot.x,
        y,
        plot.right(),
        y,
        &color,
        width,
        alpha,
        dash.as_deref(),
    );
    if let Some(label) = string_setting(geo, PropertyKey::Label) {
        sink.text(&TextRun {
            x: plot.right() - 4.0,
            y: y - 4.0,
            anchor: TextAnchor::End,
            rotate: None,
            font_family: &theme.font_family,
            font_size: theme.font_size,
            fill: &theme.text_color,
            opacity: None,
            content: &label,
        });
    }
}

pub(super) fn render_vline(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
) {
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
    let dash = string_setting(geo, PropertyKey::Dash);
    emit_svg_line_with_dash(
        sink,
        x,
        plot.y,
        x,
        plot.bottom(),
        &color,
        width,
        alpha,
        dash.as_deref(),
    );
    if let Some(label) = string_setting(geo, PropertyKey::Label) {
        sink.text(&TextRun {
            x: x + 4.0,
            y: plot.y + theme.font_size,
            anchor: TextAnchor::Start,
            rotate: None,
            font_family: &theme.font_family,
            font_size: theme.font_size,
            fill: &theme.text_color,
            opacity: None,
            content: &label,
        });
    }
}

pub(super) fn render_rug(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
) {
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
                    sink,
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
                emit_svg_line(sink, x, plot.y, x, plot.y + tick, &color, width, alpha);
            }
        }
        if sides.contains('l') {
            if let Some(y) = space.resolve_y(table, row) {
                emit_svg_line(sink, plot.x, y, plot.x + tick, y, &color, width, alpha);
            }
        }
        if sides.contains('r') {
            if let Some(y) = space.resolve_y(table, row) {
                emit_svg_line(
                    sink,
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

/// Render `Segment` marks (spec §14.19). Endpoints `x`/`y`/`xend`/`yend` may be
/// literal data values (a single annotation segment) or column mappings (one
/// segment per row, for slope/dumbbell charts). Mapped categorical endpoints
/// resolve to band centers; rows missing any endpoint are skipped and reported.
pub(super) fn render_segment(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let theme = ctx.theme;
    let scales = ctx.scales;
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let width = number_setting(geo, PropertyKey::StrokeWidth, theme.line_width);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let dash = string_setting(geo, PropertyKey::Dash);

    let x_axis = space.x_axis();
    let Some(y_axis) = space.y_axis() else {
        return;
    };
    let endpoints = [
        PropertyKey::X,
        PropertyKey::Y,
        PropertyKey::Xend,
        PropertyKey::Yend,
    ];

    let resolve = |table: &dyn Table, row: usize| {
        Some((
            pos_center(geo, PropertyKey::X, x_axis, table, row)?,
            pos_center(geo, PropertyKey::Y, y_axis, table, row)?,
            pos_center(geo, PropertyKey::Xend, x_axis, table, row)?,
            pos_center(geo, PropertyKey::Yend, y_axis, table, row)?,
        ))
    };

    if any_mapped(geo, &endpoints) {
        let mut skipped = 0usize;
        for row in render_rows(table, ctx.rows) {
            let Some((x, y, xend, yend)) = resolve(table, row) else {
                skipped += 1;
                continue;
            };
            let color = stroke
                .resolve(table, row)
                .unwrap_or_else(|| DEFAULT_STROKE.to_string());
            emit_svg_line_with_dash(
                sink,
                x,
                y,
                xend,
                yend,
                &color,
                width,
                alpha,
                dash.as_deref(),
            );
        }
        if skipped > 0 {
            diagnostics.push(Diagnostic::warning(
                codes::R0002,
                format!("Segment skipped {skipped} row(s) with a missing endpoint value"),
                geo.span,
            ));
        }
    } else if let Some((x, y, xend, yend)) = resolve(table, 0) {
        // Literal endpoints: a single annotation segment.
        let color = constant_or(&stroke, DEFAULT_STROKE);
        emit_svg_line_with_dash(
            sink,
            x,
            y,
            xend,
            yend,
            &color,
            width,
            alpha,
            dash.as_deref(),
        );
    }
}
