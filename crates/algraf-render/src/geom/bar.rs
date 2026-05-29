use std::collections::HashMap;

use algraf_core::{codes, Diagnostic};
use algraf_data::Table;
use algraf_semantics::{GeometryIr, PropertyKey};

use crate::aes::{color_spec, number_setting, ColorSpec};
use crate::helpers::{bar_layout, BarLayout};
use crate::layout::Rect;
use crate::scale::{categorical_domain, cell_category, cell_f64};
use crate::sink::{Fill, MarkSink, Paint};
use crate::space::{Polar, ScaledSpace};

use super::common::{mark_interaction, render_rows, stroke_style, DEFAULT_FILL};
use super::polar::annular_segment_path;
use super::GeometryRenderContext;

pub(super) fn render(
    sink: &mut dyn MarkSink,
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
            sink,
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
                sink,
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
                geo,
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
                sink,
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
                geo,
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
    sink: &mut dyn MarkSink,
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
    // A categorical `radius:` mapping selects concentric rings: the radial bar
    // chart, distinct from the coxcomb and pie paths (spec §16.16).
    if let Some(radius_col) = geo
        .mappings
        .iter()
        .find(|m| m.aesthetic == PropertyKey::Radius)
        .map(|m| m.column.name.clone())
    {
        render_polar_radial_bar(
            sink,
            geo,
            space,
            table,
            rows,
            polar,
            &radius_col,
            fill,
            stroke,
            stroke_width,
            alpha,
            diagnostics,
        );
        return;
    }
    if space.polar_theta_is_band() {
        render_polar_coxcomb(
            sink,
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
            sink,
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
    sink: &mut dyn MarkSink,
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
        emit_polar_path(sink, &d, fill, stroke, stroke_width, table, row, alpha, geo);
    }
}

/// Pie / donut: value → angular wedge proportional to the total, full radius.
#[allow(clippy::too_many_arguments)]
fn render_polar_pie(
    sink: &mut dyn MarkSink,
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
    let span = polar.theta_end - polar.theta_start;
    let mut acc = 0.0;
    for row in row_list {
        let Some(value) = cell_f64(table, &value_col, row) else {
            continue;
        };
        if value <= 0.0 {
            continue;
        }
        let a0 = polar.theta_start + (acc / total) * span;
        acc += value;
        let a1 = polar.theta_start + (acc / total) * span;
        // A banded radius axis (theta:"y" with a categorical radius) yields
        // concentric ring segments; otherwise the wedge spans the full radius.
        let (r0, r1) = space
            .polar_radius_band(table, row)
            .map(|(start, width)| (start, start + width))
            .unwrap_or((polar.r_inner, polar.r_outer));
        let d = annular_segment_path(polar, a0, a1, r0, r1);
        emit_polar_path(sink, &d, fill, stroke, stroke_width, table, row, alpha, geo);
    }
}

/// Radial bar chart (spec §16.16): a categorical `radius:` mapping puts each
/// category on its own concentric ring, and the theta axis (the frame value)
/// drives each bar's independent angular sweep from the start angle. Distinct
/// from the cumulative pie path (angles accumulate around the circle) and the
/// coxcomb path (a categorical angle with a value radius).
#[allow(clippy::too_many_arguments)]
fn render_polar_radial_bar(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    polar: &Polar,
    radius_col: &str,
    fill: &ColorSpec,
    stroke: &ColorSpec,
    stroke_width: f64,
    alpha: f64,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let categories = categorical_domain(table, radius_col);
    if categories.is_empty() {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "polar radial Bar requires a categorical `radius:` column with values",
            geo.span,
        ));
        return;
    }
    // Divide the drawable annulus into one ring per category, leaving a small
    // gap between rings so adjacent bars read as distinct tracks.
    let n = categories.len() as f64;
    let band = (polar.r_outer - polar.r_inner) / n;
    let gap = (band * 0.2).min(6.0);
    for row in render_rows(table, rows) {
        let Some(category) = cell_category(table, radius_col, row) else {
            continue;
        };
        let Some(index) = categories.iter().position(|c| *c == category) else {
            continue;
        };
        // Innermost category is the outermost ring so categories read outside-in
        // with the longest available track on the outside.
        let slot = (categories.len() - 1 - index) as f64;
        let r0 = polar.r_inner + slot * band;
        let r1 = r0 + (band - gap).max(1.0);
        let Some(a1) = space.polar_angle(table, row) else {
            continue;
        };
        let d = annular_segment_path(polar, polar.theta_start, a1, r0, r1);
        emit_polar_path(sink, &d, fill, stroke, stroke_width, table, row, alpha, geo);
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_polar_path(
    sink: &mut dyn MarkSink,
    d: &str,
    fill: &ColorSpec,
    stroke: &ColorSpec,
    stroke_width: f64,
    table: &dyn Table,
    row: usize,
    alpha: f64,
    geo: &GeometryIr,
) {
    let color = fill
        .resolve(table, row)
        .unwrap_or_else(|| DEFAULT_FILL.to_string());
    sink.begin_mark(mark_interaction(geo, table, row));
    sink.path(
        d,
        &Paint {
            fill: Fill::Color(color),
            stroke: stroke_style(stroke, stroke_width, table, row),
            opacity: Some(alpha),
        },
    );
    sink.end_mark();
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
    sink: &mut dyn MarkSink,
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
    geo: &GeometryIr,
) {
    let top = y_a.min(y_b).clamp(plot.y, plot.bottom());
    let bottom = y_a.max(y_b).clamp(plot.y, plot.bottom());
    let color = fill
        .resolve(table, row)
        .unwrap_or_else(|| DEFAULT_FILL.to_string());
    sink.begin_mark(mark_interaction(geo, table, row));
    sink.rect(
        x,
        top,
        width,
        bottom - top,
        &Paint {
            fill: Fill::Color(color),
            stroke: stroke_style(stroke, stroke_width, table, row),
            opacity: Some(alpha),
        },
    );
    sink.end_mark();
}
