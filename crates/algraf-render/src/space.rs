//! Trained spatial context built from a frame IR (spec §16.12, §16.1).
//!
//! [`ScaledSpace`] hides whether a position scale is continuous, temporal,
//! banded, or nested: geometries call `resolve_x`/`resolve_y` and the bandwidth
//! accessors without knowing the underlying scale kind.

use algraf_data::{DataType, Table, TemporalPrecision};
use algraf_semantics::{
    AxisSelectorIr, ColumnRef, FrameIr, ScaleIr, ScaleTargetIr, ScaleTypeIr, TemporalFormatIr,
};
use chrono::{DateTime, Datelike, NaiveDate};

use crate::domains::{AxisDomainHints, SpaceDomainHints};
use crate::projection::SpatialScale;
use crate::scale::{
    categorical_domain, cell_category, cell_f64, cell_micros, numeric_domain, temporal_domain,
    BandScale, ContinuousScale, NestedBandScale, TemporalScale,
};

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

    pub fn ticks_with_format(&self, format: Option<TemporalFormatIr>) -> Vec<(f64, String)> {
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
    format: Option<TemporalFormatIr>,
) -> String {
    match DateTime::from_timestamp_micros(micros) {
        Some(dt) => match format {
            Some(TemporalFormatIr::IsoDate) => dt.format("%Y-%m-%d").to_string(),
            Some(TemporalFormatIr::IsoMinute) => dt.format("%Y-%m-%d %H:%M").to_string(),
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
    }

    (0..=5)
        .map(|i| scale.min + (scale.max - scale.min) * i / 5)
        .collect()
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

/// A trained 2D (or 1D) position context for one space. A spatial (map) space
/// carries a [`SpatialScale`] instead of independent x/y axes (spec §16.15);
/// the placeholder `x` axis is never drawn because spatial panels skip axes and
/// grids.
pub struct ScaledSpace {
    pub x: AxisScale,
    pub y: Option<AxisScale>,
    /// Present for a spatial space: position comes from projecting geographic
    /// coordinates rather than mapping the x/y axes.
    pub spatial: Option<SpatialScale>,
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
                    spatial: None,
                })
            }
            FrameIr::Cartesian(axes) if axes.len() == 1 => {
                let x = build_axis(&axes[0], table, x_range, Some(&hints.x), &x_config)?;
                Some(ScaledSpace {
                    x,
                    y: None,
                    spatial: None,
                })
            }
            FrameIr::Vector(_) | FrameIr::Nested { .. } | FrameIr::Union(_) => {
                let x = build_axis(frame, table, x_range, Some(&hints.x), &x_config)?;
                Some(ScaledSpace {
                    x,
                    y: None,
                    spatial: None,
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
            spatial: Some(spatial),
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
        self.x.resolve(table, row)
    }

    pub fn resolve_y(&self, table: &dyn Table, row: usize) -> Option<f64> {
        if let Some(spatial) = &self.spatial {
            return self.project_row(spatial, table, row).map(|(_, y)| y);
        }
        self.y.as_ref()?.resolve(table, row)
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
