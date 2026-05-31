use std::collections::HashMap;

use algraf_core::{codes, Diagnostic};
use algraf_data::{DataValueRef, DateTimeValue, Table};
use algraf_semantics::{InsetAnchorIr, InsetIr, InsetParentRefIr, InsetPlacementIr, InsetSizeIr};

use crate::geo_stats::centroid_point;
use crate::layout::Rect;
use crate::scale::cell_f64;
use crate::space::ScaledSpace;

pub(super) const MAX_INSET_DEPTH: usize = 8;

#[derive(Clone, Copy)]
pub(super) struct RowContext<'a> {
    pub(super) table: &'a dyn Table,
    pub(super) row: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum MatchValue {
    Bool(bool),
    Number(u64),
    Temporal(DateTimeValue),
    String(String),
}

#[derive(Default)]
pub(super) struct InsetMatchIndex {
    buckets: HashMap<Vec<MatchValue>, Vec<usize>>,
}

impl InsetMatchIndex {
    pub(super) fn build(inset: &InsetIr, child_table: &dyn Table) -> InsetMatchIndex {
        let mut buckets: HashMap<Vec<MatchValue>, Vec<usize>> = HashMap::new();
        for row in 0..child_table.row_count() {
            if let Some(key) = child_match_key(inset, child_table, row) {
                buckets.entry(key).or_default().push(row);
            }
        }
        InsetMatchIndex { buckets }
    }

    pub(super) fn matched_rows(
        &self,
        inset: &InsetIr,
        child_table: &dyn Table,
        current_table: &dyn Table,
        current_row: usize,
        ancestors: &[RowContext<'_>],
    ) -> Vec<usize> {
        let Some(key) = parent_match_key(inset, current_table, current_row, ancestors) else {
            return Vec::new();
        };
        self.buckets.get(&key).map_or_else(Vec::new, |rows| {
            rows.iter()
                .copied()
                .filter(|&child_row| {
                    inset.match_rules.iter().all(|rule| {
                        let Some(child_value) = child_table.value(&rule.child.name, child_row)
                        else {
                            return false;
                        };
                        let context_value = match &rule.parent {
                            InsetParentRefIr::Current(column) => {
                                current_table.value(&column.name, current_row)
                            }
                            InsetParentRefIr::Parent(column) => ancestors
                                .first()
                                .and_then(|ctx| ctx.table.value(&column.name, ctx.row)),
                        };
                        context_value.is_some_and(|value| data_values_match(child_value, value))
                    })
                })
                .collect()
        })
    }
}

fn child_match_key(
    inset: &InsetIr,
    child_table: &dyn Table,
    row: usize,
) -> Option<Vec<MatchValue>> {
    let mut key = Vec::with_capacity(inset.match_rules.len());
    for rule in &inset.match_rules {
        key.push(match_value(child_table.value(&rule.child.name, row)?)?);
    }
    Some(key)
}

fn parent_match_key(
    inset: &InsetIr,
    current_table: &dyn Table,
    current_row: usize,
    ancestors: &[RowContext<'_>],
) -> Option<Vec<MatchValue>> {
    let mut key = Vec::with_capacity(inset.match_rules.len());
    for rule in &inset.match_rules {
        let value = match &rule.parent {
            InsetParentRefIr::Current(column) => current_table.value(&column.name, current_row),
            InsetParentRefIr::Parent(column) => ancestors
                .first()
                .and_then(|ctx| ctx.table.value(&column.name, ctx.row)),
        };
        key.push(match_value(value?)?);
    }
    Some(key)
}

fn match_value(value: DataValueRef<'_>) -> Option<MatchValue> {
    match value {
        DataValueRef::Null | DataValueRef::Geometry(_) => None,
        DataValueRef::Bool(value) => Some(MatchValue::Bool(value)),
        DataValueRef::Int(value) => number_match_value(value as f64),
        DataValueRef::Float(value) => number_match_value(value),
        DataValueRef::Temporal(value) => Some(MatchValue::Temporal(value)),
        DataValueRef::String(value) => Some(MatchValue::String(value.to_string())),
    }
}

fn number_match_value(value: f64) -> Option<MatchValue> {
    if value.is_nan() {
        return None;
    }
    let normalized = if value == 0.0 { 0.0 } else { value };
    Some(MatchValue::Number(normalized.to_bits()))
}

fn data_values_match(left: DataValueRef<'_>, right: DataValueRef<'_>) -> bool {
    match (left, right) {
        (DataValueRef::Null, _) | (_, DataValueRef::Null) => false,
        (DataValueRef::Int(a), DataValueRef::Int(b)) => a == b,
        (DataValueRef::Float(a), DataValueRef::Float(b)) => a == b,
        (DataValueRef::Int(a), DataValueRef::Float(b)) => (a as f64) == b,
        (DataValueRef::Float(a), DataValueRef::Int(b)) => a == (b as f64),
        (DataValueRef::Bool(a), DataValueRef::Bool(b)) => a == b,
        (DataValueRef::Temporal(a), DataValueRef::Temporal(b)) => a == b,
        (DataValueRef::String(a), DataValueRef::String(b)) => a == b,
        _ => false,
    }
}

pub(super) fn inset_anchor(
    inset: &InsetIr,
    scaled: &ScaledSpace,
    table: &dyn Table,
    row: usize,
    rows: Option<&[usize]>,
) -> Option<(f64, f64)> {
    if matches!(inset.anchor, InsetAnchorIr::Centroid) {
        if let Some(spatial) = &scaled.spatial {
            if let Some(geom_col) = spatial.geom_col.as_deref() {
                if let Some(DataValueRef::Geometry(geometry)) = table.value(geom_col, row) {
                    let centroid = centroid_point(geometry)?;
                    return spatial.project_ll(centroid.x(), centroid.y());
                }
            }
        }
    }
    if matches!(inset.placement, InsetPlacementIr::MarkCenter) {
        if let Some(anchor) = mark_center_anchor(scaled, table, row, rows) {
            return Some(anchor);
        }
    }
    Some((scaled.resolve_x(table, row)?, scaled.resolve_y(table, row)?))
}

fn mark_center_anchor(
    scaled: &ScaledSpace,
    table: &dyn Table,
    row: usize,
    rows: Option<&[usize]>,
) -> Option<(f64, f64)> {
    let polar = scaled.polar()?;
    if !scaled.polar_theta_is_band() {
        let value_col = scaled.polar_theta_column()?.to_string();
        let row_list = render_rows(table, rows);
        let total: f64 = row_list
            .iter()
            .filter_map(|&row| cell_f64(table, &value_col, row))
            .filter(|value| *value > 0.0)
            .sum();
        if total <= f64::EPSILON {
            return None;
        }
        let span = polar.theta_end - polar.theta_start;
        let mut acc = 0.0;
        for current in row_list {
            let Some(value) = cell_f64(table, &value_col, current) else {
                continue;
            };
            if value <= 0.0 {
                continue;
            }
            let a0 = polar.theta_start + (acc / total) * span;
            acc += value;
            let a1 = polar.theta_start + (acc / total) * span;
            if current == row {
                let (r0, r1) = scaled
                    .polar_radius_band(table, row)
                    .map(|(start, width)| (start, start + width))
                    .unwrap_or((polar.r_inner, polar.r_outer));
                return Some(polar.point((a0 + a1) / 2.0, (r0 + r1) / 2.0));
            }
        }
        return None;
    }

    let (theta, _) = scaled.polar_angle_band(table, row)?;
    let (r0, r1) = scaled
        .polar_radius_band(table, row)
        .map(|(start, width)| (start, start + width))
        .unwrap_or((polar.r_inner, polar.r_outer));
    Some(polar.point(theta, (r0 + r1) / 2.0))
}

pub(super) fn inset_size(
    inset: &InsetIr,
    table: &dyn Table,
    row: usize,
    mapped_domain: Option<(f64, f64)>,
) -> (f64, f64) {
    match &inset.size {
        InsetSizeIr::Fixed { width, height } => (*width, *height),
        InsetSizeIr::Mapped { column, min, max } => {
            let value = cell_f64(table, &column.name, row).unwrap_or(0.0);
            let (lo, hi) = mapped_domain.unwrap_or((value, value));
            let t = if (hi - lo).abs() <= f64::EPSILON {
                0.5
            } else {
                ((value - lo) / (hi - lo)).clamp(0.0, 1.0)
            };
            let size = min + (max - min) * t;
            (size, size)
        }
    }
}

pub(super) fn mapped_size_domain(
    inset: &InsetIr,
    table: &dyn Table,
    rows: &[usize],
) -> Option<(f64, f64)> {
    let InsetSizeIr::Mapped { column, .. } = &inset.size else {
        return None;
    };
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &row in rows {
        if let Some(value) = cell_f64(table, &column.name, row) {
            min = min.min(value);
            max = max.max(value);
        }
    }
    min.is_finite().then_some((min, max))
}

pub(super) fn inset_plot(viewport: Rect, padding: f64) -> Rect {
    let pad = padding
        .max(0.0)
        .min(viewport.width.min(viewport.height) / 2.0);
    Rect {
        x: viewport.x + pad,
        y: viewport.y + pad,
        width: (viewport.width - pad * 2.0).max(1.0),
        height: (viewport.height - pad * 2.0).max(1.0),
    }
}

pub(super) fn render_rows(table: &dyn Table, rows: Option<&[usize]>) -> Vec<usize> {
    rows.map_or_else(|| (0..table.row_count()).collect(), <[usize]>::to_vec)
}

pub(super) fn union_rows(matches: &[Vec<usize>]) -> Vec<usize> {
    let mut rows = matches
        .iter()
        .flat_map(|rows| rows.iter().copied())
        .collect::<Vec<_>>();
    rows.sort_unstable();
    rows.dedup();
    rows
}

pub(super) fn inset_budget_diagnostic(
    inset: &InsetIr,
    parent_count: usize,
    matches: &[Vec<usize>],
    child_table: &dyn Table,
    mark_budget: Option<usize>,
) -> Option<Diagnostic> {
    let budget = mark_budget?;
    let child_layers = inset
        .child_spaces
        .iter()
        .map(|space| space.layers.len().max(space.geometries.len()).max(1))
        .sum::<usize>()
        .max(1);
    let matched_count = matches.iter().map(Vec::len).sum::<usize>();
    let estimated = matched_count.saturating_mul(child_layers);
    if estimated <= budget {
        return None;
    }
    Some(
        Diagnostic::error(
            codes::E2110,
            format!(
                "Inset would render about {estimated} child mark(s) from {parent_count} parent row(s) and {} child row(s), above the mark budget of {budget}",
                child_table.row_count()
            ),
            inset.span,
        )
        .with_help("filter, aggregate, reduce nested inset depth, or raise --mark-budget"),
    )
}
