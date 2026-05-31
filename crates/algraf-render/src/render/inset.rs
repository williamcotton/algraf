use algraf_core::{codes, Diagnostic};
use algraf_data::{ColumnDef, DataValueRef, Table};
use algraf_semantics::{
    CoordsIr, InsetAnchorIr, InsetClipIr, InsetIr, InsetParentRefIr, InsetPlacementIr,
    InsetScalePolicyIr, InsetSizeIr, SpaceIr, SpaceLayerIr,
};

use crate::domains::train_space_domains;
use crate::geo_stats::centroid_point;
use crate::guide;
use crate::layout::Rect;
use crate::render::backend::RenderScene;
use crate::render::common::{merged_scales, resolve_space_theme};
use crate::render::derived::active_table;
use crate::render::panels::Panel;
use crate::scale::cell_f64;
use crate::sink::MarkSink;
use crate::space::ScaledSpace;

pub(super) const MAX_INSET_DEPTH: usize = 8;

#[derive(Clone, Copy)]
pub(super) struct RowContext<'a> {
    pub(super) table: &'a dyn Table,
    pub(super) row: usize,
}

pub(super) fn paint_panel_layers(
    sink: &mut dyn MarkSink,
    scene: &RenderScene<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (panel_index, panel) in scene.panels.iter().enumerate() {
        paint_panel(sink, scene, panel, panel_index, &[], 0, diagnostics);
    }
}

fn paint_panel(
    sink: &mut dyn MarkSink,
    scene: &RenderScene<'_>,
    panel: &Panel<'_>,
    panel_index: usize,
    ancestors: &[RowContext<'_>],
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if panel.clip_marks {
        sink.open_clip(panel.plot);
    }
    paint_layers(
        sink,
        scene,
        panel.layers,
        panel.table,
        &panel.scaled,
        panel.rows.as_deref(),
        panel.plot,
        &panel.theme,
        &panel.guides,
        &panel.scales,
        panel_index,
        ancestors,
        depth,
        diagnostics,
    );
    if panel.clip_marks {
        sink.close_clip();
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_layers(
    sink: &mut dyn MarkSink,
    scene: &RenderScene<'_>,
    layers: &[SpaceLayerIr],
    table: &dyn Table,
    scaled: &ScaledSpace,
    rows: Option<&[usize]>,
    plot: Rect,
    theme: &crate::theme::Theme,
    _guides: &algraf_semantics::GuideIr,
    scales: &[algraf_semantics::ScaleIr],
    panel_index: usize,
    ancestors: &[RowContext<'_>],
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for layer in layers {
        match layer {
            SpaceLayerIr::Geometry(geo) => crate::geom::render(
                sink,
                geo,
                crate::geom::GeometryRenderContext {
                    space: scaled,
                    table,
                    rows,
                    plot,
                    theme,
                    scales,
                    limits: scene.limits,
                },
                diagnostics,
            ),
            SpaceLayerIr::Inset(inset) => paint_inset(
                sink,
                scene,
                inset,
                table,
                scaled,
                rows,
                panel_index,
                ancestors,
                depth,
                diagnostics,
            ),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_inset(
    sink: &mut dyn MarkSink,
    scene: &RenderScene<'_>,
    inset: &InsetIr,
    parent_table: &dyn Table,
    parent_scaled: &ScaledSpace,
    parent_rows: Option<&[usize]>,
    panel_index: usize,
    ancestors: &[RowContext<'_>],
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if depth >= MAX_INSET_DEPTH {
        diagnostics.push(Diagnostic::error(
            codes::E2109,
            format!("nested Inset depth exceeds the limit of {MAX_INSET_DEPTH}"),
            inset.span,
        ));
        return;
    }

    let parent_row_list = render_rows(parent_table, parent_rows);
    let child_table = active_table(&inset.data, scene.primary, scene.derived);
    let matches = parent_row_list
        .iter()
        .map(|&row| matched_child_rows(inset, child_table, parent_table, row, ancestors))
        .collect::<Vec<_>>();
    let shared_rows = union_rows(&matches);

    if let Some(diagnostic) =
        inset_budget_diagnostic(inset, parent_row_list.len(), &matches, child_table, scene)
    {
        diagnostics.push(diagnostic);
        return;
    }

    let size_domain = mapped_size_domain(inset, parent_table, &parent_row_list);
    for (instance_index, parent_row) in parent_row_list.iter().copied().enumerate() {
        let child_rows = &matches[instance_index];
        if child_rows.is_empty() {
            diagnostics.push(Diagnostic::warning(
                codes::W2002,
                "Inset matched no child rows",
                inset.span,
            ));
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

        sink.open_layer("algraf-inset");
        match inset.clip {
            InsetClipIr::Rect => sink.open_clip(viewport),
            InsetClipIr::Circle => {
                sink.open_circle_clip(
                    viewport.x + viewport.width / 2.0,
                    viewport.y + viewport.height / 2.0,
                    viewport.width.min(viewport.height) / 2.0,
                );
            }
            InsetClipIr::None => {}
        }

        for child_space in &inset.child_spaces {
            paint_child_space(
                sink,
                scene,
                inset,
                child_space,
                child_table,
                child_rows,
                &shared_rows,
                plot,
                panel_index,
                &contexts,
                depth + 1,
                diagnostics,
            );
        }

        if !matches!(inset.clip, InsetClipIr::None) {
            sink.close_clip();
        }
        sink.close_layer();
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_child_space(
    sink: &mut dyn MarkSink,
    scene: &RenderScene<'_>,
    inset: &InsetIr,
    space: &SpaceIr,
    inset_table: &dyn Table,
    child_rows: &[usize],
    shared_rows: &[usize],
    plot: Rect,
    panel_index: usize,
    ancestors: &[RowContext<'_>],
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let table = active_table(&space.data, scene.primary, scene.derived);
    let owned_rows;
    let rows: &[usize] = if space.data == inset.data {
        child_rows
    } else {
        owned_rows = render_rows(table, None);
        &owned_rows
    };
    let training_rows = match inset.scale_policy {
        InsetScalePolicyIr::Shared if space.data == inset.data => shared_rows,
        _ => rows,
    };
    let training = RowsTable::new(
        if space.data == inset.data {
            inset_table
        } else {
            table
        },
        training_rows,
    );
    let render_table = if space.data == inset.data {
        inset_table
    } else {
        table
    };
    let panel_theme =
        resolve_space_theme(scene.theme, space.theme.as_ref(), scene.cli_theme_override);
    let mut space_guides = scene.ir.guides.with_overrides(&space.guides);
    if !inset.guides {
        space_guides.grid = false;
    }
    let space_scales = merged_scales(&scene.ir.scales, &space.scales);
    let hints = train_space_domains(&space.frame, &training, &space.geometries);
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
        return;
    };
    if inset.guides {
        if scaled.is_polar() {
            guide::render_polar_grid(sink, &scaled, &space_guides, &panel_theme);
        } else if space_guides.grid && !scaled.is_spatial() {
            guide::render_grid(sink, &scaled, plot, &panel_theme);
        }
    }
    paint_layers(
        sink,
        scene,
        &space.layers,
        render_table,
        &scaled,
        Some(rows),
        plot,
        &panel_theme,
        &space_guides,
        &space_scales,
        panel_index,
        ancestors,
        depth,
        diagnostics,
    );
    if inset.guides {
        if scaled.is_polar() {
            guide::render_polar_labels(sink, &scaled, &space_guides, &panel_theme);
        } else if panel_theme.axes && !scaled.is_spatial() {
            guide::render_axes(
                sink,
                &scaled,
                plot,
                &panel_theme,
                guide::AxisRenderOptions {
                    x_label_override: space_guides.x_label.as_deref(),
                    y_label_override: space_guides.y_label.as_deref(),
                    x_time_format: space_guides.x_time_format.as_ref(),
                    y_time_format: space_guides.y_time_format.as_ref(),
                    x_tick_label_angle: space_guides.x_tick_label_angle,
                    y_tick_label_angle: space_guides.y_tick_label_angle,
                    x_tick_label_rows: space_guides.x_tick_label_rows,
                    y_tick_label_rows: space_guides.y_tick_label_rows,
                },
            );
        }
    }
}

pub(super) fn matched_child_rows(
    inset: &InsetIr,
    child_table: &dyn Table,
    current_table: &dyn Table,
    current_row: usize,
    ancestors: &[RowContext<'_>],
) -> Vec<usize> {
    (0..child_table.row_count())
        .filter(|&child_row| {
            inset.match_rules.iter().all(|rule| {
                let Some(child_value) = child_table.value(&rule.child.name, child_row) else {
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

fn inset_budget_diagnostic(
    inset: &InsetIr,
    parent_count: usize,
    matches: &[Vec<usize>],
    child_table: &dyn Table,
    scene: &RenderScene<'_>,
) -> Option<Diagnostic> {
    let budget = scene.limits.mark_budget?;
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

pub(super) struct RowsTable<'a> {
    table: &'a dyn Table,
    rows: &'a [usize],
}

impl<'a> RowsTable<'a> {
    pub(super) fn new(table: &'a dyn Table, rows: &'a [usize]) -> Self {
        RowsTable { table, rows }
    }
}

impl Table for RowsTable<'_> {
    fn schema(&self) -> &[ColumnDef] {
        self.table.schema()
    }

    fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn value(&self, column: &str, row: usize) -> Option<DataValueRef<'_>> {
        let source_row = *self.rows.get(row)?;
        self.table.value(column, source_row)
    }

    fn column(&self, _column: &str) -> Option<algraf_data::ColumnView<'_>> {
        None
    }
}
