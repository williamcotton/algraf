use std::collections::HashMap;

use algraf_core::{codes, Diagnostic};
use algraf_data::{DataFrame, DataValueRef, DateTimeValue, Table};
use algraf_semantics::{
    ChartIr, CoordsIr, InsetAnchorIr, InsetClipIr, InsetIr, InsetParentRefIr, InsetPlacementIr,
    InsetScalePolicyIr, InsetSizeIr, SpaceIr,
};

use crate::domains::train_space_domains;
use crate::geo_stats::centroid_point;
use crate::layout::Rect;
use crate::scale::cell_f64;
use crate::space::ScaledSpace;
use crate::theme::Theme;

use super::common::{merged_scales, resolve_space_theme, validate_scale_configs};
use super::derived::active_table;
use super::panels::{planned_panel, Panel};
use super::row_table::RowSubsetTable;
use super::RenderLimits;

pub(super) const MAX_INSET_DEPTH: usize = 8;

#[derive(Clone, Copy)]
pub(super) struct RowContext<'a> {
    pub(super) table: &'a dyn Table,
    pub(super) row: usize,
}

pub(super) struct PlannedInset<'t> {
    pub(super) clip: InsetClipIr,
    pub(super) scale_policy: InsetScalePolicyIr,
    pub(super) instances: Vec<PlannedInsetInstance<'t>>,
}

pub(super) struct PlannedInsetInstance<'t> {
    pub(super) parent_row: usize,
    pub(super) viewport: Rect,
    pub(super) child_panels: Vec<Panel<'t>>,
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

#[allow(clippy::too_many_arguments)]
pub(super) fn plan_inset<'t>(
    ir: &'t ChartIr,
    primary: &'t dyn Table,
    derived: &'t HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: &RenderLimits,
    inset: &'t InsetIr,
    parent_table: &'t dyn Table,
    parent_scaled: &ScaledSpace,
    parent_rows: Option<&[usize]>,
    ancestors: &[RowContext<'t>],
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> PlannedInset<'t> {
    if depth >= MAX_INSET_DEPTH {
        diagnostics.push(Diagnostic::error(
            codes::E2109,
            format!("nested Inset depth exceeds the limit of {MAX_INSET_DEPTH}"),
            inset.span,
        ));
        return empty_inset(inset);
    }

    let parent_row_list = render_rows(parent_table, parent_rows);
    let child_table = active_table(&inset.data, primary, derived);
    let index = InsetMatchIndex::build(inset, child_table);
    let matches = parent_row_list
        .iter()
        .map(|&row| index.matched_rows(inset, child_table, parent_table, row, ancestors))
        .collect::<Vec<_>>();
    let shared_rows = union_rows(&matches);

    if let Some(diagnostic) = inset_budget_diagnostic(
        inset,
        parent_row_list.len(),
        &matches,
        child_table,
        limits.mark_budget,
    ) {
        diagnostics.push(diagnostic);
        return empty_inset(inset);
    }

    let unmatched_count = matches.iter().filter(|rows| rows.is_empty()).count();
    let summarize_unmatched = unmatched_count > 1;
    if summarize_unmatched {
        diagnostics.push(Diagnostic::warning(
            codes::W2002,
            format!(
                "Inset matched no child rows for {unmatched_count} of {} parent rows",
                parent_row_list.len()
            ),
            inset.span,
        ));
    }

    let size_domain = mapped_size_domain(inset, parent_table, &parent_row_list);
    let mut instances = Vec::new();
    for (instance_index, parent_row) in parent_row_list.iter().copied().enumerate() {
        let child_rows = &matches[instance_index];
        if child_rows.is_empty() {
            if !summarize_unmatched {
                diagnostics.push(Diagnostic::warning(
                    codes::W2002,
                    "Inset matched no child rows",
                    inset.span,
                ));
            }
            continue;
        }
        let Some((x, y)) =
            inset_anchor(inset, parent_scaled, parent_table, parent_row, parent_rows)
        else {
            diagnostics.push(Diagnostic::warning(
                codes::W2002,
                "Inset anchor could not be resolved",
                inset.span,
            ));
            continue;
        };
        let (width, height) = inset_size(inset, parent_table, parent_row, size_domain);
        if width <= 0.0 || height <= 0.0 {
            continue;
        }
        let viewport = Rect {
            x: x + inset.dx - width / 2.0,
            y: y + inset.dy - height / 2.0,
            width,
            height,
        };
        let plot = inset_plot(viewport, inset.padding);
        let mut contexts = Vec::with_capacity(ancestors.len() + 1);
        contexts.push(RowContext {
            table: parent_table,
            row: parent_row,
        });
        contexts.extend_from_slice(ancestors);

        let child_panels = inset
            .child_spaces
            .iter()
            .filter_map(|child_space| {
                plan_child_panel(
                    ir,
                    primary,
                    derived,
                    theme,
                    cli_theme_override,
                    limits,
                    inset,
                    child_space,
                    child_table,
                    child_rows,
                    &shared_rows,
                    plot,
                    &contexts,
                    depth + 1,
                    diagnostics,
                )
            })
            .collect::<Vec<_>>();
        instances.push(PlannedInsetInstance {
            parent_row,
            viewport,
            child_panels,
        });
    }

    PlannedInset {
        clip: inset.clip,
        scale_policy: inset.scale_policy,
        instances,
    }
}

#[allow(clippy::too_many_arguments)]
fn plan_child_panel<'t>(
    ir: &'t ChartIr,
    primary: &'t dyn Table,
    derived: &'t HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: &RenderLimits,
    inset: &InsetIr,
    space: &'t SpaceIr,
    inset_table: &'t dyn Table,
    child_rows: &[usize],
    shared_rows: &[usize],
    plot: Rect,
    ancestors: &[RowContext<'t>],
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Panel<'t>> {
    let table = active_table(&space.data, primary, derived);
    let rows = if space.data == inset.data {
        child_rows.to_vec()
    } else {
        render_rows(table, None)
    };
    let training_rows = match inset.scale_policy {
        InsetScalePolicyIr::Shared if space.data == inset.data => shared_rows,
        _ => &rows,
    };
    let legend_rows =
        (inset.scale_policy == InsetScalePolicyIr::Shared).then(|| training_rows.to_vec());
    let training_table_ref = if space.data == inset.data {
        inset_table
    } else {
        table
    };
    let training = RowSubsetTable::new(training_table_ref, training_rows);
    let panel_theme = resolve_space_theme(theme, space.theme.as_ref(), cli_theme_override);
    let space_guides = ir.guides.with_overrides(&space.guides);
    let space_scales = merged_scales(&ir.scales, &space.scales);
    let hints = train_space_domains(&space.frame, &training, &space.geometries, &space_scales);
    validate_scale_configs(
        &space.frame,
        &training,
        &space_scales,
        space.span,
        diagnostics,
    );
    let scaled = match space.coords {
        CoordsIr::Polar {
            theta,
            inner_radius,
            start_angle,
            direction,
        } => ScaledSpace::build_polar(
            &space.frame,
            &training,
            plot,
            &hints,
            &space_scales,
            theta,
            inner_radius,
            start_angle,
            direction,
            panel_theme.font_size,
        ),
        CoordsIr::Cartesian => ScaledSpace::build(
            &space.frame,
            &training,
            (plot.x, plot.right()),
            (plot.bottom(), plot.y),
            &hints,
            &space_scales,
            space.view,
        ),
    };
    let Some(scaled) = scaled else {
        diagnostics.push(Diagnostic::warning(
            codes::R0003,
            "inset child space could not be laid out",
            space.span,
        ));
        return None;
    };
    let render_table = if space.data == inset.data {
        inset_table
    } else {
        table
    };
    Some(planned_panel(
        ir,
        primary,
        derived,
        theme,
        cli_theme_override,
        limits,
        space,
        render_table,
        scaled,
        plot,
        Some(rows),
        legend_rows,
        None,
        None,
        space.view.has_zoom(),
        panel_theme,
        space_guides,
        space_scales,
        inset.guides,
        ancestors,
        depth,
        diagnostics,
    ))
}

fn empty_inset(inset: &InsetIr) -> PlannedInset<'_> {
    PlannedInset {
        clip: inset.clip,
        scale_policy: inset.scale_policy,
        instances: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use algraf_core::Span;
    use algraf_data::{Column, ColumnDef, DataFrame, DataType, DateTimeValue};
    use algraf_semantics::{
        ColumnRef, InsetClipIr, InsetMatchIr, InsetScalePolicyIr, SpaceDataRef,
    };

    fn col(name: &str, dtype: DataType) -> ColumnDef {
        ColumnDef {
            name: name.to_string(),
            dtype,
            nullable: false,
            examples: Vec::new(),
        }
    }

    fn col_ref(name: &str, dtype: DataType) -> ColumnRef {
        ColumnRef {
            name: name.to_string(),
            dtype,
            span: Span::empty(0),
        }
    }

    fn frame(columns: Vec<(&str, DataType, Column)>) -> DataFrame {
        let schema = columns
            .iter()
            .map(|(name, dtype, _)| col(name, *dtype))
            .collect();
        let columns = columns.into_iter().map(|(_, _, column)| column).collect();
        DataFrame::new(schema, columns)
    }

    fn match_inset(rules: Vec<(&str, DataType, &str, DataType)>) -> InsetIr {
        InsetIr {
            data: SpaceDataRef::Primary,
            match_rules: rules
                .into_iter()
                .map(|(child, child_dtype, parent, parent_dtype)| InsetMatchIr {
                    child: col_ref(child, child_dtype),
                    parent: InsetParentRefIr::Current(col_ref(parent, parent_dtype)),
                    span: Span::empty(0),
                })
                .collect(),
            size: InsetSizeIr::Fixed {
                width: 10.0,
                height: 10.0,
            },
            scale_policy: InsetScalePolicyIr::Shared,
            guides: false,
            clip: InsetClipIr::Rect,
            padding: 0.0,
            placement: InsetPlacementIr::Center,
            dx: 0.0,
            dy: 0.0,
            anchor: InsetAnchorIr::Position,
            child_spaces: Vec::new(),
            span: Span::empty(0),
        }
    }

    fn matched_rows(
        inset: &InsetIr,
        child: &DataFrame,
        parent: &DataFrame,
        parent_row: usize,
    ) -> Vec<usize> {
        InsetMatchIndex::build(inset, child).matched_rows(inset, child, parent, parent_row, &[])
    }

    #[test]
    fn match_index_excludes_nan_keys() {
        let inset = match_inset(vec![("k", DataType::Float, "k", DataType::Float)]);
        let child = frame(vec![(
            "k",
            DataType::Float,
            Column::from_float_options(vec![Some(f64::NAN), Some(1.0)]),
        )]);
        let parent = frame(vec![(
            "k",
            DataType::Float,
            Column::from_float_options(vec![Some(f64::NAN), Some(1.0)]),
        )]);

        assert!(matched_rows(&inset, &child, &parent, 0).is_empty());
        assert_eq!(matched_rows(&inset, &child, &parent, 1), vec![1]);
    }

    #[test]
    fn match_index_normalizes_positive_and_negative_zero() {
        let inset = match_inset(vec![("k", DataType::Float, "k", DataType::Float)]);
        let child = frame(vec![(
            "k",
            DataType::Float,
            Column::from_float_options(vec![Some(-0.0)]),
        )]);
        let parent = frame(vec![(
            "k",
            DataType::Float,
            Column::from_float_options(vec![Some(0.0)]),
        )]);

        assert_eq!(matched_rows(&inset, &child, &parent, 0), vec![0]);
    }

    #[test]
    fn match_index_filters_large_i64_bucket_collisions() {
        let inset = match_inset(vec![("k", DataType::Integer, "k", DataType::Integer)]);
        let child = frame(vec![(
            "k",
            DataType::Integer,
            Column::from_int_options(vec![
                Some(9_007_199_254_740_992),
                Some(9_007_199_254_740_993),
            ]),
        )]);
        let parent = frame(vec![(
            "k",
            DataType::Integer,
            Column::from_int_options(vec![Some(9_007_199_254_740_993)]),
        )]);

        assert_eq!(matched_rows(&inset, &child, &parent, 0), vec![1]);
    }

    #[test]
    fn match_index_preserves_int_float_comparison_semantics() {
        let int_child = match_inset(vec![("k", DataType::Integer, "k", DataType::Float)]);
        let child = frame(vec![(
            "k",
            DataType::Integer,
            Column::from_int_options(vec![Some(2), Some(3)]),
        )]);
        let parent = frame(vec![(
            "k",
            DataType::Float,
            Column::from_float_options(vec![Some(2.0), Some(2.5)]),
        )]);
        assert_eq!(matched_rows(&int_child, &child, &parent, 0), vec![0]);
        assert!(matched_rows(&int_child, &child, &parent, 1).is_empty());

        let float_child = match_inset(vec![("k", DataType::Float, "k", DataType::Integer)]);
        let child = frame(vec![(
            "k",
            DataType::Float,
            Column::from_float_options(vec![Some(3.0)]),
        )]);
        let parent = frame(vec![(
            "k",
            DataType::Integer,
            Column::from_int_options(vec![Some(3)]),
        )]);
        assert_eq!(matched_rows(&float_child, &child, &parent, 0), vec![0]);
    }

    #[test]
    fn match_index_handles_temporal_and_string_composite_keys() {
        let inset = match_inset(vec![
            ("t", DataType::Temporal, "t", DataType::Temporal),
            ("name", DataType::String, "name", DataType::String),
        ]);
        let epoch = DateTimeValue::unix_epoch();
        let child = frame(vec![
            (
                "t",
                DataType::Temporal,
                Column::from_temporal_options(vec![Some(epoch), Some(epoch)]),
            ),
            (
                "name",
                DataType::String,
                Column::String(vec![Some("a".to_string()), Some("b".to_string())]),
            ),
        ]);
        let parent = frame(vec![
            (
                "t",
                DataType::Temporal,
                Column::from_temporal_options(vec![Some(epoch)]),
            ),
            (
                "name",
                DataType::String,
                Column::String(vec![Some("b".to_string())]),
            ),
        ]);

        assert_eq!(matched_rows(&inset, &child, &parent, 0), vec![1]);
    }

    #[test]
    fn match_index_requires_all_composite_components() {
        let inset = match_inset(vec![
            ("city", DataType::String, "city", DataType::String),
            ("category", DataType::String, "category", DataType::String),
        ]);
        let child = frame(vec![
            (
                "city",
                DataType::String,
                Column::String(vec![Some("A".to_string()), Some("A".to_string())]),
            ),
            (
                "category",
                DataType::String,
                Column::String(vec![Some("x".to_string()), Some("y".to_string())]),
            ),
        ]);
        let parent = frame(vec![
            (
                "city",
                DataType::String,
                Column::String(vec![Some("A".to_string())]),
            ),
            (
                "category",
                DataType::String,
                Column::String(vec![Some("y".to_string())]),
            ),
        ]);

        assert_eq!(matched_rows(&inset, &child, &parent, 0), vec![1]);
    }

    #[test]
    fn match_index_handles_empty_inputs() {
        let inset = match_inset(vec![("k", DataType::String, "k", DataType::String)]);
        let child = frame(vec![("k", DataType::String, Column::String(Vec::new()))]);
        let parent = frame(vec![(
            "k",
            DataType::String,
            Column::String(vec![Some("a".to_string())]),
        )]);
        assert!(matched_rows(&inset, &child, &parent, 0).is_empty());

        let parent_rows = render_rows(&parent, Some(&[]));
        let matches = parent_rows
            .iter()
            .map(|&row| matched_rows(&inset, &child, &parent, row))
            .collect::<Vec<_>>();
        assert!(matches.is_empty());
        assert!(union_rows(&matches).is_empty());
    }
}
