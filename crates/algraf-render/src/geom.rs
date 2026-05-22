//! Geometry rendering (spec §14, §18.6). Supported in version 0.1: Point,
//! Line, Bar (dodge and stack), Rect, and Tile. Other geometries emit a render
//! diagnostic and are skipped.

use std::collections::HashMap;
use std::fmt::Write;

use algraf_core::Diagnostic;
use algraf_data::Table;
use algraf_semantics::{GeometryIr, GeometryKind, ScaleIr, SettingValue};

use crate::aes::{color_spec, number_for_row, number_setting, ColorSpec};
use crate::layout::Rect;
use crate::scale::{cell_category, cell_f64, cell_micros};
use crate::space::{AxisScale, ScaledSpace};
use crate::stats;
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
            w,
            geo,
            ctx.space,
            ctx.table,
            ctx.rows,
            ctx.theme,
            ctx.scales,
            diagnostics,
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
        GeometryKind::HexBin => hexbin(
            w,
            geo,
            ctx.space,
            ctx.table,
            ctx.rows,
            ctx.theme,
            ctx.scales,
            diagnostics,
        ),
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
        GeometryKind::Violin => violin(
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
            w, geo, ctx.space, ctx.table, ctx.rows, ctx.plot, ctx.theme, ctx.scales,
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
        GeometryKind::FreqPoly => "freqpoly",
        GeometryKind::Bin2D => "bin2d",
        GeometryKind::HexBin => "hexbin",
        GeometryKind::Rect => "rect",
        GeometryKind::Tile => "tile",
        GeometryKind::Smooth => "smooth",
        GeometryKind::Boxplot => "boxplot",
        GeometryKind::Violin => "violin",
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

#[allow(clippy::too_many_arguments)]
fn point(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    theme: &Theme,
    scales: &[ScaleIr],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let fill = color_spec(geo, "fill", table, scales);
    let alpha = number_setting(geo, "alpha", 1.0);
    let size = number_setting(geo, "size", theme.point_size);
    let shape = shape_spec(geo, table, diagnostics);
    for row in render_rows(table, rows) {
        let (Some(cx), Some(cy)) = (space.resolve_x(table, row), space.resolve_y(table, row))
        else {
            continue;
        };
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        emit_point_shape(w, shape.resolve(table, row), cx, cy, size, &color, alpha);
    }
}

#[derive(Debug, Clone, Copy)]
enum PointShape {
    Circle,
    Square,
    Triangle,
    Diamond,
}

struct ShapeSpec {
    constant: Option<PointShape>,
    mapping: Option<(String, Vec<String>)>,
}

impl ShapeSpec {
    fn resolve(&self, table: &dyn Table, row: usize) -> PointShape {
        if let Some(shape) = self.constant {
            return shape;
        }
        if let Some((col, categories)) = &self.mapping {
            let Some(category) = cell_category(table, col, row) else {
                return PointShape::Circle;
            };
            let index = categories
                .iter()
                .position(|value| value == &category)
                .unwrap_or(0);
            return SHAPES[index % SHAPES.len()];
        }
        PointShape::Circle
    }
}

const SHAPES: &[PointShape] = &[
    PointShape::Circle,
    PointShape::Square,
    PointShape::Triangle,
    PointShape::Diamond,
];

fn shape_spec(geo: &GeometryIr, table: &dyn Table, diagnostics: &mut Vec<Diagnostic>) -> ShapeSpec {
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == "shape") {
        return ShapeSpec {
            constant: None,
            mapping: Some((
                mapping.column.name.clone(),
                crate::scale::categorical_domain(table, &mapping.column.name),
            )),
        };
    }
    let constant = geo
        .settings
        .iter()
        .find(|setting| setting.name == "shape")
        .and_then(|setting| match &setting.value {
            SettingValue::String(value) => match value.as_str() {
                "circle" => Some(PointShape::Circle),
                "square" => Some(PointShape::Square),
                "triangle" => Some(PointShape::Triangle),
                "diamond" => Some(PointShape::Diamond),
                _ => {
                    diagnostics.push(Diagnostic::warning(
                        "W2006",
                        format!("unknown point shape `{value}`; using `circle`"),
                        geo.span,
                    ));
                    Some(PointShape::Circle)
                }
            },
            _ => None,
        });
    ShapeSpec {
        constant,
        mapping: None,
    }
}

fn emit_point_shape(
    w: &mut SvgWriter,
    shape: PointShape,
    cx: f64,
    cy: f64,
    size: f64,
    color: &str,
    alpha: f64,
) {
    match shape {
        PointShape::Circle => w.line(&format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" opacity=\"{}\" />",
            num(cx),
            num(cy),
            num(size),
            escape_attr(color),
            num(alpha),
        )),
        PointShape::Square => {
            let side = size * 2.0;
            w.line(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" opacity=\"{}\" />",
                num(cx - size),
                num(cy - size),
                num(side),
                num(side),
                escape_attr(color),
                num(alpha),
            ));
        }
        PointShape::Triangle => {
            let d = format!(
                "M{} {} L{} {} L{} {} Z",
                num(cx),
                num(cy - size),
                num(cx + size),
                num(cy + size),
                num(cx - size),
                num(cy + size)
            );
            w.line(&format!(
                "<path d=\"{}\" fill=\"{}\" opacity=\"{}\" />",
                d,
                escape_attr(color),
                num(alpha),
            ));
        }
        PointShape::Diamond => {
            let d = format!(
                "M{} {} L{} {} L{} {} L{} {} Z",
                num(cx),
                num(cy - size),
                num(cx + size),
                num(cy),
                num(cx),
                num(cy + size),
                num(cx - size),
                num(cy)
            );
            w.line(&format!(
                "<path d=\"{}\" fill=\"{}\" opacity=\"{}\" />",
                d,
                escape_attr(color),
                num(alpha),
            ));
        }
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

    // Group rows into series by `group` if present; otherwise preserve the
    // historical behavior of grouping by stroke category.
    let groups: Vec<(String, Vec<usize>)> = grouped_rows(geo, &stroke, table, row_list);

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

fn grouped_rows(
    geo: &GeometryIr,
    stroke: &ColorSpec,
    table: &dyn Table,
    rows: Vec<usize>,
) -> Vec<(String, Vec<usize>)> {
    if let Some(mapping) = geo
        .mappings
        .iter()
        .find(|mapping| mapping.aesthetic == "group")
    {
        return crate::scale::categorical_domain(table, &mapping.column.name)
            .into_iter()
            .map(|cat| {
                let group_rows = rows
                    .iter()
                    .copied()
                    .filter(|&row| {
                        cell_category(table, &mapping.column.name, row).as_deref()
                            == Some(cat.as_str())
                    })
                    .collect();
                (cat, group_rows)
            })
            .collect();
    }
    match stroke {
        ColorSpec::Categorical { categories, .. } => categories
            .iter()
            .map(|cat| {
                let group_rows = rows
                    .iter()
                    .copied()
                    .filter(|&r| {
                        stroke.resolve(table, r).is_some()
                            && row_category(stroke, table, r).as_deref() == Some(cat)
                    })
                    .collect();
                (cat.clone(), group_rows)
            })
            .collect(),
        _ => vec![(String::new(), rows)],
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

    for (_, group_rows) in grouped_rows(geo, &stroke, table, row_list) {
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
        AxisScale::Continuous { .. }
            | AxisScale::Temporal { .. }
            | AxisScale::Union { .. }
            | AxisScale::TemporalUnion { .. }
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

fn violin(
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
            "Violin requires categorical x and continuous y dimensions",
            geo.span,
        ));
        return;
    }

    let fill = color_spec(geo, "fill", table, scales);
    let stroke = color_spec(geo, "stroke", table, scales);
    let alpha = number_setting(geo, "alpha", 0.55);
    let stroke_width = number_setting(geo, "strokeWidth", 1.0);
    let quantiles = number_array_setting(geo, "quantiles").unwrap_or_default();
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
        bandwidth: number_setting_opt(geo, "bandwidth").filter(|value| *value > 0.0),
        grid_points: number_setting_opt(geo, "n")
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
        let half_width = number_setting(geo, "width", bandwidth * 0.9).clamp(1.0, bandwidth) / 2.0;
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

fn number_array_setting(geo: &GeometryIr, name: &str) -> Option<Vec<f64>> {
    geo.settings
        .iter()
        .find(|setting| setting.name == name)
        .and_then(|setting| match &setting.value {
            SettingValue::NumberArray(values) => Some(values.clone()),
            _ => None,
        })
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
            pos_bound(geo, "xmin", &space.x, table, row),
            pos_bound(geo, "xmax", &space.x, table, row),
            space
                .y
                .as_ref()
                .and_then(|axis| pos_bound(geo, "ymin", axis, table, row)),
            space
                .y
                .as_ref()
                .and_then(|axis| pos_bound(geo, "ymax", axis, table, row)),
        ) else {
            continue;
        };
        let mut x = xmin.min(xmax);
        let mut y = ymin.min(ymax);
        let mut width = (xmax - xmin).abs();
        let mut height = (ymax - ymin).abs();
        let marker = stroke_width.max(1.0);
        if width <= f64::EPSILON {
            x -= marker / 2.0;
            width = marker;
        }
        if height <= f64::EPSILON {
            y -= marker / 2.0;
            height = marker;
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

#[allow(clippy::too_many_arguments)]
fn hexbin(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    _rows: Option<&[usize]>,
    theme: &Theme,
    scales: &[ScaleIr],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (Some(x_col), Some(y_col)) = (
        space.x.data_column(),
        space.y.as_ref().and_then(|axis| axis.data_column()),
    ) else {
        diagnostics.push(Diagnostic::warning(
            "R0002",
            "HexBin requires continuous x and y dimensions",
            geo.span,
        ));
        return;
    };
    if !axis_is_continuousish(&space.x) || !space.y.as_ref().is_some_and(axis_is_continuousish) {
        diagnostics.push(Diagnostic::warning(
            "R0002",
            "HexBin requires continuous x and y dimensions",
            geo.span,
        ));
        return;
    }
    let bins = number_setting(geo, "bins", 30.0).round().max(1.0) as usize;
    let cells = stats::hexbin(table, x_col, y_col, stats::Bin2DOptions { bins });
    // Normalize fill over the count domain `[min, max]`, matching the
    // continuous legend synthesized in `collect_legends` so swatch colors and
    // hexagon colors agree.
    let min_count = cells.iter().map(|cell| cell.count).min().unwrap_or(0) as f64;
    let max_count = cells.iter().map(|cell| cell.count).max().unwrap_or(1) as f64;
    let fill = color_spec(geo, "fill", table, scales);
    let stroke = color_spec(geo, "stroke", table, scales);
    let stroke_width = number_setting(geo, "strokeWidth", theme.line_width);
    let alpha = number_setting(geo, "alpha", 0.9);
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

fn pos_bound(
    geo: &GeometryIr,
    name: &str,
    axis: &AxisScale,
    table: &dyn Table,
    row: usize,
) -> Option<f64> {
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == name) {
        let column = &mapping.column.name;
        if let Some(value) = cell_f64(table, column, row) {
            return axis.map_value_public(value);
        }
        if let Some(micros) = cell_micros(table, column, row) {
            return axis.map_value_public(micros as f64);
        }
        return categorical_bound(axis, column, table, row, bound_is_upper(name));
    }
    geo.settings
        .iter()
        .find(|s| s.name == name)
        .and_then(|s| match s.value {
            SettingValue::Number(n) => axis.map_value_public(n),
            _ => None,
        })
}

fn bound_is_upper(name: &str) -> bool {
    matches!(name, "xmax" | "ymax")
}

fn categorical_bound(
    axis: &AxisScale,
    column: &str,
    table: &dyn Table,
    row: usize,
    upper: bool,
) -> Option<f64> {
    match axis {
        AxisScale::Band { col, scale } if col == column => {
            let category = cell_category(table, col, row)?;
            let (start, width) = scale.band(&category)?;
            Some(if upper { start + width } else { start })
        }
        AxisScale::NestedBand {
            outer_col,
            inner_col,
            scale,
        } if column == outer_col || column == inner_col => {
            let outer = cell_category(table, outer_col, row)?;
            let inner = cell_category(table, inner_col, row)?;
            let (start, width) = scale.band(&outer, &inner)?;
            Some(if upper { start + width } else { start })
        }
        _ => None,
    }
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

fn bool_setting(geo: &GeometryIr, name: &str, default: bool) -> bool {
    geo.settings
        .iter()
        .find(|setting| setting.name == name)
        .and_then(|setting| match setting.value {
            SettingValue::Bool(value) => Some(value),
            _ => None,
        })
        .unwrap_or(default)
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

/// A label placed at its (possibly decluttered) screen position.
struct PlacedLabel {
    x: f64,
    y: f64,
    size: f64,
    color: String,
    text: String,
}

/// Render a `Text` geometry: draw labels at each row (spec §14.16).
///
/// `dx`/`dy` may be literals or column mappings (resolved per row). With
/// `declutter: true`, labels that overlap vertically within a shared x column
/// are spread apart before emission (spec §14.16).
#[allow(clippy::too_many_arguments)]
fn text_geom(
    w: &mut SvgWriter,
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn Table,
    rows: Option<&[usize]>,
    plot: Rect,
    theme: &Theme,
    scales: &[ScaleIr],
) {
    let fill = color_spec(geo, "fill", table, scales);
    let alpha = number_setting(geo, "alpha", 1.0);
    let size = number_setting(geo, "size", theme.font_size);
    let anchor = string_setting(geo, "anchor").unwrap_or_else(|| "middle".to_string());
    let declutter = bool_setting(geo, "declutter", false);

    let label_mapping = geo.mappings.iter().find(|m| m.aesthetic == "label");
    let label_literal = string_setting(geo, "label");

    // Phase 1: collect each resolvable label at its post-dx/dy position.
    let mut labels: Vec<PlacedLabel> = Vec::new();
    for row in render_rows(table, rows) {
        let (Some(cx), Some(cy)) = (space.resolve_x(table, row), space.resolve_y(table, row))
        else {
            continue;
        };
        let text = if let Some(mapping) = label_mapping {
            match cell_category(table, &mapping.column.name, row) {
                Some(s) => s,
                None => continue,
            }
        } else if let Some(s) = label_literal.clone() {
            s
        } else {
            continue;
        };
        let dx = number_for_row(geo, "dx", table, row, 0.0);
        let dy = number_for_row(geo, "dy", table, row, 0.0);
        let color = fill
            .resolve(table, row)
            .unwrap_or_else(|| theme.text_color.clone());
        labels.push(PlacedLabel {
            x: cx + dx,
            y: cy + dy,
            size,
            color,
            text,
        });
    }

    // Phase 2: optionally spread vertically-overlapping labels apart.
    if declutter {
        declutter_vertical(&mut labels, plot);
    }

    // Phase 3: emit in collection (row) order for deterministic output.
    for label in &labels {
        w.line(&format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\" opacity=\"{}\">{}</text>",
            num(label.x),
            num(label.y),
            escape_attr(&anchor),
            escape_attr(&theme.font_family),
            num(label.size),
            escape_attr(&label.color),
            num(alpha),
            escape_text(&label.text),
        ));
    }
}

/// Spread labels that overlap vertically apart, grouped by shared x column
/// (spec §14.16). Deterministic: groups by quantized x, and within a group lays
/// labels out with a minimum gap while staying as close as possible to their
/// targets, clamped to the plot's vertical extent.
fn declutter_vertical(labels: &mut [PlacedLabel], plot: Rect) {
    // Group label indices by rounded x so only labels sharing a column interact.
    let mut groups: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, label) in labels.iter().enumerate() {
        groups.entry(label.x.round() as i64).or_default().push(i);
    }
    // Deterministic group order.
    let mut keys: Vec<i64> = groups.keys().copied().collect();
    keys.sort_unstable();

    for key in keys {
        let indices = &groups[&key];
        if indices.len() < 2 {
            continue;
        }
        let gap = labels[indices[0]].size * 1.2;
        // Stable order by target y, breaking ties by original index.
        let mut order = indices.clone();
        order.sort_by(|&a, &b| labels[a].y.total_cmp(&labels[b].y).then_with(|| a.cmp(&b)));

        let targets: Vec<f64> = order.iter().map(|&i| labels[i].y).collect();
        let mut positions = resolve_min_gap(&targets, gap);
        clamp_group(&mut positions, gap, plot.y, plot.bottom());

        for (k, &i) in order.iter().enumerate() {
            labels[i].y = positions[k];
        }
    }
}

/// Lay out ascending `targets` so adjacent positions are at least `gap` apart,
/// minimizing displacement (a 1-D isotonic / pool-adjacent-violators layout).
/// Returns positions aligned with `targets`. Deterministic and O(n).
fn resolve_min_gap(targets: &[f64], gap: f64) -> Vec<f64> {
    const EPS: f64 = 1e-9;
    // Each cluster lays its members out at `gap` spacing centered on the mean of
    // its members' targets. Merge adjacent clusters that would overlap.
    struct Cluster {
        count: usize,
        sum: f64,
    }
    let mut clusters: Vec<Cluster> = Vec::with_capacity(targets.len());
    for &t in targets {
        clusters.push(Cluster { count: 1, sum: t });
        while clusters.len() >= 2 {
            let b = &clusters[clusters.len() - 1];
            let a = &clusters[clusters.len() - 2];
            let a_mean = a.sum / a.count as f64;
            let b_mean = b.sum / b.count as f64;
            let a_bottom = a_mean + (a.count as f64 - 1.0) / 2.0 * gap;
            let b_top = b_mean - (b.count as f64 - 1.0) / 2.0 * gap;
            if b_top - a_bottom < gap - EPS {
                let merged = Cluster {
                    count: a.count + b.count,
                    sum: a.sum + b.sum,
                };
                clusters.pop();
                clusters.pop();
                clusters.push(merged);
            } else {
                break;
            }
        }
    }

    let mut positions = Vec::with_capacity(targets.len());
    for c in &clusters {
        let mean = c.sum / c.count as f64;
        let first = mean - (c.count as f64 - 1.0) / 2.0 * gap;
        for j in 0..c.count {
            positions.push(first + j as f64 * gap);
        }
    }
    positions
}

/// Shift a laid-out group into `[top + gap, bottom]`. Prefers fitting the top;
/// if the group is taller than the band it overflows downward rather than
/// crushing the spacing.
fn clamp_group(positions: &mut [f64], gap: f64, top: f64, bottom: f64) {
    let top_limit = top + gap;
    let (Some(&first), Some(&last)) = (positions.first(), positions.last()) else {
        return;
    };
    if first < top_limit {
        let shift = top_limit - first;
        positions.iter_mut().for_each(|p| *p += shift);
    } else if last > bottom {
        // Shift up to fit the bottom, but never push the top above its limit.
        let shift = (last - bottom).min(first - top_limit);
        if shift > 0.0 {
            positions.iter_mut().for_each(|p| *p -= shift);
        }
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
