//! Trained spatial context built from a frame IR (spec §16.12, §16.1).
//!
//! [`ScaledSpace`] hides whether a position scale is continuous, temporal,
//! banded, or nested: geometries call `resolve_x`/`resolve_y` and the bandwidth
//! accessors without knowing the underlying scale kind.

use crate::domains::{AxisDomainHints, SpaceDomainHints};
use crate::guide::estimate_text_width;
use crate::layout::Rect;
use crate::projection::SpatialScale;
use crate::scale::{
    categorical_domain, cell_category, cell_f64, cell_micros, numeric_domain, temporal_domain,
    BandScale, ContinuousScale, NestedBandScale, TemporalScale, DEFAULT_BAND_PAD_INNER,
    DEFAULT_NESTED_BAND_PAD_INNER,
};
use algraf_data::{DataType, DataValueRef, DateTimeValue, Table, TemporalPrecision};
use algraf_semantics::{
    AxisSelectorIr, AxisViewDomainIr, ColumnRef, CoordinateViewIr, FrameIr, PolarDirectionIr,
    PolarThetaIr, ScaleExpansionIr, ScaleIr, ScaleTargetIr, ScaleTypeIr, TemporalFormatIr,
    TemporalTickIntervalIr,
};

mod polar;
pub(crate) mod temporal;

pub(crate) use polar::{polar_angular_range, Polar, POLAR_LABEL_GAP};
use temporal::{format_temporal, temporal_ticks};

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
    TemporalNestedBand {
        outer_col: String,
        inner_col: String,
        scale: TemporalScale,
        inner_categories: Vec<String>,
        pad_inner: f64,
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
            AxisScale::TemporalNestedBand {
                outer_col, scale, ..
            } => cell_micros(table, outer_col, row).map(|v| scale.map(v)),
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
            AxisScale::TemporalNestedBand { .. } => self
                .temporal_bucket_bounds(table, row)
                .map(|(start, end)| (end - start).abs()),
            _ => None,
        }
    }

    /// Map a raw numeric value through a continuous/temporal axis.
    fn map_value(&self, value: f64) -> Option<f64> {
        match self {
            AxisScale::Continuous { scale, .. } | AxisScale::Union { scale, .. } => {
                Some(scale.map(value))
            }
            AxisScale::Temporal { scale, .. }
            | AxisScale::TemporalNestedBand { scale, .. }
            | AxisScale::TemporalUnion { scale, .. } => Some(scale.map(value as i64)),
            _ => None,
        }
    }

    pub(crate) fn map_value_public(&self, value: f64) -> Option<f64> {
        self.map_value(value)
    }

    /// The authored temporal bucket cadence on this axis, when present.
    pub(crate) fn temporal_tick_interval(&self) -> Option<TemporalTickIntervalIr> {
        match self {
            AxisScale::Temporal { scale, .. }
            | AxisScale::TemporalNestedBand { scale, .. }
            | AxisScale::TemporalUnion { scale, .. } => scale.tick_interval,
            _ => None,
        }
    }

    /// Resolve the row's temporal value on this axis to raw UTC microseconds.
    pub(crate) fn temporal_anchor(&self, table: &dyn Table, row: usize) -> Option<i64> {
        match self {
            AxisScale::Temporal { col, .. } => cell_micros(table, col, row),
            AxisScale::TemporalNestedBand { outer_col, .. } => cell_micros(table, outer_col, row),
            AxisScale::TemporalUnion { .. } => None,
            _ => None,
        }
    }

    /// Resolve the row's centered temporal bucket to pixel bounds on this axis.
    pub(crate) fn temporal_bucket_bounds(
        &self,
        table: &dyn Table,
        row: usize,
    ) -> Option<(f64, f64)> {
        let anchor = self.temporal_anchor(table, row)?;
        let (start, end) =
            temporal::centered_bucket_bounds(anchor, self.temporal_tick_interval()?)?;
        let start_px = self.map_value(start as f64)?;
        let end_px = self.map_value(end as f64)?;
        match self {
            AxisScale::TemporalNestedBand {
                inner_col,
                inner_categories,
                pad_inner,
                ..
            } => {
                let inner = cell_category(table, inner_col, row)?;
                let (outer_start, outer_end) =
                    inset_pixel_range(start_px, end_px, DEFAULT_BAND_PAD_INNER);
                let inner_scale = BandScale {
                    categories: inner_categories.clone(),
                    temporal_values: None,
                    range: (outer_start, outer_end),
                    pad_inner: *pad_inner,
                    pad_outer: 0.0,
                };
                inner_scale
                    .band(&inner)
                    .map(|(start, width)| (start, start + width))
            }
            _ => Some(inset_pixel_range(start_px, end_px, DEFAULT_BAND_PAD_INNER)),
        }
    }

    /// Group key for stacking/fill within a temporal bucket position.
    pub(crate) fn temporal_bucket_key(&self, table: &dyn Table, row: usize) -> Option<String> {
        let anchor = self.temporal_anchor(table, row)?;
        match self {
            AxisScale::TemporalNestedBand { inner_col, .. } => Some(format!(
                "{}\u{1f}{}",
                anchor,
                cell_category(table, inner_col, row)?
            )),
            _ => Some(anchor.to_string()),
        }
    }

    /// Resolve the row's bar slot on this position axis. Categorical axes
    /// return their trained band; temporal axes return the inset bucket supplied
    /// by `tickInterval`.
    pub(crate) fn bar_slot_bounds(&self, table: &dyn Table, row: usize) -> Option<(f64, f64)> {
        match self {
            AxisScale::Band { col, scale } => {
                let category = cell_category(table, col, row)?;
                scale
                    .band(&category)
                    .map(|(start, width)| (start, start + width))
            }
            AxisScale::NestedBand {
                outer_col,
                inner_col,
                scale,
            } => {
                let outer = cell_category(table, outer_col, row)?;
                let inner = cell_category(table, inner_col, row)?;
                scale
                    .band(&outer, &inner)
                    .map(|(start, width)| (start, start + width))
            }
            AxisScale::Temporal { .. } | AxisScale::TemporalNestedBand { .. } => {
                self.temporal_bucket_bounds(table, row)
            }
            _ => None,
        }
    }

    /// Stable grouping key for bar stack/fill accumulation within a slot.
    pub(crate) fn bar_slot_key(&self, table: &dyn Table, row: usize) -> Option<String> {
        match self {
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
            AxisScale::Temporal { .. } | AxisScale::TemporalNestedBand { .. } => {
                self.temporal_bucket_key(table, row)
            }
            _ => None,
        }
    }

    pub(crate) fn is_bar_slot_axis(&self) -> bool {
        matches!(
            self,
            AxisScale::Band { .. }
                | AxisScale::NestedBand { .. }
                | AxisScale::Temporal { .. }
                | AxisScale::TemporalNestedBand { .. }
        )
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
            AxisScale::Temporal { scale, .. }
            | AxisScale::TemporalNestedBand { scale, .. }
            | AxisScale::TemporalUnion { scale, .. } => {
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
            AxisScale::TemporalNestedBand { outer_col, .. } => outer_col,
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
            AxisScale::TemporalNestedBand { outer_col, .. } => Some(outer_col),
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
                .enumerate()
                .map(|(index, t)| {
                    let label = scale
                        .tick_labels
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| crate::svg::num(t));
                    (scale.map(t), label)
                })
                .collect(),
            AxisScale::Temporal { scale, .. }
            | AxisScale::TemporalNestedBand { scale, .. }
            | AxisScale::TemporalUnion { scale, .. } => {
                let ticks = temporal_ticks(scale);
                // With no explicit timeFormat, adapt the default label
                // pattern to the tick granularity: year-start ticks read
                // `2024`, month-start ticks read `2024-04` (spec §16.4).
                let default_pattern = if format.is_none() {
                    temporal::default_tick_pattern(&ticks)
                } else {
                    None
                };
                ticks
                    .into_iter()
                    .enumerate()
                    .map(|(index, micros)| {
                        let label = scale.tick_labels.get(index).cloned().unwrap_or_else(|| {
                            match default_pattern {
                                Some(pattern) => temporal::format_with_pattern(micros, pattern),
                                None => format_temporal(micros, scale.precision, format),
                            }
                        });
                        (scale.map(micros), label)
                    })
                    .collect()
            }
            AxisScale::Band { scale, .. } => band_ticks(scale, format),
            AxisScale::NestedBand { scale, .. } => band_ticks(&scale.outer, format),
        }
    }

    /// Tick positions and labels honoring an optional numeric format (spec
    /// §19.4, §14.16). A numeric format applies only to continuous (non-temporal,
    /// non-categorical) axes; on other axes it is ignored and the default labels
    /// from [`Self::ticks_with_format`] are used.
    pub fn ticks_formatted(
        &self,
        temporal: Option<&TemporalFormatIr>,
        numeric: Option<&str>,
    ) -> Vec<(f64, String)> {
        if let Some(format) = numeric {
            if let AxisScale::Continuous { scale, .. } | AxisScale::Union { scale, .. } = self {
                return scale
                    .ticks(6)
                    .into_iter()
                    .filter(|t| *t >= scale.min - f64::EPSILON && *t <= scale.max + f64::EPSILON)
                    .map(|t| (scale.map(t), crate::helpers::format_numeric(t, format)))
                    .collect();
            }
        }
        self.ticks_with_format(temporal)
    }

    /// Domain span in units for fixed-aspect layout. Continuous and temporal
    /// axes use data units; categorical axes use one unit per trained band step.
    pub fn continuous_domain_span(&self) -> Option<f64> {
        match self {
            AxisScale::Continuous { scale, .. } | AxisScale::Union { scale, .. } => {
                Some((scale.max - scale.min).abs())
            }
            AxisScale::Temporal { scale, .. }
            | AxisScale::TemporalNestedBand { scale, .. }
            | AxisScale::TemporalUnion { scale, .. } => Some((scale.max - scale.min).abs() as f64),
            AxisScale::Band { scale, .. } => Some(scale.categories.len().max(1) as f64),
            AxisScale::NestedBand { scale, .. } => Some(scale.outer.categories.len().max(1) as f64),
        }
    }

    /// Pixel delta corresponding to a data-space offset on this axis. For band
    /// axes the offset is a fraction of the resolved band width.
    pub(crate) fn data_delta_px(&self, table: &dyn Table, row: usize, value: f64) -> Option<f64> {
        match self {
            AxisScale::Continuous { scale, .. } | AxisScale::Union { scale, .. } => {
                let span = scale.max - scale.min;
                if span.abs() <= f64::EPSILON {
                    return Some(0.0);
                }
                Some((scale.range.1 - scale.range.0) / span * value)
            }
            AxisScale::Temporal { scale, .. }
            | AxisScale::TemporalNestedBand { scale, .. }
            | AxisScale::TemporalUnion { scale, .. } => {
                let span = (scale.max - scale.min) as f64;
                if span.abs() <= f64::EPSILON {
                    return Some(0.0);
                }
                Some((scale.range.1 - scale.range.0) / span * value)
            }
            AxisScale::Band { scale, .. } => Some(scale.bandwidth() * value),
            AxisScale::NestedBand { scale, .. } => {
                let outer = match self {
                    AxisScale::NestedBand { outer_col, .. } => {
                        cell_category(table, outer_col, row)?
                    }
                    _ => return None,
                };
                let inner = match self {
                    AxisScale::NestedBand { inner_col, .. } => {
                        cell_category(table, inner_col, row)?
                    }
                    _ => return None,
                };
                scale.band(&outer, &inner).map(|(_, width)| width * value)
            }
        }
    }
}

fn band_ticks(scale: &BandScale, format: Option<&TemporalFormatIr>) -> Vec<(f64, String)> {
    scale
        .categories
        .iter()
        .enumerate()
        .filter_map(|(index, category)| {
            let label = temporal_band_label(scale, index, category, format);
            scale.center(category).map(|x| (x, label))
        })
        .collect()
}

fn inset_pixel_range(start: f64, end: f64, pad_inner: f64) -> (f64, f64) {
    let inset = (end - start) * pad_inner.clamp(0.0, 0.95) / 2.0;
    (start + inset, end - inset)
}

fn temporal_band_label(
    scale: &BandScale,
    index: usize,
    category: &str,
    format: Option<&TemporalFormatIr>,
) -> String {
    format
        .and_then(|format| {
            scale
                .temporal_values
                .as_ref()
                .and_then(|values| values.get(index))
                .and_then(|value| *value)
                .map(|value| {
                    format_temporal(
                        value.instant.and_utc().timestamp_micros(),
                        value.precision,
                        Some(format),
                    )
                })
        })
        .unwrap_or_else(|| category.to_string())
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
        view: CoordinateViewIr,
    ) -> Option<ScaledSpace> {
        Self::build_with_axis_tables(frame, table, table, x_range, y_range, hints, scales, view)
    }

    /// Build position scales while allowing a facet-free mode to train one axis
    /// from panel-local rows and the other from the full facet data.
    #[allow(clippy::too_many_arguments)]
    pub fn build_with_axis_tables(
        frame: &FrameIr,
        x_table: &dyn Table,
        y_table: &dyn Table,
        x_range: (f64, f64),
        y_range: (f64, f64),
        hints: &SpaceDomainHints,
        scales: &[ScaleIr],
        view: CoordinateViewIr,
    ) -> Option<ScaledSpace> {
        let x_config = axis_config(scales, AxisSelectorIr::X, view.zoom_x);
        let y_config = axis_config(scales, AxisSelectorIr::Y, view.zoom_y);
        match frame {
            FrameIr::Cartesian(axes) if axes.len() >= 2 => {
                let x = build_axis(&axes[0], x_table, x_range, Some(&hints.x), &x_config)?;
                let y = build_axis(&axes[1], y_table, y_range, Some(&hints.y), &y_config)?;
                Some(ScaledSpace {
                    x,
                    y: Some(y),
                    baseline_y: None,
                    spatial: None,
                    polar: None,
                })
            }
            FrameIr::Cartesian(axes) if axes.len() == 1 => {
                let x = build_axis(&axes[0], x_table, x_range, Some(&hints.x), &x_config)?;
                Some(ScaledSpace {
                    x,
                    y: None,
                    baseline_y: Some(midpoint(y_range)),
                    spatial: None,
                    polar: None,
                })
            }
            FrameIr::Vector(_) | FrameIr::Nested { .. } | FrameIr::Union(_) => {
                let x = build_axis(frame, x_table, x_range, Some(&hints.x), &x_config)?;
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
        let x_config = axis_config(scales, AxisSelectorIr::X, None);
        let y_config = axis_config(scales, AxisSelectorIr::Y, None);

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

    pub fn fixed_aspect_plot(&self, plot: Rect, ratio: f64) -> Rect {
        let Some(x_span) = self.x.continuous_domain_span() else {
            return plot;
        };
        let Some(y_span) = self.y.as_ref().and_then(AxisScale::continuous_domain_span) else {
            return plot;
        };
        if x_span <= f64::EPSILON || y_span <= f64::EPSILON || ratio <= 0.0 {
            return plot;
        }
        let desired_width = plot.height * ratio * (x_span / y_span);
        if desired_width <= plot.width {
            let width = desired_width.max(1.0);
            return Rect {
                x: plot.x + (plot.width - width) / 2.0,
                y: plot.y,
                width,
                height: plot.height,
            };
        }
        let desired_height = plot.width / (ratio * (x_span / y_span));
        let height = desired_height.max(1.0);
        Rect {
            x: plot.x,
            y: plot.y + (plot.height - height) / 2.0,
            width: plot.width,
            height,
        }
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
                if config.scale_type != Some(ScaleTypeIr::Categorical)
                    && (o.dtype == DataType::Temporal
                        || config.scale_type == Some(ScaleTypeIr::Temporal))
                {
                    let scale = temporal_axis_scale(o, table, range, hints, config);
                    return Some(AxisScale::TemporalNestedBand {
                        outer_col: o.name.clone(),
                        inner_col: i.name.clone(),
                        scale,
                        inner_categories: categorical_domain(table, &i.name),
                        pad_inner: hints
                            .and_then(AxisDomainHints::band_pad_inner)
                            .unwrap_or(DEFAULT_NESTED_BAND_PAD_INNER),
                    });
                }
                let outer_cats = ordered_categorical_domain(table, &o.name, config);
                let inner_cats = categorical_domain(table, &i.name);
                let mut outer_band = BandScale::new(outer_cats, range);
                outer_band.temporal_values =
                    temporal_category_values(table, &o.name, &outer_band.categories);
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
                if let Some(expansion) = config.expansion {
                    nested.outer.pad_outer = expansion.mult;
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
                let (min, max) = resolve_temporal_domain((min, max), hints, config);
                let mut scale = TemporalScale::new(min, max, range, precision);
                if let Some(hints) = hints {
                    scale.tick_values = hints.temporal_tick_values();
                    scale.tick_span = hints.temporal_tick_span();
                }
                apply_axis_breaks_to_temporal(&mut scale, config);
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
            let (min, max) = resolve_numeric_domain((min, max), hints, config);
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
    if config.scale_type == Some(ScaleTypeIr::Categorical) && col.dtype != DataType::Geometry {
        return band_axis(col, table, range, hints, config);
    }
    match col.dtype {
        DataType::Integer | DataType::Float => {
            let (min, max) = resolve_numeric_domain(
                numeric_domain(table, &col.name).unwrap_or((0.0, 1.0)),
                hints,
                config,
            );
            AxisScale::Continuous {
                col: col.name.clone(),
                scale: continuous_scale(min, max, range, config),
            }
        }
        DataType::Temporal => {
            let scale = temporal_axis_scale(col, table, range, hints, config);
            AxisScale::Temporal {
                col: col.name.clone(),
                scale,
            }
        }
        _ => band_axis(col, table, range, hints, config),
    }
}

fn temporal_axis_scale(
    col: &ColumnRef,
    table: &dyn Table,
    range: (f64, f64),
    hints: Option<&AxisDomainHints>,
    config: &AxisScaleConfig,
) -> TemporalScale {
    let (min, max, precision) =
        temporal_domain(table, &col.name).unwrap_or((0, 1, TemporalPrecision::Date));
    let (min, max) = resolve_temporal_domain((min, max), hints, config);
    let mut scale = TemporalScale::new(min, max, range, precision);
    if let Some(hints) = hints {
        scale.tick_values = hints.temporal_tick_values();
        scale.tick_span = hints.temporal_tick_span();
    }
    apply_axis_breaks_to_temporal(&mut scale, config);
    scale
}

fn band_axis(
    col: &ColumnRef,
    table: &dyn Table,
    range: (f64, f64),
    hints: Option<&AxisDomainHints>,
    config: &AxisScaleConfig,
) -> AxisScale {
    let cats = ordered_categorical_domain(table, &col.name, config);
    let mut scale = BandScale::new(cats, range);
    scale.temporal_values = temporal_category_values(table, &col.name, &scale.categories);
    if let Some(hints) = hints {
        if let Some(pad) = hints.band_pad_inner() {
            scale.pad_inner = pad;
        }
        if let Some(pad) = hints.band_pad_outer() {
            scale.pad_outer = pad;
        }
    }
    if let Some(expansion) = config.expansion {
        scale.pad_outer = expansion.mult;
    }
    AxisScale::Band {
        col: col.name.clone(),
        scale,
    }
}

fn ordered_categorical_domain(
    table: &dyn Table,
    column: &str,
    config: &AxisScaleConfig,
) -> Vec<String> {
    let data_categories = categorical_domain(table, column);
    let Some(declared) = &config.categorical_domain else {
        return data_categories;
    };
    let mut categories = declared.clone();
    for category in data_categories {
        if !categories.contains(&category) {
            categories.push(category);
        }
    }
    categories
}

fn temporal_category_values(
    table: &dyn Table,
    column: &str,
    categories: &[String],
) -> Option<Vec<Option<DateTimeValue>>> {
    let mut values = vec![None; categories.len()];
    let mut saw_temporal = false;
    for row in 0..table.row_count() {
        let Some(value) = temporal_cell(table, column, row) else {
            continue;
        };
        saw_temporal = true;
        let key = value.instant.and_utc().to_rfc3339();
        let Some(index) = categories.iter().position(|category| category == &key) else {
            continue;
        };
        if values[index].is_none() {
            values[index] = Some(value);
        }
    }
    saw_temporal.then_some(values)
}

fn temporal_cell(table: &dyn Table, column: &str, row: usize) -> Option<DateTimeValue> {
    if let Some(view) = table.column(column) {
        return view.temporal_at(row);
    }
    match table.value(column, row)? {
        DataValueRef::Temporal(value) => Some(value),
        _ => None,
    }
}

#[derive(Debug, Clone, Default)]
struct AxisScaleConfig {
    scale_type: Option<ScaleTypeIr>,
    domain: Option<[Option<f64>; 2]>,
    categorical_domain: Option<Vec<String>>,
    breaks: Option<Vec<f64>>,
    tick_interval: Option<TemporalTickIntervalIr>,
    break_labels: Option<Vec<String>>,
    expansion: Option<ScaleExpansionIr>,
    view_domain: Option<AxisViewDomainIr>,
    reverse: bool,
    integer: bool,
}

fn resolve_numeric_domain(
    domain: (f64, f64),
    hints: Option<&AxisDomainHints>,
    config: &AxisScaleConfig,
) -> (f64, f64) {
    let (mut min, mut max) = domain;
    // Domain ordering is shared by vector and union axes: start from finite data
    // extent (§16.3), merge geometry/domain hints and padding, apply expansion
    // before explicit Scale(domain:) bounds (§16.11), then apply visual
    // coordinate view bounds last (§16.17).
    if let Some(hints) = hints {
        hints.apply_numeric(&mut min, &mut max);
        hints.apply_padding(&mut min, &mut max);
    }
    if config.domain.is_none() {
        apply_numeric_expansion(config.expansion, &mut min, &mut max);
    }
    if let Some(bounds) = config.domain {
        apply_domain_bounds(bounds, &mut min, &mut max);
    }
    if let Some(bounds) = config.view_domain {
        apply_view_domain_bounds(bounds, &mut min, &mut max);
    }
    (min, max)
}

fn resolve_temporal_domain(
    domain: (i64, i64),
    hints: Option<&AxisDomainHints>,
    config: &AxisScaleConfig,
) -> (i64, i64) {
    let (mut min, mut max) = domain;
    // Temporal axes use the same domain pipeline as numeric axes: data extent
    // (§16.4), hints, expansion and explicit bounds (§16.11), then visual view
    // bounds (§16.17). Keeping this in one helper prevents vector and union
    // temporal paths from drifting.
    if let Some(hints) = hints {
        hints.apply_temporal(&mut min, &mut max);
    }
    if config.domain.is_none() {
        apply_temporal_expansion(config.expansion, &mut min, &mut max);
    }
    if let Some(bounds) = config.domain {
        apply_temporal_domain_bounds(bounds, &mut min, &mut max);
    }
    if let Some(bounds) = config.view_domain {
        apply_temporal_view_domain_bounds(bounds, &mut min, &mut max);
    }
    (min, max)
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

fn apply_temporal_domain_bounds(bounds: [Option<f64>; 2], min: &mut i64, max: &mut i64) {
    match bounds {
        [Some(a), Some(b)] => {
            *min = a.min(b) as i64;
            *max = a.max(b) as i64;
        }
        [Some(a), None] => *min = a as i64,
        [None, Some(b)] => *max = b as i64,
        [None, None] => {}
    }
}

fn apply_view_domain_bounds(bounds: AxisViewDomainIr, min: &mut f64, max: &mut f64) {
    match (bounds.min, bounds.max) {
        (Some(a), Some(b)) => {
            *min = a.min(b);
            *max = a.max(b);
        }
        (Some(a), None) => *min = a,
        (None, Some(b)) => *max = b,
        (None, None) => {}
    }
}

fn apply_temporal_view_domain_bounds(bounds: AxisViewDomainIr, min: &mut i64, max: &mut i64) {
    match (bounds.min, bounds.max) {
        (Some(a), Some(b)) => {
            *min = a.min(b) as i64;
            *max = a.max(b) as i64;
        }
        (Some(a), None) => *min = a as i64,
        (None, Some(b)) => *max = b as i64,
        (None, None) => {}
    }
}

impl AxisScaleConfig {
    fn apply_range(&self, range: (f64, f64)) -> (f64, f64) {
        if self.reverse {
            (range.1, range.0)
        } else {
            range
        }
    }
}

fn axis_config(
    scales: &[ScaleIr],
    axis: AxisSelectorIr,
    view_domain: Option<AxisViewDomainIr>,
) -> AxisScaleConfig {
    let mut config = AxisScaleConfig {
        view_domain,
        ..AxisScaleConfig::default()
    };
    for scale in scales {
        if scale.target == ScaleTargetIr::Axis(axis) {
            if scale.scale_type.is_some() {
                config.scale_type = scale.scale_type;
            }
            if scale.domain.is_some() {
                config.domain = scale.domain;
            }
            if scale.categorical_domain.is_some() {
                config.categorical_domain = scale.categorical_domain.clone();
            }
            if scale.breaks.is_some() {
                config.breaks = scale.breaks.clone();
            }
            if scale.tick_interval.is_some() {
                config.tick_interval = scale.tick_interval;
            }
            if scale.break_labels.is_some() {
                config.break_labels = scale.break_labels.clone();
            }
            if scale.expansion.is_some() {
                config.expansion = scale.expansion;
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
    if let Some(values) = &config.breaks {
        scale.tick_values = values.clone();
    }
    if let Some(labels) = &config.break_labels {
        scale.tick_labels = labels.clone();
    }
    scale
}

fn apply_axis_breaks_to_temporal(scale: &mut TemporalScale, config: &AxisScaleConfig) {
    if let Some(values) = &config.breaks {
        scale.tick_values = values.iter().map(|value| *value as i64).collect();
        scale.tick_span = Some((scale.min, scale.max));
        // Declared breaks are exact (spec §16.11): no index thinning. Label
        // overlap is handled by guide-planning label thinning.
        scale.exact_ticks = true;
    } else if let Some(interval) = config.tick_interval {
        scale.tick_interval = Some(interval);
        // Generated calendar cadence (spec §16.11); exact breaks win above.
        let ticks = temporal::interval_ticks(scale.min, scale.max, interval);
        if ticks.len() >= 2 {
            scale.tick_values = ticks;
            scale.tick_span = Some((scale.min, scale.max));
            scale.exact_ticks = true;
        }
    }
    if let Some(labels) = &config.break_labels {
        scale.tick_labels = labels.clone();
    }
}

fn apply_numeric_expansion(expansion: Option<ScaleExpansionIr>, min: &mut f64, max: &mut f64) {
    let Some(expansion) = expansion else {
        return;
    };
    let span = (*max - *min).abs();
    if !span.is_finite() {
        return;
    }
    let pad = span * expansion.mult + expansion.add;
    *min -= pad;
    *max += pad;
}

fn apply_temporal_expansion(expansion: Option<ScaleExpansionIr>, min: &mut i64, max: &mut i64) {
    let Some(expansion) = expansion else {
        return;
    };
    let span = (*max - *min).abs() as f64;
    if !span.is_finite() {
        return;
    }
    let pad = (span * expansion.mult + expansion.add).round() as i64;
    *min -= pad;
    *max += pad;
}
