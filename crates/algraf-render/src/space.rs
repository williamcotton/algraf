//! Trained spatial context built from a frame IR (spec §16.12, §16.1).
//!
//! [`ScaledSpace`] hides whether a position scale is continuous, temporal,
//! banded, or nested: geometries call `resolve_x`/`resolve_y` and the bandwidth
//! accessors without knowing the underlying scale kind.

use std::f64::consts::PI;

use algraf_data::{DataType, Table, TemporalPrecision};
use algraf_semantics::{
    AxisSelectorIr, ColumnRef, FrameIr, PolarDirectionIr, PolarThetaIr, ScaleIr, ScaleTargetIr,
    ScaleTypeIr, TemporalFormatIr,
};
use chrono::{DateTime, Datelike, NaiveDate};

use crate::domains::{AxisDomainHints, SpaceDomainHints};
use crate::guide::estimate_text_width;
use crate::layout::Rect;
use crate::projection::SpatialScale;
use crate::scale::{
    categorical_domain, cell_category, cell_f64, cell_micros, numeric_domain, temporal_domain,
    BandScale, ContinuousScale, NestedBandScale, TemporalScale,
};

/// The default polar angular origin: the 12-o'clock position. `θ = -π/2` is the
/// top; increasing `θ` moves clockwise in screen coordinates (where +y points
/// down). A space MAY rotate this origin (`startAngle`) and reverse the sweep
/// (`direction`) — see [`polar_angular_range`] (spec §16.16).
pub(crate) const THETA_ORIGIN: f64 = -PI / 2.0;

/// Compute the `(start, end)` angular range a polar theta axis maps into, from a
/// `start_angle` (degrees, clockwise from 12 o'clock) and a sweep direction
/// (spec §16.16). The defaults (`0`, clockwise) yield `(-π/2, 3π/2)`,
/// reproducing the fixed behavior of earlier versions.
pub(crate) fn polar_angular_range(start_angle: f64, direction: PolarDirectionIr) -> (f64, f64) {
    let start = THETA_ORIGIN + start_angle.to_radians();
    let full = 2.0 * PI;
    match direction {
        PolarDirectionIr::Clockwise => (start, start + full),
        PolarDirectionIr::CounterClockwise => (start, start - full),
    }
}

/// Radial gap (px) between the outer radius and the baseline of a perimeter
/// category label (spec §19). The polar plot reserves this plus the widest
/// label so the labels stay within the plot rect; `render_polar_grid` places
/// labels at the same offset.
pub(crate) const POLAR_LABEL_GAP: f64 = 12.0;

/// A trained polar coordinate transform for a space (spec §16.16). The `theta`
/// axis maps its domain to `[THETA_START, THETA_END]` and the radius axis maps
/// its domain to `[r_inner, r_outer]`; final pixel positions come from
/// [`Polar::point`].
#[derive(Debug, Clone, Copy)]
pub struct Polar {
    pub cx: f64,
    pub cy: f64,
    pub r_inner: f64,
    pub r_outer: f64,
    pub theta: PolarThetaIr,
    /// The angle (radians) the theta-domain minimum maps to.
    pub theta_start: f64,
    /// The angle (radians) the theta-domain maximum maps to. May be less than
    /// `theta_start` for a counterclockwise sweep.
    pub theta_end: f64,
}

impl Polar {
    /// Convert a `(θ, r)` polar coordinate to a Cartesian pixel position:
    /// `x = cx + r·cos(θ)`, `y = cy + r·sin(θ)` (spec §16.16).
    pub fn point(&self, theta: f64, r: f64) -> (f64, f64) {
        (self.cx + r * theta.cos(), self.cy + r * theta.sin())
    }

    /// Clamp a radius to the drawable annulus `[r_inner, r_outer]`.
    pub fn clamp_radius(&self, r: f64) -> f64 {
        r.clamp(self.r_inner, self.r_outer)
    }
}

/// One trained position axis.
pub enum AxisScale {
    Continuous {
        col: String,
        scale: ContinuousScale,
    },
    Temporal {
        col: String,
        scale: TemporalScale,
    },
    Band {
        col: String,
        scale: BandScale,
    },
    NestedBand {
        outer_col: String,
        inner_col: String,
        scale: NestedBandScale,
    },
    Union {
        label: String,
        scale: ContinuousScale,
    },
    TemporalUnion {
        label: String,
        scale: TemporalScale,
    },
}

impl AxisScale {
    fn resolve(&self, table: &dyn Table, row: usize) -> Option<f64> {
        match self {
            AxisScale::Continuous { col, scale } => cell_f64(table, col, row).map(|v| scale.map(v)),
            AxisScale::Temporal { col, scale } => {
                cell_micros(table, col, row).map(|v| scale.map(v))
            }
            AxisScale::Band { col, scale } => {
                cell_category(table, col, row).and_then(|c| scale.center(&c))
            }
            AxisScale::NestedBand {
                outer_col,
                inner_col,
                scale,
            } => {
                let outer = cell_category(table, outer_col, row)?;
                let inner = cell_category(table, inner_col, row)?;
                scale.band(&outer, &inner).map(|(start, w)| start + w / 2.0)
            }
            AxisScale::Union { .. } | AxisScale::TemporalUnion { .. } => None,
        }
    }

    fn bandwidth(&self, table: &dyn Table, row: usize) -> Option<f64> {
        match self {
            AxisScale::Band { scale, .. } => Some(scale.bandwidth()),
            AxisScale::NestedBand {
                outer_col,
                inner_col,
                scale,
            } => {
                let outer = cell_category(table, outer_col, row)?;
                let inner = cell_category(table, inner_col, row)?;
                scale.band(&outer, &inner).map(|(_, w)| w)
            }
            _ => None,
        }
    }

    /// Map a raw numeric value through a continuous/temporal axis.
    fn map_value(&self, value: f64) -> Option<f64> {
        match self {
            AxisScale::Continuous { scale, .. } | AxisScale::Union { scale, .. } => {
                Some(scale.map(value))
            }
            AxisScale::Temporal { scale, .. } | AxisScale::TemporalUnion { scale, .. } => {
                Some(scale.map(value as i64))
            }
            _ => None,
        }
    }

    pub(crate) fn map_value_public(&self, value: f64) -> Option<f64> {
        self.map_value(value)
    }

    /// Resolve a row's value in `column` to a pixel position on this axis, using
    /// the band center for categorical axes (spec §14.19). Used for segment
    /// endpoints mapped to a column that may differ from the axis's own column.
    pub(crate) fn resolve_column(
        &self,
        table: &dyn Table,
        column: &str,
        row: usize,
    ) -> Option<f64> {
        match self {
            AxisScale::Continuous { scale, .. } | AxisScale::Union { scale, .. } => {
                cell_f64(table, column, row).map(|v| scale.map(v))
            }
            AxisScale::Temporal { scale, .. } | AxisScale::TemporalUnion { scale, .. } => {
                cell_micros(table, column, row).map(|v| scale.map(v))
            }
            AxisScale::Band { scale, .. } => {
                cell_category(table, column, row).and_then(|c| scale.center(&c))
            }
            AxisScale::NestedBand { scale, .. } => {
                cell_category(table, column, row).and_then(|c| scale.outer.center(&c))
            }
        }
    }

    /// The axis title (column name or joined union member names).
    pub fn label(&self) -> String {
        let raw = match self {
            AxisScale::Continuous { col, .. }
            | AxisScale::Temporal { col, .. }
            | AxisScale::Band { col, .. } => col,
            AxisScale::NestedBand { outer_col, .. } => outer_col,
            AxisScale::Union { label, .. } | AxisScale::TemporalUnion { label, .. } => label,
        };
        crate::svg::display_label(raw)
    }

    /// Primary backing data column, when this axis resolves from a single column.
    pub fn data_column(&self) -> Option<&str> {
        match self {
            AxisScale::Continuous { col, .. }
            | AxisScale::Temporal { col, .. }
            | AxisScale::Band { col, .. } => Some(col),
            AxisScale::NestedBand { outer_col, .. } => Some(outer_col),
            AxisScale::Union { .. } | AxisScale::TemporalUnion { .. } => None,
        }
    }

    pub fn is_band(&self) -> bool {
        matches!(self, AxisScale::Band { .. } | AxisScale::NestedBand { .. })
    }

    /// Tick positions and labels for guide rendering (spec §19).
    pub fn ticks(&self) -> Vec<(f64, String)> {
        self.ticks_with_format(None)
    }

    pub fn ticks_with_format(&self, format: Option<&TemporalFormatIr>) -> Vec<(f64, String)> {
        match self {
            AxisScale::Continuous { scale, .. } | AxisScale::Union { scale, .. } => scale
                .ticks(6)
                .into_iter()
                .filter(|t| *t >= scale.min - f64::EPSILON && *t <= scale.max + f64::EPSILON)
                .map(|t| (scale.map(t), crate::svg::num(t)))
                .collect(),
            AxisScale::Temporal { scale, .. } => temporal_ticks(scale)
                .into_iter()
                .map(|micros| {
                    (
                        scale.map(micros),
                        format_temporal(micros, scale.precision, format),
                    )
                })
                .collect(),
            AxisScale::TemporalUnion { scale, .. } => temporal_ticks(scale)
                .into_iter()
                .map(|micros| {
                    (
                        scale.map(micros),
                        format_temporal(micros, scale.precision, format),
                    )
                })
                .collect(),
            AxisScale::Band { scale, .. } => scale
                .categories
                .iter()
                .filter_map(|c| scale.center(c).map(|x| (x, c.clone())))
                .collect(),
            AxisScale::NestedBand { scale, .. } => scale
                .outer
                .categories
                .iter()
                .filter_map(|c| scale.outer.center(c).map(|x| (x, c.clone())))
                .collect(),
        }
    }
}

fn format_temporal(
    micros: i64,
    precision: TemporalPrecision,
    format: Option<&TemporalFormatIr>,
) -> String {
    match DateTime::from_timestamp_micros(micros) {
        Some(dt) => match format {
            Some(TemporalFormatIr::IsoDate) => dt.format("%Y-%m-%d").to_string(),
            Some(TemporalFormatIr::IsoMinute) => dt.format("%Y-%m-%d %H:%M").to_string(),
            Some(TemporalFormatIr::IsoSecond) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            Some(TemporalFormatIr::IsoMillis) => dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
            Some(TemporalFormatIr::Rfc3339) => dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            Some(TemporalFormatIr::Year) => dt.format("%Y").to_string(),
            Some(TemporalFormatIr::Month) => dt.format("%Y-%m").to_string(),
            Some(TemporalFormatIr::MonthDay) => dt.format("%b %-d").to_string(),
            Some(TemporalFormatIr::TimeMinute) => dt.format("%H:%M").to_string(),
            Some(TemporalFormatIr::TimeSecond) => dt.format("%H:%M:%S").to_string(),
            Some(TemporalFormatIr::Custom(pattern)) => dt.format(pattern).to_string(),
            None => match precision {
                TemporalPrecision::Date => dt.format("%Y-%m-%d").to_string(),
                TemporalPrecision::DateTime => dt.format("%Y-%m-%d %H:%M").to_string(),
            },
        },
        None => String::new(),
    }
}

fn temporal_ticks(scale: &TemporalScale) -> Vec<i64> {
    if let Some(ticks) = hinted_temporal_ticks(scale) {
        return ticks;
    }

    if scale.precision == TemporalPrecision::Date {
        if let Some(ticks) = daily_ticks(scale.min, scale.max) {
            return ticks;
        }
        if let Some(ticks) = monthly_ticks(scale.min, scale.max) {
            return ticks;
        }
        if let Some(ticks) = yearly_ticks(scale.min, scale.max) {
            return ticks;
        }
    } else {
        if let Some(ticks) = clock_interval_ticks(scale.min, scale.max) {
            return ticks;
        }
        if let Some(ticks) = daily_ticks(scale.min, scale.max) {
            return ticks;
        }
        if let Some(ticks) = monthly_ticks(scale.min, scale.max) {
            return ticks;
        }
        if let Some(ticks) = yearly_ticks(scale.min, scale.max) {
            return ticks;
        }
    }

    (0..=5)
        .map(|i| scale.min + (scale.max - scale.min) * i / 5)
        .collect()
}

fn clock_interval_ticks(min: i64, max: i64) -> Option<Vec<i64>> {
    let span = max.checked_sub(min)?;
    if span <= 0 {
        return None;
    }
    const SECOND: i64 = 1_000_000;
    const MINUTE: i64 = 60 * SECOND;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const INTERVALS: &[i64] = &[
        1_000,
        10_000,
        100_000,
        SECOND,
        5 * SECOND,
        15 * SECOND,
        30 * SECOND,
        MINUTE,
        5 * MINUTE,
        15 * MINUTE,
        30 * MINUTE,
        HOUR,
        6 * HOUR,
        12 * HOUR,
        DAY,
        7 * DAY,
    ];
    for interval in INTERVALS {
        let first = ceil_to_interval(min, *interval)?;
        let count = if first > max {
            0
        } else {
            ((max - first) / interval) + 1
        };
        if (2..=8).contains(&count) {
            return Some((0..count).map(|i| first + i * interval).collect());
        }
    }
    None
}

fn ceil_to_interval(value: i64, interval: i64) -> Option<i64> {
    let rem = value.rem_euclid(interval);
    if rem == 0 {
        Some(value)
    } else {
        value.checked_add(interval - rem)
    }
}

fn hinted_temporal_ticks(scale: &TemporalScale) -> Option<Vec<i64>> {
    let values: Vec<i64> = scale
        .tick_values
        .iter()
        .copied()
        .filter(|value| *value >= scale.min && *value <= scale.max)
        .collect();
    if values.len() < 2 {
        return None;
    }
    if scale.tick_span != Some((scale.min, scale.max)) {
        return None;
    }
    if values.len() <= 8 {
        return Some(values);
    }

    let stride = values.len().div_ceil(8);
    let ticks: Vec<i64> = values
        .into_iter()
        .enumerate()
        .filter_map(|(index, value)| (index % stride == 0).then_some(value))
        .collect();
    (ticks.len() >= 2).then_some(ticks)
}

fn daily_ticks(min: i64, max: i64) -> Option<Vec<i64>> {
    let start = DateTime::from_timestamp_micros(min)?.date_naive();
    let end = DateTime::from_timestamp_micros(max)?.date_naive();
    let span_days = end.signed_duration_since(start).num_days().abs();
    if !(1..=40).contains(&span_days) {
        return None;
    }

    // Pick the smallest stride that produces at most 8 labels, so ticks always
    // land on whole-day boundaries even when the domain isn't a multiple of
    // five days (otherwise the equal-spaced fallback labels a fractional-day
    // position with the truncated date, which reads as misaligned).
    let stride = [1i64, 2, 3, 5, 7, 14]
        .into_iter()
        .find(|s| span_days / s < 8)?;

    let mut ticks = Vec::new();
    let mut offset = 0i64;
    while offset <= span_days {
        let day = start.checked_add_days(chrono::Days::new(offset as u64))?;
        let micros = day.and_hms_opt(0, 0, 0)?.and_utc().timestamp_micros();
        if micros >= min && micros <= max {
            ticks.push(micros);
        }
        offset += stride;
    }

    (2..=8).contains(&ticks.len()).then_some(ticks)
}

fn monthly_ticks(min: i64, max: i64) -> Option<Vec<i64>> {
    let start = DateTime::from_timestamp_micros(min)?.date_naive();
    let end = DateTime::from_timestamp_micros(max)?.date_naive();
    let span_days = end.signed_duration_since(start).num_days().abs();
    if !(45..=400).contains(&span_days) {
        return None;
    }

    let (mut year, mut month) = (start.year(), start.month());
    if start.day() > 1 {
        (year, month) = next_month(year, month);
    }

    let mut ticks = Vec::new();
    let mut guard = 0;
    while guard < 60 {
        let micros = month_start_micros(year, month)?;
        if micros > max {
            break;
        }
        if micros >= min {
            ticks.push(micros);
        }
        (year, month) = next_month(year, month);
        guard += 1;
    }

    (2..=8).contains(&ticks.len()).then_some(ticks)
}

fn yearly_ticks(min: i64, max: i64) -> Option<Vec<i64>> {
    let start = DateTime::from_timestamp_micros(min)?.date_naive();
    let end = DateTime::from_timestamp_micros(max)?.date_naive();
    let span_days = end.signed_duration_since(start).num_days().abs();
    if span_days < 365 {
        return None;
    }

    let mut year = start.year();
    if start.ordinal() > 1 {
        year += 1;
    }
    let end_year = end.year();
    let total_years = (end_year - year).max(0);
    let stride = [1, 2, 5, 10]
        .into_iter()
        .find(|stride| total_years / stride < 8)?;

    let mut ticks = Vec::new();
    while year <= end_year {
        let micros = month_start_micros(year, 1)?;
        if micros >= min && micros <= max {
            ticks.push(micros);
        }
        year += stride;
    }
    (2..=8).contains(&ticks.len()).then_some(ticks)
}

fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

fn month_start_micros(year: i32, month: u32) -> Option<i64> {
    Some(
        NaiveDate::from_ymd_opt(year, month, 1)?
            .and_hms_opt(0, 0, 0)?
            .and_utc()
            .timestamp_micros(),
    )
}

fn midpoint(range: (f64, f64)) -> f64 {
    (range.0 + range.1) / 2.0
}

/// A trained 2D (or 1D) position context for one space. A spatial (map) space
/// carries a [`SpatialScale`] instead of independent x/y axes (spec §16.15);
/// the placeholder `x` axis is never drawn because spatial panels skip axes and
/// grids.
pub struct ScaledSpace {
    pub x: AxisScale,
    pub y: Option<AxisScale>,
    /// Pixel y coordinate used by 1D Cartesian/vector spaces. This gives point,
    /// line, and text marks a row position without creating a visible y axis.
    baseline_y: Option<f64>,
    /// Present for a spatial space: position comes from projecting geographic
    /// coordinates rather than mapping the x/y axes.
    pub spatial: Option<SpatialScale>,
    /// Present for a polar space (spec §16.16): the x/y axes are trained over the
    /// angular/radial ranges and combined through this transform.
    pub polar: Option<Polar>,
}

impl ScaledSpace {
    /// Build position scales from a frame against the active table and plot
    /// rectangle ranges. Returns `None` for frames the renderer cannot lay out
    /// (e.g. faceting), so the caller can emit a render diagnostic.
    pub fn build(
        frame: &FrameIr,
        table: &dyn Table,
        x_range: (f64, f64),
        y_range: (f64, f64),
        hints: &SpaceDomainHints,
        scales: &[ScaleIr],
    ) -> Option<ScaledSpace> {
        let x_config = axis_config(scales, AxisSelectorIr::X);
        let y_config = axis_config(scales, AxisSelectorIr::Y);
        match frame {
            FrameIr::Cartesian(axes) if axes.len() >= 2 => {
                let x = build_axis(&axes[0], table, x_range, Some(&hints.x), &x_config)?;
                let y = build_axis(&axes[1], table, y_range, Some(&hints.y), &y_config)?;
                Some(ScaledSpace {
                    x,
                    y: Some(y),
                    baseline_y: None,
                    spatial: None,
                    polar: None,
                })
            }
            FrameIr::Cartesian(axes) if axes.len() == 1 => {
                let x = build_axis(&axes[0], table, x_range, Some(&hints.x), &x_config)?;
                Some(ScaledSpace {
                    x,
                    y: None,
                    baseline_y: Some(midpoint(y_range)),
                    spatial: None,
                    polar: None,
                })
            }
            FrameIr::Vector(_) | FrameIr::Nested { .. } | FrameIr::Union(_) => {
                let x = build_axis(frame, table, x_range, Some(&hints.x), &x_config)?;
                Some(ScaledSpace {
                    x,
                    y: None,
                    baseline_y: Some(midpoint(y_range)),
                    spatial: None,
                    polar: None,
                })
            }
            _ => None,
        }
    }

    /// Build a polar space from a frame (spec §16.16). Domain training is
    /// identical to Cartesian; only the *range* each axis maps into changes: the
    /// `theta` axis spans the angular range and the radius axis spans
    /// `[r_inner, r_outer]`. The plot is treated as a square centered on its
    /// midpoint with `R = min(width, height) / 2`.
    #[allow(clippy::too_many_arguments)]
    pub fn build_polar(
        frame: &FrameIr,
        table: &dyn Table,
        plot: Rect,
        hints: &SpaceDomainHints,
        scales: &[ScaleIr],
        theta: PolarThetaIr,
        inner_radius: f64,
        start_angle: f64,
        direction: PolarDirectionIr,
        font_size: f64,
    ) -> Option<ScaledSpace> {
        let cx = plot.x + plot.width / 2.0;
        let cy = plot.y + plot.height / 2.0;
        let max_r = plot.width.min(plot.height) / 2.0;
        let (theta_start, theta_end) = polar_angular_range(start_angle, direction);
        let assemble = |r_outer: f64| {
            Self::assemble_polar(
                frame,
                table,
                Polar {
                    cx,
                    cy,
                    r_inner: inner_radius * r_outer,
                    r_outer,
                    theta,
                    theta_start,
                    theta_end,
                },
                hints,
                scales,
            )
        };

        let provisional = assemble(max_r)?;

        // Get the exact horizontal and vertical reserve needed for the text
        let (reserve_x, reserve_y) = provisional.polar_perimeter_reserve(font_size);
        if reserve_x <= 0.0 && reserve_y <= 0.0 {
            return Some(provisional);
        }

        // The Right Math: Shrink the width and height of the plot rectangle
        // independently, then find the new maximum radius.
        let max_r_x = (plot.width / 2.0) - reserve_x;
        let max_r_y = (plot.height / 2.0) - reserve_y;

        // Take the minimum of the two to keep it a perfect circle,
        // but ensure it never completely collapses.
        let final_r = max_r_x.min(max_r_y).max(max_r * 0.25);

        assemble(final_r)
    }

    /// Build the trained axes for a polar space at a fixed radius. Domain
    /// training is identical to Cartesian; only the *range* each axis maps into
    /// changes (spec §16.16): the `theta` axis spans the angular range and the
    /// radius axis spans `[r_inner, r_outer]`.
    fn assemble_polar(
        frame: &FrameIr,
        table: &dyn Table,
        polar: Polar,
        hints: &SpaceDomainHints,
        scales: &[ScaleIr],
    ) -> Option<ScaledSpace> {
        let theta = polar.theta;
        let angular = (polar.theta_start, polar.theta_end);
        let radial = (polar.r_inner, polar.r_outer);
        let x_config = axis_config(scales, AxisSelectorIr::X);
        let y_config = axis_config(scales, AxisSelectorIr::Y);

        match frame {
            FrameIr::Cartesian(axes) if axes.len() >= 2 => {
                // The theta axis maps to the angular range, the other to radial.
                let (x_range, y_range) = match theta {
                    PolarThetaIr::X => (angular, radial),
                    PolarThetaIr::Y => (radial, angular),
                };
                let mut x = build_axis(&axes[0], table, x_range, Some(&hints.x), &x_config)?;
                let mut y = build_axis(&axes[1], table, y_range, Some(&hints.y), &y_config)?;
                // The angular band axis tiles the full circle: no band padding.
                match theta {
                    PolarThetaIr::X => clear_band_padding(&mut x),
                    PolarThetaIr::Y => clear_band_padding(&mut y),
                }
                Some(ScaledSpace {
                    x,
                    y: Some(y),
                    baseline_y: None,
                    spatial: None,
                    polar: Some(polar),
                })
            }
            // A 1D frame: the single value wraps around the angle; the radius
            // spans the full plotting radius (pie/donut, spec §16.16).
            FrameIr::Cartesian(axes) if axes.len() == 1 => {
                let mut x = build_axis(&axes[0], table, angular, Some(&hints.x), &x_config)?;
                clear_band_padding(&mut x);
                Some(ScaledSpace {
                    x,
                    y: None,
                    baseline_y: None,
                    spatial: None,
                    polar: Some(polar),
                })
            }
            FrameIr::Vector(_) | FrameIr::Union(_) => {
                let mut x = build_axis(frame, table, angular, Some(&hints.x), &x_config)?;
                clear_band_padding(&mut x);
                Some(ScaledSpace {
                    x,
                    y: None,
                    baseline_y: None,
                    spatial: None,
                    polar: Some(polar),
                })
            }
            _ => None,
        }
    }

    /// Build a spatial (map) space backed by a [`SpatialScale`]. The x/y axes
    /// are placeholders; spatial panels skip axis and grid rendering.
    pub fn spatial(spatial: SpatialScale) -> ScaledSpace {
        ScaledSpace {
            x: AxisScale::Continuous {
                col: String::new(),
                scale: ContinuousScale::new(0.0, 1.0, (0.0, 1.0)),
            },
            y: None,
            baseline_y: None,
            spatial: Some(spatial),
            polar: None,
        }
    }

    /// Whether this is a spatial (projected map) space.
    pub fn is_spatial(&self) -> bool {
        self.spatial.is_some()
    }

    pub fn resolve_x(&self, table: &dyn Table, row: usize) -> Option<f64> {
        if let Some(spatial) = &self.spatial {
            return self.project_row(spatial, table, row).map(|(x, _)| x);
        }
        if self.polar.is_some() {
            return self.polar_point(table, row).map(|(x, _)| x);
        }
        self.x.resolve(table, row)
    }

    pub fn resolve_y(&self, table: &dyn Table, row: usize) -> Option<f64> {
        if let Some(spatial) = &self.spatial {
            return self.project_row(spatial, table, row).map(|(_, y)| y);
        }
        if self.polar.is_some() {
            return self.polar_point(table, row).map(|(_, y)| y);
        }
        self.y
            .as_ref()
            .and_then(|axis| axis.resolve(table, row))
            .or(self.baseline_y)
    }

    /// Whether this is a polar (circular) space (spec §16.16).
    pub fn is_polar(&self) -> bool {
        self.polar.is_some()
    }

    /// The polar transform, when this space is polar.
    pub fn polar(&self) -> Option<&Polar> {
        self.polar.as_ref()
    }

    /// The axis that maps to the angle (theta) under the polar transform.
    fn theta_axis(&self) -> &AxisScale {
        match (self.polar.map(|p| p.theta), &self.y) {
            (Some(PolarThetaIr::Y), Some(y)) => y,
            _ => &self.x,
        }
    }

    /// The axis that maps to the radius, when a second axis exists. A 1D polar
    /// frame has no radius axis: the radius is the full plotting radius.
    fn radius_axis(&self) -> Option<&AxisScale> {
        match (self.polar.map(|p| p.theta), &self.y) {
            (Some(PolarThetaIr::Y), Some(_)) => Some(&self.x),
            (Some(PolarThetaIr::X), Some(y)) => Some(y),
            _ => None,
        }
    }

    /// Resolve a row to its `(θ, r)` then to a Cartesian pixel position.
    fn polar_point(&self, table: &dyn Table, row: usize) -> Option<(f64, f64)> {
        let polar = self.polar.as_ref()?;
        let theta = self.theta_axis().resolve(table, row)?;
        let r = match self.radius_axis() {
            Some(axis) => axis.resolve(table, row)?,
            None => polar.r_outer,
        };
        Some(polar.point(theta, polar.clamp_radius(r)))
    }

    /// Whether the angular (theta) axis is categorical (a band). When true, each
    /// category occupies an angular wedge (coxcomb/wind rose); when false the
    /// angle comes from a continuous value (pie/donut).
    pub fn polar_theta_is_band(&self) -> bool {
        self.theta_axis().is_band()
    }

    /// Horizontal room (px) the perimeter category labels need beyond the outer
    /// radius, used to inset the circle so they stay within the plot rect (e.g.
    /// clear of the legend). Zero for a continuous angle (pie/donut), which
    /// draws no perimeter labels (spec §16.16, §19).
    /// Horizontal and vertical room (px) the perimeter category labels need beyond
    /// the outer radius.
    fn polar_perimeter_reserve(&self, font_size: f64) -> (f64, f64) {
        if !self.polar_theta_is_band() {
            return (0.0, 0.0);
        }

        let mut max_dx = 0.0_f64;
        let mut max_dy = 0.0_f64;

        for (theta, label) in self.polar_theta_ticks() {
            let width = estimate_text_width(&label, font_size);
            let height = font_size; // approximate text height

            // Calculate the bounding box extension for this specific label's angle
            let dx = POLAR_LABEL_GAP + (width * theta.cos().abs());
            let dy = POLAR_LABEL_GAP + (height * theta.sin().abs());

            max_dx = max_dx.max(dx);
            max_dy = max_dy.max(dy);
        }

        (max_dx, max_dy)
    }

    /// The data column backing the radius axis, when present.
    pub fn polar_radius_column(&self) -> Option<&str> {
        self.radius_axis().and_then(|axis| axis.data_column())
    }

    /// The data column backing the angular (theta) axis, when present.
    pub fn polar_theta_column(&self) -> Option<&str> {
        self.theta_axis().data_column()
    }

    /// Theta-axis ticks for polar spokes: `(angle, label)` pairs (spec §16.16,
    /// §19). For a categorical angle these are the category centers.
    pub fn polar_theta_ticks(&self) -> Vec<(f64, String)> {
        self.theta_axis().ticks()
    }

    /// Radius-axis ticks for polar rings: `(radius_px, label)` pairs within the
    /// drawable annulus. Empty when there is no radius axis (a full-radius pie).
    pub fn polar_radius_ticks(&self) -> Vec<(f64, String)> {
        let Some(polar) = self.polar.as_ref() else {
            return Vec::new();
        };
        match self.radius_axis() {
            Some(axis) => axis
                .ticks()
                .into_iter()
                .filter(|(r, _)| *r >= polar.r_inner - 1.0 && *r <= polar.r_outer + 1.0)
                .collect(),
            None => Vec::new(),
        }
    }

    /// The angle (radians) a row maps to on the theta axis (for ordering polar
    /// Line/Area vertices around the circle).
    pub fn polar_angle(&self, table: &dyn Table, row: usize) -> Option<f64> {
        self.theta_axis().resolve(table, row)
    }

    /// The angle and angular bandwidth for a row's theta band (area geometries).
    pub fn polar_angle_band(&self, table: &dyn Table, row: usize) -> Option<(f64, f64)> {
        let center = self.theta_axis().resolve(table, row)?;
        let width = self.theta_axis().bandwidth(table, row).unwrap_or(0.0);
        Some((center, width))
    }

    /// Map a raw radius-axis value to a radius in pixels (e.g. the `0` baseline
    /// maps to `r_inner`). Falls back to the full radius for a 1D frame.
    pub fn polar_radius_value(&self, value: f64) -> Option<f64> {
        let polar = self.polar.as_ref()?;
        match self.radius_axis() {
            Some(axis) => axis.map_value(value).map(|r| polar.clamp_radius(r)),
            None => Some(polar.r_outer),
        }
    }

    /// The `(start_radius, radial_bandwidth)` for a banded radius axis (radial
    /// bars / annular tiles).
    pub fn polar_radius_band(&self, table: &dyn Table, row: usize) -> Option<(f64, f64)> {
        let axis = self.radius_axis()?;
        let center = axis.resolve(table, row)?;
        let width = axis.bandwidth(table, row)?;
        Some((center - width / 2.0, width))
    }

    /// Project a row's `long * lat` coordinate through a projected overlay
    /// space, for point/line marks sharing a basemap's spatial scale.
    fn project_row(
        &self,
        spatial: &SpatialScale,
        table: &dyn Table,
        row: usize,
    ) -> Option<(f64, f64)> {
        let lon = cell_f64(table, spatial.lon_col.as_deref()?, row)?;
        let lat = cell_f64(table, spatial.lat_col.as_deref()?, row)?;
        spatial.project_ll(lon, lat)
    }

    pub fn x_bandwidth(&self, table: &dyn Table, row: usize) -> Option<f64> {
        self.x.bandwidth(table, row)
    }

    pub fn y_bandwidth(&self, table: &dyn Table, row: usize) -> Option<f64> {
        self.y.as_ref()?.bandwidth(table, row)
    }

    pub fn map_x(&self, value: f64) -> Option<f64> {
        self.x.map_value(value)
    }

    pub fn map_y(&self, value: f64) -> Option<f64> {
        self.y.as_ref()?.map_value(value)
    }

    /// The x axis scale (for resolving mapped geometry endpoints, spec §14.19).
    pub fn x_axis(&self) -> &AxisScale {
        &self.x
    }

    /// The y axis scale, when present.
    pub fn y_axis(&self) -> Option<&AxisScale> {
        self.y.as_ref()
    }
}

/// Remove band padding so an angular band axis tiles the full circle without
/// gaps (spec §16.16). A no-op for non-band axes.
fn clear_band_padding(axis: &mut AxisScale) {
    match axis {
        AxisScale::Band { scale, .. } => {
            scale.pad_inner = 0.0;
            scale.pad_outer = 0.0;
        }
        AxisScale::NestedBand { scale, .. } => {
            scale.pad_inner = 0.0;
            scale.outer.pad_inner = 0.0;
            scale.outer.pad_outer = 0.0;
        }
        _ => {}
    }
}

/// Build a single axis scale from a frame sub-expression.
fn build_axis(
    frame: &FrameIr,
    table: &dyn Table,
    range: (f64, f64),
    hints: Option<&AxisDomainHints>,
    config: &AxisScaleConfig,
) -> Option<AxisScale> {
    let range = config.apply_range(range);
    match frame {
        FrameIr::Vector(col) => Some(build_vector_axis(col, table, range, hints, config)),
        FrameIr::Nested { outer, inner } => {
            if let (FrameIr::Vector(o), FrameIr::Vector(i)) = (outer.as_ref(), inner.as_ref()) {
                let outer_cats = categorical_domain(table, &o.name);
                let inner_cats = categorical_domain(table, &i.name);
                let mut outer_band = BandScale::new(outer_cats, range);
                if let Some(hints) = hints {
                    if let Some(pad) = hints.band_pad_inner() {
                        outer_band.pad_inner = pad;
                    }
                    if let Some(pad) = hints.band_pad_outer() {
                        outer_band.pad_outer = pad;
                    }
                }
                let mut nested = NestedBandScale::new(outer_band, inner_cats);
                if let Some(hints) = hints {
                    if let Some(pad) = hints.band_pad_inner() {
                        nested.pad_inner = pad;
                    }
                }
                Some(AxisScale::NestedBand {
                    outer_col: o.name.clone(),
                    inner_col: i.name.clone(),
                    scale: nested,
                })
            } else {
                // Faceting (nested Cartesian plane) is not yet laid out.
                None
            }
        }
        FrameIr::Union(members) => {
            let cols: Vec<&ColumnRef> = members
                .iter()
                .filter_map(|m| match m {
                    FrameIr::Vector(c) => Some(c),
                    _ => None,
                })
                .collect();
            let label = cols
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>()
                .join(" + ");
            if !cols.is_empty() && cols.iter().all(|column| column.dtype == DataType::Temporal) {
                let mut min = i64::MAX;
                let mut max = i64::MIN;
                let mut precision = TemporalPrecision::Date;
                for c in &cols {
                    if let Some((lo, hi, p)) = temporal_domain(table, &c.name) {
                        min = min.min(lo);
                        max = max.max(hi);
                        if p == TemporalPrecision::DateTime {
                            precision = TemporalPrecision::DateTime;
                        }
                    }
                }
                if min > max {
                    min = 0;
                    max = 1;
                }
                if let Some(hints) = hints {
                    hints.apply_temporal(&mut min, &mut max);
                }
                let mut scale = TemporalScale::new(min, max, range, precision);
                if let Some(hints) = hints {
                    scale.tick_values = hints.temporal_tick_values();
                    scale.tick_span = hints.temporal_tick_span();
                }
                return Some(AxisScale::TemporalUnion { label, scale });
            }
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            for c in &cols {
                if let Some((lo, hi)) = numeric_domain(table, &c.name) {
                    min = min.min(lo);
                    max = max.max(hi);
                }
            }
            if min > max {
                min = 0.0;
                max = 1.0;
            }
            if let Some(hints) = hints {
                hints.apply_numeric(&mut min, &mut max);
                hints.apply_padding(&mut min, &mut max);
            }
            if let Some(bounds) = config.domain {
                apply_domain_bounds(bounds, &mut min, &mut max);
            }
            Some(AxisScale::Union {
                label,
                scale: continuous_scale(min, max, range, config),
            })
        }
        _ => None,
    }
}

fn build_vector_axis(
    col: &ColumnRef,
    table: &dyn Table,
    range: (f64, f64),
    hints: Option<&AxisDomainHints>,
    config: &AxisScaleConfig,
) -> AxisScale {
    match col.dtype {
        DataType::Integer | DataType::Float => {
            let (mut min, mut max) = numeric_domain(table, &col.name).unwrap_or((0.0, 1.0));
            if let Some(hints) = hints {
                hints.apply_numeric(&mut min, &mut max);
                hints.apply_padding(&mut min, &mut max);
            }
            if let Some(bounds) = config.domain {
                apply_domain_bounds(bounds, &mut min, &mut max);
            }
            AxisScale::Continuous {
                col: col.name.clone(),
                scale: continuous_scale(min, max, range, config),
            }
        }
        DataType::Temporal => {
            let (mut min, mut max, precision) =
                temporal_domain(table, &col.name).unwrap_or((0, 1, TemporalPrecision::Date));
            if let Some(hints) = hints {
                hints.apply_temporal(&mut min, &mut max);
            }
            let mut scale = TemporalScale::new(min, max, range, precision);
            if let Some(hints) = hints {
                scale.tick_values = hints.temporal_tick_values();
                scale.tick_span = hints.temporal_tick_span();
            }
            AxisScale::Temporal {
                col: col.name.clone(),
                scale,
            }
        }
        _ => {
            let cats = categorical_domain(table, &col.name);
            let mut scale = BandScale::new(cats, range);
            if let Some(hints) = hints {
                if let Some(pad) = hints.band_pad_inner() {
                    scale.pad_inner = pad;
                }
                if let Some(pad) = hints.band_pad_outer() {
                    scale.pad_outer = pad;
                }
            }
            AxisScale::Band {
                col: col.name.clone(),
                scale,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct AxisScaleConfig {
    scale_type: Option<ScaleTypeIr>,
    domain: Option<[Option<f64>; 2]>,
    reverse: bool,
    integer: bool,
}

/// Override `(min, max)` with explicit domain bounds, leaving a bound untouched
/// where it is `null` ("infer from data", spec §16.11). When both bounds are
/// given out of order, they are normalized so `min <= max`.
fn apply_domain_bounds(bounds: [Option<f64>; 2], min: &mut f64, max: &mut f64) {
    match bounds {
        [Some(a), Some(b)] => {
            *min = a.min(b);
            *max = a.max(b);
        }
        [Some(a), None] => *min = a,
        [None, Some(b)] => *max = b,
        [None, None] => {}
    }
}

impl AxisScaleConfig {
    fn apply_range(self, range: (f64, f64)) -> (f64, f64) {
        if self.reverse {
            (range.1, range.0)
        } else {
            range
        }
    }
}

fn axis_config(scales: &[ScaleIr], axis: AxisSelectorIr) -> AxisScaleConfig {
    let mut config = AxisScaleConfig::default();
    for scale in scales {
        if scale.target == ScaleTargetIr::Axis(axis) {
            if scale.scale_type.is_some() {
                config.scale_type = scale.scale_type;
            }
            if scale.domain.is_some() {
                config.domain = scale.domain;
            }
            if let Some(reverse) = scale.reverse {
                config.reverse = reverse;
            }
            if let Some(integer) = scale.integer {
                config.integer = integer;
            }
        }
    }
    config
}

fn continuous_scale(
    min: f64,
    max: f64,
    range: (f64, f64),
    config: &AxisScaleConfig,
) -> ContinuousScale {
    let mut scale = if config.scale_type == Some(ScaleTypeIr::Log10) && min > 0.0 && max > 0.0 {
        ContinuousScale::log10(min, max, range)
    } else if config.scale_type == Some(ScaleTypeIr::Sqrt) && min >= 0.0 && max >= 0.0 {
        ContinuousScale::sqrt(min, max, range)
    } else {
        ContinuousScale::new(min, max, range)
    };
    scale.integer = config.integer;
    scale
}
