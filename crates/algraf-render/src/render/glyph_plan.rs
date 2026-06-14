use std::collections::HashMap;

use algraf_core::{codes, Diagnostic};
use algraf_data::{DataFrame, DataValueRef, DateTimeValue, Table};
use algraf_semantics::{
    ChartIr, CoordsIr, GlyphCallIr, GlyphClipIr, GlyphHostRefIr, GlyphPlacementIr,
    GlyphScalePolicyIr, GlyphSizeIr, ScaleIr, ScaleTargetIr, SpaceIr,
};

use crate::aes::{Legend, LegendKind, NumberSpec};
use crate::domains::train_space_domains;
use crate::geo_stats::centroid_point;
use crate::layout::Rect;
use crate::scale::{cell_f64, numeric_domain};
use crate::space::ScaledSpace;
use crate::svg::display_label;
use crate::theme::Theme;

use super::common::{merged_scales, resolve_space_theme, validate_scale_configs};
use super::derived::active_table;
use super::panels::{planned_panel, resolve_panel_clips_in_bounds, Panel};
use super::row_table::RowSubsetTable;
use super::RenderLimits;

pub(super) const MAX_GLYPH_DEPTH: usize = 8;

#[derive(Clone, Copy)]
pub(super) struct RowContext<'a> {
    pub(super) table: &'a dyn Table,
    pub(super) row: usize,
}

pub(super) struct PlannedGlyph<'t> {
    pub(super) clip: GlyphClipIr,
    pub(super) scale_policy: GlyphScalePolicyIr,
    pub(super) legend: bool,
    /// Pre-computed size-legend swatch for the glyph call's `size:` aesthetic
    /// when a `Scale(size: col, range:, label:)` matches — declared either in
    /// the glyph body (spec §14.27, preferred) or at chart scope. `None` for
    /// unmapped or unscaled sizes (spec §16.13).
    pub(super) size_legend: Option<Legend>,
    pub(super) instances: Vec<PlannedGlyphInstance<'t>>,
}

pub(super) struct PlannedGlyphInstance<'t> {
    pub(super) host_row: usize,
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
pub(super) struct GlyphMatchIndex {
    buckets: HashMap<Vec<MatchValue>, Vec<usize>>,
}

impl GlyphMatchIndex {
    pub(super) fn build(glyph: &GlyphCallIr, child_table: &dyn Table) -> GlyphMatchIndex {
        let mut buckets: HashMap<Vec<MatchValue>, Vec<usize>> = HashMap::new();
        for row in 0..child_table.row_count() {
            if let Some(key) = child_match_key(glyph, child_table, row) {
                buckets.entry(key).or_default().push(row);
            }
        }
        GlyphMatchIndex { buckets }
    }

    pub(super) fn matched_rows(
        &self,
        glyph: &GlyphCallIr,
        child_table: &dyn Table,
        host_table: &dyn Table,
        host_row: usize,
        ancestors: &[RowContext<'_>],
    ) -> Vec<usize> {
        let Some(key) = host_match_key(glyph, host_table, host_row, ancestors) else {
            return Vec::new();
        };
        self.buckets.get(&key).map_or_else(Vec::new, |rows| {
            rows.iter()
                .copied()
                .filter(|&child_row| {
                    glyph.key.iter().all(|rule| {
                        let Some(child_value) = child_table.value(&rule.child.name, child_row)
                        else {
                            return false;
                        };
                        let host_value = match &rule.host {
                            GlyphHostRefIr::Current(column) => {
                                host_table.value(&column.name, host_row)
                            }
                            GlyphHostRefIr::Outer(column) => ancestors
                                .first()
                                .and_then(|ctx| ctx.table.value(&column.name, ctx.row)),
                        };
                        host_value.is_some_and(|value| data_values_match(child_value, value))
                    })
                })
                .collect()
        })
    }
}

fn child_match_key(
    glyph: &GlyphCallIr,
    child_table: &dyn Table,
    row: usize,
) -> Option<Vec<MatchValue>> {
    let mut key = Vec::with_capacity(glyph.key.len());
    for rule in &glyph.key {
        key.push(match_value(child_table.value(&rule.child.name, row)?)?);
    }
    Some(key)
}

fn host_match_key(
    glyph: &GlyphCallIr,
    host_table: &dyn Table,
    host_row: usize,
    ancestors: &[RowContext<'_>],
) -> Option<Vec<MatchValue>> {
    let mut key = Vec::with_capacity(glyph.key.len());
    for rule in &glyph.key {
        let value = match &rule.host {
            GlyphHostRefIr::Current(column) => host_table.value(&column.name, host_row),
            GlyphHostRefIr::Outer(column) => ancestors
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
pub(super) fn plan_glyph<'t>(
    ir: &'t ChartIr,
    primary: &'t dyn Table,
    derived: &'t HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: &RenderLimits,
    glyph: &'t GlyphCallIr,
    host_table: &'t dyn Table,
    host_scaled: &ScaledSpace,
    host_rows: Option<&[usize]>,
    ancestors: &[RowContext<'t>],
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> PlannedGlyph<'t> {
    if depth >= MAX_GLYPH_DEPTH {
        diagnostics.push(Diagnostic::error(
            codes::E2209,
            format!("nested glyph depth exceeds the limit of {MAX_GLYPH_DEPTH}"),
            glyph.span,
        ));
        return empty_glyph(glyph);
    }

    let host_row_list = render_rows(host_table, host_rows);
    let child_table = active_table(&glyph.data, primary, derived);
    let index = GlyphMatchIndex::build(glyph, child_table);
    let matches = host_row_list
        .iter()
        .map(|&row| index.matched_rows(glyph, child_table, host_table, row, ancestors))
        .collect::<Vec<_>>();
    let shared_rows = union_rows(&matches);

    if let Some(diagnostic) = glyph_budget_diagnostic(
        glyph,
        host_row_list.len(),
        &matches,
        child_table,
        limits.mark_budget,
    ) {
        diagnostics.push(diagnostic);
        return empty_glyph(glyph);
    }

    let unmatched_count = matches.iter().filter(|rows| rows.is_empty()).count();
    let summarize_unmatched = unmatched_count > 1;
    if summarize_unmatched {
        diagnostics.push(Diagnostic::warning(
            codes::W2002,
            format!(
                "glyph `{}` matched no child rows for {unmatched_count} of {} host rows",
                glyph.glyph_name,
                host_row_list.len()
            ),
            glyph.span,
        ));
    }

    let size_domain = mapped_size_domain(glyph, host_table, &host_row_list);
    let size_range = mapped_size_pixel_range(glyph, &glyph.body_scales, &ir.scales);
    let mut instances = Vec::new();
    for (instance_index, host_row) in host_row_list.iter().copied().enumerate() {
        let child_rows = &matches[instance_index];
        if child_rows.is_empty() {
            if !summarize_unmatched {
                diagnostics.push(Diagnostic::warning(
                    codes::W2002,
                    format!("glyph `{}` matched no child rows", glyph.glyph_name),
                    glyph.span,
                ));
            }
            continue;
        }
        let Some((x, y)) = glyph_anchor(glyph, host_scaled, host_table, host_row, host_rows) else {
            diagnostics.push(Diagnostic::warning(
                codes::W2002,
                format!("glyph `{}` anchor could not be resolved", glyph.glyph_name),
                glyph.span,
            ));
            continue;
        };
        let (width, height) = glyph_size(glyph, host_table, host_row, size_domain, size_range);
        if width <= 0.0 || height <= 0.0 {
            continue;
        }
        let viewport = Rect {
            x: x + glyph.dx - width / 2.0,
            y: y + glyph.dy - height / 2.0,
            width,
            height,
        };
        let plot = glyph_plot(viewport, glyph.padding);
        let mut contexts = Vec::with_capacity(ancestors.len() + 1);
        contexts.push(RowContext {
            table: host_table,
            row: host_row,
        });
        contexts.extend_from_slice(ancestors);

        let mut child_panels = glyph
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
                    glyph,
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
        let blockers = child_panels
            .iter()
            .map(|panel| panel.plot)
            .collect::<Vec<_>>();
        resolve_panel_clips_in_bounds(&mut child_panels, viewport, &blockers);
        instances.push(PlannedGlyphInstance {
            host_row,
            viewport,
            child_panels,
        });
    }

    let size_legend = build_size_legend(glyph, &ir.scales, host_table, child_table);

    PlannedGlyph {
        clip: glyph.clip,
        scale_policy: glyph.scale_policy,
        legend: glyph.legend,
        size_legend,
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
    glyph: &GlyphCallIr,
    space: &'t SpaceIr,
    glyph_table: &'t dyn Table,
    child_rows: &[usize],
    shared_rows: &[usize],
    plot: Rect,
    ancestors: &[RowContext<'t>],
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Panel<'t>> {
    let table = active_table(&space.data, primary, derived);
    let rows = if space.data == glyph.data {
        child_rows.to_vec()
    } else {
        render_rows(table, None)
    };
    let training_rows = match glyph.scale_policy {
        GlyphScalePolicyIr::Shared if space.data == glyph.data => shared_rows,
        _ => &rows,
    };
    let legend_rows =
        (glyph.scale_policy == GlyphScalePolicyIr::Shared).then(|| training_rows.to_vec());
    let training_table_ref = if space.data == glyph.data {
        glyph_table
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
            "glyph child space could not be laid out",
            space.span,
        ));
        return None;
    };
    let render_table = if space.data == glyph.data {
        glyph_table
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
        panel_theme,
        space_guides,
        space_scales,
        glyph.guides,
        ancestors,
        depth,
        diagnostics,
    ))
}

fn empty_glyph(glyph: &GlyphCallIr) -> PlannedGlyph<'_> {
    PlannedGlyph {
        clip: glyph.clip,
        scale_policy: glyph.scale_policy,
        legend: glyph.legend,
        size_legend: None,
        instances: Vec::new(),
    }
}

/// Build the size-legend swatch for a glyph call's `size:` aesthetic
/// (spec §16.13). A glyph-body `Scale(size: col, range:, label:)` takes
/// precedence over a same-column chart-scope `Scale(size:)` (spec §14.27).
/// Returns `None` when the size is fixed. When no matching `Scale` exists the
/// legend still fires using the glyph's default diameter range (matching the
/// `Point(size:)` behavior with no chart-scope size scale).
fn build_size_legend(
    glyph: &GlyphCallIr,
    chart_scales: &[ScaleIr],
    host_table: &dyn Table,
    child_table: &dyn Table,
) -> Option<Legend> {
    let GlyphSizeIr::Mapped {
        column,
        min: default_min,
        max: default_max,
    } = &glyph.size
    else {
        return None;
    };
    let scale = glyph
        .body_scales
        .iter()
        .chain(chart_scales.iter())
        .find(|scale| match &scale.target {
            ScaleTargetIr::Aesthetic {
                aesthetic,
                column: Some(scale_col),
            } => aesthetic == "size" && scale_col.name == column.name,
            _ => false,
        });
    // The glyph's `range:` (and the GlyphSizeIr defaults) declare viewport
    // diameter (spec §14.27); `LegendKind::Radius` treats swatch magnitude as
    // a radius (`guide/emit.rs` legend layout). Halve so swatches match the
    // rendered pie radii.
    let (range_lo, range_hi) = match scale.and_then(|s| s.range) {
        Some([Some(lo), Some(hi)]) => (lo, hi),
        _ => (*default_min, *default_max),
    };
    let range = (range_lo / 2.0, range_hi / 2.0);
    // Data values for the size column live in either the host's table
    // (chart-scope case where the column duplicates into the chart primary)
    // or the glyph's `data:` child table (glyph-body case). Try both so the
    // legend domain reflects the actual values either way.
    let data_domain = numeric_domain(host_table, &column.name)
        .or_else(|| numeric_domain(child_table, &column.name))
        .unwrap_or((0.0, 1.0));
    let domain = match scale.and_then(|s| s.domain) {
        Some([lo, hi]) => (lo.unwrap_or(data_domain.0), hi.unwrap_or(data_domain.1)),
        None => data_domain,
    };
    let title = scale
        .and_then(|s| s.label.clone())
        .unwrap_or_else(|| display_label(&column.name));
    let spec = NumberSpec::Scaled {
        col: column.name.clone(),
        domain,
        range,
        breaks: scale.and_then(|s| s.breaks.clone()),
        labels: scale.and_then(|s| s.break_labels.clone()),
    };
    spec.legend(&title, LegendKind::Radius)
}

pub(super) fn glyph_anchor(
    glyph: &GlyphCallIr,
    scaled: &ScaledSpace,
    table: &dyn Table,
    row: usize,
    rows: Option<&[usize]>,
) -> Option<(f64, f64)> {
    if matches!(glyph.placement, GlyphPlacementIr::Centroid) {
        if let Some(spatial) = &scaled.spatial {
            if let Some(geom_col) = spatial.geom_col.as_deref() {
                if let Some(DataValueRef::Geometry(geometry)) = table.value(geom_col, row) {
                    let centroid = centroid_point(geometry)?;
                    return spatial.project_ll(centroid.x(), centroid.y());
                }
            }
        }
    }
    if matches!(glyph.placement, GlyphPlacementIr::MarkCenter) {
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

pub(super) fn glyph_size(
    glyph: &GlyphCallIr,
    table: &dyn Table,
    row: usize,
    mapped_domain: Option<(f64, f64)>,
    pixel_range: Option<(f64, f64)>,
) -> (f64, f64) {
    match &glyph.size {
        GlyphSizeIr::Fixed { width, height } => (*width, *height),
        GlyphSizeIr::Mapped { column, min, max } => {
            let value = cell_f64(table, &column.name, row).unwrap_or(0.0);
            let (lo, hi) = mapped_domain.unwrap_or((value, value));
            let t = if (hi - lo).abs() <= f64::EPSILON {
                0.5
            } else {
                ((value - lo) / (hi - lo)).clamp(0.0, 1.0)
            };
            let (out_min, out_max) = pixel_range.unwrap_or((*min, *max));
            let size = out_min + (out_max - out_min) * t;
            (size, size)
        }
    }
}

/// If a `Scale(size: column, range: [min, max])` matches the glyph's mapped
/// column, use its `range:` as the glyph's pixel min/max. Glyph-body scales
/// (spec §14.27) take precedence over chart-scope scales for the same
/// aesthetic-column pair.
pub(super) fn mapped_size_pixel_range(
    glyph: &GlyphCallIr,
    body_scales: &[ScaleIr],
    chart_scales: &[ScaleIr],
) -> Option<(f64, f64)> {
    let GlyphSizeIr::Mapped { column, .. } = &glyph.size else {
        return None;
    };
    body_scales
        .iter()
        .chain(chart_scales.iter())
        .find_map(|scale| {
            let ScaleTargetIr::Aesthetic {
                aesthetic,
                column: scale_col,
            } = &scale.target
            else {
                return None;
            };
            if aesthetic != "size" {
                return None;
            }
            match scale_col.as_ref() {
                Some(c) if c.name == column.name => {}
                _ => return None,
            }
            match scale.range {
                Some([Some(lo), Some(hi)]) => Some((lo, hi)),
                _ => None,
            }
        })
}

pub(super) fn mapped_size_domain(
    glyph: &GlyphCallIr,
    table: &dyn Table,
    rows: &[usize],
) -> Option<(f64, f64)> {
    let GlyphSizeIr::Mapped { column, .. } = &glyph.size else {
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

pub(super) fn glyph_plot(viewport: Rect, padding: f64) -> Rect {
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

pub(super) fn glyph_budget_diagnostic(
    glyph: &GlyphCallIr,
    host_count: usize,
    matches: &[Vec<usize>],
    child_table: &dyn Table,
    mark_budget: Option<usize>,
) -> Option<Diagnostic> {
    let budget = mark_budget?;
    let child_layers = glyph
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
            codes::E2210,
            format!(
                "glyph `{}` would render about {estimated} child mark(s) from {host_count} host row(s) and {} child row(s), above the mark budget of {budget}",
                glyph.glyph_name,
                child_table.row_count()
            ),
            glyph.span,
        )
        .with_help("filter, aggregate, reduce nested glyph depth, or raise --mark-budget"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use algraf_core::Span;
    use algraf_data::{Column, ColumnDef, DataFrame, DataType, DateTimeValue};
    use algraf_semantics::{ColumnRef, GlyphClipIr, GlyphKeyIr, GlyphScalePolicyIr, SpaceDataRef};

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

    fn match_glyph(rules: Vec<(&str, DataType, &str, DataType)>) -> GlyphCallIr {
        GlyphCallIr {
            glyph_name: "test".to_string(),
            data: SpaceDataRef::Primary,
            key: rules
                .into_iter()
                .map(|(child, child_dtype, host, host_dtype)| GlyphKeyIr {
                    child: col_ref(child, child_dtype),
                    host: GlyphHostRefIr::Current(col_ref(host, host_dtype)),
                    span: Span::empty(0),
                })
                .collect(),
            size: GlyphSizeIr::Fixed {
                width: 10.0,
                height: 10.0,
            },
            scale_policy: GlyphScalePolicyIr::Shared,
            guides: false,
            clip: GlyphClipIr::Rect,
            padding: 0.0,
            placement: GlyphPlacementIr::Position,
            dx: 0.0,
            dy: 0.0,
            legend: true,
            body_scales: Vec::new(),
            child_spaces: Vec::new(),
            span: Span::empty(0),
        }
    }

    fn size_scale_for(column: &ColumnRef, lo: f64, hi: f64) -> ScaleIr {
        ScaleIr {
            target: ScaleTargetIr::Aesthetic {
                aesthetic: "size".into(),
                column: Some(column.clone()),
            },
            scale_type: None,
            mode: None,
            domain: None,
            categorical_domain: None,
            breaks: None,
            tick_interval: None,
            break_labels: None,
            expansion: None,
            range: Some([Some(lo), Some(hi)]),
            color_range: None,
            reverse: None,
            integer: None,
            palette: None,
            gradient: None,
            color_map: None,
            label_map: None,
            label: None,
            train: None,
            span: Span::empty(0),
        }
    }

    fn glyph_with_mapped_size(column: ColumnRef) -> GlyphCallIr {
        GlyphCallIr {
            glyph_name: "test".into(),
            data: SpaceDataRef::Primary,
            key: Vec::new(),
            size: GlyphSizeIr::Mapped {
                column,
                min: 0.0,
                max: 1.0,
            },
            scale_policy: GlyphScalePolicyIr::Shared,
            guides: false,
            clip: GlyphClipIr::Rect,
            padding: 0.0,
            placement: GlyphPlacementIr::Position,
            dx: 0.0,
            dy: 0.0,
            legend: true,
            body_scales: Vec::new(),
            child_spaces: Vec::new(),
            span: Span::empty(0),
        }
    }

    #[test]
    fn body_scale_takes_precedence_over_chart_scale_in_pixel_range() {
        let column = col_ref("weight", DataType::Float);
        let glyph = glyph_with_mapped_size(column.clone());
        let body = vec![size_scale_for(&column, 10.0, 20.0)];
        let chart = vec![size_scale_for(&column, 100.0, 200.0)];

        assert_eq!(
            mapped_size_pixel_range(&glyph, &body, &[]),
            Some((10.0, 20.0))
        );
        assert_eq!(
            mapped_size_pixel_range(&glyph, &[], &chart),
            Some((100.0, 200.0))
        );
        assert_eq!(
            mapped_size_pixel_range(&glyph, &body, &chart),
            Some((10.0, 20.0)),
            "body wins over chart"
        );
    }

    fn matched_rows(
        glyph: &GlyphCallIr,
        child: &DataFrame,
        host: &DataFrame,
        host_row: usize,
    ) -> Vec<usize> {
        GlyphMatchIndex::build(glyph, child).matched_rows(glyph, child, host, host_row, &[])
    }

    #[test]
    fn match_index_excludes_nan_keys() {
        let glyph = match_glyph(vec![("k", DataType::Float, "k", DataType::Float)]);
        let child = frame(vec![(
            "k",
            DataType::Float,
            Column::from_float_options(vec![Some(f64::NAN), Some(1.0)]),
        )]);
        let host = frame(vec![(
            "k",
            DataType::Float,
            Column::from_float_options(vec![Some(f64::NAN), Some(1.0)]),
        )]);

        assert!(matched_rows(&glyph, &child, &host, 0).is_empty());
        assert_eq!(matched_rows(&glyph, &child, &host, 1), vec![1]);
    }

    #[test]
    fn match_index_normalizes_positive_and_negative_zero() {
        let glyph = match_glyph(vec![("k", DataType::Float, "k", DataType::Float)]);
        let child = frame(vec![(
            "k",
            DataType::Float,
            Column::from_float_options(vec![Some(-0.0)]),
        )]);
        let host = frame(vec![(
            "k",
            DataType::Float,
            Column::from_float_options(vec![Some(0.0)]),
        )]);

        assert_eq!(matched_rows(&glyph, &child, &host, 0), vec![0]);
    }

    #[test]
    fn match_index_filters_large_i64_bucket_collisions() {
        let glyph = match_glyph(vec![("k", DataType::Integer, "k", DataType::Integer)]);
        let child = frame(vec![(
            "k",
            DataType::Integer,
            Column::from_int_options(vec![
                Some(9_007_199_254_740_992),
                Some(9_007_199_254_740_993),
            ]),
        )]);
        let host = frame(vec![(
            "k",
            DataType::Integer,
            Column::from_int_options(vec![Some(9_007_199_254_740_993)]),
        )]);

        assert_eq!(matched_rows(&glyph, &child, &host, 0), vec![1]);
    }

    #[test]
    fn match_index_preserves_int_float_comparison_semantics() {
        let int_child = match_glyph(vec![("k", DataType::Integer, "k", DataType::Float)]);
        let child = frame(vec![(
            "k",
            DataType::Integer,
            Column::from_int_options(vec![Some(2), Some(3)]),
        )]);
        let host = frame(vec![(
            "k",
            DataType::Float,
            Column::from_float_options(vec![Some(2.0), Some(2.5)]),
        )]);
        assert_eq!(matched_rows(&int_child, &child, &host, 0), vec![0]);
        assert!(matched_rows(&int_child, &child, &host, 1).is_empty());

        let float_child = match_glyph(vec![("k", DataType::Float, "k", DataType::Integer)]);
        let child = frame(vec![(
            "k",
            DataType::Float,
            Column::from_float_options(vec![Some(3.0)]),
        )]);
        let host = frame(vec![(
            "k",
            DataType::Integer,
            Column::from_int_options(vec![Some(3)]),
        )]);
        assert_eq!(matched_rows(&float_child, &child, &host, 0), vec![0]);
    }

    #[test]
    fn match_index_handles_temporal_and_string_composite_keys() {
        let glyph = match_glyph(vec![
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
        let host = frame(vec![
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

        assert_eq!(matched_rows(&glyph, &child, &host, 0), vec![1]);
    }

    #[test]
    fn match_index_requires_all_composite_components() {
        let glyph = match_glyph(vec![
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
        let host = frame(vec![
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

        assert_eq!(matched_rows(&glyph, &child, &host, 0), vec![1]);
    }

    #[test]
    fn match_index_handles_empty_inputs() {
        let glyph = match_glyph(vec![("k", DataType::String, "k", DataType::String)]);
        let child = frame(vec![("k", DataType::String, Column::String(Vec::new()))]);
        let host = frame(vec![(
            "k",
            DataType::String,
            Column::String(vec![Some("a".to_string())]),
        )]);
        assert!(matched_rows(&glyph, &child, &host, 0).is_empty());

        let host_rows = render_rows(&host, Some(&[]));
        let matches = host_rows
            .iter()
            .map(|&row| matched_rows(&glyph, &child, &host, row))
            .collect::<Vec<_>>();
        assert!(matches.is_empty());
        assert!(union_rows(&matches).is_empty());
    }
}
