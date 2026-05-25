//! Axis, grid, and legend rendering (spec §19).

use crate::aes::{Legend, LegendKind};
use crate::layout::Rect;
use crate::space::ScaledSpace;
use crate::svg::{escape_attr, escape_text, num, SvgWriter};
use crate::theme::Theme;

/// Draw grid lines behind the data marks (spec §17.6). Only continuous and
/// temporal axes get grid lines; categorical axes do not.
pub fn render_grid(w: &mut SvgWriter, space: &ScaledSpace, plot: Rect, theme: &Theme) {
    if !theme.grid {
        return;
    }
    w.open_group("class=\"algraf-grid\"");
    let color = &theme.grid_major_color;
    let width = theme.grid_major_width;
    if !space.x.is_band() {
        for (x, _) in space.x.ticks() {
            w.line(&grid_line(x, plot.y, x, plot.bottom(), color, width));
        }
    }
    if let Some(y) = &space.y {
        if !y.is_band() {
            for (yp, _) in y.ticks() {
                w.line(&grid_line(plot.x, yp, plot.right(), yp, color, width));
            }
        }
    }
    w.close_group();
}

fn grid_line(x1: f64, y1: f64, x2: f64, y2: f64, color: &str, width: f64) -> String {
    format!(
        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" />",
        num(x1),
        num(y1),
        num(x2),
        num(y2),
        escape_attr(color),
        num(width),
    )
}

/// Gap between the plot edge and the right edge of the y tick labels.
pub(crate) const Y_TICK_GAP: f64 = 8.0;
/// Gap between the left edge of the y tick labels and the rotated axis title.
pub(crate) const Y_TITLE_GAP: f64 = 6.0;

/// A coarse per-glyph width estimate for layout reservations. We have no font
/// metrics at render time, so approximate every glyph as `0.6 * font_size`,
/// which is a safe-ish upper bound for the digits and short words that appear
/// in tick labels and axis titles.
pub(crate) fn estimate_text_width(text: &str, font_size: f64) -> f64 {
    text.chars().count() as f64 * font_size * 0.6
}

/// The x coordinate for the (rotated) y-axis title, placed just left of the
/// widest tick label. Clamped so the title never runs off the left edge.
pub(crate) fn y_axis_title_x(plot_x: f64, max_label_width: f64, font_size: f64) -> f64 {
    (plot_x - Y_TICK_GAP - max_label_width - Y_TITLE_GAP).max(font_size)
}

/// The left margin a y axis needs so its tick labels and rotated title both
/// fit without overlapping. Compared against the default margin to decide how
/// much extra room to reserve.
pub(crate) fn y_axis_left_margin(max_label_width: f64, font_size: f64) -> f64 {
    font_size + Y_TICK_GAP + max_label_width + Y_TITLE_GAP
}

/// The widest y tick label width for a scaled space, or 0.0 when there is no
/// continuous y axis to label.
pub(crate) fn max_y_tick_label_width(space: &ScaledSpace, font_size: f64) -> f64 {
    let Some(y) = &space.y else {
        return 0.0;
    };
    y.ticks()
        .iter()
        .map(|(_, label)| estimate_text_width(label, font_size))
        .fold(0.0_f64, f64::max)
}

/// Draw x and y axes with ticks, labels, and titles (spec §19.1–19.4).
///
/// `x_label_override` and `y_label_override` come from `Guide(axis: ..., label: "...")`
/// declarations (spec §19.4).
pub fn render_axes(
    w: &mut SvgWriter,
    space: &ScaledSpace,
    plot: Rect,
    theme: &Theme,
    x_label_override: Option<&str>,
    y_label_override: Option<&str>,
) {
    w.open_group("class=\"algraf-axes\"");

    // X axis along the bottom.
    w.line(&grid_line(
        plot.x,
        plot.bottom(),
        plot.right(),
        plot.bottom(),
        &theme.axis_color,
        1.0,
    ));
    for (x, label) in space.x.ticks() {
        w.line(&grid_line(
            x,
            plot.bottom(),
            x,
            plot.bottom() + 5.0,
            &theme.axis_color,
            1.0,
        ));
        w.line(&text(
            x,
            plot.bottom() + 18.0,
            x_tick_label_anchor(x, plot),
            &label,
            theme,
        ));
    }
    // An override of "" suppresses the axis title (`Guide(axis: x, label: null)`,
    // spec §19.x); ticks and grid are unaffected.
    let x_label = x_label_override
        .map(str::to_string)
        .unwrap_or_else(|| space.x.label());
    if x_label_override != Some("") {
        w.line(&text(
            plot.x + plot.width / 2.0,
            plot.bottom() + 38.0,
            "middle",
            &x_label,
            theme,
        ));
    }

    // Y axis along the left.
    if let Some(y) = &space.y {
        w.line(&grid_line(
            plot.x,
            plot.y,
            plot.x,
            plot.bottom(),
            &theme.axis_color,
            1.0,
        ));
        for (yp, label) in y.ticks() {
            w.line(&grid_line(
                plot.x - 5.0,
                yp,
                plot.x,
                yp,
                &theme.axis_color,
                1.0,
            ));
            w.line(&text(plot.x - 8.0, yp + 4.0, "end", &label, theme));
        }
        let cy = plot.y + plot.height / 2.0;
        let max_label_width = max_y_tick_label_width(space, theme.font_size);
        let label_x = y_axis_title_x(plot.x, max_label_width, theme.font_size);
        let y_label = y_label_override
            .map(str::to_string)
            .unwrap_or_else(|| y.label());
        if y_label_override != Some("") {
            w.line(&format!(
                "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" transform=\"rotate(-90 {} {})\" \
                 font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
                num(label_x),
                num(cy),
                num(label_x),
                num(cy),
                escape_attr(&theme.font_family),
                num(theme.font_size),
                escape_attr(&theme.text_color),
                escape_text(&y_label),
            ));
        }
    }

    w.close_group();
}

/// Draw a facet strip label (spec §17.4).
pub fn render_facet_label(w: &mut SvgWriter, label: &str, area: Rect, theme: &Theme) {
    if area.height <= 0.0 {
        return;
    }
    w.open_group("class=\"algraf-facet-strip\"");
    w.line(&format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" />",
        num(area.x),
        num(area.y),
        num(area.width),
        num(area.height),
        escape_attr(&theme.plot_background),
    ));
    w.line(&text(
        area.x + area.width / 2.0,
        area.y + area.height - 4.0,
        "middle",
        label,
        theme,
    ));
    w.close_group();
}

fn text(x: f64, y: f64, anchor: &str, content: &str, theme: &Theme) -> String {
    format!(
        "<text x=\"{}\" y=\"{}\" text-anchor=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
        num(x),
        num(y),
        anchor,
        escape_attr(&theme.font_family),
        num(theme.font_size),
        escape_attr(&theme.text_color),
        escape_text(content),
    )
}

fn x_tick_label_anchor(x: f64, plot: Rect) -> &'static str {
    const EPSILON: f64 = 1e-6;
    if x <= plot.x + EPSILON {
        "start"
    } else if x >= plot.right() - EPSILON {
        "end"
    } else {
        "middle"
    }
}

/// Draw legends for mapped aesthetics (spec §19.5).
pub fn render_legends(w: &mut SvgWriter, legends: &[Legend], area: Rect, theme: &Theme) {
    if legends.is_empty() {
        return;
    }
    w.open_group("class=\"algraf-legends\"");
    let mut y = area.y + 4.0;
    for legend in legends {
        w.line(&text(area.x, y, "start", &legend.title, theme));
        match legend.kind {
            LegendKind::Discrete => {
                y += 18.0;
                for (index, (label, color)) in legend.entries.iter().enumerate() {
                    // A merged fill+stroke legend draws each swatch with the
                    // fill color and a stroke outline (spec §19.7).
                    let stroke_attr = legend
                        .stroke_entries
                        .get(index)
                        .map(|s| format!(" stroke=\"{}\" stroke-width=\"2\"", escape_attr(s)))
                        .unwrap_or_default();
                    w.line(&format!(
                        "<rect x=\"{}\" y=\"{}\" width=\"12\" height=\"12\" fill=\"{}\"{} />",
                        num(area.x),
                        num(y - 10.0),
                        escape_attr(color),
                        stroke_attr,
                    ));
                    w.line(&text(area.x + 18.0, y, "start", label, theme));
                    y += 18.0;
                }
            }
            LegendKind::Continuous => {
                y += 18.0;
                y = render_continuous_legend(w, legend, area.x, y, theme);
            }
            LegendKind::Width | LegendKind::Radius => {
                // `render_size_legend` centers each swatch within its own row, so
                // it needs only a small gap below the title; the row's half-height
                // supplies the rest. The fixed 18px discrete gap would double up.
                y += 6.0;
                y = render_size_legend(w, legend, area.x, y, theme);
            }
        }
        // Separate one legend's content from the next legend's title.
        y += 16.0;
    }
    w.close_group();
}

/// Draw a size legend whose swatch is a line of the mapped thickness
/// ([`LegendKind::Width`]) or a circle of the mapped radius
/// ([`LegendKind::Radius`]). Swatches share a fixed-width column sized to the
/// largest entry, so labels never overlap the widest swatch, and each row is
/// tall enough for its swatch's full vertical extent — the thickest line or the
/// largest circle's diameter — so swatches never collide vertically (spec
/// §19.5).
fn render_size_legend(
    w: &mut SvgWriter,
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
                w.line(&format!(
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" />",
                    num(x),
                    num(center),
                    num(x + LINE_LEN),
                    num(center),
                    escape_attr(color),
                    num(magnitude),
                ));
            }
            LegendKind::Radius if magnitude > 0.0 => {
                // Center every circle on a common vertical axis through the
                // swatch column so the stack reads as concentric sizes.
                w.line(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" />",
                    num(x + max_mag),
                    num(center),
                    num(magnitude),
                    escape_attr(color),
                ));
            }
            _ => {}
        }
        w.line(&text(label_x, center + 4.0, "start", label, theme));
        y += row_height;
    }
    y
}

fn render_continuous_legend(
    w: &mut SvgWriter,
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
        w.line(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"12\" height=\"{}\" fill=\"{}\" />",
            num(x),
            num(y0 - 10.0),
            num(step),
            escape_attr(color),
        ));
        w.line(&text(x + 18.0, y0, "start", label, theme));
    }
    y + legend.entries.len() as f64 * step
}
