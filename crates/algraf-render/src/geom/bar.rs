use std::collections::HashMap;

use algraf_core::{codes, Diagnostic};
use algraf_data::Table;
use algraf_semantics::{GeometryIr, PropertyKey};

use crate::aes::{color_spec, number_setting, ColorSpec};
use crate::helpers::{bar_layout, BarLayout};
use crate::layout::Rect;
use crate::scale::{cell_category, cell_f64};
use crate::space::{Polar, ScaledSpace, THETA_END, THETA_START};
use crate::svg::{escape_attr, num, SvgWriter};

use super::common::{render_rows, stroke_attrs, DEFAULT_FILL};
use super::polar::annular_segment_path;
use super::GeometryRenderContext;

pub(super) fn render(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let plot = ctx.plot;
    let scales = ctx.scales;
    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, 1.0);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let layout = bar_layout(geo);
    let stacked = matches!(layout, BarLayout::Stack | BarLayout::Fill);

    // Polar bars draw wedges/annular segments instead of rectangles (spec §16.16).
    if let Some(polar) = space.polar() {
        render_polar(
            w,
            geo,
            space,
            table,
            rows,
            polar,
            &fill,
            &stroke,
            stroke_width,
            alpha,
            layout,
            diagnostics,
        );
        return;
    }

    let Some(y_col) = space.y.as_ref().and_then(|a| a.data_column()) else {
        return;
    };
    if !space.x.is_band() {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "Bar requires a categorical x dimension",
            geo.span,
        ));
        return;
    }

    let Some(baseline) = space.map_y(0.0) else {
        return;
    };

    if stacked {
        let Some(x_col) = space.x.data_column() else {
            return;
        };
        let totals = if layout == BarLayout::Fill {
            fill_totals(table, rows, x_col, y_col)
        } else {
            HashMap::new()
        };
        let mut cumulative: HashMap<String, f64> = HashMap::new();
        for row in render_rows(table, rows) {
            let (Some(cx), Some(bw)) = (space.resolve_x(table, row), space.x_bandwidth(table, row))
            else {
                continue;
            };
            let Some(value) = cell_f64(table, y_col, row) else {
                continue;
            };
            let key = crate::scale::cell_category(table, x_col, row).unwrap_or_default();
            let value = if layout == BarLayout::Fill {
                let total = totals.get(&key).copied().unwrap_or(0.0);
                if total.abs() <= f64::EPSILON {
                    continue;
                }
                value / total
            } else {
                value
            };
            let base = *cumulative.get(&key).unwrap_or(&0.0);
            let top = base + value;
            cumulative.insert(key, top);
            let (Some(y0), Some(y1)) = (space.map_y(base), space.map_y(top)) else {
                continue;
            };
            emit_bar(
                w,
                cx - bw / 2.0,
                bw,
                y0,
                y1,
                plot,
                &fill,
                &stroke,
                stroke_width,
                table,
                row,
                alpha,
            );
        }
    } else {
        for row in render_rows(table, rows) {
            let (Some(cx), Some(bw)) = (space.resolve_x(table, row), space.x_bandwidth(table, row))
            else {
                continue;
            };
            let Some(top) = space.resolve_y(table, row) else {
                continue;
            };
            emit_bar(
                w,
                cx - bw / 2.0,
                bw,
                baseline,
                top,
                plot,
                &fill,
                &stroke,
                stroke_width,
                table,
                row,
                alpha,
            );
        }
    }
}

/// Render polar bars (spec §16.16). Two forms, distinguished by whether the
/// angular axis is categorical:
///
/// - Coxcomb / wind rose (`theta: "x"`, categorical angle): each category is an
///   angular wedge whose radius encodes the value; stacking grows the radius.
/// - Pie / donut (`theta: "y"` or a 1D frame, continuous angle): each value gets
///   an angular wedge proportional to its share of the total, spanning the full
///   radius (`innerRadius` cuts a donut hole).
#[allow(clippy::too_many_arguments)]
fn render_polar(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    polar: &Polar,
    fill: &ColorSpec,
    stroke: &ColorSpec,
    stroke_width: f64,
    alpha: f64,
    layout: BarLayout,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if space.polar_theta_is_band() {
        render_polar_coxcomb(
            w,
            space,
            table,
            rows,
            polar,
            fill,
            stroke,
            stroke_width,
            alpha,
            layout,
            geo,
            diagnostics,
        );
    } else {
        render_polar_pie(
            w,
            space,
            table,
            rows,
            polar,
            fill,
            stroke,
            stroke_width,
            alpha,
            geo,
            diagnostics,
        );
    }
}

/// Coxcomb / wind rose: category → angular wedge, value → radius.
#[allow(clippy::too_many_arguments)]
fn render_polar_coxcomb(
    w: &mut SvgWriter,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    polar: &Polar,
    fill: &ColorSpec,
    stroke: &ColorSpec,
    stroke_width: f64,
    alpha: f64,
    layout: BarLayout,
    geo: &GeometryIr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(value_col) = space.polar_radius_column().map(str::to_string) else {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "polar Bar requires a value (radius) dimension",
            geo.span,
        ));
        return;
    };
    let Some(theta_col) = space.polar_theta_column().map(str::to_string) else {
        return;
    };
    let stacked = matches!(layout, BarLayout::Stack | BarLayout::Fill);
    let totals = if layout == BarLayout::Fill {
        fill_totals(table, rows, &theta_col, &value_col)
    } else {
        HashMap::new()
    };
    let mut cumulative: HashMap<String, f64> = HashMap::new();
    for row in render_rows(table, rows) {
        let Some((center, bw)) = space.polar_angle_band(table, row) else {
            continue;
        };
        let Some(value) = cell_f64(table, &value_col, row) else {
            continue;
        };
        let key = cell_category(table, &theta_col, row).unwrap_or_default();
        let value = if layout == BarLayout::Fill {
            let total = totals.get(&key).copied().unwrap_or(0.0);
            if total.abs() <= f64::EPSILON {
                continue;
            }
            value / total
        } else {
            value
        };
        let base = if stacked {
            *cumulative.get(&key).unwrap_or(&0.0)
        } else {
            0.0
        };
        let top = base + value;
        if stacked {
            cumulative.insert(key, top);
        }
        let (Some(r0), Some(r1)) = (
            space.polar_radius_value(base),
            space.polar_radius_value(top),
        ) else {
            continue;
        };
        let d = annular_segment_path(polar, center - bw / 2.0, center + bw / 2.0, r0, r1);
        emit_polar_path(w, &d, fill, stroke, stroke_width, table, row, alpha);
    }
}

/// Pie / donut: value → angular wedge proportional to the total, full radius.
#[allow(clippy::too_many_arguments)]
fn render_polar_pie(
    w: &mut SvgWriter,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    polar: &Polar,
    fill: &ColorSpec,
    stroke: &ColorSpec,
    stroke_width: f64,
    alpha: f64,
    geo: &GeometryIr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(value_col) = space.polar_theta_column().map(str::to_string) else {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "polar Bar requires a value dimension",
            geo.span,
        ));
        return;
    };
    let row_list = render_rows(table, rows);
    let total: f64 = row_list
        .iter()
        .filter_map(|&row| cell_f64(table, &value_col, row))
        .filter(|v| *v > 0.0)
        .sum();
    if total <= f64::EPSILON {
        return;
    }
    let span = THETA_END - THETA_START;
    let mut acc = 0.0;
    for row in row_list {
        let Some(value) = cell_f64(table, &value_col, row) else {
            continue;
        };
        if value <= 0.0 {
            continue;
        }
        let a0 = THETA_START + (acc / total) * span;
        acc += value;
        let a1 = THETA_START + (acc / total) * span;
        // A banded radius axis (theta:"y" with a categorical radius) yields
        // concentric ring segments; otherwise the wedge spans the full radius.
        let (r0, r1) = space
            .polar_radius_band(table, row)
            .map(|(start, width)| (start, start + width))
            .unwrap_or((polar.r_inner, polar.r_outer));
        let d = annular_segment_path(polar, a0, a1, r0, r1);
        emit_polar_path(w, &d, fill, stroke, stroke_width, table, row, alpha);
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_polar_path(
    w: &mut SvgWriter,
    d: &str,
    fill: &ColorSpec,
    stroke: &ColorSpec,
    stroke_width: f64,
    table: &dyn Table,
    row: usize,
    alpha: f64,
) {
    let color = fill
        .resolve(table, row)
        .unwrap_or_else(|| DEFAULT_FILL.to_string());
    w.line(&format!(
        "<path d=\"{}\" fill=\"{}\"{} opacity=\"{}\" />",
        d,
        escape_attr(&color),
        stroke_attrs(stroke, stroke_width, table, row),
        num(alpha),
    ));
}

fn fill_totals(
    table: &dyn Table,
    rows: Option<&[usize]>,
    x_col: &str,
    y_col: &str,
) -> HashMap<String, f64> {
    let mut totals: HashMap<String, f64> = HashMap::new();
    for row in render_rows(table, rows) {
        let Some(value) = cell_f64(table, y_col, row) else {
            continue;
        };
        let key = cell_category(table, x_col, row).unwrap_or_default();
        *totals.entry(key).or_insert(0.0) += value;
    }
    totals
}

#[allow(clippy::too_many_arguments)]
fn emit_bar(
    w: &mut SvgWriter,
    x: f64,
    width: f64,
    y_a: f64,
    y_b: f64,
    plot: Rect,
    fill: &ColorSpec,
    stroke: &ColorSpec,
    stroke_width: f64,
    table: &dyn Table,
    row: usize,
    alpha: f64,
) {
    let top = y_a.min(y_b).clamp(plot.y, plot.bottom());
    let bottom = y_a.max(y_b).clamp(plot.y, plot.bottom());
    let color = fill
        .resolve(table, row)
        .unwrap_or_else(|| DEFAULT_FILL.to_string());
    w.line(&format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{} opacity=\"{}\" />",
        num(x),
        num(top),
        num(width),
        num(bottom - top),
        escape_attr(&color),
        stroke_attrs(stroke, stroke_width, table, row),
        num(alpha),
    ));
}
