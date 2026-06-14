//! Guide emission: writes grid lines, axes, facet strips, and legends from
//! trained scales and the planning results in [`super::plan`] (spec §19).
//!
//! Emission goes through the backend-neutral [`MarkSink`] seam (spec §24.6), so
//! the SVG and draw-list backends agree on guide coordinates and colors.

use algraf_semantics::{AxisPositionIr, GridShapeIr, GuideIr, LegendPositionIr, TemporalFormatIr};

use crate::aes::{Legend, LegendKind};
use crate::layout::Rect;
use crate::marker::emit_marker;
use crate::render::TextAnchor;
use crate::sink::{Fill, MarkSink, Paint, Stroke, TextRun};
use crate::space::ScaledSpace;
use crate::svg::num;
use crate::theme::{TextStyle, Theme};

use super::plan::{
    estimate_text_width, horizontal_legend_width, max_x_tick_label_height, max_y_tick_label_width,
    tick_label_row_count, tick_label_row_gap, x_axis_title_y, y_axis_title_x, X_TICK_BASELINE,
    X_TITLE_GAP, Y_TICK_GAP, Y_TITLE_GAP,
};

pub(crate) struct AxisRenderOptions<'a> {
    pub(crate) x_label_override: Option<&'a str>,
    pub(crate) y_label_override: Option<&'a str>,
    pub(crate) x_time_format: Option<&'a TemporalFormatIr>,
    pub(crate) y_time_format: Option<&'a TemporalFormatIr>,
    /// Numeric tick-label formats for continuous axes (spec §19.4, §14.16).
    pub(crate) x_numeric_format: Option<&'a str>,
    pub(crate) y_numeric_format: Option<&'a str>,
    pub(crate) x_tick_label_angle: Option<f64>,
    pub(crate) y_tick_label_angle: Option<f64>,
    pub(crate) x_tick_label_rows: Option<usize>,
    pub(crate) y_tick_label_rows: Option<usize>,
    /// Resolved axis sides (spec §19.2, §19.3).
    pub(crate) x_position: AxisPositionIr,
    pub(crate) y_position: AxisPositionIr,
}

impl<'a> AxisRenderOptions<'a> {
    /// Build axis options from a panel's resolved guides and theme, resolving the
    /// axis side from `Guide(position:)` over the theme default (spec §19.2–19.3,
    /// §20.1).
    pub(crate) fn from_guides(guides: &'a GuideIr, theme: &Theme) -> AxisRenderOptions<'a> {
        AxisRenderOptions {
            x_label_override: guides.x_label.as_deref(),
            y_label_override: guides.y_label.as_deref(),
            x_time_format: guides.x_time_format.as_ref(),
            y_time_format: guides.y_time_format.as_ref(),
            x_numeric_format: guides.x_format.as_deref(),
            y_numeric_format: guides.y_format.as_deref(),
            x_tick_label_angle: guides.x_tick_label_angle,
            y_tick_label_angle: guides.y_tick_label_angle,
            x_tick_label_rows: guides.x_tick_label_rows,
            y_tick_label_rows: guides.y_tick_label_rows,
            x_position: guides.x_position.unwrap_or(theme.axis_x_position),
            y_position: guides.y_position.unwrap_or(theme.axis_y_position),
        }
    }
}

/// Map a guide anchor string to a [`TextAnchor`].
fn anchor(value: &str) -> TextAnchor {
    match value {
        "start" => TextAnchor::Start,
        "end" => TextAnchor::End,
        _ => TextAnchor::Middle,
    }
}

/// Draw grid lines behind the data marks (spec §17.6, §19). Only continuous and
/// temporal axes get grid lines; categorical axes do not. `draw_x` toggles the
/// vertical lines at x ticks; `draw_y` toggles the horizontal lines at y ticks,
/// so a house style can keep only horizontal rules.
pub(crate) fn render_grid(
    sink: &mut dyn MarkSink,
    space: &ScaledSpace,
    plot: Rect,
    theme: &Theme,
    draw_x: bool,
    draw_y: bool,
) {
    if !theme.grid || (!draw_x && !draw_y) {
        return;
    }
    sink.open_layer("algraf-grid");
    let minor = &theme.grid_minor;
    if minor.stroke_width > 0.0 {
        if draw_x && !space.x.is_band() {
            for x in minor_tick_positions(&space.x.ticks()) {
                grid_line(
                    sink,
                    x,
                    plot.y,
                    x,
                    plot.bottom(),
                    &minor.stroke,
                    minor.stroke_width,
                );
            }
        }
        if draw_y {
            if let Some(y) = &space.y {
                if !y.is_band() {
                    for yp in minor_tick_positions(&y.ticks()) {
                        grid_line(
                            sink,
                            plot.x,
                            yp,
                            plot.right(),
                            yp,
                            &minor.stroke,
                            minor.stroke_width,
                        );
                    }
                }
            }
        }
    }
    let color = &theme.grid_major.stroke;
    let width = theme.grid_major.stroke_width;
    if draw_x && !space.x.is_band() {
        for (x, _) in space.x.ticks() {
            grid_line(sink, x, plot.y, x, plot.bottom(), color, width);
        }
    }
    if draw_y {
        if let Some(y) = &space.y {
            if !y.is_band() {
                for (yp, _) in y.ticks() {
                    grid_line(sink, plot.x, yp, plot.right(), yp, color, width);
                }
            }
        }
    }
    sink.close_layer();
}

/// Draw polar grid lines (spec §16.16, §19): concentric radius rings (circle or
/// polygon) and angular spokes. Labels are emitted separately after geometry so
/// opaque polar marks do not cover them.
pub(crate) fn render_polar_grid(
    sink: &mut dyn MarkSink,
    space: &ScaledSpace,
    guides: &GuideIr,
    theme: &Theme,
) {
    let Some(polar) = space.polar() else {
        return;
    };
    if !guides.grid || !theme.grid {
        return;
    }
    let color = &theme.grid_major.stroke;
    let width = theme.grid_major.stroke_width;
    let theta_ticks = space.polar_theta_ticks();
    // Spokes only make sense for a categorical angle (coxcomb/radar); a pie's
    // continuous angle has no meaningful spokes.
    let spoke_angles: Vec<f64> = if space.polar_theta_is_band() {
        theta_ticks.iter().map(|(a, _)| *a).collect()
    } else {
        Vec::new()
    };
    let polygon = guides.grid_shape == GridShapeIr::Polygon && !spoke_angles.is_empty();

    sink.open_layer("algraf-polar-grid");
    // Radius rings at each tick, plus the outer boundary.
    let mut radii: Vec<f64> = space
        .polar_radius_ticks()
        .into_iter()
        .map(|(r, _)| r)
        .collect();
    if !radii.iter().any(|r| (*r - polar.r_outer).abs() < 1.0) {
        radii.push(polar.r_outer);
    }
    for r in radii {
        if r <= polar.r_inner + f64::EPSILON {
            continue;
        }
        if polygon {
            polar_ring_polygon(sink, polar, &spoke_angles, r, color, width);
        } else {
            polar_ring_circle(sink, polar, r, color, width);
        }
    }
    // Spokes from the inner radius to the perimeter.
    for angle in &spoke_angles {
        let (x0, y0) = polar.point(*angle, polar.r_inner);
        let (x1, y1) = polar.point(*angle, polar.r_outer);
        grid_line(sink, x0, y0, x1, y1, color, width);
    }
    sink.close_layer();
}

/// Draw polar perimeter and radius labels above the data marks.
pub(crate) fn render_polar_labels(
    sink: &mut dyn MarkSink,
    space: &ScaledSpace,
    guides: &GuideIr,
    theme: &Theme,
) {
    let Some(polar) = space.polar() else {
        return;
    };
    if !guides.grid || !theme.grid {
        return;
    }
    let theta_ticks = space.polar_theta_ticks();
    // Perimeter labels (categories) around the outside.
    if space.polar_theta_is_band() {
        sink.open_layer("algraf-polar-theta-labels");
        for (angle, label) in &theta_ticks {
            let (lx, ly) = polar.point(*angle, polar.r_outer + crate::space::POLAR_LABEL_GAP);
            styled_text(
                sink,
                lx,
                ly,
                perimeter_anchor(*angle),
                label,
                &theme.axis_text,
            );
        }
        sink.close_layer();
    }

    // Radius labels along the top spoke.
    let radius_ticks = space.polar_radius_ticks();
    if !radius_ticks.is_empty() {
        sink.open_layer("algraf-polar-radius-labels");
        for (r, label) in radius_ticks {
            let (lx, ly) = polar.point(polar.theta_start, r);
            styled_text(sink, lx + 3.0, ly - 2.0, "start", &label, &theme.axis_text);
        }
        sink.close_layer();
    }
}

fn polar_ring_circle(
    sink: &mut dyn MarkSink,
    polar: &crate::space::Polar,
    r: f64,
    color: &str,
    width: f64,
) {
    sink.circle(
        polar.cx,
        polar.cy,
        r,
        &Paint {
            fill: Fill::None,
            stroke: Stroke::Solid {
                color: color.to_string(),
                width,
            },
            opacity: None,
        },
    );
}

fn polar_ring_polygon(
    sink: &mut dyn MarkSink,
    polar: &crate::space::Polar,
    angles: &[f64],
    r: f64,
    color: &str,
    width: f64,
) {
    let points: Vec<String> = angles
        .iter()
        .map(|a| {
            let (x, y) = polar.point(*a, r);
            format!("{},{}", num(x), num(y))
        })
        .collect();
    sink.polygon(
        &points.join(" "),
        &Paint {
            fill: Fill::None,
            stroke: Stroke::Solid {
                color: color.to_string(),
                width,
            },
            opacity: None,
        },
    );
}

/// Horizontal anchor for a perimeter label, by its position around the circle.
fn perimeter_anchor(angle: f64) -> &'static str {
    let c = angle.cos();
    if c > 0.2 {
        "start"
    } else if c < -0.2 {
        "end"
    } else {
        "middle"
    }
}

fn grid_line(sink: &mut dyn MarkSink, x1: f64, y1: f64, x2: f64, y2: f64, color: &str, width: f64) {
    sink.line(x1, y1, x2, y2, color, width, false, None, None);
}

fn minor_tick_positions(ticks: &[(f64, String)]) -> Vec<f64> {
    ticks
        .windows(2)
        .filter_map(|pair| {
            let a = pair.first()?.0;
            let b = pair.get(1)?.0;
            ((a - b).abs() > f64::EPSILON).then_some((a + b) / 2.0)
        })
        .collect()
}

fn non_overlapping_x_tick_labels(ticks: &[(f64, String)], font_size: f64, angle: f64) -> Vec<bool> {
    if ticks.len() <= 2 {
        return vec![true; ticks.len()];
    }

    const LABEL_GAP: f64 = 4.0;
    const ROTATION_EPSILON: f64 = 1e-3;
    let sin = angle.to_radians().sin().abs();

    // Effective horizontal half-extent used for adjacency testing. A horizontal
    // label collides with its neighbor when their text boxes overlap, so it uses
    // its full text width. A rotated label is a diagonal strand parallel to its
    // neighbors: adjacent strands collide only when the perpendicular gap between
    // baselines (`spacing · sin|θ|`) drops below the text height — the label's
    // *length* never causes adjacent overlap. Reducing that to a horizontal
    // spacing gives a constant `textHeight / sin|θ|`, expressed here as a uniform
    // half-extent so the same greedy selection serves both cases (spec §19.4).
    let half = |label: &str| -> f64 {
        if sin > ROTATION_EPSILON {
            (((font_size + LABEL_GAP) / sin) - LABEL_GAP).max(0.0) / 2.0
        } else {
            estimate_text_width(label, font_size) / 2.0
        }
    };
    let bounds = |index: usize| {
        let h = half(&ticks[index].1);
        (ticks[index].0 - h, ticks[index].0 + h)
    };

    let mut visual_order: Vec<usize> = (0..ticks.len()).collect();
    visual_order.sort_by(|a, b| ticks[*a].0.total_cmp(&ticks[*b].0));

    let mut selected = Vec::new();
    let mut last_right = f64::NEG_INFINITY;
    for index in visual_order.iter().copied() {
        let (left, right) = bounds(index);
        if selected.is_empty() || left >= last_right + LABEL_GAP {
            selected.push(index);
            last_right = right;
        }
    }

    let last_index = visual_order.last().copied().expect("non-empty tick order");
    if selected.last().copied() != Some(last_index) {
        let (last_left, _) = bounds(last_index);
        while selected.len() > 1 {
            let previous = *selected.last().expect("selected tick");
            let (_, previous_right) = bounds(previous);
            if last_left >= previous_right + LABEL_GAP {
                break;
            }
            selected.pop();
        }
        if let Some(previous) = selected.last().copied() {
            let (_, previous_right) = bounds(previous);
            if last_left >= previous_right + LABEL_GAP {
                selected.push(last_index);
            }
        }
    }

    let mut mask = vec![false; ticks.len()];
    for index in selected {
        mask[index] = true;
    }
    mask
}

/// Draw x and y axes with ticks, labels, and titles (spec §19.1–19.4).
///
/// `x_label_override` and `y_label_override` come from `Guide(axis: ..., label: "...")`
/// declarations (spec §19.4).
pub(crate) fn render_axes(
    sink: &mut dyn MarkSink,
    space: &ScaledSpace,
    plot: Rect,
    theme: &Theme,
    options: AxisRenderOptions<'_>,
) {
    sink.open_layer("algraf-axes");
    render_x_axis(sink, space, plot, theme, &options);
    render_y_axis(sink, space, plot, theme, &options);
    sink.close_layer();
}

/// Draw the x axis on the bottom (default) or top edge (spec §19.2). Only guide
/// placement moves; tick positions along the axis are unchanged.
fn render_x_axis(
    sink: &mut dyn MarkSink,
    space: &ScaledSpace,
    plot: Rect,
    theme: &Theme,
    options: &AxisRenderOptions<'_>,
) {
    let top = matches!(options.x_position, AxisPositionIr::Top);
    let axis_y = if top { plot.y } else { plot.bottom() };
    // Tick marks and labels point away from the plot rectangle.
    let tick_dir = if top { -5.0 } else { 5.0 };
    grid_line(
        sink,
        plot.x,
        axis_y,
        plot.right(),
        axis_y,
        &theme.axis_color,
        1.0,
    );
    let x_ticks = space
        .x
        .ticks_formatted(options.x_time_format, options.x_numeric_format);
    let x_angle = options.x_tick_label_angle.unwrap_or(0.0);
    let x_rows = tick_label_row_count(options.x_tick_label_rows);
    let x_label_mask = if x_rows > 1 {
        vec![true; x_ticks.len()]
    } else {
        non_overlapping_x_tick_labels(&x_ticks, theme.axis_text.size, x_angle)
    };
    let row_gap = tick_label_row_gap(theme.axis_text.size);
    for (index, (x, label)) in x_ticks.iter().enumerate() {
        grid_line(
            sink,
            *x,
            axis_y,
            *x,
            axis_y + tick_dir,
            &theme.axis_color,
            1.0,
        );
        // A hidden `axisText` token suppresses tick labels but keeps tick marks
        // and the axis line (spec §20.8).
        if theme.axis_text.hidden || !x_label_mask.get(index).copied().unwrap_or(true) {
            continue;
        }
        let tick_anchor = if x_angle < 0.0 {
            "end"
        } else if x_angle > 0.0 {
            "start"
        } else {
            "middle"
        };
        let row_offset = (index % x_rows) as f64 * row_gap;
        let label_y = if top {
            axis_y - X_TOP_TICK_GAP - row_offset
        } else {
            axis_y + X_TICK_BASELINE + row_offset
        };
        tick_text(
            sink,
            *x,
            label_y,
            tick_anchor,
            label,
            &theme.axis_text,
            x_angle,
        );
    }
    // An override of "" suppresses the axis title (`Guide(axis: x, label: null)`,
    // spec §19.4); a hidden `axisTitle` token suppresses it too (spec §20.8).
    // Ticks and grid are unaffected.
    if options.x_label_override != Some("") && !theme.axis_title.hidden {
        let x_label = options
            .x_label_override
            .map(str::to_string)
            .unwrap_or_else(|| space.x.label());
        let max_label_height = max_x_tick_label_height(
            space,
            theme.axis_text.size,
            options.x_time_format,
            options.x_numeric_format,
            options.x_tick_label_angle,
            options.x_tick_label_rows,
        );
        let title_y = if top {
            plot.y - X_TOP_TICK_GAP - max_label_height.max(theme.axis_title.size) - X_TITLE_GAP
        } else {
            x_axis_title_y(plot.bottom(), max_label_height, theme.axis_title.size)
        };
        styled_text(
            sink,
            plot.x + plot.width / 2.0,
            title_y,
            "middle",
            &x_label,
            &theme.axis_title,
        );
    }
}

/// Draw the y axis on the left (default) or right edge (spec §19.3). Only guide
/// placement moves; tick positions along the axis are unchanged.
fn render_y_axis(
    sink: &mut dyn MarkSink,
    space: &ScaledSpace,
    plot: Rect,
    theme: &Theme,
    options: &AxisRenderOptions<'_>,
) {
    let Some(y) = &space.y else {
        return;
    };
    let right = matches!(options.y_position, AxisPositionIr::Right);
    let axis_x = if right { plot.right() } else { plot.x };
    let tick_label_anchor = if right { "start" } else { "end" };
    // Tick marks point away from the plot. For the left axis we keep the
    // original (outer → inner) coordinate order so default output is byte-stable.
    let (tick_x1, tick_x2) = if right {
        (axis_x, axis_x + 5.0)
    } else {
        (axis_x - 5.0, axis_x)
    };
    grid_line(
        sink,
        axis_x,
        plot.y,
        axis_x,
        plot.bottom(),
        &theme.axis_color,
        1.0,
    );
    let y_rows = tick_label_row_count(options.y_tick_label_rows);
    let row_gap = tick_label_row_gap(theme.axis_text.size);
    for (index, (yp, label)) in y
        .ticks_formatted(options.y_time_format, options.y_numeric_format)
        .iter()
        .enumerate()
    {
        grid_line(sink, tick_x1, *yp, tick_x2, *yp, &theme.axis_color, 1.0);
        // A hidden `axisText` token suppresses tick labels but keeps tick marks
        // and the axis line (spec §20.8).
        if theme.axis_text.hidden {
            continue;
        }
        let row_offset = (index % y_rows) as f64 * row_gap;
        let label_x = if right {
            axis_x + Y_TICK_GAP + row_offset
        } else {
            axis_x - Y_TICK_GAP - row_offset
        };
        tick_text(
            sink,
            label_x,
            *yp + 4.0,
            tick_label_anchor,
            label,
            &theme.axis_text,
            options.y_tick_label_angle.unwrap_or(0.0),
        );
    }
    if options.y_label_override != Some("") && !theme.axis_title.hidden {
        let cy = plot.y + plot.height / 2.0;
        let max_label_width = max_y_tick_label_width(
            space,
            theme.axis_text.size,
            options.y_time_format,
            options.y_numeric_format,
            options.y_tick_label_angle,
            options.y_tick_label_rows,
        );
        let y_label = options
            .y_label_override
            .map(str::to_string)
            .unwrap_or_else(|| y.label());
        // The y-axis title is rotated upright just past the widest tick label,
        // on the same side as the axis.
        let (title_x, rotation) = if right {
            (
                plot.right() + Y_TICK_GAP + max_label_width + Y_TITLE_GAP,
                90.0,
            )
        } else {
            (
                y_axis_title_x(plot.x, max_label_width, theme.axis_title.size),
                -90.0,
            )
        };
        sink.text(&TextRun {
            x: title_x,
            y: cy,
            anchor: TextAnchor::Middle,
            rotate: Some((rotation, title_x, cy)),
            font_family: &theme.axis_title.font_family,
            font_size: theme.axis_title.size,
            font_weight: theme.axis_title.weight,
            font_style: theme.axis_title.style,
            fill: &theme.axis_title.fill,
            opacity: None,
            content: &y_label,
        });
    }
}

/// Gap above the top x-axis line to the tick-label baseline (spec §19.2).
const X_TOP_TICK_GAP: f64 = 8.0;

/// Draw a facet strip label (spec §17.4).
pub(crate) fn render_facet_label(sink: &mut dyn MarkSink, label: &str, area: Rect, theme: &Theme) {
    if area.height <= 0.0 {
        return;
    }
    sink.open_layer("algraf-facet-strip");
    sink.rect(
        area.x,
        area.y,
        area.width,
        area.height,
        &Paint {
            fill: Fill::Color(theme.panel_background.fill.clone()),
            stroke: theme
                .panel_background
                .stroke
                .as_ref()
                .map_or(Stroke::Omit, |color| Stroke::Solid {
                    color: color.clone(),
                    width: theme.panel_background.stroke_width,
                }),
            opacity: None,
        },
    );
    // A hidden `stripText` token suppresses the label while keeping the strip
    // background (spec §20.8).
    if !theme.strip_text.hidden {
        styled_text(
            sink,
            area.x + area.width / 2.0,
            area.y + area.height - 4.0,
            "middle",
            label,
            &theme.strip_text,
        );
    }
    sink.close_layer();
}

fn styled_text(
    sink: &mut dyn MarkSink,
    x: f64,
    y: f64,
    text_anchor: &str,
    content: &str,
    style: &TextStyle,
) {
    sink.text(&TextRun {
        x,
        y,
        anchor: anchor(text_anchor),
        rotate: None,
        font_family: &style.font_family,
        font_size: style.size,
        font_weight: style.weight,
        font_style: style.style,
        fill: &style.fill,
        opacity: None,
        content,
    });
}

fn tick_text(
    sink: &mut dyn MarkSink,
    x: f64,
    y: f64,
    text_anchor: &str,
    content: &str,
    style: &TextStyle,
    angle: f64,
) {
    sink.text(&TextRun {
        x,
        y,
        anchor: anchor(text_anchor),
        rotate: (angle != 0.0).then_some((angle, x, y)),
        font_family: &style.font_family,
        font_size: style.size,
        font_weight: style.weight,
        font_style: style.style,
        fill: &style.fill,
        opacity: None,
        content,
    });
}

/// Draw legends for mapped aesthetics (spec §19.5).
pub(crate) fn render_legends(
    sink: &mut dyn MarkSink,
    legends: &[Legend],
    area: Rect,
    theme: &Theme,
) {
    if legends.is_empty() {
        return;
    }
    sink.open_layer("algraf-legends");
    if matches!(
        theme.legend_position,
        LegendPositionIr::Top | LegendPositionIr::Bottom
    ) {
        render_legends_horizontal(sink, legends, area, theme);
        sink.close_layer();
        return;
    }
    let mut y = area.y + 4.0;
    for legend in legends {
        // A hidden `legendTitle`/`legendText` token suppresses the text while
        // keeping the legend slot and swatches (spec §20.8).
        if !legend.title.is_empty() && !theme.legend_title.hidden {
            styled_text(sink, area.x, y, "start", &legend.title, &theme.legend_title);
        }
        match legend.kind {
            LegendKind::Discrete | LegendKind::Image => {
                if !legend.title.is_empty() {
                    y += theme.legend_title.size + 6.0;
                }
                for (index, (label, color)) in legend.entries.iter().enumerate() {
                    // A merged fill+stroke legend draws each swatch with the
                    // fill color and a stroke outline (spec §19.7).
                    let stroke = match legend.stroke_entries.get(index) {
                        Some(s) => Stroke::Solid {
                            color: s.clone(),
                            width: 2.0,
                        },
                        None => Stroke::Omit,
                    };
                    let paint = Paint {
                        fill: Fill::Color(color.clone()),
                        stroke,
                        opacity: None,
                    };
                    // When the column is also `shape`-mapped, draw the swatch as
                    // that marker glyph so the legend matches the points; the
                    // glyph fills the same 12px box a plain square would occupy
                    // (spec §19.5).
                    if legend.kind == LegendKind::Image {
                        if let Some(image) = legend.images.get(index) {
                            let (width, height) = legend_image_size(
                                image.intrinsic_width,
                                image.intrinsic_height,
                                LEGEND_IMAGE_SWATCH,
                            );
                            sink.image(
                                &image.href,
                                area.x + (LEGEND_IMAGE_SWATCH - width) / 2.0,
                                y - 10.0 + (LEGEND_IMAGE_SWATCH - height) / 2.0,
                                width,
                                height,
                                None,
                            );
                        }
                    } else {
                        match legend.shapes.get(index) {
                            Some(shape) => {
                                emit_marker(sink, *shape, area.x + 6.0, y - 4.0, 6.0, &paint)
                            }
                            None => sink.rect(area.x, y - 10.0, 12.0, 12.0, &paint),
                        }
                    }
                    if !theme.legend_text.hidden {
                        styled_text(sink, area.x + 18.0, y, "start", label, &theme.legend_text);
                    }
                    y += theme.legend_text.size + 6.0;
                }
            }
            LegendKind::Continuous => {
                y += 18.0;
                y = render_continuous_legend(sink, legend, area.x, y, theme);
            }
            LegendKind::Width | LegendKind::Radius => {
                // `render_size_legend` centers each swatch within its own row, so
                // it needs only a small gap below the title; the row's half-height
                // supplies the rest. The fixed 18px discrete gap would double up.
                y += 6.0;
                y = render_size_legend(sink, legend, area.x, y, theme);
            }
        }
        // Separate one legend's content from the next legend's title.
        y += theme.legend_spacing;
    }
    sink.close_layer();
}

fn render_legends_horizontal(
    sink: &mut dyn MarkSink,
    legends: &[Legend],
    area: Rect,
    theme: &Theme,
) {
    let row_height = theme.legend_text.size.max(theme.legend_title.size) + 10.0;
    let mut rows: Vec<(Vec<usize>, f64)> = Vec::new();
    let mut row = Vec::new();
    let mut row_width = 0.0;

    for (index, legend) in legends.iter().enumerate() {
        let width = horizontal_legend_width(legend, theme).min(area.width);
        let next_width = if row.is_empty() {
            width
        } else {
            row_width + theme.legend_spacing + width
        };
        if !row.is_empty() && next_width > area.width {
            rows.push((row, row_width));
            row = vec![index];
            row_width = width;
        } else {
            if !row.is_empty() {
                row_width += theme.legend_spacing;
            }
            row.push(index);
            row_width += width;
        }
    }
    if !row.is_empty() {
        rows.push((row, row_width));
    }

    let mut y = area.y + theme.legend_text.size.max(theme.legend_title.size) + 2.0;
    for (row, row_width) in rows {
        let mut x = area.x + ((area.width - row_width).max(0.0) / 2.0);
        for index in row {
            let legend = &legends[index];
            let width = horizontal_legend_width(legend, theme).min(area.width);
            render_legend_compact(sink, legend, x, y, theme);
            x += width + theme.legend_spacing;
        }
        y += row_height + theme.legend_spacing;
    }
}

fn render_legend_compact(sink: &mut dyn MarkSink, legend: &Legend, x: f64, y: f64, theme: &Theme) {
    let mut cursor = x;
    if !legend.title.is_empty() {
        if !theme.legend_title.hidden {
            styled_text(sink, cursor, y, "start", &legend.title, &theme.legend_title);
        }
        cursor += estimate_text_width(&legend.title, theme.legend_title.size) + 14.0;
    }
    match legend.kind {
        LegendKind::Discrete | LegendKind::Image => {
            for (index, (label, color)) in legend.entries.iter().enumerate() {
                let stroke = match legend.stroke_entries.get(index) {
                    Some(s) => Stroke::Solid {
                        color: s.clone(),
                        width: 2.0,
                    },
                    None => Stroke::Omit,
                };
                let paint = Paint {
                    fill: Fill::Color(color.clone()),
                    stroke,
                    opacity: None,
                };
                if legend.kind == LegendKind::Image {
                    if let Some(image) = legend.images.get(index) {
                        let (width, height) = legend_image_size(
                            image.intrinsic_width,
                            image.intrinsic_height,
                            LEGEND_IMAGE_SWATCH,
                        );
                        sink.image(
                            &image.href,
                            cursor + (LEGEND_IMAGE_SWATCH - width) / 2.0,
                            y - 10.0 + (LEGEND_IMAGE_SWATCH - height) / 2.0,
                            width,
                            height,
                            None,
                        );
                    }
                } else {
                    match legend.shapes.get(index) {
                        Some(shape) => {
                            emit_marker(sink, *shape, cursor + 6.0, y - 4.0, 6.0, &paint)
                        }
                        None => sink.rect(cursor, y - 10.0, 12.0, 12.0, &paint),
                    }
                }
                cursor += 18.0;
                if !theme.legend_text.hidden {
                    styled_text(sink, cursor, y, "start", label, &theme.legend_text);
                }
                cursor += estimate_text_width(label, theme.legend_text.size) + 12.0;
            }
        }
        LegendKind::Continuous => {
            let _ = render_continuous_legend(sink, legend, cursor, y, theme);
        }
        LegendKind::Width | LegendKind::Radius => {
            let _ = render_size_legend(sink, legend, cursor, y - 12.0, theme);
        }
    }
}

const LEGEND_IMAGE_SWATCH: f64 = 12.0;

fn legend_image_size(intrinsic_width: f64, intrinsic_height: f64, max_side: f64) -> (f64, f64) {
    if intrinsic_width >= intrinsic_height {
        (max_side, max_side * intrinsic_height / intrinsic_width)
    } else {
        (max_side * intrinsic_width / intrinsic_height, max_side)
    }
}

/// Draw a size legend whose swatch is a line of the mapped thickness
/// ([`LegendKind::Width`]) or a circle of the mapped radius
/// ([`LegendKind::Radius`]). Swatches share a fixed-width column sized to the
/// largest entry, so labels never overlap the widest swatch, and each row is
/// tall enough for its swatch's full vertical extent — the thickest line or the
/// largest circle's diameter — so swatches never collide vertically (spec
/// §19.5).
fn render_size_legend(
    sink: &mut dyn MarkSink,
    legend: &Legend,
    x: f64,
    mut y: f64,
    theme: &Theme,
) -> f64 {
    const LINE_LEN: f64 = 28.0;
    const ROW_GAP: f64 = 6.0;
    const LABEL_PAD: f64 = 8.0;
    let color = &theme.legend_text.fill;
    let max_mag = legend.sizes.iter().copied().fold(0.0_f64, f64::max);
    // The x where labels start, reserved past the largest swatch so every label
    // clears it. A round-capped line overhangs its endpoints by half its
    // thickness; a circle's right edge sits a full radius past its center.
    let label_x = match legend.kind {
        LegendKind::Radius => x + 2.0 * max_mag + LABEL_PAD,
        _ => x + LINE_LEN + max_mag / 2.0 + LABEL_PAD,
    };
    for (index, (label, _)) in legend.entries.iter().enumerate() {
        let magnitude = legend.sizes.get(index).copied().unwrap_or(0.0);
        // A line's vertical extent is its thickness; a circle's is its diameter.
        let extent = match legend.kind {
            LegendKind::Radius => 2.0 * magnitude,
            _ => magnitude,
        };
        let row_height = (extent + ROW_GAP).max(18.0);
        let center = y + row_height / 2.0;
        match legend.kind {
            LegendKind::Width if magnitude > 0.0 => {
                sink.line(
                    x,
                    center,
                    x + LINE_LEN,
                    center,
                    color,
                    magnitude,
                    true,
                    None,
                    None,
                );
            }
            LegendKind::Radius if magnitude > 0.0 => {
                // Center every circle on a common vertical axis through the
                // swatch column so the stack reads as concentric sizes.
                sink.circle(
                    x + max_mag,
                    center,
                    magnitude,
                    &Paint::fill(color.clone(), None),
                );
            }
            _ => {}
        }
        styled_text(
            sink,
            label_x,
            center + 4.0,
            "start",
            label,
            &theme.legend_text,
        );
        y += row_height;
    }
    y
}

fn render_continuous_legend(
    sink: &mut dyn MarkSink,
    legend: &Legend,
    x: f64,
    y: f64,
    theme: &Theme,
) -> f64 {
    if legend.entries.is_empty() {
        return y;
    }
    let step = 16.0;
    for (index, (label, color)) in legend.entries.iter().rev().enumerate() {
        let y0 = y + index as f64 * step;
        sink.rect(x, y0 - 10.0, 12.0, step, &Paint::fill(color.clone(), None));
        styled_text(sink, x + 18.0, y0, "start", label, &theme.legend_text);
    }
    y + legend.entries.len() as f64 * step
}
