//! Guide planning: tick-label measurement and axis-margin reservations derived
//! from trained scales (spec §17.3, §19). Pure geometry — nothing here writes
//! SVG; [`super::emit`] consumes these results.

use algraf_semantics::{LegendPositionIr, TemporalFormatIr};

use crate::aes::{Legend, LegendKind};
use crate::layout::LegendSize;
use crate::space::ScaledSpace;
use crate::theme::Theme;

/// Gap between the plot edge and the right edge of the y tick labels.
pub(crate) const Y_TICK_GAP: f64 = 8.0;
/// Gap between the left edge of the y tick labels and the rotated axis title.
pub(crate) const Y_TITLE_GAP: f64 = 12.0;
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
    rows: Option<usize>,
) -> f64 {
    let Some(y) = &space.y else {
        return 0.0;
    };
    let angle = angle.unwrap_or(0.0);
    let width = y
        .ticks_with_format(format)
        .iter()
        .map(|(_, label)| rotated_text_size(label, font_size, angle).0)
        .fold(0.0_f64, f64::max);
    width + row_offset_extent(rows, font_size)
}

/// The tallest x tick label for a scaled space, accounting for rotation.
pub(crate) fn max_x_tick_label_height(
    space: &ScaledSpace,
    font_size: f64,
    format: Option<&TemporalFormatIr>,
    angle: Option<f64>,
    rows: Option<usize>,
) -> f64 {
    let angle = angle.unwrap_or(0.0);
    let height = space
        .x
        .ticks_with_format(format)
        .iter()
        .map(|(_, label)| rotated_text_size(label, font_size, angle).1)
        .fold(0.0_f64, f64::max);
    height + row_offset_extent(rows, font_size)
}

pub(crate) fn tick_label_row_count(rows: Option<usize>) -> usize {
    rows.unwrap_or(1).clamp(1, 8)
}

pub(crate) fn tick_label_row_gap(font_size: f64) -> f64 {
    font_size + 4.0
}

/// Estimate the legend content box before final layout. Right/left legends need
/// measured width; top/bottom legends need measured height after wrapping.
pub(crate) fn legend_size(
    legends: &[Legend],
    theme: &Theme,
    position: LegendPositionIr,
    available_width: f64,
) -> LegendSize {
    if matches!(position, LegendPositionIr::Top | LegendPositionIr::Bottom) {
        horizontal_legend_size(legends, theme, available_width)
    } else {
        vertical_legend_size(legends, theme)
    }
}

pub(crate) fn horizontal_legend_width(legend: &Legend, theme: &Theme) -> f64 {
    let title = if legend.title.is_empty() {
        0.0
    } else {
        estimate_text_width(&legend.title, theme.legend_title.size) + 14.0
    };
    let entries = match legend.kind {
        LegendKind::Discrete | LegendKind::Image => legend
            .entries
            .iter()
            .map(|(label, _)| 18.0 + estimate_text_width(label, theme.legend_text.size) + 12.0)
            .sum::<f64>(),
        LegendKind::Continuous => 18.0 + max_entry_label_width(legend, theme) + 12.0,
        LegendKind::Width | LegendKind::Radius => size_legend_width(legend, theme) + 12.0,
    };
    (title + entries).max(80.0)
}

fn horizontal_legend_size(legends: &[Legend], theme: &Theme, available_width: f64) -> LegendSize {
    let row_height = theme.legend_text.size.max(theme.legend_title.size) + 10.0;
    let available_width = available_width.max(1.0);
    let mut rows = 0usize;
    let mut row_width = 0.0;

    for legend in legends {
        let width = horizontal_legend_width(legend, theme).min(available_width);
        let next_width = if row_width == 0.0 {
            width
        } else {
            row_width + theme.legend_spacing + width
        };
        if row_width > 0.0 && next_width > available_width {
            rows += 1;
            row_width = width;
        } else {
            if row_width > 0.0 {
                row_width += theme.legend_spacing;
            }
            row_width += width;
        }
    }
    if row_width > 0.0 {
        rows += 1;
    }

    LegendSize {
        width: available_width,
        height: rows as f64 * (row_height + theme.legend_spacing),
    }
}

fn vertical_legend_size(legends: &[Legend], theme: &Theme) -> LegendSize {
    let mut width = 0.0_f64;
    let mut height = 4.0_f64;
    for legend in legends {
        if !legend.title.is_empty() {
            width = width.max(estimate_text_width(&legend.title, theme.legend_title.size));
            height += theme.legend_title.size + 6.0;
        }
        match legend.kind {
            LegendKind::Discrete | LegendKind::Image => {
                for (label, _) in &legend.entries {
                    width = width.max(18.0 + estimate_text_width(label, theme.legend_text.size));
                    height += theme.legend_text.size + 6.0;
                }
            }
            LegendKind::Continuous => {
                width = width.max(18.0 + max_entry_label_width(legend, theme));
                height += 18.0 + legend.entries.len() as f64 * 16.0;
            }
            LegendKind::Width | LegendKind::Radius => {
                let (size_width, size_height) = size_legend_metrics(legend, theme);
                width = width.max(size_width);
                height += 6.0 + size_height;
            }
        }
        height += theme.legend_spacing;
    }

    LegendSize {
        width: width + 4.0,
        height,
    }
}

fn max_entry_label_width(legend: &Legend, theme: &Theme) -> f64 {
    legend
        .entries
        .iter()
        .map(|(label, _)| estimate_text_width(label, theme.legend_text.size))
        .fold(0.0_f64, f64::max)
}

fn size_legend_width(legend: &Legend, theme: &Theme) -> f64 {
    size_legend_metrics(legend, theme).0
}

fn size_legend_metrics(legend: &Legend, theme: &Theme) -> (f64, f64) {
    const LINE_LEN: f64 = 28.0;
    const ROW_GAP: f64 = 6.0;
    const LABEL_PAD: f64 = 8.0;

    let max_mag = legend.sizes.iter().copied().fold(0.0_f64, f64::max);
    let label_x = match legend.kind {
        LegendKind::Radius => 2.0 * max_mag + LABEL_PAD,
        _ => LINE_LEN + max_mag / 2.0 + LABEL_PAD,
    };
    let mut width = 0.0_f64;
    let mut height = 0.0_f64;
    for (index, (label, _)) in legend.entries.iter().enumerate() {
        let magnitude = legend.sizes.get(index).copied().unwrap_or(0.0);
        let extent = match legend.kind {
            LegendKind::Radius => 2.0 * magnitude,
            _ => magnitude,
        };
        height += (extent + ROW_GAP).max(18.0);
        width = width.max(label_x + estimate_text_width(label, theme.legend_text.size));
    }
    (width, height)
}

fn row_offset_extent(rows: Option<usize>, font_size: f64) -> f64 {
    (tick_label_row_count(rows).saturating_sub(1)) as f64 * tick_label_row_gap(font_size)
}
