//! Guide planning: tick-label measurement and axis-margin reservations derived
//! from trained scales (spec §17.3, §19). Pure geometry — nothing here writes
//! SVG; [`super::emit`] consumes these results.

use algraf_semantics::TemporalFormatIr;

use crate::space::ScaledSpace;

/// Gap between the plot edge and the right edge of the y tick labels.
pub(crate) const Y_TICK_GAP: f64 = 8.0;
/// Gap between the left edge of the y tick labels and the rotated axis title.
pub(crate) const Y_TITLE_GAP: f64 = 6.0;
/// Baseline offset for x tick labels below the plot edge.
pub(crate) const X_TICK_BASELINE: f64 = 18.0;
/// Gap between x tick labels and the x-axis title.
pub(crate) const X_TITLE_GAP: f64 = 8.0;

/// A coarse per-glyph width estimate for layout reservations. We have no font
/// metrics at render time, so approximate every glyph as `0.6 * font_size`,
/// which is a safe-ish upper bound for the digits and short words that appear
/// in tick labels and axis titles.
pub(crate) fn estimate_text_width(text: &str, font_size: f64) -> f64 {
    text.chars().count() as f64 * font_size * 0.6
}

pub(crate) fn rotated_text_size(text: &str, font_size: f64, angle: f64) -> (f64, f64) {
    let width = estimate_text_width(text, font_size);
    let height = font_size;
    let radians = angle.to_radians();
    let sin = radians.sin().abs();
    let cos = radians.cos().abs();
    (width * cos + height * sin, width * sin + height * cos)
}

/// The y coordinate for the x-axis title, placed below the tallest tick label.
pub(crate) fn x_axis_title_y(plot_bottom: f64, max_label_height: f64, font_size: f64) -> f64 {
    plot_bottom + X_TICK_BASELINE + max_label_height.max(font_size) + X_TITLE_GAP
}

/// The bottom margin an x axis needs so its tick labels and title fit.
pub(crate) fn x_axis_bottom_margin(max_label_height: f64, font_size: f64) -> f64 {
    X_TICK_BASELINE + max_label_height.max(font_size) + X_TITLE_GAP + font_size
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
pub(crate) fn max_y_tick_label_width(
    space: &ScaledSpace,
    font_size: f64,
    format: Option<&TemporalFormatIr>,
    angle: Option<f64>,
) -> f64 {
    let Some(y) = &space.y else {
        return 0.0;
    };
    let angle = angle.unwrap_or(0.0);
    y.ticks_with_format(format)
        .iter()
        .map(|(_, label)| rotated_text_size(label, font_size, angle).0)
        .fold(0.0_f64, f64::max)
}

/// The tallest x tick label for a scaled space, accounting for rotation.
pub(crate) fn max_x_tick_label_height(
    space: &ScaledSpace,
    font_size: f64,
    format: Option<&TemporalFormatIr>,
    angle: Option<f64>,
) -> f64 {
    let angle = angle.unwrap_or(0.0);
    space
        .x
        .ticks_with_format(format)
        .iter()
        .map(|(_, label)| rotated_text_size(label, font_size, angle).1)
        .fold(0.0_f64, f64::max)
}
