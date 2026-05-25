use std::fmt::Write;

use algraf_core::{codes, Diagnostic};
use algraf_semantics::{GeometryIr, PropertyKey};

use crate::aes::{color_spec, number_setting, number_spec, ColorSpec, NumberSpec};
use crate::svg::{escape_attr, num, SvgWriter};

use super::common::{
    axis_is_continuousish, constant_or, grouped_rows, grouped_rows_by_color, pos, render_rows,
    stroke_attrs, DEFAULT_FILL, DEFAULT_STROKE, DEFAULT_STROKE_WIDTH_RANGE,
};
use super::GeometryRenderContext;

/// Render a `Line` (`sort = true`, x-sorted) or `Path` (`sort = false`, source
/// order) polyline (spec §14.x). `strokeWidth` may be a constant or a column
/// mapping; a mapped width is drawn per segment from its endpoints' scaled
/// values (spec §13.8).
pub(super) fn render_polyline(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    sort: bool,
) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let theme = ctx.theme;
    let scales = ctx.scales;
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let width = number_spec(
        geo,
        PropertyKey::StrokeWidth,
        table,
        scales,
        DEFAULT_STROKE_WIDTH_RANGE,
        theme.line_width,
    );
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let row_list = render_rows(table, rows);

    // Group rows into series by `group` if present; otherwise preserve the
    // historical behavior of grouping by stroke category.
    let groups: Vec<(String, Vec<usize>)> = grouped_rows(geo, &stroke, table, row_list);

    for (cat, rows) in groups {
        let mut points: Vec<(f64, f64, usize)> = rows
            .iter()
            .filter_map(|&r| Some((space.resolve_x(table, r)?, space.resolve_y(table, r)?, r)))
            .collect();
        if sort {
            points.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        if points.is_empty() {
            continue;
        }
        let group_color = if cat.is_empty() {
            constant_or(&stroke, DEFAULT_STROKE)
        } else {
            stroke
                .resolve(table, points[0].2)
                .unwrap_or_else(|| DEFAULT_STROKE.to_string())
        };

        match &width {
            // Constant width: a single polyline path (compact output).
            NumberSpec::Constant(width) => {
                let mut d = String::new();
                for (i, (x, y, _)) in points.iter().enumerate() {
                    let cmd = if i == 0 { 'M' } else { 'L' };
                    let _ = write!(d, "{cmd}{} {} ", num(*x), num(*y));
                }
                w.line(&format!(
                    "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"{}\" />",
                    d.trim_end(),
                    escape_attr(&group_color),
                    num(*width),
                    num(alpha),
                ));
            }
            // Mapped width: one segment per adjacent pair, each with a width
            // averaged from its endpoints' scaled values (spec §13.8).
            NumberSpec::Scaled { .. } => {
                for pair in points.windows(2) {
                    let (x0, y0, r0) = pair[0];
                    let (x1, y1, r1) = pair[1];
                    let seg_width = (width.at(table, r0, theme.line_width)
                        + width.at(table, r1, theme.line_width))
                        / 2.0;
                    let color = stroke
                        .resolve(table, r0)
                        .unwrap_or_else(|| group_color.clone());
                    w.line(&format!(
                        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" opacity=\"{}\" />",
                        num(x0),
                        num(y0),
                        num(x1),
                        num(y1),
                        escape_attr(&color),
                        num(seg_width.max(0.0)),
                        num(alpha),
                    ));
                }
            }
        }
    }
}

pub(super) fn render_smooth(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let theme = ctx.theme;
    let scales = ctx.scales;
    if !axis_is_continuousish(&space.x) || !space.y.as_ref().is_some_and(axis_is_continuousish) {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "Smooth requires continuous x and y dimensions",
            geo.span,
        ));
        return;
    }

    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let width = number_setting(geo, PropertyKey::StrokeWidth, theme.line_width);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let row_list = render_rows(table, rows);

    for (_, group_rows) in grouped_rows(geo, &stroke, table, row_list) {
        let mut points: Vec<(f64, f64)> = group_rows
            .iter()
            .filter_map(|&r| Some((space.resolve_x(table, r)?, space.resolve_y(table, r)?)))
            .collect();
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        let Some((x0, y0, x1, y1)) = linear_fit_segment(&points) else {
            diagnostics.push(Diagnostic::warning(
                codes::R0002,
                "Smooth requires at least two distinct x values",
                geo.span,
            ));
            continue;
        };
        let color = group_rows
            .first()
            .and_then(|&row| stroke.resolve(table, row))
            .unwrap_or_else(|| DEFAULT_STROKE.to_string());
        w.line(&format!(
            "<path d=\"M{} {} L{} {}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"{}\" />",
            num(x0),
            num(y0),
            num(x1),
            num(y1),
            escape_attr(&color),
            num(width),
            num(alpha),
        ));
    }
}

fn linear_fit_segment(points: &[(f64, f64)]) -> Option<(f64, f64, f64, f64)> {
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let sum_x: f64 = points.iter().map(|(x, _)| *x).sum();
    let sum_y: f64 = points.iter().map(|(_, y)| *y).sum();
    let mean_x = sum_x / n;
    let mean_y = sum_y / n;
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for (x, y) in points {
        numerator += (x - mean_x) * (y - mean_y);
        denominator += (x - mean_x).powi(2);
    }
    if denominator.abs() <= f64::EPSILON {
        return None;
    }
    let slope = numerator / denominator;
    let intercept = mean_y - slope * mean_x;
    let x0 = points.first()?.0;
    let x1 = points.last()?.0;
    Some((x0, intercept + slope * x0, x1, intercept + slope * x1))
}

pub(super) fn render_ribbon(w: &mut SvgWriter, geo: &GeometryIr, ctx: GeometryRenderContext<'_>) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let scales = ctx.scales;
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, 1.0);
    let alpha = number_setting(geo, PropertyKey::Alpha, 0.25);
    let row_list = render_rows(table, rows);
    let groups = match (&fill, &stroke) {
        (ColorSpec::Categorical { .. }, _) => grouped_rows_by_color(&fill, table, row_list),
        (_, ColorSpec::Categorical { .. }) => grouped_rows_by_color(&stroke, table, row_list),
        _ => vec![row_list],
    };

    for group_rows in groups {
        let mut points: Vec<(f64, f64, f64, usize)> = group_rows
            .iter()
            .filter_map(|&row| {
                let x = space.resolve_x(table, row)?;
                let ymin = pos(geo, PropertyKey::Ymin, table, row).and_then(|v| space.map_y(v))?;
                let ymax = pos(geo, PropertyKey::Ymax, table, row).and_then(|v| space.map_y(v))?;
                Some((x, ymin, ymax, row))
            })
            .collect();
        if points.len() < 2 {
            continue;
        }
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut d = String::new();
        for (i, (x, _, ymax, _)) in points.iter().enumerate() {
            let cmd = if i == 0 { 'M' } else { 'L' };
            let _ = write!(d, "{cmd}{} {} ", num(*x), num(*ymax));
        }
        for (x, ymin, _, _) in points.iter().rev() {
            let _ = write!(d, "L{} {} ", num(*x), num(*ymin));
        }
        d.push('Z');

        let first_row = points[0].3;
        let fill_color = fill
            .resolve(table, first_row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        w.line(&format!(
            "<path d=\"{}\" fill=\"{}\"{} opacity=\"{}\" />",
            d.trim_end(),
            escape_attr(&fill_color),
            stroke_attrs(&stroke, stroke_width, table, first_row),
            num(alpha),
        ));
    }
}

/// Render an `Area` geometry: fill between y and a baseline (spec §14.14).
pub(super) fn render_area(w: &mut SvgWriter, geo: &GeometryIr, ctx: GeometryRenderContext<'_>) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let scales = ctx.scales;
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, 1.0);
    let alpha = number_setting(geo, PropertyKey::Alpha, 0.4);
    let baseline_value = number_setting(geo, PropertyKey::Baseline, 0.0);
    let Some(baseline_y) = space.map_y(baseline_value) else {
        return;
    };

    let row_list = render_rows(table, rows);
    let groups = match &fill {
        ColorSpec::Categorical { .. } => grouped_rows_by_color(&fill, table, row_list),
        _ => match &stroke {
            ColorSpec::Categorical { .. } => grouped_rows_by_color(&stroke, table, row_list),
            _ => vec![row_list],
        },
    };

    for group_rows in groups {
        let mut points: Vec<(f64, f64, usize)> = group_rows
            .iter()
            .filter_map(|&row| {
                Some((
                    space.resolve_x(table, row)?,
                    space.resolve_y(table, row)?,
                    row,
                ))
            })
            .collect();
        if points.len() < 2 {
            continue;
        }
        points.sort_by(|a, b| a.0.total_cmp(&b.0));

        let mut d = String::new();
        for (i, (x, y, _)) in points.iter().enumerate() {
            let cmd = if i == 0 { 'M' } else { 'L' };
            let _ = write!(d, "{cmd}{} {} ", num(*x), num(*y));
        }
        let last_x = points.last().unwrap().0;
        let first_x = points.first().unwrap().0;
        let _ = write!(d, "L{} {} ", num(last_x), num(baseline_y));
        let _ = write!(d, "L{} {} ", num(first_x), num(baseline_y));
        d.push('Z');

        let first_row = points[0].2;
        let fill_color = fill
            .resolve(table, first_row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        w.line(&format!(
            "<path d=\"{}\" fill=\"{}\"{} opacity=\"{}\" />",
            d.trim_end(),
            escape_attr(&fill_color),
            stroke_attrs(&stroke, stroke_width, table, first_row),
            num(alpha),
        ));
    }
}
