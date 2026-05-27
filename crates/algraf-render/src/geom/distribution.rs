use std::collections::HashMap;
use std::fmt::Write;

use algraf_core::{codes, Diagnostic};
use algraf_semantics::{GeometryIr, PropertyKey};

use crate::aes::{color_spec, number_setting, ColorSpec};
use crate::helpers::{bool_setting, number_array_setting, number_setting_opt};
use crate::scale::cell_f64;
use crate::stats;
use crate::svg::{escape_attr, num, SvgWriter};

use super::common::{
    axis_is_continuousish, emit_svg_line, quantile_type7, render_rows, stroke_attrs, x_group_key,
    DEFAULT_FILL, DEFAULT_STROKE,
};
use super::GeometryRenderContext;

pub(super) fn render_boxplot(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let scales = ctx.scales;
    let Some(y_col) = space.y.as_ref().and_then(|axis| axis.data_column()) else {
        return;
    };
    if !space.x.is_band() || !space.y.as_ref().is_some_and(axis_is_continuousish) {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "Boxplot requires categorical x and continuous y dimensions",
            geo.span,
        ));
        return;
    }

    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, 1.0);
    // Points beyond the 1.5·IQR whiskers render as small circles by default
    // (spec §14.11); `outliers: false` suppresses them.
    let show_outliers = bool_setting(geo, PropertyKey::Outliers, true);
    let mut groups: HashMap<String, Vec<(usize, f64)>> = HashMap::new();
    let mut order = Vec::new();

    for row in render_rows(table, rows) {
        let Some(key) = x_group_key(space, table, row) else {
            continue;
        };
        let Some(value) = cell_f64(table, y_col, row) else {
            continue;
        };
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push((row, value));
    }

    for key in order {
        let Some(group) = groups.get_mut(&key) else {
            continue;
        };
        group.sort_by(|a, b| a.1.total_cmp(&b.1));
        let first_row = group[0].0;
        let values: Vec<f64> = group.iter().map(|(_, value)| *value).collect();
        let q1 = quantile_type7(&values, 0.25);
        let median = quantile_type7(&values, 0.5);
        let q3 = quantile_type7(&values, 0.75);
        let iqr = q3 - q1;
        let lower_bound = q1 - 1.5 * iqr;
        let upper_bound = q3 + 1.5 * iqr;
        let whisker_low = values
            .iter()
            .copied()
            .find(|value| *value >= lower_bound)
            .unwrap_or(values[0]);
        let whisker_high = values
            .iter()
            .copied()
            .rev()
            .find(|value| *value <= upper_bound)
            .unwrap_or(*values.last().unwrap());

        let (Some(cx), Some(bandwidth)) = (
            space.resolve_x(table, first_row),
            space.x_bandwidth(table, first_row),
        ) else {
            continue;
        };
        let width_setting = number_setting(geo, PropertyKey::Width, bandwidth * 0.7);
        let box_width = width_setting.clamp(1.0, bandwidth);
        let half = box_width / 2.0;
        let (Some(y_q1), Some(y_median), Some(y_q3), Some(y_low), Some(y_high)) = (
            space.map_y(q1),
            space.map_y(median),
            space.map_y(q3),
            space.map_y(whisker_low),
            space.map_y(whisker_high),
        ) else {
            continue;
        };

        let fill_color = fill
            .resolve(table, first_row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        let stroke_color = stroke
            .resolve(table, first_row)
            .unwrap_or_else(|| DEFAULT_STROKE.to_string());
        let top = y_q3.min(y_q1);
        let height = (y_q1 - y_q3).abs().max(1.0);

        emit_svg_line(w, cx, y_low, cx, y_high, &stroke_color, stroke_width, alpha);
        emit_svg_line(
            w,
            cx - half * 0.4,
            y_low,
            cx + half * 0.4,
            y_low,
            &stroke_color,
            stroke_width,
            alpha,
        );
        emit_svg_line(
            w,
            cx - half * 0.4,
            y_high,
            cx + half * 0.4,
            y_high,
            &stroke_color,
            stroke_width,
            alpha,
        );
        w.line(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"{}\" />",
            num(cx - half),
            num(top),
            num(box_width),
            num(height),
            escape_attr(&fill_color),
            escape_attr(&stroke_color),
            num(stroke_width),
            num(alpha),
        ));
        emit_svg_line(
            w,
            cx - half,
            y_median,
            cx + half,
            y_median,
            &stroke_color,
            stroke_width,
            alpha,
        );

        // Outliers: observations beyond the 1.5·IQR fences, drawn as small open
        // circles centered on the box (spec §14.11). Order follows the sorted
        // group, so output stays deterministic.
        if show_outliers {
            let radius = (stroke_width * 1.5).max(2.0);
            for (_, value) in group.iter() {
                if *value < lower_bound || *value > upper_bound {
                    if let Some(cy) = space.map_y(*value) {
                        w.line(&format!(
                            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"{}\" />",
                            num(cx),
                            num(cy),
                            num(radius),
                            escape_attr(&stroke_color),
                            num(stroke_width),
                            num(alpha),
                        ));
                    }
                }
            }
        }
    }
}

pub(super) fn render_violin(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let scales = ctx.scales;
    let Some(y_col) = space.y.as_ref().and_then(|axis| axis.data_column()) else {
        return;
    };
    if !space.x.is_band() || !space.y.as_ref().is_some_and(axis_is_continuousish) {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "Violin requires categorical x and continuous y dimensions",
            geo.span,
        ));
        return;
    }

    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let alpha = number_setting(geo, PropertyKey::Alpha, 0.55);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, 1.0);
    let quantiles = number_array_setting(geo, PropertyKey::Quantiles).unwrap_or_default();
    let mut groups: HashMap<String, Vec<(usize, f64)>> = HashMap::new();
    let mut order = Vec::new();
    for row in render_rows(table, rows) {
        let Some(key) = x_group_key(space, table, row) else {
            continue;
        };
        let Some(value) = cell_f64(table, y_col, row) else {
            continue;
        };
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push((row, value));
    }

    let options = stats::DensityOptions {
        bandwidth: number_setting_opt(geo, PropertyKey::Bandwidth).filter(|value| *value > 0.0),
        grid_points: number_setting_opt(geo, PropertyKey::N)
            .filter(|value| *value >= 2.0)
            .map(|value| value.round() as usize)
            .unwrap_or(256),
    };

    for key in order {
        let Some(group) = groups.get_mut(&key) else {
            continue;
        };
        group.sort_by(|a, b| a.1.total_cmp(&b.1));
        let first_row = group[0].0;
        let mut values: Vec<f64> = group.iter().map(|(_, value)| *value).collect();
        let curve = stats::density_values(&mut values, options);
        if curve.len() < 2 {
            continue;
        }
        let max_density = curve
            .iter()
            .map(|point| point.density)
            .fold(0.0_f64, f64::max);
        if max_density <= f64::EPSILON {
            continue;
        }
        let (Some(cx), Some(bandwidth)) = (
            space.resolve_x(table, first_row),
            space.x_bandwidth(table, first_row),
        ) else {
            continue;
        };
        let half_width =
            number_setting(geo, PropertyKey::Width, bandwidth * 0.9).clamp(1.0, bandwidth) / 2.0;
        let mut right = Vec::new();
        let mut left = Vec::new();
        for point in &curve {
            let Some(y) = space.map_y(point.x) else {
                continue;
            };
            let dx = point.density / max_density * half_width;
            right.push((cx + dx, y));
            left.push((cx - dx, y));
        }
        if right.len() < 2 {
            continue;
        }
        let mut d = String::new();
        for (i, (x, y)) in right.iter().enumerate() {
            let cmd = if i == 0 { 'M' } else { 'L' };
            let _ = write!(d, "{cmd}{} {} ", num(*x), num(*y));
        }
        for (x, y) in left.iter().rev() {
            let _ = write!(d, "L{} {} ", num(*x), num(*y));
        }
        d.push('Z');
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

        let stroke_color = stroke
            .resolve(table, first_row)
            .unwrap_or_else(|| DEFAULT_STROKE.to_string());
        for q in quantiles
            .iter()
            .copied()
            .filter(|q| (0.0..=1.0).contains(q))
        {
            let value = quantile_type7(&values, q);
            let Some(y) = space.map_y(value) else {
                continue;
            };
            let density = interpolate_density(&curve, value);
            let dx = density / max_density * half_width;
            emit_svg_line(w, cx - dx, y, cx + dx, y, &stroke_color, stroke_width, 1.0);
        }
    }
}

fn interpolate_density(curve: &[stats::DensityPoint], x: f64) -> f64 {
    if curve.is_empty() {
        return 0.0;
    }
    if x <= curve[0].x {
        return curve[0].density;
    }
    for window in curve.windows(2) {
        let a = window[0];
        let b = window[1];
        if x <= b.x {
            let t = if (b.x - a.x).abs() <= f64::EPSILON {
                0.0
            } else {
                (x - a.x) / (b.x - a.x)
            };
            return a.density + (b.density - a.density) * t;
        }
    }
    curve.last().map(|point| point.density).unwrap_or(0.0)
}

pub(super) fn render_hexbin(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let theme = ctx.theme;
    let scales = ctx.scales;
    let (Some(x_col), Some(y_col)) = (
        space.x.data_column(),
        space.y.as_ref().and_then(|axis| axis.data_column()),
    ) else {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "HexBin requires continuous x and y dimensions",
            geo.span,
        ));
        return;
    };
    if !axis_is_continuousish(&space.x) || !space.y.as_ref().is_some_and(axis_is_continuousish) {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "HexBin requires continuous x and y dimensions",
            geo.span,
        ));
        return;
    }
    let bins = number_setting(geo, PropertyKey::Bins, 30.0)
        .round()
        .max(1.0) as usize;
    let cells = stats::hexbin(table, x_col, y_col, stats::Bin2DOptions { bins });
    // Normalize fill over the count domain `[min, max]`, matching the
    // continuous legend synthesized in `collect_legends` so swatch colors and
    // hexagon colors agree.
    let min_count = cells.iter().map(|cell| cell.count).min().unwrap_or(0) as f64;
    let max_count = cells.iter().map(|cell| cell.count).max().unwrap_or(1) as f64;
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, theme.line_width);
    let alpha = number_setting(geo, PropertyKey::Alpha, 0.9);
    for cell in cells {
        let Some(cx) = space.map_x(cell.x) else {
            continue;
        };
        let Some(cy) = space.map_y(cell.y) else {
            continue;
        };
        let rx = (space.map_x(cell.x + cell.radius).unwrap_or(cx) - cx)
            .abs()
            .max(1.0);
        let ry = (space.map_y(cell.y + cell.y_radius).unwrap_or(cy) - cy)
            .abs()
            .max(1.0);
        let color = match &fill {
            ColorSpec::Constant(color) => color.clone(),
            _ => {
                let t = if (max_count - min_count).abs() < f64::EPSILON {
                    0.5
                } else {
                    (cell.count as f64 - min_count) / (max_count - min_count)
                };
                crate::theme::gradient_color(t)
            }
        };
        let mut points = String::new();
        for i in 0..6 {
            // Pointy-top orientation (vertices at top/bottom): matches the
            // row-offset tessellation produced by `stats::hexbin`, where odd
            // rows are shifted horizontally by half a column.
            let angle = std::f64::consts::TAU * (i as f64) / 6.0 + std::f64::consts::FRAC_PI_2;
            if i > 0 {
                points.push(' ');
            }
            let _ = write!(
                points,
                "{},{}",
                num(cx + rx * angle.cos()),
                num(cy + ry * angle.sin())
            );
        }
        w.line(&format!(
            "<polygon points=\"{}\" fill=\"{}\"{} opacity=\"{}\" />",
            points,
            escape_attr(&color),
            stroke_attrs(&stroke, stroke_width, table, 0),
            num(alpha),
        ));
    }
}
