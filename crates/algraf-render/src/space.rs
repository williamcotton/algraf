//! Trained spatial context built from a frame IR (spec §16.12, §16.1).
//!
//! [`ScaledSpace`] hides whether a position scale is continuous, temporal,
//! banded, or nested: geometries call `resolve_x`/`resolve_y` and the bandwidth
//! accessors without knowing the underlying scale kind.

use algraf_data::{DataType, Table, TemporalPrecision};
use algraf_semantics::{ColumnRef, FrameIr};
use chrono::{DateTime, Datelike, NaiveDate};

use crate::domains::{AxisDomainHints, SpaceDomainHints};
use crate::scale::{
    categorical_domain, cell_category, cell_f64, cell_micros, nice_ticks, numeric_domain,
    temporal_domain, BandScale, ContinuousScale, NestedBandScale, TemporalScale,
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
            AxisScale::Union { .. } => None,
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
            AxisScale::Temporal { scale, .. } => Some(scale.map(value as i64)),
            _ => None,
        }
    }

    /// The axis title (column name or joined union member names).
    pub fn label(&self) -> String {
        let raw = match self {
            AxisScale::Continuous { col, .. }
            | AxisScale::Temporal { col, .. }
            | AxisScale::Band { col, .. } => col,
            AxisScale::NestedBand { outer_col, .. } => outer_col,
            AxisScale::Union { label, .. } => label,
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
            AxisScale::Union { .. } => None,
        }
    }

    pub fn is_band(&self) -> bool {
        matches!(self, AxisScale::Band { .. } | AxisScale::NestedBand { .. })
    }

    /// Tick positions and labels for guide rendering (spec §19).
    pub fn ticks(&self) -> Vec<(f64, String)> {
        match self {
            AxisScale::Continuous { scale, .. } | AxisScale::Union { scale, .. } => {
                nice_ticks(scale.min, scale.max, 6)
                    .into_iter()
                    .filter(|t| *t >= scale.min - f64::EPSILON && *t <= scale.max + f64::EPSILON)
                    .map(|t| (scale.map(t), crate::svg::num(t)))
                    .collect()
            }
            AxisScale::Temporal { scale, .. } => temporal_ticks(scale)
                .into_iter()
                .map(|micros| (scale.map(micros), format_temporal(micros, scale.precision)))
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

fn format_temporal(micros: i64, precision: TemporalPrecision) -> String {
    match DateTime::from_timestamp_micros(micros) {
        Some(dt) => match precision {
            TemporalPrecision::Date => dt.format("%Y-%m-%d").to_string(),
            TemporalPrecision::DateTime => dt.format("%Y-%m-%d %H:%M").to_string(),
        },
        None => String::new(),
    }
}

fn temporal_ticks(scale: &TemporalScale) -> Vec<i64> {
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

fn daily_ticks(min: i64, max: i64) -> Option<Vec<i64>> {
    let start = DateTime::from_timestamp_micros(min)?.date_naive();
    let end = DateTime::from_timestamp_micros(max)?.date_naive();
    let span_days = end.signed_duration_since(start).num_days().abs();
    if !(1..=10).contains(&span_days) {
        return None;
    }

    let mut ticks = Vec::new();
    for offset in 0..=span_days {
        let day = start.checked_add_days(chrono::Days::new(offset as u64))?;
        let micros = day.and_hms_opt(0, 0, 0)?.and_utc().timestamp_micros();
        if micros >= min && micros <= max {
            ticks.push(micros);
        }
    }

    (2..=11).contains(&ticks.len()).then_some(ticks)
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

/// A trained 2D (or 1D) spatial context for one space.
pub struct ScaledSpace {
    pub x: AxisScale,
    pub y: Option<AxisScale>,
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
    ) -> Option<ScaledSpace> {
        match frame {
            FrameIr::Cartesian(axes) if axes.len() >= 2 => {
                let x = build_axis(&axes[0], table, x_range, Some(&hints.x))?;
                let y = build_axis(&axes[1], table, y_range, Some(&hints.y))?;
                Some(ScaledSpace { x, y: Some(y) })
            }
            FrameIr::Cartesian(axes) if axes.len() == 1 => {
                let x = build_axis(&axes[0], table, x_range, Some(&hints.x))?;
                Some(ScaledSpace { x, y: None })
            }
            FrameIr::Vector(_) | FrameIr::Nested { .. } | FrameIr::Union(_) => {
                let x = build_axis(frame, table, x_range, Some(&hints.x))?;
                Some(ScaledSpace { x, y: None })
            }
            _ => None,
        }
    }

    pub fn resolve_x(&self, table: &dyn Table, row: usize) -> Option<f64> {
        self.x.resolve(table, row)
    }

    pub fn resolve_y(&self, table: &dyn Table, row: usize) -> Option<f64> {
        self.y.as_ref()?.resolve(table, row)
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
}

/// Build a single axis scale from a frame sub-expression.
fn build_axis(
    frame: &FrameIr,
    table: &dyn Table,
    range: (f64, f64),
    hints: Option<&AxisDomainHints>,
) -> Option<AxisScale> {
    match frame {
        FrameIr::Vector(col) => Some(build_vector_axis(col, table, range, hints)),
        FrameIr::Nested { outer, inner } => {
            if let (FrameIr::Vector(o), FrameIr::Vector(i)) = (outer.as_ref(), inner.as_ref()) {
                let outer_cats = categorical_domain(table, &o.name);
                let inner_cats = categorical_domain(table, &i.name);
                let outer_band = BandScale::new(outer_cats, range);
                Some(AxisScale::NestedBand {
                    outer_col: o.name.clone(),
                    inner_col: i.name.clone(),
                    scale: NestedBandScale::new(outer_band, inner_cats),
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
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            for c in &cols {
                if let Some((lo, hi)) = numeric_domain(table, &c.name) {
                    min = min.min(lo);
                    max = max.max(hi);
                }
            }
            let label = cols
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>()
                .join(" + ");
            if min > max {
                min = 0.0;
                max = 1.0;
            }
            if let Some(hints) = hints {
                hints.apply_numeric(&mut min, &mut max);
                hints.apply_padding(&mut min, &mut max);
            }
            Some(AxisScale::Union {
                label,
                scale: ContinuousScale::new(min, max, range),
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
) -> AxisScale {
    match col.dtype {
        DataType::Integer | DataType::Float => {
            let (mut min, mut max) = numeric_domain(table, &col.name).unwrap_or((0.0, 1.0));
            if let Some(hints) = hints {
                hints.apply_numeric(&mut min, &mut max);
                hints.apply_padding(&mut min, &mut max);
            }
            AxisScale::Continuous {
                col: col.name.clone(),
                scale: ContinuousScale::new(min, max, range),
            }
        }
        DataType::Temporal => {
            let (min, max, precision) =
                temporal_domain(table, &col.name).unwrap_or((0, 1, TemporalPrecision::Date));
            AxisScale::Temporal {
                col: col.name.clone(),
                scale: TemporalScale::new(min, max, range, precision),
            }
        }
        _ => {
            let cats = categorical_domain(table, &col.name);
            AxisScale::Band {
                col: col.name.clone(),
                scale: BandScale::new(cats, range),
            }
        }
    }
}
