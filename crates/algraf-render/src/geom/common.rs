use algraf_data::Table;
use algraf_semantics::{GeometryIr, PropertyKey, SettingValue};

use crate::aes::ColorSpec;
use crate::scale::{cell_category, cell_f64, cell_micros};
use crate::space::{AxisScale, ScaledSpace};
use crate::svg::{escape_attr, num, SvgWriter};

pub(super) const DEFAULT_FILL: &str = "#4E79A7";
pub(super) const DEFAULT_STROKE: &str = "#333333";
/// Default output range (px) for a mapped `strokeWidth` scale (spec §16.8).
pub(crate) const DEFAULT_STROKE_WIDTH_RANGE: (f64, f64) = (0.5, 4.0);
/// Default output range (radius px) for a mapped `size` scale (spec §16.8).
pub(crate) const DEFAULT_SIZE_RANGE: (f64, f64) = (2.0, 8.0);

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

pub(super) fn x_group_key(space: &ScaledSpace, table: &dyn Table, row: usize) -> Option<String> {
    match &space.x {
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
    w: &mut SvgWriter,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: &str,
    width: f64,
    alpha: f64,
) {
    emit_svg_line_with_dash(w, x1, y1, x2, y2, stroke, width, alpha, None);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_svg_line_with_dash(
    w: &mut SvgWriter,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: &str,
    width: f64,
    alpha: f64,
    dash: Option<&str>,
) {
    let dash_attr = match dash {
        Some("dotted") => " stroke-dasharray=\"1 2\"",
        Some("dashed") => " stroke-dasharray=\"4 4\"",
        _ => "",
    };
    w.line(&format!(
        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"{}\"{} />",
        num(x1),
        num(y1),
        num(x2),
        num(y2),
        escape_attr(stroke),
        num(width.max(0.0)),
        num(alpha),
        dash_attr,
    ));
}

pub(super) fn stroke_attrs(spec: &ColorSpec, width: f64, table: &dyn Table, row: usize) -> String {
    if matches!(spec, ColorSpec::None) {
        return String::new();
    }
    let Some(color) = spec.resolve(table, row) else {
        return String::new();
    };
    format!(
        " stroke=\"{}\" stroke-width=\"{}\"",
        escape_attr(&color),
        num(width.max(0.0)),
    )
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

pub(super) fn opacity_attr(alpha: f64) -> String {
    if alpha < 1.0 {
        format!(" opacity=\"{}\"", num(alpha))
    } else {
        String::new()
    }
}
