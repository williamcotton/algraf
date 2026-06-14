use algraf_core::{codes, Diagnostic};
use algraf_data::Table;
use algraf_semantics::{GeometryIr, PropertyKey};

use crate::aes::{color_spec, number_setting};
use crate::helpers::{number_setting_opt, string_setting};
use crate::layout::Rect;
use crate::render::TextAnchor;
use crate::sink::{Fill, MarkSink, Paint, Stroke, TextRun};
use crate::theme::Theme;

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
        if has_badge_args(geo) {
            // For HLine the label rides at the start (left) or end (right) of the
            // rule, centered on the line (spec §14.17).
            let position = string_setting(geo, PropertyKey::LabelPosition);
            let end = !matches!(position.as_deref(), Some("start"));
            let extent = badge_extent(&label, theme);
            let cx = if end {
                plot.right() - extent - 2.0
            } else {
                plot.x + extent + 2.0
            };
            draw_callout_badge(sink, geo, cx, y, &label, &color, theme);
        } else {
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
        if has_badge_args(geo) {
            // For VLine the label rides at the top (default) or bottom of the
            // rule, centered on the line (spec §14.18).
            let position = string_setting(geo, PropertyKey::LabelPosition);
            let bottom = matches!(position.as_deref(), Some("bottom"));
            let extent = badge_extent(&label, theme);
            let cy = if bottom {
                plot.bottom() - extent - 2.0
            } else {
                plot.y + extent + 2.0
            };
            draw_callout_badge(sink, geo, x, cy, &label, &color, theme);
        } else {
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
}

/// Whether a reference rule uses any callout-badge argument (spec §14.17–14.18).
/// When none are present the legacy plain-text label path runs unchanged, so
/// existing `VLine`/`HLine` output stays byte-for-byte identical.
fn has_badge_args(geo: &GeometryIr) -> bool {
    string_setting(geo, PropertyKey::LabelShape).is_some()
        || string_setting(geo, PropertyKey::LabelPosition).is_some()
        || string_setting(geo, PropertyKey::LabelFill).is_some()
        || string_setting(geo, PropertyKey::LabelStroke).is_some()
}

/// Half the badge's box size, also used to inset it from the plot edge. Derived
/// from the label text via the deterministic estimated-width model so output
/// stays byte-stable (spec §14.18).
fn badge_extent(label: &str, theme: &Theme) -> f64 {
    let font = theme.font_size;
    let text_w = label.chars().count() as f64 * font * 0.6;
    text_w.max(font) / 2.0 + 3.0
}

/// Draw a callout badge (circle/square box plus centered text) or, for
/// `labelShape: "none"`, plain centered text on the rule. The badge is emitted as
/// existing scene primitives so it participates in draw-list metadata
/// (spec §14.17–14.18).
fn draw_callout_badge(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    cx: f64,
    cy: f64,
    label: &str,
    rule_color: &str,
    theme: &Theme,
) {
    let shape = string_setting(geo, PropertyKey::LabelShape);
    let shape = shape.as_deref().unwrap_or("none");
    let extent = badge_extent(label, theme);
    let font = theme.font_size;
    let baseline_shift = font * 0.35;
    if shape == "none" {
        // Plain centered text uses the badge fill (or the rule color) directly.
        let fill =
            string_setting(geo, PropertyKey::LabelFill).unwrap_or_else(|| rule_color.to_string());
        sink.text(&TextRun {
            x: cx,
            y: cy + baseline_shift,
            anchor: TextAnchor::Middle,
            rotate: None,
            font_family: &theme.font_family,
            font_size: font,
            fill: &fill,
            opacity: None,
            content: label,
        });
        return;
    }
    let badge_fill =
        string_setting(geo, PropertyKey::LabelFill).unwrap_or_else(|| rule_color.to_string());
    let badge_stroke = string_setting(geo, PropertyKey::LabelStroke);
    let paint = Paint {
        fill: Fill::Color(badge_fill.clone()),
        stroke: match &badge_stroke {
            Some(color) => Stroke::Solid {
                color: color.clone(),
                width: 1.0,
            },
            None => Stroke::Omit,
        },
        opacity: None,
    };
    if shape == "square" {
        let rect = Rect {
            x: cx - extent,
            y: cy - extent,
            width: extent * 2.0,
            height: extent * 2.0,
        };
        sink.rect(rect.x, rect.y, rect.width, rect.height, &paint);
    } else {
        // Default badge box is a circle.
        sink.circle(cx, cy, extent, &paint);
    }
    // The digit/text inside a filled badge needs a contrasting color.
    let text_fill = readable_text_color(&badge_fill);
    sink.text(&TextRun {
        x: cx,
        y: cy + baseline_shift,
        anchor: TextAnchor::Middle,
        rotate: None,
        font_family: &theme.font_family,
        font_size: font,
        fill: text_fill,
        opacity: None,
        content: label,
    });
}

/// Pick black or white text for legibility on the badge fill color (spec
/// §14.18). Unparseable colors default to dark text.
fn readable_text_color(fill: &str) -> &'static str {
    match crate::theme::parse_svg_color(fill) {
        Some(rgba) => {
            // Rec. 601 relative luminance.
            let luma =
                0.299 * f64::from(rgba.r) + 0.587 * f64::from(rgba.g) + 0.114 * f64::from(rgba.b);
            if luma < 140.0 {
                "#ffffff"
            } else {
                "#111111"
            }
        }
        None => "#111111",
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
