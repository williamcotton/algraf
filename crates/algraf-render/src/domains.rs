//! Render-time domain requirements for position scales.
//!
//! This keeps geometry-specific scale requirements out of [`ScaledSpace`]:
//! geometries contribute data-dependent domain hints here, then scale training
//! consumes those hints while remaining a pure mapping layer.

use std::collections::HashMap;

use algraf_data::Table;
use algraf_semantics::{FrameIr, GeometryIr, GeometryKind, SettingValue};

use crate::scale::{cell_category, cell_f64};

#[derive(Debug, Clone, Default)]
pub struct SpaceDomainHints {
    pub x: AxisDomainHints,
    pub y: AxisDomainHints,
}

#[derive(Debug, Clone, Default)]
pub struct AxisDomainHints {
    numeric: NumericDomainHints,
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
}

pub fn train_space_domains(
    frame: &FrameIr,
    table: &dyn Table,
    geometries: &[GeometryIr],
) -> SpaceDomainHints {
    let mut hints = SpaceDomainHints::default();
    for geometry in geometries {
        match geometry.kind {
            GeometryKind::Bar => train_bar(frame, table, geometry, &mut hints),
            GeometryKind::Rect => train_rect(table, geometry, &mut hints),
            GeometryKind::HLine => {
                if let Some(y) = numeric_setting(geometry, "y") {
                    hints.y.add_numeric(y);
                }
            }
            GeometryKind::VLine => {
                if let Some(x) = numeric_setting(geometry, "x") {
                    hints.x.add_numeric(x);
                }
            }
            _ => {}
        }
    }
    hints
}

fn train_bar(
    frame: &FrameIr,
    table: &dyn Table,
    geometry: &GeometryIr,
    hints: &mut SpaceDomainHints,
) {
    hints.y.include_zero();

    if bar_layout(geometry) == BarLayout::Fill {
        hints.y.set_numeric_bounds(0.0, 1.0);
        return;
    }

    if !is_stacked(geometry) {
        return;
    }

    let Some(x_axis) = frame_axis(frame, 0) else {
        return;
    };
    let Some(y_col) = frame_axis(frame, 1).and_then(vector_column) else {
        return;
    };

    let mut positive: HashMap<String, f64> = HashMap::new();
    let mut negative: HashMap<String, f64> = HashMap::new();
    for row in 0..table.row_count() {
        let Some(key) = axis_group_key(x_axis, table, row) else {
            continue;
        };
        let Some(value) = cell_f64(table, y_col, row) else {
            continue;
        };
        if value >= 0.0 {
            let total = positive.entry(key).or_insert(0.0);
            *total += value;
            hints.y.add_numeric(*total);
        } else {
            let total = negative.entry(key).or_insert(0.0);
            *total += value;
            hints.y.add_numeric(*total);
        }
    }
}

fn train_rect(table: &dyn Table, geometry: &GeometryIr, hints: &mut SpaceDomainHints) {
    hints.x.lock_bounds();
    for row in 0..table.row_count() {
        for property in ["xmin", "xmax"] {
            if let Some(value) = positional_value(geometry, property, table, row) {
                hints.x.add_numeric(value);
            }
        }
        for property in ["ymin", "ymax"] {
            if let Some(value) = positional_value(geometry, property, table, row) {
                hints.y.add_numeric(value);
                if value.abs() < f64::EPSILON {
                    hints.y.include_zero();
                }
            }
        }
    }
}

fn is_stacked(geometry: &GeometryIr) -> bool {
    matches!(bar_layout(geometry), BarLayout::Stack | BarLayout::Fill)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BarLayout {
    Identity,
    Stack,
    Fill,
}

fn bar_layout(geometry: &GeometryIr) -> BarLayout {
    geometry
        .settings
        .iter()
        .find(|setting| setting.name == "layout")
        .and_then(|setting| match &setting.value {
            SettingValue::String(value) if value == "stack" => Some(BarLayout::Stack),
            SettingValue::String(value) if value == "fill" => Some(BarLayout::Fill),
            _ => None,
        })
        .unwrap_or(BarLayout::Identity)
}

fn frame_axis(frame: &FrameIr, index: usize) -> Option<&FrameIr> {
    match frame {
        FrameIr::Cartesian(axes) => axes.get(index),
        _ if index == 0 => Some(frame),
        _ => None,
    }
}

fn vector_column(frame: &FrameIr) -> Option<&str> {
    match frame {
        FrameIr::Vector(column) => Some(&column.name),
        _ => None,
    }
}

fn axis_group_key(frame: &FrameIr, table: &dyn Table, row: usize) -> Option<String> {
    match frame {
        FrameIr::Vector(column) => cell_category(table, &column.name, row),
        FrameIr::Nested { outer, .. } => {
            vector_column(outer).and_then(|col| cell_category(table, col, row))
        }
        _ => None,
    }
}

fn positional_value(
    geometry: &GeometryIr,
    property: &str,
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
    numeric_setting(geometry, property)
}

fn numeric_setting(geometry: &GeometryIr, property: &str) -> Option<f64> {
    geometry
        .settings
        .iter()
        .find(|setting| setting.name == property)
        .and_then(|setting| match setting.value {
            SettingValue::Number(value) => Some(value),
            _ => None,
        })
}
