use std::collections::HashMap;
use std::fmt::Write;

use algraf_core::{codes, Diagnostic};
use algraf_semantics::{GeometryIr, PropertyKey};

use crate::aes::{color_spec, number_setting, number_spec, ColorSpec};
use crate::helpers::{bool_setting, number_array_setting, number_setting_opt, string_setting};
use crate::marker::{emit_marker, MarkerShape};
use crate::scale::cell_f64;
use crate::sink::{Fill, MarkSink, Paint, Stroke};
use crate::space::ScaledSpace;
use crate::stats;
use crate::svg::num;

use super::common::{
    axis_is_continuousish, categorical_value_orientation, deterministic_unit, emit_svg_line,
    map_value_axis, position_bandwidth, position_center, position_group_key, quantile_type7,
    render_rows, stroke_style, value_axis_data_column, Orientation, DEFAULT_FILL,
    DEFAULT_SIZE_RANGE, DEFAULT_STROKE,
};
use super::GeometryRenderContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DistributionSide {
    Both,
    Left,
    Right,
    Top,
    Bottom,
}

impl DistributionSide {
    fn from_geometry(geo: &GeometryIr) -> Self {
        match string_setting(geo, PropertyKey::Side).as_deref() {
            Some("left") => DistributionSide::Left,
            Some("right") => DistributionSide::Right,
            Some("top") => DistributionSide::Top,
            Some("bottom") => DistributionSide::Bottom,
            _ => DistributionSide::Both,
        }
    }

    fn normalized(self, orientation: Orientation) -> Self {
        match (orientation, self) {
            (_, DistributionSide::Both) => DistributionSide::Both,
            (Orientation::Vertical, DistributionSide::Left | DistributionSide::Right) => self,
            (Orientation::Vertical, DistributionSide::Top) => DistributionSide::Left,
            (Orientation::Vertical, DistributionSide::Bottom) => DistributionSide::Right,
            (Orientation::Horizontal, DistributionSide::Top | DistributionSide::Bottom) => self,
            (Orientation::Horizontal, DistributionSide::Left) => DistributionSide::Bottom,
            (Orientation::Horizontal, DistributionSide::Right) => DistributionSide::Top,
        }
    }
}

struct OrderedDistributionGroups {
    order: Vec<String>,
    groups: HashMap<String, Vec<(usize, f64)>>,
}

struct DistributionDensityLayout {
    group_key: String,
    samples: Vec<(usize, f64)>,
    values: Vec<f64>,
    curve: Vec<stats::DensityPoint>,
    max_density: f64,
    first_row: usize,
    center: f64,
    bandwidth: f64,
    half_width: f64,
}

fn collect_distribution_groups(
    space: &ScaledSpace,
    table: &dyn algraf_data::Table,
    rows: Option<&[usize]>,
    orientation: Orientation,
    value_col: &str,
) -> OrderedDistributionGroups {
    let mut groups: HashMap<String, Vec<(usize, f64)>> = HashMap::new();
    let mut order = Vec::new();

    for row in render_rows(table, rows) {
        let Some(key) = position_group_key(space, table, row, orientation) else {
            continue;
        };
        let Some(value) = cell_f64(table, value_col, row) else {
            continue;
        };
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push((row, value));
    }

    OrderedDistributionGroups { order, groups }
}

fn distribution_density_options(geo: &GeometryIr) -> stats::DensityOptions {
    stats::DensityOptions {
        bandwidth: number_setting_opt(geo, PropertyKey::Bandwidth).filter(|value| *value > 0.0),
        grid_points: number_setting_opt(geo, PropertyKey::N)
            .filter(|value| *value >= 2.0)
            .map(|value| value.round() as usize)
            .unwrap_or(256),
    }
}

fn density_layouts(
    geo: &GeometryIr,
    space: &ScaledSpace,
    table: &dyn algraf_data::Table,
    groups: &mut OrderedDistributionGroups,
    orientation: Orientation,
    options: stats::DensityOptions,
) -> Vec<DistributionDensityLayout> {
    let mut layouts = Vec::new();

    for key in &groups.order {
        let Some(group) = groups.groups.get_mut(key) else {
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
        let (Some(center), Some(bandwidth)) = (
            position_center(space, table, first_row, orientation),
            position_bandwidth(space, table, first_row, orientation),
        ) else {
            continue;
        };
        let half_width =
            number_setting(geo, PropertyKey::Width, bandwidth * 0.9).clamp(1.0, bandwidth) / 2.0;
        layouts.push(DistributionDensityLayout {
            group_key: key.clone(),
            samples: group.clone(),
            values,
            curve,
            max_density,
            first_row,
            center,
            bandwidth,
            half_width,
        });
    }

    layouts
}

pub(super) fn render_boxplot(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let scales = ctx.scales;
    let Some(orientation) = categorical_value_orientation(space) else {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "Boxplot requires one categorical position axis and one continuous value axis",
            geo.span,
        ));
        return;
    };
    let Some(value_col) = value_axis_data_column(space, orientation) else {
        return;
    };

    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, 1.0);
    // Points beyond the 1.5·IQR whiskers render as small circles by default
    // (spec §14.11); `outliers: false` suppresses them.
    let show_outliers = bool_setting(geo, PropertyKey::Outliers, true);
    let mut groups = collect_distribution_groups(space, table, rows, orientation, value_col);

    for key in groups.order {
        let Some(group) = groups.groups.get_mut(&key) else {
            continue;
        };
        group.sort_by(|a, b| a.1.total_cmp(&b.1));
        let Some(&(first_row, _)) = group.first() else {
            continue;
        };
        let values: Vec<f64> = group.iter().map(|(_, value)| *value).collect();
        let Some(last_value) = values.last().copied() else {
            continue;
        };
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
            .unwrap_or(last_value);

        let (Some(center), Some(bandwidth)) = (
            position_center(space, table, first_row, orientation),
            position_bandwidth(space, table, first_row, orientation),
        ) else {
            continue;
        };
        let width_setting = number_setting(geo, PropertyKey::Width, bandwidth * 0.7);
        let box_width = width_setting.clamp(1.0, bandwidth);
        let half = box_width / 2.0;
        let (Some(q1_pos), Some(median_pos), Some(q3_pos), Some(low_pos), Some(high_pos)) = (
            map_value_axis(space, q1, orientation),
            map_value_axis(space, median, orientation),
            map_value_axis(space, q3, orientation),
            map_value_axis(space, whisker_low, orientation),
            map_value_axis(space, whisker_high, orientation),
        ) else {
            continue;
        };

        let fill_color = fill
            .resolve(table, first_row)
            .unwrap_or_else(|| DEFAULT_FILL.to_string());
        let stroke_color = stroke
            .resolve(table, first_row)
            .unwrap_or_else(|| DEFAULT_STROKE.to_string());
        match orientation {
            Orientation::Vertical => {
                let top = q3_pos.min(q1_pos);
                let height = (q1_pos - q3_pos).abs().max(1.0);
                emit_svg_line(
                    sink,
                    center,
                    low_pos,
                    center,
                    high_pos,
                    &stroke_color,
                    stroke_width,
                    alpha,
                );
                emit_svg_line(
                    sink,
                    center - half * 0.4,
                    low_pos,
                    center + half * 0.4,
                    low_pos,
                    &stroke_color,
                    stroke_width,
                    alpha,
                );
                emit_svg_line(
                    sink,
                    center - half * 0.4,
                    high_pos,
                    center + half * 0.4,
                    high_pos,
                    &stroke_color,
                    stroke_width,
                    alpha,
                );
                sink.rect(
                    center - half,
                    top,
                    box_width,
                    height,
                    &Paint {
                        fill: Fill::Color(fill_color),
                        stroke: Stroke::Solid {
                            color: stroke_color.clone(),
                            width: stroke_width,
                        },
                        opacity: Some(alpha),
                    },
                );
                emit_svg_line(
                    sink,
                    center - half,
                    median_pos,
                    center + half,
                    median_pos,
                    &stroke_color,
                    stroke_width,
                    alpha,
                );
            }
            Orientation::Horizontal => {
                let left = q1_pos.min(q3_pos);
                let width = (q3_pos - q1_pos).abs().max(1.0);
                emit_svg_line(
                    sink,
                    low_pos,
                    center,
                    high_pos,
                    center,
                    &stroke_color,
                    stroke_width,
                    alpha,
                );
                emit_svg_line(
                    sink,
                    low_pos,
                    center - half * 0.4,
                    low_pos,
                    center + half * 0.4,
                    &stroke_color,
                    stroke_width,
                    alpha,
                );
                emit_svg_line(
                    sink,
                    high_pos,
                    center - half * 0.4,
                    high_pos,
                    center + half * 0.4,
                    &stroke_color,
                    stroke_width,
                    alpha,
                );
                sink.rect(
                    left,
                    center - half,
                    width,
                    box_width,
                    &Paint {
                        fill: Fill::Color(fill_color),
                        stroke: Stroke::Solid {
                            color: stroke_color.clone(),
                            width: stroke_width,
                        },
                        opacity: Some(alpha),
                    },
                );
                emit_svg_line(
                    sink,
                    median_pos,
                    center - half,
                    median_pos,
                    center + half,
                    &stroke_color,
                    stroke_width,
                    alpha,
                );
            }
        }

        // Outliers: observations beyond the 1.5·IQR fences, drawn as small open
        // circles centered on the box (spec §14.11). Order follows the sorted
        // group, so output stays deterministic.
        if show_outliers {
            let radius = (stroke_width * 1.5).max(2.0);
            for (_, value) in group.iter() {
                if *value < lower_bound || *value > upper_bound {
                    if let Some(value_pos) = map_value_axis(space, *value, orientation) {
                        let (cx, cy) = match orientation {
                            Orientation::Vertical => (center, value_pos),
                            Orientation::Horizontal => (value_pos, center),
                        };
                        sink.circle(
                            cx,
                            cy,
                            radius,
                            &Paint {
                                fill: Fill::None,
                                stroke: Stroke::Solid {
                                    color: stroke_color.clone(),
                                    width: stroke_width,
                                },
                                opacity: Some(alpha),
                            },
                        );
                    }
                }
            }
        }
    }
}

pub(super) fn render_violin(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let scales = ctx.scales;
    let Some(orientation) = categorical_value_orientation(space) else {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "Violin requires one categorical position axis and one continuous value axis",
            geo.span,
        ));
        return;
    };
    let Some(value_col) = value_axis_data_column(space, orientation) else {
        return;
    };

    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    let alpha = number_setting(geo, PropertyKey::Alpha, 0.55);
    let stroke_width = number_setting(geo, PropertyKey::StrokeWidth, 1.0);
    let quantiles = number_array_setting(geo, PropertyKey::Quantiles).unwrap_or_default();
    let side = DistributionSide::from_geometry(geo).normalized(orientation);
    let mut groups = collect_distribution_groups(space, table, rows, orientation, value_col);
    let options = distribution_density_options(geo);

    for layout in density_layouts(geo, space, table, &mut groups, orientation, options) {
        let DistributionDensityLayout {
            group_key: _group_key,
            samples: _samples,
            values,
            curve,
            max_density,
            first_row,
            center,
            bandwidth: _bandwidth,
            half_width,
        } = layout;
        let mut side_a = Vec::new();
        let mut side_b = Vec::new();
        for point in &curve {
            let Some(value_pos) = map_value_axis(space, point.x, orientation) else {
                continue;
            };
            let offset = point.density / max_density * half_width;
            match (orientation, side) {
                (Orientation::Vertical, DistributionSide::Left) => {
                    side_a.push((center - offset, value_pos));
                    side_b.push((center, value_pos));
                }
                (Orientation::Vertical, DistributionSide::Right) => {
                    side_a.push((center + offset, value_pos));
                    side_b.push((center, value_pos));
                }
                (Orientation::Vertical, _) => {
                    side_a.push((center + offset, value_pos));
                    side_b.push((center - offset, value_pos));
                }
                (Orientation::Horizontal, DistributionSide::Top) => {
                    side_a.push((value_pos, center - offset));
                    side_b.push((value_pos, center));
                }
                (Orientation::Horizontal, DistributionSide::Bottom) => {
                    side_a.push((value_pos, center + offset));
                    side_b.push((value_pos, center));
                }
                (Orientation::Horizontal, _) => {
                    side_a.push((value_pos, center - offset));
                    side_b.push((value_pos, center + offset));
                }
            }
        }
        if side_a.len() < 2 {
            continue;
        }
        let mut d = String::new();
        for (i, (x, y)) in side_a.iter().enumerate() {
            let cmd = if i == 0 { 'M' } else { 'L' };
            let _ = write!(d, "{cmd}{} {} ", num(*x), num(*y));
        }
        for (x, y) in side_b.iter().rev() {
            let _ = write!(d, "L{} {} ", num(*x), num(*y));
        }
        d.push('Z');
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

        let stroke_color = stroke
            .resolve(table, first_row)
            .unwrap_or_else(|| DEFAULT_STROKE.to_string());
        for q in quantiles
            .iter()
            .copied()
            .filter(|q| (0.0..=1.0).contains(q))
        {
            let value = quantile_type7(&values, q);
            let Some(value_pos) = map_value_axis(space, value, orientation) else {
                continue;
            };
            let density = interpolate_density(&curve, value);
            let offset = density / max_density * half_width;
            match (orientation, side) {
                (Orientation::Vertical, DistributionSide::Left) => emit_svg_line(
                    sink,
                    center - offset,
                    value_pos,
                    center,
                    value_pos,
                    &stroke_color,
                    stroke_width,
                    1.0,
                ),
                (Orientation::Vertical, DistributionSide::Right) => emit_svg_line(
                    sink,
                    center,
                    value_pos,
                    center + offset,
                    value_pos,
                    &stroke_color,
                    stroke_width,
                    1.0,
                ),
                (Orientation::Vertical, _) => emit_svg_line(
                    sink,
                    center - offset,
                    value_pos,
                    center + offset,
                    value_pos,
                    &stroke_color,
                    stroke_width,
                    1.0,
                ),
                (Orientation::Horizontal, DistributionSide::Top) => emit_svg_line(
                    sink,
                    value_pos,
                    center - offset,
                    value_pos,
                    center,
                    &stroke_color,
                    stroke_width,
                    1.0,
                ),
                (Orientation::Horizontal, DistributionSide::Bottom) => emit_svg_line(
                    sink,
                    value_pos,
                    center,
                    value_pos,
                    center + offset,
                    &stroke_color,
                    stroke_width,
                    1.0,
                ),
                (Orientation::Horizontal, _) => emit_svg_line(
                    sink,
                    value_pos,
                    center - offset,
                    value_pos,
                    center + offset,
                    &stroke_color,
                    stroke_width,
                    1.0,
                ),
            }
        }
    }
}

pub(super) fn render_sina(
    sink: &mut dyn MarkSink,
    geo: &GeometryIr,
    ctx: GeometryRenderContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let space = ctx.space;
    let table = ctx.table;
    let rows = ctx.rows;
    let scales = ctx.scales;
    let Some(orientation) = categorical_value_orientation(space) else {
        diagnostics.push(Diagnostic::warning(
            codes::R0002,
            "Sina requires one categorical position axis and one continuous value axis",
            geo.span,
        ));
        return;
    };
    let Some(value_col) = value_axis_data_column(space, orientation) else {
        return;
    };

    let fill = color_spec(geo, PropertyKey::Fill, table, scales);
    let alpha = number_setting(geo, PropertyKey::Alpha, 1.0);
    let size = number_spec(
        geo,
        PropertyKey::Size,
        table,
        scales,
        DEFAULT_SIZE_RANGE,
        ctx.theme.point_size,
    );
    let side = DistributionSide::from_geometry(geo).normalized(orientation);
    let mut groups = collect_distribution_groups(space, table, rows, orientation, value_col);
    let options = distribution_density_options(geo);

    for layout in density_layouts(geo, space, table, &mut groups, orientation, options) {
        let DistributionDensityLayout {
            group_key: _group_key,
            samples,
            values: _values,
            curve,
            max_density,
            first_row: _first_row,
            center,
            bandwidth: _bandwidth,
            half_width,
        } = layout;

        for (row, value) in samples.iter().copied() {
            let Some(value_pos) = map_value_axis(space, value, orientation) else {
                continue;
            };
            let density = interpolate_density(&curve, value);
            let max_offset = density / max_density * half_width;
            let unit = deterministic_unit(row, 0xd1b5_4a32_d192_ed03);
            let unsigned = (unit + 0.5) * max_offset;
            let centered = unit * 2.0 * max_offset;
            let (cx, cy) = match (orientation, side) {
                (Orientation::Vertical, DistributionSide::Left) => (center - unsigned, value_pos),
                (Orientation::Vertical, DistributionSide::Right) => (center + unsigned, value_pos),
                (Orientation::Vertical, _) => (center + centered, value_pos),
                (Orientation::Horizontal, DistributionSide::Top) => (value_pos, center - unsigned),
                (Orientation::Horizontal, DistributionSide::Bottom) => {
                    (value_pos, center + unsigned)
                }
                (Orientation::Horizontal, _) => (value_pos, center + centered),
            };
            let color = fill
                .resolve(table, row)
                .unwrap_or_else(|| DEFAULT_FILL.to_string());
            let radius = size.at(table, row, ctx.theme.point_size);
            let paint = Paint::fill(color, Some(alpha));
            emit_marker(sink, MarkerShape::Circle, cx, cy, radius, &paint);
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
    sink: &mut dyn MarkSink,
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
        sink.polygon(
            &points,
            &Paint {
                fill: Fill::Color(color),
                stroke: stroke_style(&stroke, stroke_width, table, 0),
                opacity: Some(alpha),
            },
        );
    }
}
