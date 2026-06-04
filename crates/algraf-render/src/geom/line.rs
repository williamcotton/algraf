use std::collections::HashMap;
use std::fmt::Write;

use algraf_core::{codes, Diagnostic};
use algraf_semantics::{GeometryIr, PropertyKey};

use crate::aes::{color_spec, number_setting, number_spec, ColorSpec, NumberSpec};
use crate::helpers::{area_layout, bool_setting, number_setting_opt, string_setting, AreaLayout};
use crate::scale::cell_f64;
use crate::sink::{Dash, Fill, MarkSink, Paint, Stroke};
use crate::stats;
use crate::svg::num;

use super::common::{
    axis_is_continuousish, constant_or, grouped_rows, grouped_rows_by_color, pos, render_rows,
    stroke_style, DEFAULT_FILL, DEFAULT_STROKE, DEFAULT_STROKE_WIDTH_RANGE,
};
use super::polar::{ordered_points, point_path, point_path_with_spaced_close};
use super::GeometryRenderContext;

/// Render a `Line` (`sort = true`, x-sorted) or `Path` (`sort = false`, source
/// order) polyline (spec §14.x). `strokeWidth` may be a constant or a column
/// mapping; a mapped width is drawn per segment from its endpoints' scaled
/// values (spec §13.8).
pub(super) fn render_polyline(
    sink: &mut dyn MarkSink,
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
    let dash_setting = string_setting(geo, PropertyKey::Dash);
    let dash = Dash::from_setting(dash_setting.as_deref());
    // A mapped `strokeWidth` may render as a filled tapered ribbon instead of
    // per-segment strokes (spec §14.x). With a constant width it has no effect.
    let taper = bool_setting(geo, PropertyKey::Taper, false);
    let row_list = render_rows(table, rows);

    // Group rows into series by `group` if present; otherwise preserve the
    // historical behavior of grouping by stroke category.
    let groups: Vec<(String, Vec<usize>)> = grouped_rows(geo, &stroke, table, row_list);

    // Polar Line/Path: order vertices around the circle by angle; a `Line`
    // (sort) closes back to the first category for a radar polygon (spec §16.16).
    if space.is_polar() {
        let const_width = match &width {
            NumberSpec::Constant(wd) => *wd,
            _ => theme.line_width,
        };
        for (cat, rows) in groups {
            let points = ordered_points(space, table, &rows);
            if points.is_empty() {
                continue;
            }
            let group_color = if cat.is_empty() {
                constant_or(&stroke, DEFAULT_STROKE)
            } else {
                stroke
                    .resolve(table, points[0].row)
                    .unwrap_or_else(|| DEFAULT_STROKE.to_string())
            };
            // A closed radar polygon for `Line`; `Path` stays open.
            sink.path_with_dash(
                &point_path(&points, sort),
                &Paint {
                    fill: Fill::None,
                    stroke: Stroke::Solid {
                        color: group_color,
                        width: const_width,
                    },
                    opacity: Some(alpha),
                },
                dash,
            );
        }
        return;
    }

    for (cat, rows) in groups {
        let mut runs: Vec<Vec<(f64, f64, usize)>> = Vec::new();
        let mut current = Vec::new();
        for &r in &rows {
            if let (Some(x), Some(y)) = (space.resolve_x(table, r), space.resolve_y(table, r)) {
                current.push((x, y, r));
            } else if !current.is_empty() {
                runs.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            runs.push(current);
        }

        for mut points in runs {
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
                    sink.path_with_dash(
                        d.trim_end(),
                        &Paint {
                            fill: Fill::None,
                            stroke: Stroke::Solid {
                                color: group_color.clone(),
                                width: *width,
                            },
                            opacity: Some(alpha),
                        },
                        dash,
                    );
                }
                // Mapped width + taper: a single filled polygon whose half-width
                // at each vertex is the scaled strokeWidth (spec §14.x).
                NumberSpec::Scaled { .. } if taper && points.len() >= 2 => {
                    let pts: Vec<(f64, f64)> = points.iter().map(|(x, y, _)| (*x, *y)).collect();
                    let halves: Vec<f64> = points
                        .iter()
                        .map(|(_, _, r)| width.at(table, *r, theme.line_width).max(0.0) / 2.0)
                        .collect();
                    sink.path(
                        &tapered_ribbon_path(&pts, &halves),
                        &Paint {
                            fill: Fill::Color(group_color.clone()),
                            stroke: Stroke::None,
                            opacity: Some(alpha),
                        },
                    );
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
                        sink.line(x0, y0, x1, y1, &color, seg_width, true, Some(alpha), dash);
                    }
                }
            }
        }
    }
}

/// Build a filled tapered-ribbon path from a polyline and per-vertex half-widths
/// (spec §14.x). The outline runs forward along the `+normal` offset, then back
/// along `-normal`, and closes. `half[i]` aligns with `points[i]`.
fn tapered_ribbon_path(points: &[(f64, f64)], half: &[f64]) -> String {
    let offsets = vertex_offsets(points);
    let mut d = String::new();
    for (i, ((x, y), (ux, uy))) in points.iter().zip(&offsets).enumerate() {
        let cmd = if i == 0 { 'M' } else { 'L' };
        let _ = write!(
            d,
            "{cmd}{} {} ",
            num(x + ux * half[i]),
            num(y + uy * half[i])
        );
    }
    for i in (0..points.len()).rev() {
        let (x, y) = points[i];
        let (ux, uy) = offsets[i];
        let _ = write!(d, "L{} {} ", num(x - ux * half[i]), num(y - uy * half[i]));
    }
    d.push('Z');
    d
}

/// Per-vertex offset vectors (unit half-width = 1) for a tapered ribbon. Each is
/// the miter direction at the vertex, scaled so a half-width `h` offsets both
/// adjacent edges by `h` perpendicular distance. Sharp turns are capped to avoid
/// runaway miter spikes; endpoints use the single adjacent segment normal.
fn vertex_offsets(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let n = points.len();
    let seg_normal = |a: (f64, f64), b: (f64, f64)| -> Option<(f64, f64)> {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        (len > f64::EPSILON).then(|| (-dy / len, dx / len))
    };
    (0..n)
        .map(|i| {
            let prev = (i > 0)
                .then(|| seg_normal(points[i - 1], points[i]))
                .flatten();
            let next = (i + 1 < n)
                .then(|| seg_normal(points[i], points[i + 1]))
                .flatten();
            match (prev, next) {
                (Some(a), Some(b)) => {
                    let denom = 1.0 + a.0 * b.0 + a.1 * b.1;
                    let sum = (a.0 + b.0, a.1 + b.1);
                    if denom > 0.2 {
                        // Miter: keeps perpendicular width constant through the
                        // bend, capped to a 4× miter length.
                        let (mx, my) = (sum.0 / denom, sum.1 / denom);
                        let len = (mx * mx + my * my).sqrt();
                        if len > 4.0 {
                            (mx / len * 4.0, my / len * 4.0)
                        } else {
                            (mx, my)
                        }
                    } else {
                        // Near-reversal: fall back to a unit averaged normal.
                        let len = (sum.0 * sum.0 + sum.1 * sum.1).sqrt();
                        if len > f64::EPSILON {
                            (sum.0 / len, sum.1 / len)
                        } else {
                            a
                        }
                    }
                }
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => (0.0, 1.0),
            }
        })
        .collect()
}

pub(super) fn render_smooth(
    sink: &mut dyn MarkSink,
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
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let width = number_setting(geo, PropertyKey::StrokeWidth, theme.line_width);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let dash_setting = string_setting(geo, PropertyKey::Dash);
    let dash = Dash::from_setting(dash_setting.as_deref());

    // Fitting happens in pixel space; for linear position scales this is an
    // affine image of data space, so the fit is identical (spec §15.x).
    let method = match string_setting(geo, PropertyKey::Method).as_deref() {
        Some("loess") => stats::SmoothMethod::Loess,
        _ => stats::SmoothMethod::Lm,
    };
    let span = number_setting_opt(geo, PropertyKey::Span).unwrap_or(0.75);
    let se = bool_setting(geo, PropertyKey::Se, false);
    let options = stats::SmoothOptions {
        method,
        span,
        se,
        ..stats::SmoothOptions::default()
    };

    let row_list = render_rows(table, rows);

    for (_, group_rows) in grouped_rows(geo, &stroke, table, row_list) {
        if group_rows.is_empty() {
            continue;
        }
        let mut points: Vec<(f64, f64)> = group_rows
            .iter()
            .filter_map(|&r| Some((space.resolve_x(table, r)?, space.resolve_y(table, r)?)))
            .collect();
        let fitted = stats::smooth_points(&mut points, options);
        if fitted.len() < 2 {
            diagnostics.push(Diagnostic::warning(
                codes::R0002,
                "Smooth requires at least two distinct x values",
                geo.span,
            ));
            continue;
        }
        let color = group_rows
            .first()
            .and_then(|&row| stroke.resolve(table, row))
            .unwrap_or_else(|| DEFAULT_STROKE.to_string());

        // Confidence band first, so the fitted line draws on top of it.
        if se {
            let band_color = group_rows
                .first()
                .and_then(|&row| fill.resolve(table, row))
                .unwrap_or_else(|| color.clone());
            let mut d = String::new();
            for (i, p) in fitted.iter().enumerate() {
                let cmd = if i == 0 { 'M' } else { 'L' };
                let _ = write!(d, "{cmd}{} {} ", num(p.x), num(p.y - options.z * p.se));
            }
            for p in fitted.iter().rev() {
                let _ = write!(d, "L{} {} ", num(p.x), num(p.y + options.z * p.se));
            }
            d.push('Z');
            sink.path(
                d.trim_end(),
                &Paint {
                    fill: Fill::Color(band_color),
                    stroke: Stroke::None,
                    opacity: Some(0.2 * alpha),
                },
            );
        }

        let mut d = String::new();
        for (i, p) in fitted.iter().enumerate() {
            let cmd = if i == 0 { 'M' } else { 'L' };
            let _ = write!(d, "{cmd}{} {} ", num(p.x), num(p.y));
        }
        sink.path_with_dash(
            d.trim_end(),
            &Paint {
                fill: Fill::None,
                stroke: Stroke::Solid {
                    color: color.clone(),
                    width,
                },
                opacity: Some(alpha),
            },
            dash,
        );
    }
}

pub(super) fn render_ribbon(
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
    let alpha = number_setting(geo, PropertyKey::Alpha, 0.25);
    let row_list = render_rows(table, rows);
    let groups = match (&fill, &stroke) {
        (ColorSpec::Categorical { .. } | ColorSpec::Binned { .. }, _) => {
            grouped_rows_by_color(&fill, table, row_list)
        }
        (_, ColorSpec::Categorical { .. } | ColorSpec::Binned { .. }) => {
            grouped_rows_by_color(&stroke, table, row_list)
        }
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
        sink.path(
            d.trim_end(),
            &Paint {
                fill: Fill::Color(fill_color),
                stroke: stroke_style(&stroke, stroke_width, table, first_row),
                opacity: Some(alpha),
            },
        );
    }
}

/// Render an `Area` geometry: fill between y and a baseline (spec §14.14).
pub(super) fn render_area(
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
    let alpha = number_setting(geo, PropertyKey::Alpha, 0.4);

    let row_list = render_rows(table, rows);
    let groups = area_groups(geo, &fill, &stroke, table, row_list.clone());

    // Polar Area: a closed polygon through the angle-ordered vertices (radar),
    // filled directly rather than down to a baseline (spec §16.16).
    if space.is_polar() {
        for group_rows in groups {
            let points = ordered_points(space, table, &group_rows);
            if points.len() < 2 {
                continue;
            }
            let first_row = points[0].row;
            let fill_color = fill
                .resolve(table, first_row)
                .unwrap_or_else(|| DEFAULT_FILL.to_string());
            sink.path(
                &point_path_with_spaced_close(&points),
                &Paint {
                    fill: Fill::Color(fill_color),
                    stroke: stroke_style(&stroke, stroke_width, table, first_row),
                    opacity: Some(alpha),
                },
            );
        }
        return;
    }

    let baseline_value = number_setting(geo, PropertyKey::Baseline, 0.0);
    let Some(baseline_y) = space.map_y(baseline_value) else {
        return;
    };
    let layout = area_layout(geo);

    if layout != AreaLayout::Identity {
        let Some(value_col) = space.y_axis().and_then(|axis| axis.data_column()) else {
            return;
        };
        let x_positions = area_x_positions(space, table, &row_list, value_col);
        if x_positions.len() < 2 {
            return;
        }
        let totals = if layout == AreaLayout::Fill {
            area_totals_by_x(space, table, &row_list, value_col)
        } else {
            HashMap::new()
        };
        let mut positive: HashMap<String, f64> = HashMap::new();
        let mut negative: HashMap<String, f64> = HashMap::new();
        for group_rows in groups {
            let Some(group_row) = group_rows.first().copied() else {
                continue;
            };
            let group_values = area_group_values_by_x(space, table, &group_rows, value_col);
            if group_values.is_empty() {
                continue;
            }
            let mut points: Vec<(f64, f64, f64, usize)> = Vec::with_capacity(x_positions.len());
            for (key, x) in &x_positions {
                let mut value = group_values.get(key).copied().unwrap_or(0.0);
                if layout == AreaLayout::Fill {
                    let total = if value >= 0.0 {
                        totals
                            .get(key)
                            .map(|(positive, _)| *positive)
                            .unwrap_or(0.0)
                    } else {
                        totals
                            .get(key)
                            .map(|(_, negative)| negative.abs())
                            .unwrap_or(0.0)
                    };
                    if total <= f64::EPSILON {
                        value = 0.0;
                    } else {
                        value /= total;
                    }
                }
                let accumulator = if value >= 0.0 {
                    positive.entry(key.clone()).or_insert(0.0)
                } else {
                    negative.entry(key.clone()).or_insert(0.0)
                };
                let lower_value = baseline_value + *accumulator;
                *accumulator += value;
                let upper_value = baseline_value + *accumulator;
                let (Some(lower), Some(upper)) =
                    (space.map_y(lower_value), space.map_y(upper_value))
                else {
                    continue;
                };
                points.push((*x, lower, upper, group_row));
            }
            emit_area_polygon(sink, table, &points, &fill, &stroke, stroke_width, alpha);
        }
        return;
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

        let area_points = points
            .into_iter()
            .map(|(x, y, row)| (x, baseline_y, y, row))
            .collect::<Vec<_>>();
        emit_area_polygon(
            sink,
            table,
            &area_points,
            &fill,
            &stroke,
            stroke_width,
            alpha,
        );
    }
}

fn area_groups(
    geo: &GeometryIr,
    fill: &ColorSpec,
    stroke: &ColorSpec,
    table: &dyn algraf_data::Table,
    rows: Vec<usize>,
) -> Vec<Vec<usize>> {
    if geo
        .mappings
        .iter()
        .any(|mapping| mapping.aesthetic == PropertyKey::Group)
    {
        return grouped_rows(geo, stroke, table, rows)
            .into_iter()
            .map(|(_, rows)| rows)
            .collect();
    }

    match fill {
        ColorSpec::Categorical { .. } | ColorSpec::Binned { .. } => {
            grouped_rows_by_color(fill, table, rows)
        }
        _ => match stroke {
            ColorSpec::Categorical { .. } | ColorSpec::Binned { .. } => {
                grouped_rows_by_color(stroke, table, rows)
            }
            _ => vec![rows],
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_area_polygon(
    sink: &mut dyn MarkSink,
    table: &dyn algraf_data::Table,
    points: &[(f64, f64, f64, usize)],
    fill: &ColorSpec,
    stroke: &ColorSpec,
    stroke_width: f64,
    alpha: f64,
) {
    if points.len() < 2 {
        return;
    }
    let mut points = points.to_vec();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut d = String::new();
    for (i, (x, _, upper, _)) in points.iter().enumerate() {
        let cmd = if i == 0 { 'M' } else { 'L' };
        let _ = write!(d, "{cmd}{} {} ", num(*x), num(*upper));
    }
    for (x, lower, _, _) in points.iter().rev() {
        let _ = write!(d, "L{} {} ", num(*x), num(*lower));
    }
    d.push('Z');

    let first_row = points[0].3;
    let fill_color = fill
        .resolve(table, first_row)
        .unwrap_or_else(|| DEFAULT_FILL.to_string());
    sink.path(
        d.trim_end(),
        &Paint {
            fill: Fill::Color(fill_color),
            stroke: stroke_style(stroke, stroke_width, table, first_row),
            opacity: Some(alpha),
        },
    );
}

fn area_totals_by_x(
    space: &crate::space::ScaledSpace,
    table: &dyn algraf_data::Table,
    rows: &[usize],
    value_col: &str,
) -> HashMap<String, (f64, f64)> {
    let mut totals = HashMap::new();
    for &row in rows {
        let Some(x) = space.resolve_x(table, row) else {
            continue;
        };
        let Some(value) = cell_f64(table, value_col, row) else {
            continue;
        };
        let entry = totals.entry(area_x_key(x)).or_insert((0.0, 0.0));
        if value >= 0.0 {
            entry.0 += value;
        } else {
            entry.1 += value;
        }
    }
    totals
}

fn area_group_values_by_x(
    space: &crate::space::ScaledSpace,
    table: &dyn algraf_data::Table,
    rows: &[usize],
    value_col: &str,
) -> HashMap<String, f64> {
    let mut values = HashMap::new();
    for &row in rows {
        let Some(x) = space.resolve_x(table, row) else {
            continue;
        };
        let Some(value) = cell_f64(table, value_col, row) else {
            continue;
        };
        *values.entry(area_x_key(x)).or_insert(0.0) += value;
    }
    values
}

fn area_x_positions(
    space: &crate::space::ScaledSpace,
    table: &dyn algraf_data::Table,
    rows: &[usize],
    value_col: &str,
) -> Vec<(String, f64)> {
    let mut positions = Vec::new();
    for &row in rows {
        let Some(x) = space.resolve_x(table, row) else {
            continue;
        };
        if cell_f64(table, value_col, row).is_some() {
            positions.push((area_x_key(x), x));
        }
    }
    positions.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    positions.dedup_by(|a, b| a.0 == b.0);
    positions
}

fn area_x_key(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

#[cfg(test)]
mod taper_tests {
    use super::{tapered_ribbon_path, vertex_offsets};

    #[test]
    fn straight_ribbon_offsets_are_perpendicular_unit_normals() {
        // A horizontal line: every vertex normal points straight up (0, ±1).
        let pts = [(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)];
        for (ux, uy) in vertex_offsets(&pts) {
            assert!((ux).abs() < 1e-9);
            assert!((uy.abs() - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn ribbon_path_is_closed_and_widths_track_half() {
        // A horizontal line at y=0 with half-widths 1,2,1 produces a polygon
        // reaching y=±half at each vertex.
        let pts = [(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)];
        let d = tapered_ribbon_path(&pts, &[1.0, 2.0, 1.0]);
        assert!(d.ends_with('Z'));
        // The forward edge places the middle vertex at y = -2 or +2 (SVG y is
        // unsigned here; just assert a |2| coordinate appears).
        assert!(d.contains(" 2 ") || d.contains(" -2 "));
    }

    #[test]
    fn miter_is_capped_on_sharp_turns() {
        // A near-reversal must not produce an enormous offset.
        let pts = [(0.0, 0.0), (1.0, 0.0), (0.0, 0.01)];
        for (ux, uy) in vertex_offsets(&pts) {
            assert!((ux * ux + uy * uy).sqrt() <= 4.0 + 1e-9);
        }
    }
}
