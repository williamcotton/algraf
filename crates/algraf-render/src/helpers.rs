//! Shared render-time helpers for IR frame and setting inspection.

use algraf_semantics::{AxisSelectorIr, ColumnRef, FrameIr, GeometryIr, PropertyKey, SettingValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BarLayout {
    Identity,
    Stack,
    Fill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AreaLayout {
    Identity,
    Stack,
    Fill,
}

pub(crate) fn bar_layout(geometry: &GeometryIr) -> BarLayout {
    geometry
        .settings
        .iter()
        .find(|setting| setting.name == PropertyKey::Layout)
        .and_then(|setting| match &setting.value {
            SettingValue::String(value) if value == "stack" => Some(BarLayout::Stack),
            SettingValue::String(value) if value == "fill" => Some(BarLayout::Fill),
            _ => None,
        })
        .unwrap_or(BarLayout::Identity)
}

pub(crate) fn area_layout(geometry: &GeometryIr) -> AreaLayout {
    geometry
        .settings
        .iter()
        .find(|setting| setting.name == PropertyKey::Layout)
        .and_then(|setting| match &setting.value {
            SettingValue::String(value) if value == "stack" => Some(AreaLayout::Stack),
            SettingValue::String(value) if value == "fill" => Some(AreaLayout::Fill),
            _ => None,
        })
        .unwrap_or(AreaLayout::Identity)
}

pub(crate) fn frame_axis(frame: &FrameIr, axis: AxisSelectorIr) -> Option<&FrameIr> {
    match axis {
        AxisSelectorIr::X => frame_axis_index(frame, 0),
        AxisSelectorIr::Y => frame_axis_index(frame, 1),
    }
}

pub(crate) fn frame_axis_index(frame: &FrameIr, index: usize) -> Option<&FrameIr> {
    match frame {
        FrameIr::Cartesian(axes) => axes.get(index),
        _ if index == 0 => Some(frame),
        _ => None,
    }
}

pub(crate) fn vector_column(frame: &FrameIr) -> Option<&ColumnRef> {
    match frame {
        FrameIr::Vector(column) => Some(column),
        _ => None,
    }
}

pub(crate) fn vector_column_name(frame: &FrameIr) -> Option<&str> {
    vector_column(frame).map(|column| column.name.as_str())
}

pub(crate) fn number_setting_opt(geometry: &GeometryIr, key: PropertyKey) -> Option<f64> {
    geometry
        .settings
        .iter()
        .find(|setting| setting.name == key)
        .and_then(|setting| match setting.value {
            SettingValue::Number(value) => Some(value),
            _ => None,
        })
}

pub(crate) fn string_setting(geometry: &GeometryIr, key: PropertyKey) -> Option<String> {
    geometry
        .settings
        .iter()
        .find(|setting| setting.name == key)
        .and_then(|setting| match &setting.value {
            SettingValue::String(value) => Some(value.clone()),
            _ => None,
        })
}

pub(crate) fn bool_setting(geometry: &GeometryIr, key: PropertyKey, default: bool) -> bool {
    geometry
        .settings
        .iter()
        .find(|setting| setting.name == key)
        .and_then(|setting| match setting.value {
            SettingValue::Bool(value) => Some(value),
            _ => None,
        })
        .unwrap_or(default)
}

pub(crate) fn number_array_setting(geometry: &GeometryIr, key: PropertyKey) -> Option<Vec<f64>> {
    geometry
        .settings
        .iter()
        .find(|setting| setting.name == key)
        .and_then(|setting| match &setting.value {
            SettingValue::NumberArray(values) => Some(values.clone()),
            _ => None,
        })
}
