use algraf_data::Table;
use algraf_semantics::{GeometryIr, PropertyKey, SettingValue};

use crate::aes::ColorSpec;
use crate::scale::{cell_category, cell_f64, cell_micros};
use crate::sink::{Dash, MarkInteraction, MarkSink, Stroke};
use crate::space::{AxisScale, ScaledSpace};

pub(crate) const DEFAULT_FILL: &str = "#4E79A7";
pub(super) const DEFAULT_STROKE: &str = "#333333";
/// Default output range (px) for a mapped `strokeWidth` scale (spec §16.8).
pub(crate) const DEFAULT_STROKE_WIDTH_RANGE: (f64, f64) = (0.5, 4.0);
/// Default output range (radius px) for a mapped `size` scale (spec §16.8).
pub(crate) const DEFAULT_SIZE_RANGE: (f64, f64) = (2.0, 8.0);

/// Build the inert per-datum interaction metadata for a mark (spec §14.25,
/// §24.6). Returns an empty [`MarkInteraction`] when the geometry declares none,
/// so callers can wrap every datum unconditionally. Tooltip text is a stable,
/// locale-independent sequence of `label: value` lines, one per declared column.
pub(super) fn mark_interaction(geo: &GeometryIr, table: &dyn Table, row: usize) -> MarkInteraction {
    let interaction = &geo.interaction;
    if interaction.is_empty() {
        return MarkInteraction::default();
    }
    let tooltip = (!interaction.tooltip.is_empty()).then(|| {
        interaction
            .tooltip
            .iter()
            .map(|col| {
                let value = cell_category(table, &col.name, row).unwrap_or_default();
                format!("{}: {}", col.name, value)
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    let highlight = interaction
        .highlight
        .as_ref()
        .and_then(|col| cell_category(table, &col.name, row));
    MarkInteraction { tooltip, highlight }
}

pub(super) fn row_category(spec: &ColorSpec, table: &dyn Table, row: usize) -> Option<String> {
    match spec {
        ColorSpec::Categorical { col, .. } => crate::scale::cell_category(table, col, row),
        _ => None,
    }
}

pub(super) fn grouped_rows(
    geo: &GeometryIr,
    stroke: &ColorSpec,
    table: &dyn Table,
    rows: Vec<usize>,
) -> Vec<(String, Vec<usize>)> {
    if let Some(mapping) = geo
        .mappings
        .iter()
        .find(|mapping| mapping.aesthetic == PropertyKey::Group)
    {
        return crate::scale::categorical_domain(table, &mapping.column.name)
            .into_iter()
            .map(|cat| {
                let group_rows = rows
                    .iter()
                    .copied()
                    .filter(|&row| {
                        cell_category(table, &mapping.column.name, row).as_deref()
                            == Some(cat.as_str())
                    })
                    .collect();
                (cat, group_rows)
            })
            .collect();
    }
    match stroke {
        ColorSpec::Categorical { categories, .. } => categories
            .iter()
            .map(|cat| {
                let group_rows = rows
                    .iter()
                    .copied()
                    .filter(|&r| {
                        stroke.resolve(table, r).is_some()
                            && row_category(stroke, table, r).as_deref() == Some(cat)
                    })
                    .collect();
                (cat.clone(), group_rows)
            })
            .collect(),
        _ => vec![(String::new(), rows)],
    }
}

pub(super) fn axis_is_continuousish(axis: &AxisScale) -> bool {
    matches!(
        axis,
        AxisScale::Continuous { .. }
            | AxisScale::Temporal { .. }
            | AxisScale::Union { .. }
            | AxisScale::TemporalUnion { .. }
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Orientation {
    Vertical,
    Horizontal,
}

pub(super) fn categorical_value_orientation(space: &ScaledSpace) -> Option<Orientation> {
    if space.x.is_band() && space.y.as_ref().is_some_and(axis_is_continuousish) {
        Some(Orientation::Vertical)
    } else if space.y.as_ref().is_some_and(AxisScale::is_band) && axis_is_continuousish(&space.x) {
        Some(Orientation::Horizontal)
    } else {
        None
    }
}

pub(super) fn position_group_key(
    space: &ScaledSpace,
    table: &dyn Table,
    row: usize,
    orientation: Orientation,
) -> Option<String> {
    match orientation {
        Orientation::Vertical => band_group_key(&space.x, table, row),
        Orientation::Horizontal => band_group_key(space.y.as_ref()?, table, row),
    }
}

pub(super) fn position_center(
    space: &ScaledSpace,
    table: &dyn Table,
    row: usize,
    orientation: Orientation,
) -> Option<f64> {
    match orientation {
        Orientation::Vertical => space.resolve_x(table, row),
        Orientation::Horizontal => space.resolve_y(table, row),
    }
}

pub(super) fn position_bandwidth(
    space: &ScaledSpace,
    table: &dyn Table,
    row: usize,
    orientation: Orientation,
) -> Option<f64> {
    match orientation {
        Orientation::Vertical => space.x_bandwidth(table, row),
        Orientation::Horizontal => space.y_bandwidth(table, row),
    }
}

pub(super) fn value_axis_data_column(
    space: &ScaledSpace,
    orientation: Orientation,
) -> Option<&str> {
    match orientation {
        Orientation::Vertical => space.y.as_ref().and_then(AxisScale::data_column),
        Orientation::Horizontal => space.x.data_column(),
    }
}

pub(super) fn value_position(
    space: &ScaledSpace,
    table: &dyn Table,
    row: usize,
    orientation: Orientation,
) -> Option<f64> {
    match orientation {
        Orientation::Vertical => space.resolve_y(table, row),
        Orientation::Horizontal => space.resolve_x(table, row),
    }
}

pub(super) fn map_value_axis(
    space: &ScaledSpace,
    value: f64,
    orientation: Orientation,
) -> Option<f64> {
    match orientation {
        Orientation::Vertical => space.map_y(value),
        Orientation::Horizontal => space.map_x(value),
    }
}

pub(super) fn grouped_rows_by_color(
    spec: &ColorSpec,
    table: &dyn Table,
    rows: Vec<usize>,
) -> Vec<Vec<usize>> {
    match spec {
        ColorSpec::Categorical { categories, .. } => categories
            .iter()
            .map(|cat| {
                rows.iter()
                    .copied()
                    .filter(|&row| row_category(spec, table, row).as_deref() == Some(cat))
                    .collect::<Vec<_>>()
            })
            .filter(|group| !group.is_empty())
            .collect(),
        _ => vec![rows],
    }
}

fn band_group_key(axis: &AxisScale, table: &dyn Table, row: usize) -> Option<String> {
    match axis {
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
        _ => None,
    }
}

pub(super) fn quantile_type7(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    if values.len() == 1 {
        return values[0];
    }
    let pos = (values.len() - 1) as f64 * p;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        values[lo]
    } else {
        values[lo] + (values[hi] - values[lo]) * (pos - lo as f64)
    }
}

pub(super) fn pos(
    geo: &GeometryIr,
    name: PropertyKey,
    table: &dyn Table,
    row: usize,
) -> Option<f64> {
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == name) {
        let column = &mapping.column.name;
        if let Some(value) = cell_f64(table, column, row) {
            return Some(value);
        }
        return cell_micros(table, column, row).map(|micros| micros as f64);
    }
    geo.settings
        .iter()
        .find(|s| s.name == name)
        .and_then(|s| match s.value {
            SettingValue::Number(n) => Some(n),
            _ => None,
        })
}

pub(super) fn pos_bound(
    geo: &GeometryIr,
    name: PropertyKey,
    axis: &AxisScale,
    table: &dyn Table,
    row: usize,
) -> Option<f64> {
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == name) {
        let column = &mapping.column.name;
        if let Some(value) = cell_f64(table, column, row) {
            return axis.map_value_public(value);
        }
        if let Some(micros) = cell_micros(table, column, row) {
            return axis.map_value_public(micros as f64);
        }
        return categorical_bound(axis, column, table, row, bound_is_upper(name));
    }
    geo.settings
        .iter()
        .find(|s| s.name == name)
        .and_then(|s| match s.value {
            SettingValue::Number(n) => axis.map_value_public(n),
            _ => None,
        })
}

fn bound_is_upper(name: PropertyKey) -> bool {
    matches!(name, PropertyKey::Xmax | PropertyKey::Ymax)
}

/// Resolve a segment endpoint property (`x`/`y`/`xend`/`yend`) to a pixel
/// position on `axis`. A column mapping uses the band center for categorical
/// axes; a literal number maps through a continuous/temporal axis (spec §14.19).
pub(super) fn pos_center(
    geo: &GeometryIr,
    name: PropertyKey,
    axis: &AxisScale,
    table: &dyn Table,
    row: usize,
) -> Option<f64> {
    if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == name) {
        return axis.resolve_column(table, &mapping.column.name, row);
    }
    geo.settings
        .iter()
        .find(|s| s.name == name)
        .and_then(|s| match s.value {
            SettingValue::Number(n) => axis.map_value_public(n),
            _ => None,
        })
}

/// Whether the geometry maps any of the given properties to a data column.
pub(super) fn any_mapped(geo: &GeometryIr, names: &[PropertyKey]) -> bool {
    geo.mappings.iter().any(|m| names.contains(&m.aesthetic))
}

fn categorical_bound(
    axis: &AxisScale,
    column: &str,
    table: &dyn Table,
    row: usize,
    upper: bool,
) -> Option<f64> {
    match axis {
        AxisScale::Band { col, scale } if col == column => {
            let category = cell_category(table, col, row)?;
            let (start, width) = scale.band(&category)?;
            Some(if upper { start + width } else { start })
        }
        AxisScale::NestedBand {
            outer_col,
            inner_col,
            scale,
        } if column == outer_col || column == inner_col => {
            let outer = cell_category(table, outer_col, row)?;
            let inner = cell_category(table, inner_col, row)?;
            let (start, width) = scale.band(&outer, &inner)?;
            Some(if upper { start + width } else { start })
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_svg_line(
    sink: &mut dyn MarkSink,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: &str,
    width: f64,
    alpha: f64,
) {
    emit_svg_line_with_dash(sink, x1, y1, x2, y2, stroke, width, alpha, None);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_svg_line_with_dash(
    sink: &mut dyn MarkSink,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: &str,
    width: f64,
    alpha: f64,
    dash: Option<&str>,
) {
    let dash = match dash {
        Some("dotted") => Some(Dash::Dotted),
        Some("dashed") => Some(Dash::Dashed),
        _ => None,
    };
    sink.line(x1, y1, x2, y2, stroke, width, false, Some(alpha), dash);
}

/// Resolve a geometry's stroke into a [`Stroke`] for paint, matching the SVG
/// backend's optional `stroke`/`stroke-width` attributes (spec §16.8).
pub(super) fn stroke_style(spec: &ColorSpec, width: f64, table: &dyn Table, row: usize) -> Stroke {
    if matches!(spec, ColorSpec::None) {
        return Stroke::Omit;
    }
    let Some(color) = spec.resolve(table, row) else {
        return Stroke::Omit;
    };
    Stroke::Solid {
        color,
        width: width.max(0.0),
    }
}

pub(super) fn constant_or(spec: &ColorSpec, default: &str) -> String {
    match spec {
        ColorSpec::Constant(c) => c.clone(),
        _ => default.to_string(),
    }
}

pub(super) fn render_rows(table: &dyn Table, rows: Option<&[usize]>) -> Vec<usize> {
    rows.map(|rows| rows.to_vec())
        .unwrap_or_else(|| (0..table.row_count()).collect())
}

/// An opacity that is omitted from output when fully opaque (matches the SVG
/// backend's conditional `opacity` attribute).
pub(super) fn opacity_when_translucent(alpha: f64) -> Option<f64> {
    (alpha < 1.0).then_some(alpha)
}
