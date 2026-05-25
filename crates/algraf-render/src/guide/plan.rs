//! Guide planning: tick-label measurement and axis-margin reservations derived
//! from trained scales (spec §17.3, §19). Pure geometry — nothing here writes
//! SVG; [`super::emit`] consumes these results.

use crate::layout::Rect;
use crate::space::ScaledSpace;

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

/// Where an x tick label anchors: labels at the plot edges anchor inward so they
/// stay inside the plot box; interior labels center on the tick.
pub(crate) fn x_tick_label_anchor(x: f64, plot: Rect) -> &'static str {
    const EPSILON: f64 = 1e-6;
    if x <= plot.x + EPSILON {
        "start"
    } else if x >= plot.right() - EPSILON {
        "end"
    } else {
        "middle"
    }
}
