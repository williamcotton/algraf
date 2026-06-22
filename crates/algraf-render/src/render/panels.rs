use std::collections::HashMap;

use algraf_core::{codes, Diagnostic};
use algraf_data::{DataFrame, Table};
use algraf_semantics::{
    AxisSelectorIr, ChartIr, ColumnRef, CoordsIr, FacetGridIr, FacetLabelModeIr, FacetScaleModeIr,
    FrameIr, GeometryIr, GeometryKind, GuideIr, PropertyKey, ScaleIr, ScaleTargetIr, SpaceIr,
    SpaceLayerIr,
};

use crate::aes::{color_spec, number_setting, number_spec, ColorSpec, NumberSpec};
use crate::domains::{train_space_domains, AxisDomainHints, SpaceDomainHints};
use crate::geom::{DEFAULT_SIZE_RANGE, DEFAULT_STROKE_WIDTH_RANGE};
use crate::guide;
use crate::helpers::frame_axis;
use crate::layout::{AxisSides, GuideExtra, Layout, Margins, Rect, MARGIN_BOTTOM, MARGIN_LEFT};
use crate::scale::{categorical_domain, cell_category, numeric_domain, temporal_domain};
use crate::space::{AxisScale, ScaledSpace};
use crate::theme::Theme;

use super::common::{merged_scales, resolve_space_theme, validate_scale_configs};
use super::derived::active_table;
use super::glyph_plan::{plan_glyph, PlannedGlyph, RowContext};
use super::legend::collect_legends;
use super::panel_space::{build_cartesian_scaled, compute_layout};
use super::row_table::RowSubsetTable;
use super::spatial::{build_spatial_plan, is_spatial_space};
use super::{ImageAssets, RenderLimits};

pub(super) struct Panel<'t> {
    pub(super) table: &'t dyn Table,
    pub(super) frame: &'t FrameIr,
    pub(super) scaled: ScaledSpace,
    pub(super) layers: Vec<PlannedLayer<'t>>,
    pub(super) plot: Rect,
    pub(super) rows: Option<Vec<usize>>,
    pub(super) legend_rows: Option<Vec<usize>>,
    pub(super) label: Option<String>,
    pub(super) facet_index: Option<usize>,
    pub(super) clip_edges: ClipEdges,
    pub(super) clip: Option<PanelClip>,
    pub(super) theme: Theme,
    pub(super) guides: GuideIr,
    pub(super) scales: Vec<ScaleIr>,
    pub(super) show_guides: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ClipEdges {
    pub(super) top: bool,
    pub(super) right: bool,
    pub(super) bottom: bool,
    pub(super) left: bool,
}

impl ClipEdges {
    fn any(self) -> bool {
        self.top || self.right || self.bottom || self.left
    }

    fn set(&mut self, edge: ClipEdge) {
        match edge {
            ClipEdge::Top => self.top = true,
            ClipEdge::Right => self.right = true,
            ClipEdge::Bottom => self.bottom = true,
            ClipEdge::Left => self.left = true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipEdge {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PanelClip {
    pub(super) edges: ClipEdges,
    pub(super) rect: Rect,
}

impl PanelClip {
    pub(super) fn clips_point(self, x: f64, y: f64) -> bool {
        (self.edges.left && x < self.rect.x - f64::EPSILON)
            || (self.edges.right && x > self.rect.right() + f64::EPSILON)
            || (self.edges.top && y < self.rect.y - f64::EPSILON)
            || (self.edges.bottom && y > self.rect.bottom() + f64::EPSILON)
    }
}

pub(super) enum PlannedLayer<'t> {
    Geometry(&'t GeometryIr),
    Glyph(PlannedGlyph<'t>),
}

pub(super) struct RenderPlan<'t> {
    pub(super) layout: Layout,
    pub(super) legends: Vec<crate::aes::Legend>,
    pub(super) panels: Vec<Panel<'t>>,
}

pub(super) struct PanelSlot<'a> {
    pub(super) plot: Rect,
    pub(super) strip: Option<Rect>,
    pub(super) label: Option<&'a str>,
    pub(super) facet_index: Option<usize>,
    pub(super) panel: Option<&'a Panel<'a>>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_render_plan<'t>(
    ir: &'t ChartIr,
    primary: &'t dyn Table,
    derived: &'t HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    assets: &ImageAssets,
    limits: &RenderLimits,
    diagnostics: &mut Vec<Diagnostic>,
) -> RenderPlan<'t> {
    let width = ir.width as f64;
    let height = ir.height as f64;

    let is_polar = ir
        .spaces
        .iter()
        .all(|s| matches!(s.coords, CoordsIr::Polar { .. }));
    let has_axes = theme.axes && !is_polar;

    let margins = Margins {
        top: ir.margin_top.map(f64::from),
        right: ir.margin_right.map(f64::from),
        bottom: ir.margin_bottom.map(f64::from),
        left: ir.margin_left.map(f64::from),
    };
    let (top_extra, chart_bottom_extra) = chart_text_reserve(ir, theme);
    // The y axis can move to the right and the x axis to the top; a per-chart
    // `Guide(position:)` overrides the theme default, which overrides the
    // built-in left/bottom (spec §19.2, §19.3, §20.1).
    let y_axis_position = ir.guides.y_position.unwrap_or(theme.axis_y_position);
    let x_axis_position = ir.guides.x_position.unwrap_or(theme.axis_x_position);
    let axis_sides = AxisSides::from_positions(y_axis_position, x_axis_position);
    // A first pass with a provisional layout to discover legends.
    let provisional = Layout::compute_with_text(
        width,
        height,
        false,
        has_axes,
        top_extra,
        chart_bottom_extra,
        GuideExtra::default(),
        axis_sides,
        margins,
        theme.legend_position,
    );
    let bottom_extra = chart_bottom_extra;
    let guide_extra = if has_axes {
        GuideExtra {
            x: x_label_bottom_extra(ir, primary, derived, &provisional, theme),
            y: y_label_left_extra(ir, primary, derived, &provisional, theme),
        }
    } else {
        GuideExtra::default()
    };
    let grid_categories = ir
        .layout
        .facet_grid
        .as_ref()
        .map(|grid| facet_grid_categories(primary, grid));
    let facet_panel_count = facet_panel_count(ir, primary, derived);
    let layout_without_legends = compute_layout(
        ir,
        width,
        height,
        false,
        has_axes,
        top_extra,
        bottom_extra,
        guide_extra,
        axis_sides,
        margins,
        grid_categories.as_ref(),
        facet_panel_count,
        theme,
        None,
    );

    // Position scales are shared across overlaid (non-faceted) spaces, even when
    // they back onto different tables (spec §17.5). Compute the unioned x/y
    // extent across those spaces and inject it as a soft bound below.
    let shared_x = shared_axis_extent(ir, primary, derived, AxisSelectorIr::X);
    let shared_y = shared_axis_extent(ir, primary, derived, AxisSelectorIr::Y);

    // Spatial (map) spaces share one projection and one projected bounding box
    // across overlaid layers, so a basemap and a point overlay align
    // (spec §16.15, §17.5). Resolve that plan once.
    let spatial_plan = build_spatial_plan(ir, primary, derived, diagnostics);

    let build_panels_for_layout = |layout: &Layout,
                                   diagnostics: &mut Vec<Diagnostic>|
     -> Vec<Panel<'t>> {
        let mut panels = Vec::new();
        for space in &ir.spaces {
            let table = active_table(&space.data, primary, derived);
            let panel_theme = resolve_space_theme(theme, space.theme.as_ref(), cli_theme_override);
            let space_guides = ir.guides.with_overrides(&space.guides);
            let space_scales = merged_scales(&ir.scales, &space.scales);
            validate_scale_configs(&space.frame, table, &space_scales, space.span, diagnostics);
            if is_spatial_space(space) {
                // A spatial space projects geographic coordinates into the plot;
                // it has no planar axes or facets.
                if let Some(plan) = &spatial_plan {
                    if let Some(scaled) = plan.scaled_space(space, layout.plot) {
                        panels.push(build_root_panel(
                            ir,
                            primary,
                            derived,
                            theme,
                            cli_theme_override,
                            limits,
                            space,
                            table,
                            scaled,
                            layout.plot,
                            None,
                            None,
                            None,
                            panel_theme,
                            space_guides,
                            space_scales,
                            true,
                            diagnostics,
                        ));
                    }
                }
                continue;
            }
            if let (Some(grid), Some((row_categories, col_categories))) =
                (&ir.layout.facet_grid, &grid_categories)
            {
                let Some(plane) = facet_grid_plane(&space.frame) else {
                    diagnostics.push(Diagnostic::warning(
                        codes::R0003,
                        "this space is not compatible with the facet grid",
                        space.span,
                    ));
                    continue;
                };
                let fixed_hints =
                    train_space_domains(plane, table, &space.geometries, &space_scales);
                for (index, facet) in layout.facets.iter().enumerate() {
                    let row_index = index / col_categories.len().max(1);
                    let col_index = index % col_categories.len().max(1);
                    let row_value = grid
                        .rows
                        .as_ref()
                        .and_then(|_| row_categories.get(row_index))
                        .cloned();
                    let col_value = grid
                        .columns
                        .as_ref()
                        .and_then(|_| col_categories.get(col_index))
                        .cloned();
                    let rows =
                        facet_grid_rows(table, grid, row_value.as_deref(), col_value.as_deref());
                    let filtered = RowSubsetTable::new(table, &rows);
                    let free_hints =
                        train_space_domains(plane, &filtered, &space.geometries, &space_scales);
                    let (x_table, y_table, domain_hints) = facet_training(
                        ir.layout.facet_scales,
                        table,
                        &filtered,
                        fixed_hints.clone(),
                        free_hints,
                    );
                    match build_cartesian_scaled(
                        plane,
                        x_table,
                        y_table,
                        facet.plot,
                        &domain_hints,
                        &space_scales,
                        space,
                    ) {
                        Some((scaled, plot)) => panels.push(build_root_panel(
                            ir,
                            primary,
                            derived,
                            theme,
                            cli_theme_override,
                            limits,
                            space,
                            table,
                            scaled,
                            plot,
                            Some(rows),
                            Some(facet_grid_label(
                                grid,
                                row_value.as_deref(),
                                col_value.as_deref(),
                                ir.layout.facet_label,
                                &ir.layout.facet_label_map,
                            )),
                            Some(index),
                            panel_theme.clone(),
                            space_guides.clone(),
                            space_scales.clone(),
                            true,
                            diagnostics,
                        )),
                        None => diagnostics.push(Diagnostic::warning(
                            codes::R0003,
                            "this facet-grid panel could not be laid out",
                            space.span,
                        )),
                    }
                }
                continue;
            }
            if let Some((plane, facet_col)) = facet_frame(&space.frame) {
                let domain_hints =
                    train_space_domains(plane, table, &space.geometries, &space_scales);
                for (index, category) in facet_categories(table, &facet_col.name).iter().enumerate()
                {
                    let Some(facet) = layout.facets.get(index) else {
                        continue;
                    };
                    let rows = facet_rows(table, &facet_col.name, category);
                    let filtered = RowSubsetTable::new(table, &rows);
                    let free_hints =
                        train_space_domains(plane, &filtered, &space.geometries, &space_scales);
                    let (x_table, y_table, domain_hints) = facet_training(
                        ir.layout.facet_scales,
                        table,
                        &filtered,
                        domain_hints.clone(),
                        free_hints,
                    );
                    match build_cartesian_scaled(
                        plane,
                        x_table,
                        y_table,
                        facet.plot,
                        &domain_hints,
                        &space_scales,
                        space,
                    ) {
                        Some((scaled, plot)) => panels.push(build_root_panel(
                            ir,
                            primary,
                            derived,
                            theme,
                            cli_theme_override,
                            limits,
                            space,
                            table,
                            scaled,
                            plot,
                            Some(rows),
                            Some(facet_value_label(
                                &facet_col.name,
                                category,
                                ir.layout.facet_label,
                                &ir.layout.facet_label_map,
                            )),
                            Some(index),
                            panel_theme.clone(),
                            space_guides.clone(),
                            space_scales.clone(),
                            true,
                            diagnostics,
                        )),
                        None => diagnostics.push(Diagnostic::warning(
                            codes::R0003,
                            "this faceted space could not be laid out",
                            space.span,
                        )),
                    }
                }
            } else {
                let mut domain_hints =
                    train_space_domains(&space.frame, table, &space.geometries, &space_scales);
                // Polar spaces are self-contained (one circular plot); Cartesian
                // axis-sharing across overlaid spaces does not apply (spec §16.16).
                if let CoordsIr::Polar {
                    theta,
                    inner_radius,
                    start_angle,
                    direction,
                } = space.coords
                {
                    match ScaledSpace::build_polar(
                        &space.frame,
                        table,
                        layout.plot,
                        &domain_hints,
                        &space_scales,
                        theta,
                        inner_radius,
                        start_angle,
                        direction,
                        panel_theme.font_size,
                    ) {
                        Some(scaled) => panels.push(build_root_panel(
                            ir,
                            primary,
                            derived,
                            theme,
                            cli_theme_override,
                            limits,
                            space,
                            table,
                            scaled,
                            layout.plot,
                            None,
                            None,
                            None,
                            panel_theme,
                            space_guides,
                            space_scales,
                            true,
                            diagnostics,
                        )),
                        None => diagnostics.push(Diagnostic::warning(
                            codes::R0003,
                            "this space could not be laid out",
                            space.span,
                        )),
                    }
                } else {
                    shared_x.apply(&mut domain_hints.x);
                    shared_y.apply(&mut domain_hints.y);
                    match build_cartesian_scaled(
                        &space.frame,
                        table,
                        table,
                        layout.plot,
                        &domain_hints,
                        &space_scales,
                        space,
                    ) {
                        Some((scaled, plot)) => panels.push(build_root_panel(
                            ir,
                            primary,
                            derived,
                            theme,
                            cli_theme_override,
                            limits,
                            space,
                            table,
                            scaled,
                            plot,
                            None,
                            None,
                            None,
                            panel_theme,
                            space_guides,
                            space_scales,
                            true,
                            diagnostics,
                        )),
                        None => diagnostics.push(Diagnostic::warning(
                            codes::R0003,
                            "this space could not be laid out",
                            space.span,
                        )),
                    }
                }
            }
        }
        panels
    };

    let mut scratch_diagnostics = Vec::new();
    let mut panels = build_panels_for_layout(&layout_without_legends, &mut scratch_diagnostics);
    let mut legends = collect_legends(&panels, theme, assets);
    let layout = if legends.is_empty() {
        diagnostics.append(&mut scratch_diagnostics);
        layout_without_legends
    } else {
        let legend_size = guide::legend_size(
            &legends,
            theme,
            theme.legend_position,
            layout_without_legends.plot.width,
        );
        let layout = compute_layout(
            ir,
            width,
            height,
            true,
            has_axes,
            top_extra,
            bottom_extra,
            guide_extra,
            axis_sides,
            margins,
            grid_categories.as_ref(),
            facet_panel_count,
            theme,
            Some(legend_size),
        );
        panels = build_panels_for_layout(&layout, diagnostics);
        legends = collect_legends(&panels, theme, assets);
        layout
    };
    resolve_panel_clips(&mut panels, &layout);

    RenderPlan {
        layout,
        legends,
        panels,
    }
}

pub(super) fn resolve_panel_clips(panels: &mut [Panel<'_>], layout: &Layout) {
    let blockers = clip_blockers(layout);
    resolve_panel_clips_in_bounds(panels, layout.svg, &blockers);
}

pub(super) fn resolve_panel_clips_in_bounds(
    panels: &mut [Panel<'_>],
    bounds: Rect,
    blockers: &[Rect],
) {
    for panel in panels {
        if !panel.clip_edges.any() {
            panel.clip = None;
            continue;
        }
        let bleed = panel_mark_extent(panel);
        let caps = clip_bleed_caps(panel.plot, bounds, blockers);
        panel.clip = Some(PanelClip {
            edges: panel.clip_edges,
            rect: resolved_clip_rect(panel.plot, bounds, panel.clip_edges, bleed, caps),
        });
    }
}

fn clip_blockers(layout: &Layout) -> Vec<Rect> {
    let mut blockers = Vec::new();
    if layout.facets.is_empty() {
        blockers.push(layout.plot);
    } else {
        for facet in &layout.facets {
            if facet.strip.width > 0.0 && facet.strip.height > 0.0 {
                blockers.push(facet.strip);
            }
            blockers.push(facet.plot);
        }
    }
    if let Some(legend) = layout.legend {
        blockers.push(legend);
    }
    blockers
}

fn cartesian_clip_edges(space: &SpaceIr, scaled: &ScaledSpace, scales: &[ScaleIr]) -> ClipEdges {
    if !matches!(space.coords, CoordsIr::Cartesian) {
        return ClipEdges::default();
    }
    let mut edges = ClipEdges::default();
    if space.view.zoom_x.is_some() {
        edges.left = true;
        edges.right = true;
    }
    if space.view.zoom_y.is_some() && scaled.y.is_some() {
        edges.top = true;
        edges.bottom = true;
    }
    if let Some(bounds) = axis_domain_bounds(scales, AxisSelectorIr::X) {
        add_bound_edges(&mut edges, AxisSelectorIr::X, &scaled.x, bounds);
    }
    if let (Some(axis), Some(bounds)) = (
        scaled.y.as_ref(),
        axis_domain_bounds(scales, AxisSelectorIr::Y),
    ) {
        add_bound_edges(&mut edges, AxisSelectorIr::Y, axis, bounds);
    }
    edges
}

fn axis_domain_bounds(scales: &[ScaleIr], axis: AxisSelectorIr) -> Option<[Option<f64>; 2]> {
    scales.iter().rev().find_map(|scale| match scale.target {
        ScaleTargetIr::Axis(target) if target == axis => scale.domain,
        _ => None,
    })
}

fn add_bound_edges(
    edges: &mut ClipEdges,
    selector: AxisSelectorIr,
    axis: &AxisScale,
    bounds: [Option<f64>; 2],
) {
    let Some((lower, upper)) = bound_edges(selector, axis) else {
        return;
    };
    if bounds[0].is_some() {
        edges.set(lower);
    }
    if bounds[1].is_some() {
        edges.set(upper);
    }
}

fn bound_edges(selector: AxisSelectorIr, axis: &AxisScale) -> Option<(ClipEdge, ClipEdge)> {
    let (range0, range1) = continuousish_axis_range(axis)?;
    let lower_before_upper = range0 <= range1;
    match selector {
        AxisSelectorIr::X if lower_before_upper => Some((ClipEdge::Left, ClipEdge::Right)),
        AxisSelectorIr::X => Some((ClipEdge::Right, ClipEdge::Left)),
        AxisSelectorIr::Y if lower_before_upper => Some((ClipEdge::Top, ClipEdge::Bottom)),
        AxisSelectorIr::Y => Some((ClipEdge::Bottom, ClipEdge::Top)),
    }
}

fn continuousish_axis_range(axis: &AxisScale) -> Option<(f64, f64)> {
    match axis {
        AxisScale::Continuous { scale, .. } | AxisScale::Union { scale, .. } => Some(scale.range),
        AxisScale::Temporal { scale, .. }
        | AxisScale::TemporalNestedBand { scale, .. }
        | AxisScale::TemporalUnion { scale, .. } => Some(scale.range),
        AxisScale::Band { .. } | AxisScale::NestedBand { .. } => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct ClipBleedCaps {
    top: f64,
    right: f64,
    bottom: f64,
    left: f64,
}

fn clip_bleed_caps(plot: Rect, bounds: Rect, blockers: &[Rect]) -> ClipBleedCaps {
    let mut caps = ClipBleedCaps {
        top: (plot.y - bounds.y).max(0.0),
        right: (bounds.right() - plot.right()).max(0.0),
        bottom: (bounds.bottom() - plot.bottom()).max(0.0),
        left: (plot.x - bounds.x).max(0.0),
    };
    for blocker in blockers {
        if same_rect(*blocker, plot) {
            continue;
        }
        if spans_overlap(blocker.y, blocker.bottom(), plot.y, plot.bottom()) {
            if blocker.right() <= plot.x + f64::EPSILON {
                caps.left = caps.left.min((plot.x - blocker.right()).max(0.0));
            }
            if blocker.x >= plot.right() - f64::EPSILON {
                caps.right = caps.right.min((blocker.x - plot.right()).max(0.0));
            }
        }
        if spans_overlap(blocker.x, blocker.right(), plot.x, plot.right()) {
            if blocker.bottom() <= plot.y + f64::EPSILON {
                caps.top = caps.top.min((plot.y - blocker.bottom()).max(0.0));
            }
            if blocker.y >= plot.bottom() - f64::EPSILON {
                caps.bottom = caps.bottom.min((blocker.y - plot.bottom()).max(0.0));
            }
        }
    }
    caps
}

fn resolved_clip_rect(
    plot: Rect,
    bounds: Rect,
    edges: ClipEdges,
    bleed: f64,
    caps: ClipBleedCaps,
) -> Rect {
    let left = if edges.left {
        plot.x - bleed.min(caps.left)
    } else {
        bounds.x
    };
    let right = if edges.right {
        plot.right() + bleed.min(caps.right)
    } else {
        bounds.right()
    };
    let top = if edges.top {
        plot.y - bleed.min(caps.top)
    } else {
        bounds.y
    };
    let bottom = if edges.bottom {
        plot.bottom() + bleed.min(caps.bottom)
    } else {
        bounds.bottom()
    };
    Rect {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    }
}

fn same_rect(a: Rect, b: Rect) -> bool {
    (a.x - b.x).abs() <= f64::EPSILON
        && (a.y - b.y).abs() <= f64::EPSILON
        && (a.width - b.width).abs() <= f64::EPSILON
        && (a.height - b.height).abs() <= f64::EPSILON
}

fn spans_overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> bool {
    a0 < b1 - f64::EPSILON && b0 < a1 - f64::EPSILON
}

fn panel_mark_extent(panel: &Panel<'_>) -> f64 {
    panel
        .layers
        .iter()
        .map(|layer| match layer {
            PlannedLayer::Geometry(geo) => geometry_mark_extent(
                geo,
                panel.table,
                panel.rows.as_deref(),
                &panel.theme,
                &panel.scales,
            ),
            PlannedLayer::Glyph(glyph) => glyph
                .instances
                .iter()
                .map(|instance| instance.viewport.width.max(instance.viewport.height) / 2.0)
                .fold(0.0, f64::max),
        })
        .fold(0.0, f64::max)
}

fn geometry_mark_extent(
    geo: &GeometryIr,
    table: &dyn Table,
    rows: Option<&[usize]>,
    theme: &Theme,
    scales: &[ScaleIr],
) -> f64 {
    match geo.kind {
        GeometryKind::Point => max_number_spec(
            &number_spec(
                geo,
                PropertyKey::Size,
                table,
                scales,
                DEFAULT_SIZE_RANGE,
                theme.point_size,
            ),
            table,
            rows,
            theme.point_size,
        )
        .max(0.0),
        GeometryKind::Image => {
            max_number_spec(
                &number_spec(
                    geo,
                    PropertyKey::Size,
                    table,
                    scales,
                    DEFAULT_SIZE_RANGE,
                    theme.point_size,
                ),
                table,
                rows,
                theme.point_size,
            )
            .max(0.0)
                / 2.0
        }
        GeometryKind::Line | GeometryKind::Path => {
            max_number_spec(
                &number_spec(
                    geo,
                    PropertyKey::StrokeWidth,
                    table,
                    scales,
                    DEFAULT_STROKE_WIDTH_RANGE,
                    theme.line_width,
                ),
                table,
                rows,
                theme.line_width,
            )
            .max(0.0)
                / 2.0
        }
        GeometryKind::Smooth
        | GeometryKind::Boxplot
        | GeometryKind::Violin
        | GeometryKind::Density
        | GeometryKind::ErrorBar
        | GeometryKind::LineRange
        | GeometryKind::PointRange
        | GeometryKind::CrossBar
        | GeometryKind::HexBin
        | GeometryKind::HLine
        | GeometryKind::VLine
        | GeometryKind::Rug
        | GeometryKind::Segment => {
            number_setting(geo, PropertyKey::StrokeWidth, theme.line_width).max(0.0) / 2.0
        }
        GeometryKind::Bar
        | GeometryKind::Rect
        | GeometryKind::Tile
        | GeometryKind::Ribbon
        | GeometryKind::Area
        | GeometryKind::Geo => {
            if stroke_visible(geo, table, rows, scales) {
                number_setting(geo, PropertyKey::StrokeWidth, 1.0).max(0.0) / 2.0
            } else {
                0.0
            }
        }
        GeometryKind::Text
        | GeometryKind::Label
        | GeometryKind::Graticule
        | GeometryKind::Histogram
        | GeometryKind::FreqPoly
        | GeometryKind::Bin2D => 0.0,
    }
}

fn max_number_spec(
    spec: &NumberSpec,
    table: &dyn Table,
    rows: Option<&[usize]>,
    default: f64,
) -> f64 {
    render_row_indices(table, rows)
        .into_iter()
        .map(|row| spec.at(table, row, default))
        .fold(0.0, f64::max)
}

fn stroke_visible(
    geo: &GeometryIr,
    table: &dyn Table,
    rows: Option<&[usize]>,
    scales: &[ScaleIr],
) -> bool {
    let stroke = color_spec(geo, PropertyKey::Stroke, table, scales);
    match stroke {
        ColorSpec::None => false,
        ColorSpec::Constant(_) => true,
        _ => render_row_indices(table, rows)
            .into_iter()
            .any(|row| stroke.resolve(table, row).is_some()),
    }
}

fn render_row_indices(table: &dyn Table, rows: Option<&[usize]>) -> Vec<usize> {
    rows.map_or_else(|| (0..table.row_count()).collect(), <[usize]>::to_vec)
}

#[allow(clippy::too_many_arguments)]
fn build_root_panel<'t>(
    ir: &'t ChartIr,
    primary: &'t dyn Table,
    derived: &'t HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: &RenderLimits,
    space: &'t SpaceIr,
    table: &'t dyn Table,
    scaled: ScaledSpace,
    plot: Rect,
    rows: Option<Vec<usize>>,
    label: Option<String>,
    facet_index: Option<usize>,
    panel_theme: Theme,
    space_guides: GuideIr,
    space_scales: Vec<ScaleIr>,
    show_guides: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Panel<'t> {
    planned_panel(
        ir,
        primary,
        derived,
        theme,
        cli_theme_override,
        limits,
        space,
        table,
        scaled,
        plot,
        rows,
        None,
        label,
        facet_index,
        panel_theme,
        space_guides,
        space_scales,
        show_guides,
        &[],
        0,
        diagnostics,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn planned_panel<'t>(
    ir: &'t ChartIr,
    primary: &'t dyn Table,
    derived: &'t HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: &RenderLimits,
    space: &'t SpaceIr,
    table: &'t dyn Table,
    scaled: ScaledSpace,
    plot: Rect,
    rows: Option<Vec<usize>>,
    legend_rows: Option<Vec<usize>>,
    label: Option<String>,
    facet_index: Option<usize>,
    panel_theme: Theme,
    space_guides: GuideIr,
    space_scales: Vec<ScaleIr>,
    show_guides: bool,
    ancestors: &[RowContext<'t>],
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Panel<'t> {
    let layers = plan_layers(
        ir,
        primary,
        derived,
        theme,
        cli_theme_override,
        limits,
        &space.layers,
        table,
        &scaled,
        rows.as_deref(),
        ancestors,
        depth,
        diagnostics,
    );
    let clip_edges = cartesian_clip_edges(space, &scaled, &space_scales);
    Panel {
        table,
        frame: &space.frame,
        scaled,
        layers,
        plot,
        rows,
        legend_rows,
        label,
        facet_index,
        clip_edges,
        clip: None,
        theme: panel_theme,
        guides: space_guides,
        scales: space_scales,
        show_guides,
    }
}

#[allow(clippy::too_many_arguments)]
fn plan_layers<'t>(
    ir: &'t ChartIr,
    primary: &'t dyn Table,
    derived: &'t HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: &RenderLimits,
    layers: &'t [SpaceLayerIr],
    parent_table: &'t dyn Table,
    parent_scaled: &ScaledSpace,
    parent_rows: Option<&[usize]>,
    ancestors: &[RowContext<'t>],
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<PlannedLayer<'t>> {
    layers
        .iter()
        .map(|layer| match layer {
            SpaceLayerIr::Geometry(geo) => PlannedLayer::Geometry(geo),
            SpaceLayerIr::Glyph(glyph) => PlannedLayer::Glyph(plan_glyph(
                ir,
                primary,
                derived,
                theme,
                cli_theme_override,
                limits,
                glyph,
                parent_table,
                parent_scaled,
                parent_rows,
                ancestors,
                depth,
                diagnostics,
            )),
        })
        .collect()
}

fn facet_training<'a>(
    mode: FacetScaleModeIr,
    full: &'a dyn Table,
    filtered: &'a dyn Table,
    fixed_hints: SpaceDomainHints,
    free_hints: SpaceDomainHints,
) -> (&'a dyn Table, &'a dyn Table, SpaceDomainHints) {
    match mode {
        FacetScaleModeIr::Fixed => (full, full, fixed_hints),
        FacetScaleModeIr::FreeX => (
            filtered,
            full,
            SpaceDomainHints {
                x: free_hints.x,
                y: fixed_hints.y,
            },
        ),
        FacetScaleModeIr::FreeY => (
            full,
            filtered,
            SpaceDomainHints {
                x: fixed_hints.x,
                y: free_hints.y,
            },
        ),
        FacetScaleModeIr::Free => (filtered, filtered, free_hints),
    }
}

fn facet_grid_plane(frame: &FrameIr) -> Option<&FrameIr> {
    if let Some((plane, _)) = facet_frame(frame) {
        return Some(plane);
    }
    matches!(frame, FrameIr::Cartesian(axes) if axes.len() == 2).then_some(frame)
}

fn facet_grid_categories(table: &dyn Table, grid: &FacetGridIr) -> (Vec<String>, Vec<String>) {
    let rows = grid
        .rows
        .as_ref()
        .map(|column| facet_categories(table, &column.name))
        .unwrap_or_else(|| vec![String::new()]);
    let cols = grid
        .columns
        .as_ref()
        .map(|column| facet_categories(table, &column.name))
        .unwrap_or_else(|| vec![String::new()]);
    (rows, cols)
}

fn facet_grid_rows(
    table: &dyn Table,
    grid: &FacetGridIr,
    row_value: Option<&str>,
    col_value: Option<&str>,
) -> Vec<usize> {
    (0..table.row_count())
        .filter(|&row| {
            let row_ok = match (&grid.rows, row_value) {
                (Some(column), Some(value)) => {
                    cell_category(table, &column.name, row).as_deref() == Some(value)
                }
                (Some(_), None) => false,
                (None, _) => true,
            };
            let col_ok = match (&grid.columns, col_value) {
                (Some(column), Some(value)) => {
                    cell_category(table, &column.name, row).as_deref() == Some(value)
                }
                (Some(_), None) => false,
                (None, _) => true,
            };
            row_ok && col_ok
        })
        .collect()
}

fn facet_grid_label(
    grid: &FacetGridIr,
    row_value: Option<&str>,
    col_value: Option<&str>,
    mode: FacetLabelModeIr,
    label_map: &[(String, String)],
) -> String {
    let mut parts = Vec::new();
    if let (Some(column), Some(value)) = (&grid.rows, row_value) {
        parts.push(facet_value_label(&column.name, value, mode, label_map));
    }
    if let (Some(column), Some(value)) = (&grid.columns, col_value) {
        parts.push(facet_value_label(&column.name, value, mode, label_map));
    }
    parts.join("\n")
}

fn facet_value_label(
    column: &str,
    value: &str,
    mode: FacetLabelModeIr,
    label_map: &[(String, String)],
) -> String {
    let label = label_map
        .iter()
        .find(|(raw, _)| raw == value)
        .map(|(_, label)| label.as_str())
        .unwrap_or(value);
    match mode {
        FacetLabelModeIr::Value => label.to_string(),
        FacetLabelModeIr::NameValue => format!("{column}: {label}"),
        FacetLabelModeIr::Null => String::new(),
    }
}

pub(super) fn panel_slots<'a>(layout: &'a Layout, panels: &'a [Panel<'a>]) -> Vec<PanelSlot<'a>> {
    if layout.facets.is_empty() {
        let panel = panels.first();
        return vec![PanelSlot {
            plot: panel.map_or(layout.plot, |panel| panel.plot),
            strip: None,
            label: None,
            facet_index: None,
            panel,
        }];
    }

    layout
        .facets
        .iter()
        .enumerate()
        .map(|(index, facet)| {
            let panel = panels.iter().find(|panel| panel.facet_index == Some(index));
            PanelSlot {
                plot: panel.map_or(facet.plot, |panel| panel.plot),
                strip: Some(facet.strip),
                label: panel.and_then(|panel| panel.label.as_deref()),
                facet_index: Some(index),
                panel,
            }
        })
        .collect()
}

/// Extra bottom margin needed so rotated x tick labels and the x-axis title fit
/// (spec §17.3). Tick label text depends on the domain, not the pixel range, so
/// a provisional layout is enough to measure it. Returns 0 when labels fit the
/// default margin.
fn x_label_bottom_extra(
    ir: &ChartIr,
    primary: &dyn Table,
    derived: &HashMap<String, DataFrame>,
    provisional: &Layout,
    theme: &Theme,
) -> f64 {
    let x_range = (provisional.plot.x, provisional.plot.right());
    let y_range = (provisional.plot.bottom(), provisional.plot.y);
    let mut max_label_height = 0.0_f64;
    for space in &ir.spaces {
        let table = active_table(&space.data, primary, derived);
        let space_scales = merged_scales(&ir.scales, &space.scales);
        let frame = match facet_frame(&space.frame) {
            Some((plane, _)) => plane,
            None => &space.frame,
        };
        let hints = train_space_domains(frame, table, &space.geometries, &space_scales);
        if let Some(scaled) = ScaledSpace::build(
            frame,
            table,
            x_range,
            y_range,
            &hints,
            &space_scales,
            space.view,
        ) {
            let guides = ir.guides.with_overrides(&space.guides);
            max_label_height = max_label_height.max(guide::max_x_tick_label_height(
                &scaled,
                theme.axis_text.size,
                guides.x_time_format.as_ref(),
                guides.x_format.as_deref(),
                guides.x_tick_label_angle,
                guides.x_tick_label_rows,
            ));
        }
    }
    // A hidden `axisText`/`axisTitle` token reclaims its share of the band
    // (spec §20.8): no tick labels means zero label height; no axis title means
    // zero title height.
    let title_hidden = theme.axis_title.hidden;
    if theme.axis_text.hidden {
        if title_hidden {
            return 0.0;
        }
        return (guide::x_axis_bottom_margin(0.0, theme.axis_title.size) - MARGIN_BOTTOM).max(0.0);
    }
    if max_label_height <= 0.0 {
        return 0.0;
    }
    let title_size = if title_hidden {
        0.0
    } else {
        theme.axis_title.size
    };
    (guide::x_axis_bottom_margin(max_label_height, title_size) - MARGIN_BOTTOM).max(0.0)
}

/// Extra left margin needed so the widest y tick label and the rotated y-axis
/// title both fit (spec §17.3). Tick label *text* depends on the domain, not
/// the pixel range, so a provisional layout is enough to measure it. Returns 0
/// when no space has a continuous y axis or the labels fit the default margin.
fn y_label_left_extra(
    ir: &ChartIr,
    primary: &dyn Table,
    derived: &HashMap<String, DataFrame>,
    provisional: &Layout,
    theme: &Theme,
) -> f64 {
    let x_range = (provisional.plot.x, provisional.plot.right());
    let y_range = (provisional.plot.bottom(), provisional.plot.y);
    let mut max_label_width = 0.0_f64;
    for space in &ir.spaces {
        let table = active_table(&space.data, primary, derived);
        let space_scales = merged_scales(&ir.scales, &space.scales);
        let frame = match facet_frame(&space.frame) {
            Some((plane, _)) => plane,
            None => &space.frame,
        };
        let hints = train_space_domains(frame, table, &space.geometries, &space_scales);
        if let Some(scaled) = ScaledSpace::build(
            frame,
            table,
            x_range,
            y_range,
            &hints,
            &space_scales,
            space.view,
        ) {
            let guides = ir.guides.with_overrides(&space.guides);
            max_label_width = max_label_width.max(guide::max_y_tick_label_width(
                &scaled,
                theme.axis_text.size,
                guides.y_time_format.as_ref(),
                guides.y_format.as_deref(),
                guides.y_tick_label_angle,
                guides.y_tick_label_rows,
            ));
        }
    }
    // A hidden `axisText`/`axisTitle` token reclaims its share of the band
    // (spec §20.8).
    let title_hidden = theme.axis_title.hidden;
    if theme.axis_text.hidden {
        if title_hidden {
            return 0.0;
        }
        return (guide::y_axis_left_margin(0.0, theme.axis_title.size) - MARGIN_LEFT).max(0.0);
    }
    if max_label_width <= 0.0 {
        return 0.0;
    }
    let title_size = if title_hidden {
        0.0
    } else {
        theme.axis_title.size
    };
    (guide::y_axis_left_margin(max_label_width, title_size) - MARGIN_LEFT).max(0.0)
}

fn chart_text_reserve(ir: &ChartIr, theme: &Theme) -> (f64, f64) {
    // A hidden chrome token reserves nothing, so the plot reclaims the band
    // (spec §20.8).
    let mut top = 0.0;
    if ir.title.is_some() && !theme.plot_title.hidden {
        top += theme.plot_title.size + 8.0;
    }
    if ir.subtitle.is_some() && !theme.plot_subtitle.hidden {
        top += theme.plot_subtitle.size + 4.0;
    }
    // Stack one line per `\n`-separated caption line, then the source line(s)
    // (spec §17.3). Each line reserves its own height so nothing clips.
    let mut bottom = 0.0;
    if let Some(caption) = &ir.caption {
        if !theme.plot_caption.hidden {
            bottom += caption_line_count(caption) as f64 * (theme.plot_caption.size + 4.0) + 4.0;
        }
    }
    if let Some(source) = &ir.source {
        if !theme.plot_source.hidden {
            bottom += caption_line_count(source) as f64 * (theme.plot_source.size + 4.0) + 4.0;
        }
    }
    (top, bottom)
}

/// Number of stacked lines in a `\n`-separated caption/source string (spec
/// §17.3, §14.16). Always at least one line for a non-empty string.
pub(super) fn caption_line_count(text: &str) -> usize {
    text.split('\n').count().max(1)
}

fn facet_panel_count(
    ir: &ChartIr,
    primary: &dyn Table,
    derived: &HashMap<String, DataFrame>,
) -> Option<usize> {
    ir.spaces
        .iter()
        .filter_map(|space| {
            let (_, facet_col) = facet_frame(&space.frame)?;
            let table = active_table(&space.data, primary, derived);
            Some(facet_categories(table, &facet_col.name).len())
        })
        .max()
}

fn facet_frame(frame: &FrameIr) -> Option<(&FrameIr, &ColumnRef)> {
    let FrameIr::Nested { outer, inner } = frame else {
        return None;
    };
    if !matches!(outer.as_ref(), FrameIr::Cartesian(axes) if axes.len() == 2) {
        return None;
    }
    match inner.as_ref() {
        FrameIr::Vector(column) => Some((outer.as_ref(), column)),
        _ => None,
    }
}

/// The unioned numeric/temporal extent of one axis across all non-faceted
/// spaces, used to share position scales across overlaid spaces (spec §17.5).
#[derive(Default)]
struct AxisExtent {
    numeric: Option<(f64, f64)>,
    temporal: Option<(i64, i64)>,
    include_zero: bool,
}

impl AxisExtent {
    fn add_numeric(&mut self, min: f64, max: f64) {
        self.numeric = Some(match self.numeric {
            Some((lo, hi)) => (lo.min(min), hi.max(max)),
            None => (min, max),
        });
    }

    fn add_temporal(&mut self, min: i64, max: i64) {
        self.temporal = Some(match self.temporal {
            Some((lo, hi)) => (lo.min(min), hi.max(max)),
            None => (min, max),
        });
    }

    fn apply(&self, hints: &mut crate::domains::AxisDomainHints) {
        if let Some((min, max)) = self.numeric {
            hints.merge_numeric_extent(min, max);
        }
        if let Some((min, max)) = self.temporal {
            hints.merge_temporal_extent(min, max);
        }
        // A zero baseline changes domain padding, so it must be shared too or
        // overlaid spaces train domains that differ by the padding amount.
        if self.include_zero {
            hints.merge_include_zero();
        }
    }

    fn add_hints(&mut self, hints: &AxisDomainHints) {
        if let Some((min, max)) = hints.numeric_extent() {
            self.add_numeric(min, max);
        }
        if let Some((min, max)) = hints.temporal_extent() {
            self.add_temporal(min, max);
        }
        self.include_zero |= hints.includes_zero();
    }
}

fn shared_axis_extent(
    ir: &ChartIr,
    primary: &dyn Table,
    derived: &HashMap<String, DataFrame>,
    axis: AxisSelectorIr,
) -> AxisExtent {
    let mut extent = AxisExtent::default();
    for space in &ir.spaces {
        // Faceted spaces lay out in their own panels, so they do not share an
        // axis with the main overlaid spaces.
        if facet_frame(&space.frame).is_some()
            || matches!(space.coords, CoordsIr::Polar { .. })
            || is_spatial_space(space)
        {
            continue;
        }
        let table = active_table(&space.data, primary, derived);
        if let Some(axis_frame) = frame_axis(&space.frame, axis) {
            accumulate_axis_extent(axis_frame, table, &mut extent);
        }
        let space_scales = merged_scales(&ir.scales, &space.scales);
        let hints = train_space_domains(&space.frame, table, &space.geometries, &space_scales);
        match axis {
            AxisSelectorIr::X => extent.add_hints(&hints.x),
            AxisSelectorIr::Y => extent.add_hints(&hints.y),
        }
    }
    extent
}

/// Accumulate the numeric/temporal extent of an axis frame's backing columns.
fn accumulate_axis_extent(frame: &FrameIr, table: &dyn Table, extent: &mut AxisExtent) {
    match frame {
        FrameIr::Vector(col) => match col.dtype {
            algraf_data::DataType::Integer | algraf_data::DataType::Float => {
                if let Some((min, max)) = numeric_domain(table, &col.name) {
                    extent.add_numeric(min, max);
                }
            }
            algraf_data::DataType::Temporal => {
                if let Some((min, max, _)) = temporal_domain(table, &col.name) {
                    extent.add_temporal(min, max);
                }
            }
            _ => {}
        },
        FrameIr::Union(members) => {
            for member in members {
                accumulate_axis_extent(member, table, extent);
            }
        }
        _ => {}
    }
}

fn facet_categories(table: &dyn Table, column: &str) -> Vec<String> {
    let categories = categorical_domain(table, column);
    if categories.is_empty() {
        vec![String::new()]
    } else {
        categories
    }
}

fn facet_rows(table: &dyn Table, column: &str, category: &str) -> Vec<usize> {
    (0..table.row_count())
        .filter(|&row| cell_category(table, column, row).as_deref() == Some(category))
        .collect()
}
