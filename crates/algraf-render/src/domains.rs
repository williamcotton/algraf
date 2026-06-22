//! Render-time domain requirements for position scales.
//!
//! This keeps geometry-specific scale requirements out of [`ScaledSpace`]:
//! geometries contribute data-dependent domain hints here, then scale training
//! consumes those hints while remaining a pure mapping layer.

use std::collections::HashMap;

use algraf_data::{DataType, Table};
use algraf_semantics::{
    AxisSelectorIr, FrameIr, GeometryIr, GeometryKind, PropertyKey, ScaleIr, ScaleTargetIr,
    ScaleTypeIr, TemporalTickIntervalIr,
};

use crate::helpers::{
    area_layout, bar_layout, frame_axis_index, number_setting_opt, vector_column_name, AreaLayout,
    BarLayout,
};
use crate::scale::{cell_category, cell_f64, cell_micros};
use crate::space::temporal::centered_bucket_bounds;
use crate::stats;

#[derive(Debug, Clone, Default)]
pub struct SpaceDomainHints {
    pub x: AxisDomainHints,
    pub y: AxisDomainHints,
}

#[derive(Debug, Clone, Default)]
pub struct AxisDomainHints {
    numeric: NumericDomainHints,
    band: BandDomainHints,
    temporal: TemporalDomainHints,
}

#[derive(Debug, Clone, Default)]
struct TemporalDomainHints {
    min: Option<i64>,
    max: Option<i64>,
    tick_values: Vec<i64>,
    tick_min: Option<i64>,
    tick_max: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct BandDomainHints {
    pad_inner: Option<f64>,
    pad_outer: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct NumericDomainHints {
    min: Option<f64>,
    max: Option<f64>,
    include_zero: bool,
    hard_min: bool,
    hard_max: bool,
}

impl AxisDomainHints {
    pub fn apply_numeric(&self, min: &mut f64, max: &mut f64) {
        if let Some(value) = self.numeric.min {
            if self.numeric.hard_min {
                *min = value;
            } else {
                *min = min.min(value);
            }
        }
        if let Some(value) = self.numeric.max {
            if self.numeric.hard_max {
                *max = value;
            } else {
                *max = max.max(value);
            }
        }
        if self.numeric.include_zero {
            *min = min.min(0.0);
            *max = max.max(0.0);
        }
    }

    pub fn apply_padding(&self, min: &mut f64, max: &mut f64) {
        let span = *max - *min;
        if !span.is_finite() || span <= f64::EPSILON {
            return;
        }

        let lower_is_zero = self.numeric.include_zero && min.abs() < f64::EPSILON;
        let upper_is_zero = self.numeric.include_zero && max.abs() < f64::EPSILON;
        let pad = span * 0.08;
        if !self.numeric.hard_min && !lower_is_zero {
            *min -= pad;
        }
        if !self.numeric.hard_max && !upper_is_zero {
            *max += pad;
        }
    }

    /// Union an externally computed numeric extent into this axis as a soft
    /// bound, so position scales align across overlaid spaces backed by
    /// different tables (spec §17.5). A bound that this axis has already locked
    /// (e.g. a `fill`-layout bar or a `Rect`) is left untouched.
    pub fn merge_numeric_extent(&mut self, min: f64, max: f64) {
        if !min.is_finite() || !max.is_finite() {
            return;
        }
        if !self.numeric.hard_min {
            self.numeric.min = Some(self.numeric.min.map_or(min, |m| m.min(min)));
        }
        if !self.numeric.hard_max {
            self.numeric.max = Some(self.numeric.max.map_or(max, |m| m.max(max)));
        }
    }

    /// Union an externally computed temporal extent into this axis (spec §17.5).
    pub fn merge_temporal_extent(&mut self, min: i64, max: i64) {
        self.add_temporal(min);
        self.add_temporal(max);
    }

    /// Whether geometry hints require the numeric domain to include zero.
    pub fn includes_zero(&self) -> bool {
        self.numeric.include_zero
    }

    /// Adopt a zero-baseline requirement from an overlaid space (spec §17.5).
    /// Sharing the requirement makes every overlaid space resolve the same
    /// padding at zero, so their trained domains stay identical. Locked bounds
    /// keep their exact values, so the requirement is skipped there.
    pub fn merge_include_zero(&mut self) {
        if !self.numeric.hard_min && !self.numeric.hard_max {
            self.numeric.include_zero = true;
        }
    }

    /// The numeric extent requested by geometry-specific domain hints, including
    /// a required zero baseline when present.
    pub fn numeric_extent(&self) -> Option<(f64, f64)> {
        let mut min = self.numeric.min;
        let mut max = self.numeric.max;
        if self.numeric.include_zero {
            min = Some(min.map_or(0.0, |value| value.min(0.0)));
            max = Some(max.map_or(0.0, |value| value.max(0.0)));
        }
        match (min, max) {
            (Some(lo), Some(hi)) => Some((lo, hi)),
            (Some(value), None) | (None, Some(value)) => Some((value, value)),
            (None, None) => None,
        }
    }

    /// The temporal extent requested by geometry-specific domain hints.
    pub fn temporal_extent(&self) -> Option<(i64, i64)> {
        match (self.temporal.min, self.temporal.max) {
            (Some(lo), Some(hi)) => Some((lo, hi)),
            (Some(value), None) | (None, Some(value)) => Some((value, value)),
            (None, None) => None,
        }
    }

    fn add_numeric(&mut self, value: f64) {
        if !value.is_finite() {
            return;
        }
        self.numeric.min = Some(self.numeric.min.map_or(value, |min| min.min(value)));
        self.numeric.max = Some(self.numeric.max.map_or(value, |max| max.max(value)));
    }

    fn include_zero(&mut self) {
        self.numeric.include_zero = true;
    }

    fn lock_bounds(&mut self) {
        self.numeric.hard_min = true;
        self.numeric.hard_max = true;
    }

    fn set_numeric_bounds(&mut self, min: f64, max: f64) {
        self.numeric.min = Some(min);
        self.numeric.max = Some(max);
        self.lock_bounds();
    }

    fn set_band_padding(&mut self, pad_inner: f64, pad_outer: f64) {
        self.band.pad_inner = Some(pad_inner);
        self.band.pad_outer = Some(pad_outer);
    }

    pub fn band_pad_inner(&self) -> Option<f64> {
        self.band.pad_inner
    }

    pub fn band_pad_outer(&self) -> Option<f64> {
        self.band.pad_outer
    }

    fn add_temporal(&mut self, micros: i64) {
        self.temporal.min = Some(self.temporal.min.map_or(micros, |m| m.min(micros)));
        self.temporal.max = Some(self.temporal.max.map_or(micros, |m| m.max(micros)));
    }

    fn add_temporal_interval_tick(&mut self, start: i64, end: i64) {
        let (lo, hi) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        self.add_temporal(lo);
        self.add_temporal(hi);
        self.temporal.tick_min = Some(self.temporal.tick_min.map_or(lo, |value| value.min(lo)));
        self.temporal.tick_max = Some(self.temporal.tick_max.map_or(hi, |value| value.max(hi)));

        let midpoint = lo + (hi - lo) / 2;
        if !self.temporal.tick_values.contains(&midpoint) {
            self.temporal.tick_values.push(midpoint);
        }
    }

    pub fn apply_temporal(&self, min: &mut i64, max: &mut i64) {
        if let Some(value) = self.temporal.min {
            *min = (*min).min(value);
        }
        if let Some(value) = self.temporal.max {
            *max = (*max).max(value);
        }
    }

    pub fn temporal_tick_values(&self) -> Vec<i64> {
        let mut values = self.temporal.tick_values.clone();
        values.sort_unstable();
        values.dedup();
        values
    }

    pub fn temporal_tick_span(&self) -> Option<(i64, i64)> {
        Some((self.temporal.tick_min?, self.temporal.tick_max?))
    }
}

pub fn train_space_domains(
    frame: &FrameIr,
    table: &dyn Table,
    geometries: &[GeometryIr],
    scales: &[ScaleIr],
) -> SpaceDomainHints {
    let mut hints = SpaceDomainHints::default();
    for geometry in geometries {
        match geometry.kind {
            GeometryKind::Bar => train_bar(frame, table, geometry, scales, &mut hints),
            GeometryKind::Rect => train_rect(table, geometry, &mut hints),
            GeometryKind::Violin => train_violin(frame, table, geometry, scales, &mut hints),
            // Area's baseline is a y-domain value: the polygon closes back to
            // it, so the trained y domain must reach the baseline or the
            // bottom edge will fall outside the plot rect. When the baseline
            // is zero, also suppress lower padding so the x-axis sits flush
            // against the area's bottom edge.
            GeometryKind::Area => {
                train_area(frame, table, geometry, &mut hints);
            }
            GeometryKind::HLine => {
                if let Some(y) = number_setting_opt(geometry, PropertyKey::Y) {
                    hints.y.add_numeric(y);
                }
            }
            GeometryKind::VLine => {
                if let Some(x) = number_setting_opt(geometry, PropertyKey::X) {
                    hints.x.add_numeric(x);
                }
            }
            // Tiles fill the band cell, so zero out band padding on both
            // axes — adjacent tiles should touch.
            GeometryKind::Tile => {
                hints.x.set_band_padding(0.0, 0.0);
                hints.y.set_band_padding(0.0, 0.0);
            }
            // Segment endpoints (literal values or column mappings) must stay
            // inside the plot rect (spec §14.19). Numeric/temporal endpoints
            // extend the continuous domain; categorical endpoints are already
            // covered by the frame's band domain.
            GeometryKind::Segment => {
                for row in 0..table.row_count().max(1) {
                    for property in [PropertyKey::X, PropertyKey::Xend] {
                        if let Some(value) = positional_value(geometry, property, table, row) {
                            hints.x.add_numeric(value);
                        }
                        if let Some(micros) = positional_temporal(geometry, property, table, row) {
                            hints.x.add_temporal(micros);
                        }
                    }
                    for property in [PropertyKey::Y, PropertyKey::Yend] {
                        if let Some(value) = positional_value(geometry, property, table, row) {
                            hints.y.add_numeric(value);
                        }
                        if let Some(micros) = positional_temporal(geometry, property, table, row) {
                            hints.y.add_temporal(micros);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    hints
}

fn train_area(
    frame: &FrameIr,
    table: &dyn Table,
    geometry: &GeometryIr,
    hints: &mut SpaceDomainHints,
) {
    let baseline = number_setting_opt(geometry, PropertyKey::Baseline).unwrap_or(0.0);
    hints.y.add_numeric(baseline);
    if baseline.abs() < f64::EPSILON {
        hints.y.include_zero();
    }
    if area_layout(geometry) == AreaLayout::Identity {
        return;
    }
    let Some(x_axis) = frame_axis_index(frame, 0) else {
        return;
    };
    let Some(y_col) = frame_axis_index(frame, 1).and_then(vector_column_name) else {
        return;
    };
    let mut positive: HashMap<String, f64> = HashMap::new();
    let mut negative: HashMap<String, f64> = HashMap::new();
    for row in 0..table.row_count() {
        let Some(key) = area_stack_key(x_axis, table, row) else {
            continue;
        };
        let Some(value) = cell_f64(table, y_col, row) else {
            continue;
        };
        if value >= 0.0 {
            *positive.entry(key).or_insert(0.0) += value;
        } else {
            *negative.entry(key).or_insert(0.0) += value;
        }
    }
    match area_layout(geometry) {
        AreaLayout::Identity => {}
        AreaLayout::Stack => {
            for value in positive.values().chain(negative.values()) {
                hints.y.add_numeric(baseline + *value);
            }
        }
        AreaLayout::Fill => {
            let has_positive = positive.values().any(|value| value.abs() > f64::EPSILON);
            let has_negative = negative.values().any(|value| value.abs() > f64::EPSILON);
            let min = if has_negative {
                baseline - 1.0
            } else {
                baseline
            };
            let max = if has_positive {
                baseline + 1.0
            } else {
                baseline
            };
            hints.y.set_numeric_bounds(min, max);
        }
    }
}

fn train_violin(
    frame: &FrameIr,
    table: &dyn Table,
    geometry: &GeometryIr,
    scales: &[ScaleIr],
    hints: &mut SpaceDomainHints,
) {
    let Some((orientation, group_axis, value_col)) = categorical_value_axes(frame, scales) else {
        return;
    };
    let mut groups: HashMap<String, Vec<f64>> = HashMap::new();
    for row in 0..table.row_count() {
        let Some(key) = axis_group_key(group_axis, table, row) else {
            continue;
        };
        let Some(value) = cell_f64(table, value_col, row) else {
            continue;
        };
        groups.entry(key).or_default().push(value);
    }
    let options = stats::DensityOptions {
        bandwidth: number_setting_opt(geometry, PropertyKey::Bandwidth)
            .filter(|value| *value > 0.0),
        grid_points: number_setting_opt(geometry, PropertyKey::N)
            .filter(|value| *value >= 2.0)
            .map(|value| value.round() as usize)
            .unwrap_or(256),
    };
    for values in groups.values_mut() {
        for point in stats::density_values(values, options) {
            match orientation {
                DomainOrientation::Vertical => hints.y.add_numeric(point.x),
                DomainOrientation::Horizontal => hints.x.add_numeric(point.x),
            }
        }
    }
}

fn train_bar(
    frame: &FrameIr,
    table: &dyn Table,
    geometry: &GeometryIr,
    scales: &[ScaleIr],
    hints: &mut SpaceDomainHints,
) {
    let Some((orientation, group_axis, value_col)) = categorical_value_axes(frame, scales) else {
        if !train_temporal_bar(frame, table, geometry, scales, hints) {
            hints.y.include_zero();
        }
        return;
    };
    match orientation {
        DomainOrientation::Vertical => hints.y.include_zero(),
        DomainOrientation::Horizontal => hints.x.include_zero(),
    }

    if bar_layout(geometry) == BarLayout::Fill {
        match orientation {
            DomainOrientation::Vertical => hints.y.set_numeric_bounds(0.0, 1.0),
            DomainOrientation::Horizontal => hints.x.set_numeric_bounds(0.0, 1.0),
        }
        return;
    }

    if !is_stacked(geometry) {
        return;
    }

    train_stacked_bar_value_domain(table, orientation, group_axis, value_col, hints);
}

fn train_temporal_bar(
    frame: &FrameIr,
    table: &dyn Table,
    geometry: &GeometryIr,
    scales: &[ScaleIr],
    hints: &mut SpaceDomainHints,
) -> bool {
    let Some((orientation, position_axis, value_col, interval)) =
        temporal_value_axes(frame, scales)
    else {
        return false;
    };
    match orientation {
        DomainOrientation::Vertical => hints.y.include_zero(),
        DomainOrientation::Horizontal => hints.x.include_zero(),
    }

    if let Some(interval) = interval {
        train_temporal_bar_position_domain(table, position_axis, interval, orientation, hints);
    }

    if bar_layout(geometry) == BarLayout::Fill {
        match orientation {
            DomainOrientation::Vertical => hints.y.set_numeric_bounds(0.0, 1.0),
            DomainOrientation::Horizontal => hints.x.set_numeric_bounds(0.0, 1.0),
        }
        return true;
    }

    if is_stacked(geometry) {
        train_stacked_bar_value_domain(table, orientation, position_axis, value_col, hints);
    }
    true
}

fn train_stacked_bar_value_domain(
    table: &dyn Table,
    orientation: DomainOrientation,
    group_axis: &FrameIr,
    value_col: &str,
    hints: &mut SpaceDomainHints,
) {
    let mut positive: HashMap<String, f64> = HashMap::new();
    let mut negative: HashMap<String, f64> = HashMap::new();
    for row in 0..table.row_count() {
        let Some(key) = axis_group_key(group_axis, table, row) else {
            continue;
        };
        let Some(value) = cell_f64(table, value_col, row) else {
            continue;
        };
        if value >= 0.0 {
            let total = positive.entry(key).or_insert(0.0);
            *total += value;
            match orientation {
                DomainOrientation::Vertical => hints.y.add_numeric(*total),
                DomainOrientation::Horizontal => hints.x.add_numeric(*total),
            }
        } else {
            let total = negative.entry(key).or_insert(0.0);
            *total += value;
            match orientation {
                DomainOrientation::Vertical => hints.y.add_numeric(*total),
                DomainOrientation::Horizontal => hints.x.add_numeric(*total),
            }
        }
    }
}

fn train_temporal_bar_position_domain(
    table: &dyn Table,
    position_axis: &FrameIr,
    interval: TemporalTickIntervalIr,
    orientation: DomainOrientation,
    hints: &mut SpaceDomainHints,
) {
    for row in 0..table.row_count() {
        let Some(anchor) = frame_temporal_micros(position_axis, table, row) else {
            continue;
        };
        let Some((start, end)) = centered_bucket_bounds(anchor, interval) else {
            continue;
        };
        let axis = match orientation {
            DomainOrientation::Vertical => &mut hints.x,
            DomainOrientation::Horizontal => &mut hints.y,
        };
        axis.add_temporal(start);
        axis.add_temporal(end);
    }
}

fn train_rect(table: &dyn Table, geometry: &GeometryIr, hints: &mut SpaceDomainHints) {
    hints.x.lock_bounds();
    let use_interval_center_ticks = rect_uses_bin_boundaries(geometry);
    for row in 0..table.row_count() {
        if use_interval_center_ticks {
            if let (Some(start), Some(end)) = (
                positional_temporal(geometry, PropertyKey::Xmin, table, row),
                positional_temporal(geometry, PropertyKey::Xmax, table, row),
            ) {
                hints.x.add_temporal_interval_tick(start, end);
            }
        }
        for property in [PropertyKey::Xmin, PropertyKey::Xmax] {
            if let Some(value) = positional_value(geometry, property, table, row) {
                hints.x.add_numeric(value);
            }
            if let Some(micros) = positional_temporal(geometry, property, table, row) {
                hints.x.add_temporal(micros);
            }
        }
        for property in [PropertyKey::Ymin, PropertyKey::Ymax] {
            if let Some(value) = positional_value(geometry, property, table, row) {
                hints.y.add_numeric(value);
                if value.abs() < f64::EPSILON {
                    hints.y.include_zero();
                }
            }
            if let Some(micros) = positional_temporal(geometry, property, table, row) {
                hints.y.add_temporal(micros);
            }
        }
    }
}

fn rect_uses_bin_boundaries(geometry: &GeometryIr) -> bool {
    mapping_column_name(geometry, PropertyKey::Xmin) == Some("bin_start")
        && mapping_column_name(geometry, PropertyKey::Xmax) == Some("bin_end")
}

fn mapping_column_name(geometry: &GeometryIr, property: PropertyKey) -> Option<&str> {
    geometry
        .mappings
        .iter()
        .find(|mapping| mapping.aesthetic == property)
        .map(|mapping| mapping.column.name.as_str())
}

fn is_stacked(geometry: &GeometryIr) -> bool {
    matches!(bar_layout(geometry), BarLayout::Stack | BarLayout::Fill)
}

#[derive(Debug, Clone, Copy)]
enum DomainOrientation {
    Vertical,
    Horizontal,
}

fn categorical_value_axes<'a>(
    frame: &'a FrameIr,
    scales: &[ScaleIr],
) -> Option<(DomainOrientation, &'a FrameIr, &'a str)> {
    let x_axis = frame_axis_index(frame, 0)?;
    let y_axis = frame_axis_index(frame, 1)?;
    if frame_axis_is_band(x_axis, scales, AxisSelectorIr::X) {
        if let Some(value_col) = vector_column_name(y_axis) {
            return Some((DomainOrientation::Vertical, x_axis, value_col));
        }
    }
    if frame_axis_is_band(y_axis, scales, AxisSelectorIr::Y) {
        if let Some(value_col) = vector_column_name(x_axis) {
            return Some((DomainOrientation::Horizontal, y_axis, value_col));
        }
    }
    None
}

fn temporal_value_axes<'a>(
    frame: &'a FrameIr,
    scales: &[ScaleIr],
) -> Option<(
    DomainOrientation,
    &'a FrameIr,
    &'a str,
    Option<TemporalTickIntervalIr>,
)> {
    let x_axis = frame_axis_index(frame, 0)?;
    let y_axis = frame_axis_index(frame, 1)?;
    if frame_axis_is_temporal(x_axis, scales, AxisSelectorIr::X) {
        if let Some(value_col) = numeric_value_column(y_axis) {
            return Some((
                DomainOrientation::Vertical,
                x_axis,
                value_col,
                tick_interval_for_axis(scales, AxisSelectorIr::X),
            ));
        }
    }
    if frame_axis_is_temporal(y_axis, scales, AxisSelectorIr::Y) {
        if let Some(value_col) = numeric_value_column(x_axis) {
            return Some((
                DomainOrientation::Horizontal,
                y_axis,
                value_col,
                tick_interval_for_axis(scales, AxisSelectorIr::Y),
            ));
        }
    }
    None
}

fn numeric_value_column(frame: &FrameIr) -> Option<&str> {
    match frame {
        FrameIr::Vector(column)
            if matches!(
                column.dtype,
                DataType::Integer | DataType::Float | DataType::Unknown
            ) =>
        {
            Some(column.name.as_str())
        }
        FrameIr::Union(_) => vector_column_name(frame),
        _ => None,
    }
}

fn frame_axis_is_temporal(frame: &FrameIr, scales: &[ScaleIr], axis: AxisSelectorIr) -> bool {
    match frame {
        FrameIr::Vector(column) => {
            !categorical_axis_override(scales, axis)
                && (column.dtype == DataType::Temporal || temporal_axis_override(scales, axis))
        }
        FrameIr::Nested { outer, .. } => frame_axis_is_temporal(outer, scales, axis),
        FrameIr::Cartesian(_) | FrameIr::Union(_) | FrameIr::Invalid => false,
    }
}

fn frame_axis_is_band(frame: &FrameIr, scales: &[ScaleIr], axis: AxisSelectorIr) -> bool {
    match frame {
        FrameIr::Vector(column) => {
            column.dtype.is_categorical()
                || column.dtype == DataType::Unknown
                || (column.dtype != DataType::Geometry && categorical_axis_override(scales, axis))
        }
        FrameIr::Nested { outer, .. } => !frame_axis_is_temporal(outer, scales, axis),
        FrameIr::Cartesian(_) | FrameIr::Union(_) | FrameIr::Invalid => false,
    }
}

fn categorical_axis_override(scales: &[ScaleIr], axis: AxisSelectorIr) -> bool {
    let mut forced = false;
    for scale in scales {
        if scale.target == ScaleTargetIr::Axis(axis) && scale.scale_type.is_some() {
            forced = scale.scale_type == Some(ScaleTypeIr::Categorical);
        }
    }
    forced
}

fn temporal_axis_override(scales: &[ScaleIr], axis: AxisSelectorIr) -> bool {
    let mut forced = false;
    for scale in scales {
        if scale.target == ScaleTargetIr::Axis(axis) && scale.scale_type.is_some() {
            forced = scale.scale_type == Some(ScaleTypeIr::Temporal);
        }
    }
    forced
}

fn tick_interval_for_axis(
    scales: &[ScaleIr],
    axis: AxisSelectorIr,
) -> Option<TemporalTickIntervalIr> {
    let mut interval = None;
    for scale in scales {
        if scale.target == ScaleTargetIr::Axis(axis) && scale.tick_interval.is_some() {
            interval = scale.tick_interval;
        }
    }
    interval
}

fn axis_group_key(frame: &FrameIr, table: &dyn Table, row: usize) -> Option<String> {
    match frame {
        FrameIr::Vector(column) => cell_category(table, &column.name, row),
        FrameIr::Nested { outer, inner } => {
            let outer_col = vector_column_name(outer)?;
            let inner_col = vector_column_name(inner)?;
            Some(format!(
                "{}\u{1f}{}",
                cell_category(table, outer_col, row)?,
                cell_category(table, inner_col, row)?
            ))
        }
        _ => None,
    }
}

fn frame_temporal_micros(frame: &FrameIr, table: &dyn Table, row: usize) -> Option<i64> {
    match frame {
        FrameIr::Vector(column) => cell_micros(table, &column.name, row),
        FrameIr::Nested { outer, .. } => frame_temporal_micros(outer, table, row),
        _ => None,
    }
}

fn area_stack_key(frame: &FrameIr, table: &dyn Table, row: usize) -> Option<String> {
    match frame {
        FrameIr::Vector(column) => match column.dtype {
            DataType::Integer | DataType::Float => {
                cell_f64(table, &column.name, row).map(|value| format!("{:016x}", value.to_bits()))
            }
            DataType::Temporal => {
                cell_micros(table, &column.name, row).map(|value| value.to_string())
            }
            _ => cell_category(table, &column.name, row),
        },
        FrameIr::Nested { outer, .. } => area_stack_key(outer, table, row),
        _ => Some(row.to_string()),
    }
}

fn positional_value(
    geometry: &GeometryIr,
    property: PropertyKey,
    table: &dyn Table,
    row: usize,
) -> Option<f64> {
    if let Some(mapping) = geometry
        .mappings
        .iter()
        .find(|mapping| mapping.aesthetic == property)
    {
        return cell_f64(table, &mapping.column.name, row);
    }
    number_setting_opt(geometry, property)
}

fn positional_temporal(
    geometry: &GeometryIr,
    property: PropertyKey,
    table: &dyn Table,
    row: usize,
) -> Option<i64> {
    let mapping = geometry
        .mappings
        .iter()
        .find(|mapping| mapping.aesthetic == property)?;
    cell_micros(table, &mapping.column.name, row)
}

#[cfg(test)]
mod tests {
    use algraf_data::{read_csv_str, Table};
    use algraf_semantics::analyze;
    use algraf_syntax::parse;

    use super::train_space_domains;

    #[test]
    fn area_stack_domain_training_uses_stacked_totals() {
        let frame = read_csv_str("x,y,series\n1,2,A\n1,3,B\n2,4,A\n2,1,B\n")
            .expect("csv")
            .frame;
        let parsed = parse(
            "Chart(data: \"p.csv\") { Space(x * y) { Area(fill: series, layout: \"stack\") } }",
        );
        let ir = analyze(&parsed.syntax(), frame.schema()).ir.expect("ir");
        let space = &ir.spaces[0];
        let hints = train_space_domains(&space.frame, &frame, &space.geometries, &space.scales);

        assert_eq!(hints.y.numeric_extent(), Some((0.0, 5.0)));
    }
}
