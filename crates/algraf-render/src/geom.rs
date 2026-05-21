//! Geometry rendering (spec §14, §18.6). Supported in version 0.1: Point,
//! Line, Bar (dodge and stack), Rect, and Tile. Other geometries emit a render
//! diagnostic and are skipped.

use std::collections::HashMap;
use std::fmt::Write;

use algraf_core::Diagnostic;
use algraf_data::Table;
use algraf_semantics::{GeometryIr, GeometryKind, ScaleIr, SettingValue};

use crate::aes::{color_spec, number_setting, ColorSpec};
use crate::layout::Rect;
use crate::scale::{cell_category, cell_f64, cell_micros};
use crate::space::{AxisScale, ScaledSpace};
use crate::svg::{escape_attr, escape_text, num, SvgWriter};
use crate::theme::Theme;

const DEFAULT_FILL: &str = "#4E79A7";
const DEFAULT_STROKE: &str = "#333333";

#[derive(Clone, Copy)]
pub(crate) struct GeometryRenderContext<'a> {
    pub(crate) space: &'a ScaledSpace,
    pub(crate) table: &'a dyn Table,
    pub(crate) rows: Option<&'a [usize]>,
    pub(crate) plot: Rect,
    pub(crate) theme: &'a Theme,
    pub(crate) scales: &'a [ScaleIr],
}

/// Render one geometry layer into the writer.
pub(crate) fn render(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let class = format!("algraf-layer algraf-geom-{}", geo_class(geo.kind));
    w.open_group(&format!("class=\"{class}\""));
    let before = w.byte_len();
    match geo.kind {
        GeometryKind::Point => point(
            w, geo, ctx.space, ctx.table, ctx.rows, ctx.theme, ctx.scales,
        ),
        GeometryKind::Line => line(
            w, geo, ctx.space, ctx.table, ctx.rows, ctx.theme, ctx.scales,
        ),
        GeometryKind::Bar => bar(
            w,
            geo,
            ctx.space,
            ctx.table,
            ctx.rows,
            ctx.plot,
            ctx.scales,
            diagnostics,
        ),
        GeometryKind::Rect => rect(w, geo, ctx.space, ctx.table, ctx.rows, ctx.scales),
        GeometryKind::Tile => tile(w, geo, ctx.space, ctx.table, ctx.rows, ctx.scales),
        GeometryKind::Smooth => smooth(
            w,
            geo,
            ctx.space,
            ctx.table,
            ctx.rows,
            ctx.theme,
            ctx.scales,
            diagnostics,
        ),
        GeometryKind::Boxplot => boxplot(
            w,
            geo,
            ctx.space,
            ctx.table,
            ctx.rows,
            ctx.scales,
            diagnostics,
        ),
        GeometryKind::Ribbon => ribbon(w, geo, ctx.space, ctx.table, ctx.rows, ctx.scales),
        GeometryKind::HLine => hline(
            w, geo, ctx.space, ctx.plot, ctx.table, ctx.theme, ctx.scales,
        ),
        GeometryKind::VLine => vline(
            w, geo, ctx.space, ctx.plot, ctx.table, ctx.theme, ctx.scales,
        ),
        GeometryKind::Rug => rug(
            w, geo, ctx.space, ctx.table, ctx.rows, ctx.plot, ctx.theme, ctx.scales,
        ),
        GeometryKind::Area => area(w, geo, ctx.space, ctx.table, ctx.rows, ctx.scales),
        GeometryKind::Text => text_geom(
            w, geo, ctx.space, ctx.table, ctx.rows, ctx.theme, ctx.scales,
        ),
        GeometryKind::Segment => segment(w, geo, ctx.space, ctx.table, ctx.theme, ctx.scales),
        other => diagnostics.push(Diagnostic::warning(
            "R0001",
            format!("geometry `{other:?}` is not yet supported by the renderer"),
            geo.span,
        )),
    }
    // W2002: geometry produced no marks (spec §26.3).
    if w.byte_len() == before {
        diagnostics.push(Diagnostic::warning(
            "W2002",
            "geometry produced no marks",
            geo.span,
        ));
    }
    w.close_group();
}

fn geo_class(kind: GeometryKind) -> &'static str {
    match kind {
        GeometryKind::Point => "point",
        GeometryKind::Line => "line",
        GeometryKind::Bar => "bar",
        GeometryKind::Rect => "rect",
        GeometryKind::Tile => "tile",
        GeometryKind::Smooth => "smooth",
        GeometryKind::Boxplot => "boxplot",
        GeometryKind::Ribbon => "ribbon",
        GeometryKind::HLine => "hline",
        GeometryKind::VLine => "vline",
        GeometryKind::Rug => "rug",
        GeometryKind::Area => "area",
        GeometryKind::Text => "text",
        GeometryKind::Segment => "segment",
        _ => "other",
    }
}

fn point(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    theme: &Theme,
    scales: &[ScaleIr],
) {
    let fill = color_spec(geo, "fill", table, scales);
    let alpha = number_setting(geo, "alpha", 1.0);
    let size = number_setting(geo, "size", theme.point_size);
    for row in render_rows(table, rows) {
        let (Some(cx), Some(cy)) = (space.resolve_x(table, row), space.resolve_y(table, row))
        else {
            continue;
        };
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        w.line(&format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" opacity=\"{}\" />",
            num(cx),
            num(cy),
            num(size),
            escape_attr(&color),
            num(alpha),
        ));
    }
}

fn line(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    theme: &Theme,
    scales: &[ScaleIr],
) {
    let stroke = color_spec(geo, "stroke", table, scales);
    let width = number_setting(geo, "strokeWidth", theme.line_width);
    let alpha = number_setting(geo, "alpha", 1.0);
    let row_list = render_rows(table, rows);

    // Group rows into series by the stroke category, preserving domain order.
    let groups: Vec<(String, Vec<usize>)> = match &stroke {
        ColorSpec::Categorical { categories, .. } => categories
            .iter()
            .map(|cat| {
                let rows = row_list
                    .iter()
                    .copied()
                    .filter(|&r| {
                        stroke.resolve(table, r).is_some()
                            && row_category(&stroke, table, r).as_deref() == Some(cat)
                    })
                    .collect();
                (cat.clone(), rows)
            })
            .collect(),
        _ => vec![(String::new(), row_list)],
    };

    for (cat, rows) in groups {
        let mut points: Vec<(f64, f64)> = rows
            .iter()
            .filter_map(|&r| Some((space.resolve_x(table, r)?, space.resolve_y(table, r)?)))
            .collect();
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        if points.is_empty() {
            continue;
        }
        let color = if cat.is_empty() {
            constant_or(&stroke, DEFAULT_STROKE)
        } else {
            stroke
                .resolve(table, *rows.first().unwrap())
                .unwrap_or_else(|| DEFAULT_STROKE.to_string())
        };
        let mut d = String::new();
        for (i, (x, y)) in points.iter().enumerate() {
            let cmd = if i == 0 { 'M' } else { 'L' };
            let _ = write!(d, "{cmd}{} {} ", num(*x), num(*y));
        }
        w.line(&format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"{}\" />",
            d.trim_end(),
            escape_attr(&color),
            num(width),
            num(alpha),
        ));
    }
}

fn row_category(spec: &ColorSpec, table: &dyn Table, row: usize) -> Option<String> {
    match spec {
        ColorSpec::Categorical { col, .. } => crate::scale::cell_category(table, col, row),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn smooth(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    theme: &Theme,
    scales: &[ScaleIr],
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !axis_is_continuousish(&space.x) || !space.y.as_ref().is_some_and(axis_is_continuousish) {
        diagnostics.push(Diagnostic::warning(
            "R0002",
            "Smooth requires continuous x and y dimensions",
            geo.span,
        ));
        return;
    }

    let stroke = color_spec(geo, "stroke", table, scales);
    let width = number_setting(geo, "strokeWidth", theme.line_width);
    let alpha = number_setting(geo, "alpha", 1.0);
    let row_list = render_rows(table, rows);

    for group_rows in grouped_rows_by_color(&stroke, table, row_list) {
        let mut points: Vec<(f64, f64)> = group_rows
            .iter()
            .filter_map(|&r| Some((space.resolve_x(table, r)?, space.resolve_y(table, r)?)))
            .collect();
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        let Some((x0, y0, x1, y1)) = linear_fit_segment(&points) else {
            diagnostics.push(Diagnostic::warning(
                "R0002",
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

fn axis_is_continuousish(axis: &AxisScale) -> bool {
    matches!(
        axis,
        AxisScale::Continuous { .. } | AxisScale::Temporal { .. } | AxisScale::Union { .. }
    )
}

fn grouped_rows_by_color(spec: &ColorSpec, table: &dyn Table, rows: Vec<usize>) -> Vec<Vec<usize>> {
    match spec {
        ColorSpec::Categorical { categories, .. } => categories
            .iter()
            .map(|cat| {
                rows.iter()
                    .copied()
                    .filter(|&row| row_category(spec, table, row).as_deref() == Some(cat))
                    .collect::<Vec<_>>()
            })
            .filter(|group| !group.is_empty())
            .collect(),
        _ => vec![rows],
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

#[allow(clippy::too_many_arguments)]
fn bar(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    plot: Rect,
    scales: &[ScaleIr],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(y_col) = space.y.as_ref().and_then(|a| a.data_column()) else {
        return;
    };
    if !space.x.is_band() {
        diagnostics.push(Diagnostic::warning(
            "R0002",
            "Bar requires a categorical x dimension",
            geo.span,
        ));
        return;
    }
    let fill = color_spec(geo, "fill", table, scales);
    let stroke = color_spec(geo, "stroke", table, scales);
    let stroke_width = number_setting(geo, "strokeWidth", 1.0);
    let alpha = number_setting(geo, "alpha", 1.0);
    let layout = bar_layout(geo);
    let stacked = matches!(layout, BarLayout::Stack | BarLayout::Fill);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BarLayout {
    Identity,
    Stack,
    Fill,
}

fn bar_layout(geo: &GeometryIr) -> BarLayout {
    geo.settings
        .iter()
        .find(|s| s.name == "layout")
        .and_then(|s| match &s.value {
            SettingValue::String(v) if v == "stack" => Some(BarLayout::Stack),
            SettingValue::String(v) if v == "fill" => Some(BarLayout::Fill),
            _ => None,
        })
        .unwrap_or(BarLayout::Identity)
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

fn boxplot(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    scales: &[ScaleIr],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(y_col) = space.y.as_ref().and_then(|axis| axis.data_column()) else {
        return;
    };
    if !space.x.is_band() || !space.y.as_ref().is_some_and(axis_is_continuousish) {
        diagnostics.push(Diagnostic::warning(
            "R0002",
            "Boxplot requires categorical x and continuous y dimensions",
            geo.span,
        ));
        return;
    }

    let fill = color_spec(geo, "fill", table, scales);
    let stroke = color_spec(geo, "stroke", table, scales);
    let alpha = number_setting(geo, "alpha", 1.0);
    let stroke_width = number_setting(geo, "strokeWidth", 1.0);
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
        let width_setting = number_setting(geo, "width", bandwidth * 0.7);
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
    }
}

fn x_group_key(space: &ScaledSpace, table: &dyn Table, row: usize) -> Option<String> {
    match &space.x {
        AxisScale::Band { col, .. } => cell_category(table, col, row),
        AxisScale::NestedBand {
            outer_col,
            inner_col,
            ..
        } => Some(format!(
            "{}\u{1f}{}",
            cell_category(table, outer_col, row)?,
            cell_category(table, inner_col, row)?
        )),
        _ => None,
    }
}

fn quantile_type7(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    if values.len() == 1 {
        return values[0];
    }
    let pos = (values.len() - 1) as f64 * p;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        values[lo]
    } else {
        values[lo] + (values[hi] - values[lo]) * (pos - lo as f64)
    }
}

fn rect(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    scales: &[ScaleIr],
) {
    let fill = color_spec(geo, "fill", table, scales);
    let stroke = color_spec(geo, "stroke", table, scales);
    let stroke_width = number_setting(geo, "strokeWidth", 1.0);
    let alpha = number_setting(geo, "alpha", 1.0);
    for row in render_rows(table, rows) {
        let (Some(xmin), Some(xmax), Some(ymin), Some(ymax)) = (
            pos(geo, "xmin", table, row).and_then(|v| space.map_x(v)),
            pos(geo, "xmax", table, row).and_then(|v| space.map_x(v)),
            pos(geo, "ymin", table, row).and_then(|v| space.map_y(v)),
            pos(geo, "ymax", table, row).and_then(|v| space.map_y(v)),
        ) else {
            continue;
        };
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        w.line(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{} opacity=\"{}\" />",
            num(xmin.min(xmax)),
            num(ymin.min(ymax)),
            num((xmax - xmin).abs()),
            num((ymax - ymin).abs()),
            escape_attr(&color),
            stroke_attrs(&stroke, stroke_width, table, row),
            num(alpha),
        ));
    }
}

/// The raw value of a positional property: a mapped column cell (numeric or
/// temporal-as-microseconds) or a literal. Temporal cells round-trip through
/// `f64` and are converted back to `i64` inside `AxisScale::map_value`; the
/// range of microsecond instants we encounter fits well within the 53-bit f64
/// mantissa.
fn pos(geo: &GeometryIr, name: &str, table: &dyn Table, row: usize) -> Option<f64> {
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == name) {
        let column = &mapping.column.name;
        if let Some(value) = cell_f64(table, column, row) {
            return Some(value);
        }
        return cell_micros(table, column, row).map(|micros| micros as f64);
    }
    geo.settings
        .iter()
        .find(|s| s.name == name)
        .and_then(|s| match s.value {
            SettingValue::Number(n) => Some(n),
            _ => None,
        })
}

fn ribbon(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    scales: &[ScaleIr],
) {
    let fill = color_spec(geo, "fill", table, scales);
    let stroke = color_spec(geo, "stroke", table, scales);
    let stroke_width = number_setting(geo, "strokeWidth", 1.0);
    let alpha = number_setting(geo, "alpha", 0.25);
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
                let ymin = pos(geo, "ymin", table, row).and_then(|v| space.map_y(v))?;
                let ymax = pos(geo, "ymax", table, row).and_then(|v| space.map_y(v))?;
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

fn tile(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    scales: &[ScaleIr],
) {
    let fill = color_spec(geo, "fill", table, scales);
    let stroke = color_spec(geo, "stroke", table, scales);
    let stroke_width = number_setting(geo, "strokeWidth", 1.0);
    let alpha = number_setting(geo, "alpha", 1.0);
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

fn hline(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    plot: Rect,
    table: &dyn Table,
    theme: &Theme,
    scales: &[ScaleIr],
) {
    let Some(y) = number_setting_opt(geo, "y").and_then(|value| space.map_y(value)) else {
        return;
    };
    let stroke = color_spec(geo, "stroke", table, scales);
    let color = constant_or(&stroke, DEFAULT_STROKE);
    let width = number_setting(geo, "strokeWidth", theme.line_width);
    let alpha = number_setting(geo, "alpha", 1.0);
    emit_svg_line(w, plot.x, y, plot.right(), y, &color, width, alpha);
    if let Some(label) = string_setting(geo, "label") {
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

fn vline(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    plot: Rect,
    table: &dyn Table,
    theme: &Theme,
    scales: &[ScaleIr],
) {
    let Some(x) = number_setting_opt(geo, "x").and_then(|value| space.map_x(value)) else {
        return;
    };
    let stroke = color_spec(geo, "stroke", table, scales);
    let color = constant_or(&stroke, DEFAULT_STROKE);
    let width = number_setting(geo, "strokeWidth", theme.line_width);
    let alpha = number_setting(geo, "alpha", 1.0);
    emit_svg_line(w, x, plot.y, x, plot.bottom(), &color, width, alpha);
    if let Some(label) = string_setting(geo, "label") {
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

#[allow(clippy::too_many_arguments)]
fn rug(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    plot: Rect,
    theme: &Theme,
    scales: &[ScaleIr],
) {
    let sides = string_setting(geo, "sides").unwrap_or_else(|| "b".to_string());
    let stroke = color_spec(geo, "stroke", table, scales);
    let width = number_setting(geo, "strokeWidth", theme.line_width);
    let alpha = number_setting(geo, "alpha", 0.55);
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

#[allow(clippy::too_many_arguments)]
fn emit_svg_line(
    w: &mut SvgWriter,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: &str,
    width: f64,
    alpha: f64,
) {
    w.line(&format!(
        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"{}\" />",
        num(x1),
        num(y1),
        num(x2),
        num(y2),
        escape_attr(stroke),
        num(width.max(0.0)),
        num(alpha),
    ));
}

fn number_setting_opt(geo: &GeometryIr, name: &str) -> Option<f64> {
    geo.settings
        .iter()
        .find(|setting| setting.name == name)
        .and_then(|setting| match setting.value {
            SettingValue::Number(value) => Some(value),
            _ => None,
        })
}

fn string_setting(geo: &GeometryIr, name: &str) -> Option<String> {
    geo.settings
        .iter()
        .find(|setting| setting.name == name)
        .and_then(|setting| match &setting.value {
            SettingValue::String(value) => Some(value.clone()),
            _ => None,
        })
}

fn stroke_attrs(spec: &ColorSpec, width: f64, table: &dyn Table, row: usize) -> String {
    if matches!(spec, ColorSpec::None) {
        return String::new();
    }
    let Some(color) = spec.resolve(table, row) else {
        return String::new();
    };
    format!(
        " stroke=\"{}\" stroke-width=\"{}\"",
        escape_attr(&color),
        num(width.max(0.0)),
    )
}

fn constant_or(spec: &ColorSpec, default: &str) -> String {
    match spec {
        ColorSpec::Constant(c) => c.clone(),
        _ => default.to_string(),
    }
}

fn render_rows(table: &dyn Table, rows: Option<&[usize]>) -> Vec<usize> {
    rows.map(|rows| rows.to_vec())
        .unwrap_or_else(|| (0..table.row_count()).collect())
}

/// Render an `Area` geometry: fill between y and a baseline (spec §14.14).
fn area(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    scales: &[ScaleIr],
) {
    let fill = color_spec(geo, "fill", table, scales);
    let stroke = color_spec(geo, "stroke", table, scales);
    let stroke_width = number_setting(geo, "strokeWidth", 1.0);
    let alpha = number_setting(geo, "alpha", 0.4);
    let baseline_value = number_setting(geo, "baseline", 0.0);
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

/// Render a `Text` geometry: draw labels at each row (spec §14.16).
fn text_geom(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    theme: &Theme,
    scales: &[ScaleIr],
) {
    let fill = color_spec(geo, "fill", table, scales);
    let alpha = number_setting(geo, "alpha", 1.0);
    let size = number_setting(geo, "size", theme.font_size);
    let dx = number_setting(geo, "dx", 0.0);
    let dy = number_setting(geo, "dy", 0.0);
    let anchor = string_setting(geo, "anchor").unwrap_or_else(|| "middle".to_string());

    let label_mapping = geo.mappings.iter().find(|m| m.aesthetic == "label");
    let label_literal = string_setting(geo, "label");

    for row in render_rows(table, rows) {
        let (Some(cx), Some(cy)) = (space.resolve_x(table, row), space.resolve_y(table, row))
        else {
            continue;
        };
        let label = if let Some(mapping) = label_mapping {
            match cell_category(table, &mapping.column.name, row) {
                Some(s) => s,
                None => continue,
            }
        } else if let Some(s) = label_literal.clone() {
            s
        } else {
            continue;
        };
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| theme.text_color.clone());
        w.line(&format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\" opacity=\"{}\">{}</text>",
            num(cx + dx),
            num(cy + dy),
            escape_attr(&anchor),
            escape_attr(&theme.font_family),
            num(size),
            escape_attr(&color),
            num(alpha),
            escape_text(&label),
        ));
    }
}

/// Render a `Segment` geometry: a straight line between literal endpoints
/// (spec §14.19).
fn segment(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    theme: &Theme,
    scales: &[ScaleIr],
) {
    let stroke = color_spec(geo, "stroke", table, scales);
    let color = constant_or(&stroke, DEFAULT_STROKE);
    let width = number_setting(geo, "strokeWidth", theme.line_width);
    let alpha = number_setting(geo, "alpha", 1.0);

    let (Some(x), Some(y), Some(xend), Some(yend)) = (
        number_setting_opt(geo, "x").and_then(|v| space.map_x(v)),
        number_setting_opt(geo, "y").and_then(|v| space.map_y(v)),
        number_setting_opt(geo, "xend").and_then(|v| space.map_x(v)),
        number_setting_opt(geo, "yend").and_then(|v| space.map_y(v)),
    ) else {
        return;
    };

    emit_svg_line(w, x, y, xend, yend, &color, width, alpha);
}
