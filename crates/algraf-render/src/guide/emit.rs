//! Guide emission: writes grid lines, axes, facet strips, and legends from
//! trained scales and the planning results in [`super::plan`] (spec §19).
//!
//! Emission goes through the backend-neutral [`MarkSink`] seam (spec §24.6), so
//! the SVG and draw-list backends agree on guide coordinates and colors.

use algraf_semantics::{GridShapeIr, GuideIr, TemporalFormatIr};

use crate::aes::{Legend, LegendKind};
use crate::layout::Rect;
use crate::render::TextAnchor;
use crate::sink::{Fill, MarkSink, Paint, Stroke, TextRun};
use crate::space::ScaledSpace;
use crate::svg::num;
use crate::theme::Theme;

use super::plan::{
    max_x_tick_label_height, max_y_tick_label_width, rotated_text_size, x_axis_title_y,
    y_axis_title_x,
};

pub(crate) struct AxisRenderOptions<'a> {
    pub(crate) x_label_override: Option<&'a str>,
    pub(crate) y_label_override: Option<&'a str>,
    pub(crate) x_time_format: Option<&'a TemporalFormatIr>,
    pub(crate) y_time_format: Option<&'a TemporalFormatIr>,
    pub(crate) x_tick_label_angle: Option<f64>,
    pub(crate) y_tick_label_angle: Option<f64>,
}

/// Map a guide anchor string to a [`TextAnchor`].
fn anchor(value: &str) -> TextAnchor {
    match value {
        "start" => TextAnchor::Start,
        "end" => TextAnchor::End,
        _ => TextAnchor::Middle,
    }
}

/// Draw grid lines behind the data marks (spec §17.6). Only continuous and
/// temporal axes get grid lines; categorical axes do not.
pub(crate) fn render_grid(sink: &mut dyn MarkSink, space: &ScaledSpace, plot: Rect, theme: &Theme) {
    if !theme.grid {
        return;
    }
    sink.open_layer("algraf-grid");
    let color = &theme.grid_major_color;
    let width = theme.grid_major_width;
    if !space.x.is_band() {
        for (x, _) in space.x.ticks() {
            grid_line(sink, x, plot.y, x, plot.bottom(), color, width);
        }
    }
    if let Some(y) = &space.y {
        if !y.is_band() {
            for (yp, _) in y.ticks() {
                grid_line(sink, plot.x, yp, plot.right(), yp, color, width);
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
    let color = &theme.grid_major_color;
    let width = theme.grid_major_width;
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
            polar_label(sink, lx, ly, perimeter_anchor(*angle), label, theme);
        }
        sink.close_layer();
    }

    // Radius labels along the top spoke.
    let radius_ticks = space.polar_radius_ticks();
    if !radius_ticks.is_empty() {
        sink.open_layer("algraf-polar-radius-labels");
        for (r, label) in radius_ticks {
            let (lx, ly) = polar.point(crate::space::THETA_START, r);
            polar_label(sink, lx + 3.0, ly - 2.0, "start", &label, theme);
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

fn polar_label(
    sink: &mut dyn MarkSink,
    x: f64,
    y: f64,
    label_anchor: &str,
    content: &str,
    theme: &Theme,
) {
    sink.text(&TextRun {
        x,
        y,
        anchor: anchor(label_anchor),
        rotate: None,
        font_family: &theme.font_family,
        font_size: theme.font_size,
        fill: &theme.text_color,
        opacity: None,
        content,
    });
}

fn grid_line(sink: &mut dyn MarkSink, x1: f64, y1: f64, x2: f64, y2: f64, color: &str, width: f64) {
    sink.line(x1, y1, x2, y2, color, width, false, None, None);
}

fn non_overlapping_x_tick_labels(ticks: &[(f64, String)], font_size: f64, angle: f64) -> Vec<bool> {
    if ticks.len() <= 2 {
        return vec![true; ticks.len()];
    }

    const LABEL_GAP: f64 = 4.0;
    let mut selected = Vec::new();
    let mut last_right = f64::NEG_INFINITY;
    for (index, (x, label)) in ticks.iter().enumerate() {
        let (left, right) = x_tick_label_bounds(*x, label, font_size, angle);
        if selected.is_empty() || left >= last_right + LABEL_GAP {
            selected.push(index);
            last_right = right;
        }
    }

    let last_index = ticks.len() - 1;
    if selected.last().copied() != Some(last_index) {
        let (last_left, _) =
            x_tick_label_bounds(ticks[last_index].0, &ticks[last_index].1, font_size, angle);
        while selected.len() > 1 {
            let previous = *selected.last().expect("selected tick");
            let (_, previous_right) =
                x_tick_label_bounds(ticks[previous].0, &ticks[previous].1, font_size, angle);
            if last_left >= previous_right + LABEL_GAP {
                break;
            }
            selected.pop();
        }
        if let Some(previous) = selected.last().copied() {
            let (_, previous_right) =
                x_tick_label_bounds(ticks[previous].0, &ticks[previous].1, font_size, angle);
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

fn x_tick_label_bounds(x: f64, label: &str, font_size: f64, angle: f64) -> (f64, f64) {
    let width = rotated_text_size(label, font_size, angle).0;
    (x - width / 2.0, x + width / 2.0)
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

    // X axis along the bottom.
    grid_line(
        sink,
        plot.x,
        plot.bottom(),
        plot.right(),
        plot.bottom(),
        &theme.axis_color,
        1.0,
    );
    let x_ticks = space.x.ticks_with_format(options.x_time_format);
    let x_angle = options.x_tick_label_angle.unwrap_or(0.0);
    let x_label_mask = non_overlapping_x_tick_labels(&x_ticks, theme.font_size, x_angle);
    for (index, (x, label)) in x_ticks.iter().enumerate() {
        grid_line(
            sink,
            *x,
            plot.bottom(),
            *x,
            plot.bottom() + 5.0,
            &theme.axis_color,
            1.0,
        );
        if !x_label_mask.get(index).copied().unwrap_or(true) {
            continue;
        }
        let tick_anchor = if x_angle < 0.0 {
            "end"
        } else if x_angle > 0.0 {
            "start"
        } else {
            "middle"
        };
        tick_text(
            sink,
            *x,
            plot.bottom() + super::plan::X_TICK_BASELINE,
            tick_anchor,
            label,
            theme,
            x_angle,
        );
    }
    // An override of "" suppresses the axis title (`Guide(axis: x, label: null)`,
    // spec §19.x); ticks and grid are unaffected.
    let x_label = options
        .x_label_override
        .map(str::to_string)
        .unwrap_or_else(|| space.x.label());
    if options.x_label_override != Some("") {
        let max_label_height = max_x_tick_label_height(
            space,
            theme.font_size,
            options.x_time_format,
            options.x_tick_label_angle,
        );
        text(
            sink,
            plot.x + plot.width / 2.0,
            x_axis_title_y(plot.bottom(), max_label_height, theme.font_size),
            "middle",
            &x_label,
            theme,
        );
    }

    // Y axis along the left.
    if let Some(y) = &space.y {
        grid_line(
            sink,
            plot.x,
            plot.y,
            plot.x,
            plot.bottom(),
            &theme.axis_color,
            1.0,
        );
        for (yp, label) in y.ticks_with_format(options.y_time_format) {
            grid_line(sink, plot.x - 5.0, yp, plot.x, yp, &theme.axis_color, 1.0);
            tick_text(
                sink,
                plot.x - 8.0,
                yp + 4.0,
                "end",
                &label,
                theme,
                options.y_tick_label_angle.unwrap_or(0.0),
            );
        }
        let cy = plot.y + plot.height / 2.0;
        let max_label_width = max_y_tick_label_width(
            space,
            theme.font_size,
            options.y_time_format,
            options.y_tick_label_angle,
        );
        let label_x = y_axis_title_x(plot.x, max_label_width, theme.font_size);
        let y_label = options
            .y_label_override
            .map(str::to_string)
            .unwrap_or_else(|| y.label());
        if options.y_label_override != Some("") {
            // The y-axis title is rotated upright along the left edge.
            sink.text(&TextRun {
                x: label_x,
                y: cy,
                anchor: TextAnchor::Middle,
                rotate: Some((-90.0, label_x, cy)),
                font_family: &theme.font_family,
                font_size: theme.font_size,
                fill: &theme.text_color,
                opacity: None,
                content: &y_label,
            });
        }
    }

    sink.close_layer();
}

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
        &Paint::fill(theme.plot_background.clone(), None),
    );
    text(
        sink,
        area.x + area.width / 2.0,
        area.y + area.height - 4.0,
        "middle",
        label,
        theme,
    );
    sink.close_layer();
}

fn text(sink: &mut dyn MarkSink, x: f64, y: f64, text_anchor: &str, content: &str, theme: &Theme) {
    sink.text(&TextRun {
        x,
        y,
        anchor: anchor(text_anchor),
        rotate: None,
        font_family: &theme.font_family,
        font_size: theme.font_size,
        fill: &theme.text_color,
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
    theme: &Theme,
    angle: f64,
) {
    sink.text(&TextRun {
        x,
        y,
        anchor: anchor(text_anchor),
        rotate: (angle != 0.0).then_some((angle, x, y)),
        font_family: &theme.font_family,
        font_size: theme.font_size,
        fill: &theme.text_color,
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
    let mut y = area.y + 4.0;
    for legend in legends {
        if !legend.title.is_empty() {
            text(sink, area.x, y, "start", &legend.title, theme);
        }
        match legend.kind {
            LegendKind::Discrete => {
                if !legend.title.is_empty() {
                    y += 18.0;
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
                    sink.rect(
                        area.x,
                        y - 10.0,
                        12.0,
                        12.0,
                        &Paint {
                            fill: Fill::Color(color.clone()),
                            stroke,
                            opacity: None,
                        },
                    );
                    text(sink, area.x + 18.0, y, "start", label, theme);
                    y += 18.0;
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
        y += 16.0;
    }
    sink.close_layer();
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
    let color = &theme.text_color;
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
        text(sink, label_x, center + 4.0, "start", label, theme);
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
        text(sink, x + 18.0, y0, "start", label, theme);
    }
    y + legend.entries.len() as f64 * step
}
