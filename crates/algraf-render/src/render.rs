//! Render orchestration: IR + data to a deterministic SVG string
//! (spec §24.4, §24.5, §18).

use std::collections::HashMap;

use algraf_core::Diagnostic;
use algraf_data::{DataFrame, DataType, DataValueRef, Table};
use algraf_semantics::{
    ir::Setting, AxisSelectorIr, ChartIr, ColumnRef, FrameIr, GeometryIr, GeometryKind, GuideIr,
    ScaleIr, ScaleTargetIr, ScaleTypeIr, SettingValue, SpaceDataRef, SpaceIr, StatKind, ThemeIr,
};

use crate::aes::{color_spec, ColorSpec, Legend, LegendKind};
use crate::domains::train_space_domains;
use crate::error::RenderError;
use crate::guide;
use crate::layout::{Layout, Margins, Rect, MARGIN_LEFT};
use crate::projection::{for_each_coord, Projection, SpatialScale};
use crate::scale::{categorical_domain, cell_category, cell_f64, numeric_domain, temporal_domain};
use crate::space::ScaledSpace;
use crate::stats;
use crate::svg::{escape_attr, escape_text, num, SvgWriter};
use crate::theme::Theme;

/// The result of rendering: an SVG document plus render diagnostics.
#[derive(Debug, Clone)]
pub struct RenderResult {
    pub svg: String,
    pub diagnostics: Vec<Diagnostic>,
    pub layout: Layout,
}

struct Panel<'t> {
    table: &'t dyn Table,
    scaled: ScaledSpace,
    geometries: &'t [GeometryIr],
    plot: Rect,
    rows: Option<Vec<usize>>,
    label: Option<String>,
    facet_index: Option<usize>,
    theme: Theme,
    guides: GuideIr,
    scales: Vec<ScaleIr>,
}

/// Render a chart IR against its primary data table (spec §24.4).
///
/// `theme` is the base (chart-level) theme already resolved by the caller.
/// `cli_theme_override`, if `Some`, replaces space-local theme overrides too
/// (spec §22.3): CLI `--theme` is the strongest source.
pub fn render(
    ir: &ChartIr,
    primary: &dyn Table,
    theme: &Theme,
    cli_theme_override: Option<&str>,
) -> Result<RenderResult, RenderError> {
    render_with_tables(ir, primary, &HashMap::new(), theme, cli_theme_override)
}

/// Render a chart IR against its primary table plus chart-scoped named tables
/// (spec §10.x). `named_tables` maps each `Table name = "..."` declaration's
/// name to its loaded frame; the caller loads them at the I/O boundary.
pub fn render_with_tables(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
) -> Result<RenderResult, RenderError> {
    let mut diagnostics = Vec::new();

    // Compute derived tables (spec §24.1 step 12), seeding the table map with
    // the named CSV tables so derived stats may read from them too.
    let derived = compute_derived(ir, primary, named_tables);

    let width = ir.width as f64;
    let height = ir.height as f64;

    let has_axes = theme.axes;
    let margins = Margins {
        top: ir.margin_top.map(f64::from),
        right: ir.margin_right.map(f64::from),
        bottom: ir.margin_bottom.map(f64::from),
        left: ir.margin_left.map(f64::from),
    };
    let (top_extra, bottom_extra) = chart_text_reserve(ir, theme);
    // A first pass with a provisional layout to discover legends.
    let provisional = Layout::compute_with_text(
        width,
        height,
        false,
        has_axes,
        top_extra,
        bottom_extra,
        0.0,
        margins,
    );
    let legends = collect_legends(ir, primary, &derived, &provisional);
    let left_extra = if has_axes {
        y_label_left_extra(ir, primary, &derived, &provisional, theme)
    } else {
        0.0
    };
    let facet_panel_count = facet_panel_count(ir, primary, &derived);
    let layout = match facet_panel_count {
        Some(count) => Layout::compute_facets_with_text(
            width,
            height,
            !legends.is_empty(),
            has_axes,
            count,
            ir.layout.facet_columns,
            top_extra,
            bottom_extra,
            left_extra,
            margins,
        ),
        None => Layout::compute_with_text(
            width,
            height,
            !legends.is_empty(),
            has_axes,
            top_extra,
            bottom_extra,
            left_extra,
            margins,
        ),
    };

    let x_range = (layout.plot.x, layout.plot.right());
    let y_range = (layout.plot.bottom(), layout.plot.y); // inverted for SVG

    // Position scales are shared across overlaid (non-faceted) spaces, even when
    // they back onto different tables (spec §17.5). Compute the unioned x/y
    // extent across those spaces and inject it as a soft bound below.
    let shared_x = shared_axis_extent(ir, primary, &derived, AxisSelectorIr::X);
    let shared_y = shared_axis_extent(ir, primary, &derived, AxisSelectorIr::Y);

    // Spatial (map) spaces share one projection and one projected bounding box
    // across overlaid layers, so a basemap and a point overlay align
    // (spec §16.15, §17.5). Resolve that plan once.
    let spatial_plan = build_spatial_plan(ir, primary, &derived, &mut diagnostics);

    let mut panels = Vec::new();
    for space in &ir.spaces {
        let table = active_table(&space.data, primary, &derived);
        let panel_theme = resolve_space_theme(theme, space.theme.as_ref(), cli_theme_override);
        let space_guides = ir.guides.with_overrides(&space.guides);
        let space_scales = merged_scales(&ir.scales, &space.scales);
        validate_scale_configs(&space.frame, &space_scales, space.span, &mut diagnostics);
        if is_spatial_space(space) {
            // A spatial space projects geographic coordinates into the plot;
            // it has no planar axes or facets.
            if let Some(plan) = &spatial_plan {
                if let Some(scaled) = plan.scaled_space(space, layout.plot) {
                    panels.push(Panel {
                        table,
                        scaled,
                        geometries: &space.geometries,
                        plot: layout.plot,
                        rows: None,
                        label: None,
                        facet_index: None,
                        theme: panel_theme,
                        guides: space_guides,
                        scales: space_scales,
                    });
                }
            }
            continue;
        }
        if let Some((plane, facet_col)) = facet_frame(&space.frame) {
            let domain_hints = train_space_domains(plane, table, &space.geometries);
            for (index, category) in facet_categories(table, &facet_col.name).iter().enumerate() {
                let Some(facet) = layout.facets.get(index) else {
                    continue;
                };
                let x_range = (facet.plot.x, facet.plot.right());
                let y_range = (facet.plot.bottom(), facet.plot.y);
                match ScaledSpace::build(
                    plane,
                    table,
                    x_range,
                    y_range,
                    &domain_hints,
                    &space_scales,
                ) {
                    Some(scaled) => panels.push(Panel {
                        table,
                        scaled,
                        geometries: &space.geometries,
                        plot: facet.plot,
                        rows: Some(facet_rows(table, &facet_col.name, category)),
                        label: Some(category.clone()),
                        facet_index: Some(index),
                        theme: panel_theme.clone(),
                        guides: space_guides.clone(),
                        scales: space_scales.clone(),
                    }),
                    None => diagnostics.push(Diagnostic::warning(
                        "R0003",
                        "this faceted space could not be laid out",
                        space.span,
                    )),
                }
            }
        } else {
            let mut domain_hints = train_space_domains(&space.frame, table, &space.geometries);
            shared_x.apply(&mut domain_hints.x);
            shared_y.apply(&mut domain_hints.y);
            match ScaledSpace::build(
                &space.frame,
                table,
                x_range,
                y_range,
                &domain_hints,
                &space_scales,
            ) {
                Some(scaled) => panels.push(Panel {
                    table,
                    scaled,
                    geometries: &space.geometries,
                    plot: layout.plot,
                    rows: None,
                    label: None,
                    facet_index: None,
                    theme: panel_theme,
                    guides: space_guides,
                    scales: space_scales,
                }),
                None => diagnostics.push(Diagnostic::warning(
                    "R0003",
                    "this space could not be laid out",
                    space.span,
                )),
            }
        }
    }

    // --- SVG emission (spec §24.5) ---
    let mut w = SvgWriter::new();
    let aria = ir
        .title
        .as_ref()
        .map(|title| format!(" aria-label=\"{}\"", escape_attr(title)))
        .unwrap_or_default();
    w.line(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" \
         viewBox=\"0 0 {} {}\" role=\"img\"{}>",
        num(width),
        num(height),
        num(width),
        num(height),
        aria,
    ));
    if let Some(title) = &ir.title {
        w.line(&format!("<title>{}</title>", escape_text(title)));
    }
    if let Some(desc) = chart_desc(ir) {
        w.line(&format!("<desc>{}</desc>", escape_text(&desc)));
    }

    // Background.
    w.line(&rect_fill(
        0.0,
        0.0,
        width,
        height,
        &theme.background,
        "algraf-background",
    ));
    render_chart_text(&mut w, ir, width, height, &layout, theme);
    // Plot panel background.
    if layout.facets.is_empty() {
        w.line(&rect_fill(
            layout.plot.x,
            layout.plot.y,
            layout.plot.width,
            layout.plot.height,
            &theme.plot_background,
            "algraf-plot-area",
        ));
    } else {
        for facet in &layout.facets {
            w.line(&rect_fill(
                facet.plot.x,
                facet.plot.y,
                facet.plot.width,
                facet.plot.height,
                &theme.plot_background,
                "algraf-plot-area algraf-facet-panel",
            ));
        }
    }

    if layout.facets.is_empty() {
        // Grid (from the first laid-out panel). Spatial spaces draw no grid
        // (spec §16.15).
        if let Some(first) = panels.first() {
            if first.guides.grid && !first.scaled.is_spatial() {
                guide::render_grid(&mut w, &first.scaled, first.plot, &first.theme);
            }
        }
    } else {
        for_each_unique_facet_panel(&panels, |panel| {
            guide::render_facet_label(
                &mut w,
                panel.label.as_deref().unwrap_or_default(),
                layout.facets[panel.facet_index.unwrap()].strip,
                &panel.theme,
            );
        });
        for_each_unique_facet_panel(&panels, |panel| {
            if panel.guides.grid {
                guide::render_grid(&mut w, &panel.scaled, panel.plot, &panel.theme);
            }
        });
    }

    // Data layers in source order (spec §18.3).
    for panel in &panels {
        for geo in panel.geometries {
            crate::geom::render(
                &mut w,
                geo,
                crate::geom::GeometryRenderContext {
                    space: &panel.scaled,
                    table: panel.table,
                    rows: panel.rows.as_deref(),
                    plot: panel.plot,
                    theme: &panel.theme,
                    scales: &panel.scales,
                },
                &mut diagnostics,
            );
        }
    }

    // Axes (from the first panel) and legends.
    if layout.facets.is_empty() {
        if let Some(first) = panels.first() {
            // Spatial spaces have no lat/lon axes (spec §16.15).
            if first.theme.axes && !first.scaled.is_spatial() {
                guide::render_axes(
                    &mut w,
                    &first.scaled,
                    first.plot,
                    &first.theme,
                    first.guides.x_label.as_deref(),
                    first.guides.y_label.as_deref(),
                );
            }
        }
    } else {
        for_each_unique_facet_panel(&panels, |panel| {
            if panel.theme.axes {
                guide::render_axes(
                    &mut w,
                    &panel.scaled,
                    panel.plot,
                    &panel.theme,
                    panel.guides.x_label.as_deref(),
                    panel.guides.y_label.as_deref(),
                );
            }
        });
    }
    if let Some(area) = layout.legend {
        guide::render_legends(&mut w, &legends, area, theme);
    }

    w.line("</svg>");

    Ok(RenderResult {
        svg: w.finish(),
        diagnostics,
        layout,
    })
}

/// Resolve a per-space theme, applying space-local overrides on top of the base.
/// CLI `--theme` (passed as `cli_override`) is the strongest source and is
/// applied last (spec §22.3).
fn resolve_space_theme(
    base: &Theme,
    space_theme: Option<&ThemeIr>,
    cli_override: Option<&str>,
) -> Theme {
    let mut theme = base.clone();
    if let Some(ir) = space_theme {
        // A space theme starts from its own named base if it gives one, or else
        // inherits the chart base, then layers its overrides (spec §7.3, §20.8).
        let mut t = match &ir.base {
            Some(name) => Theme::by_name(name),
            None => base.clone(),
        };
        t.apply_overrides(&ir.overrides);
        theme = t;
    }
    if let Some(name) = cli_override {
        theme = Theme::by_name(name);
    }
    theme
}

fn merged_scales(chart_scales: &[ScaleIr], space_scales: &[ScaleIr]) -> Vec<ScaleIr> {
    chart_scales
        .iter()
        .chain(space_scales.iter())
        .cloned()
        .collect()
}

fn validate_scale_configs(
    frame: &FrameIr,
    scales: &[ScaleIr],
    span: algraf_core::Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for scale in scales {
        let ScaleTargetIr::Axis(axis) = &scale.target else {
            continue;
        };
        let Some(axis_frame) = frame_axis(frame, *axis) else {
            continue;
        };
        if scale.scale_type == Some(ScaleTypeIr::Log10) {
            let Some(column) = vector_column(axis_frame) else {
                diagnostics.push(Diagnostic::warning(
                    "R0004",
                    "log10 scale requires a continuous numeric axis",
                    scale.span,
                ));
                continue;
            };
            if !matches!(
                column.dtype,
                algraf_data::DataType::Integer | algraf_data::DataType::Float
            ) {
                diagnostics.push(Diagnostic::warning(
                    "R0004",
                    "log10 scale requires a continuous numeric axis",
                    column.span,
                ));
            }
        }
        if let Some([a, b]) = scale.domain {
            if let (Some(a), Some(b)) = (a, b) {
                if (a - b).abs() <= f64::EPSILON {
                    diagnostics.push(Diagnostic::warning(
                        "R0004",
                        "scale domain endpoints must be distinct",
                        scale.span,
                    ));
                }
            }
            if scale.scale_type == Some(ScaleTypeIr::Log10)
                && [a, b].into_iter().flatten().any(|bound| bound <= 0.0)
            {
                diagnostics.push(Diagnostic::warning(
                    "R0004",
                    "log10 scale domain must be positive",
                    scale.span,
                ));
            }
        }
    }

    if frame_axis(frame, AxisSelectorIr::X).is_none()
        && frame_axis(frame, AxisSelectorIr::Y).is_none()
    {
        diagnostics.push(Diagnostic::warning(
            "R0004",
            "scale declarations could not be matched to this space",
            span,
        ));
    }
}

/// Whether a column reference holds geometry (spec §10.11).
fn frame_is_geometry(frame: &FrameIr) -> bool {
    matches!(frame, FrameIr::Vector(col) if col.dtype == DataType::Geometry)
}

/// Whether a space renders as a spatial (projected map) space: it either frames
/// a geometry column or declares a `projection:` (spec §16.14, §16.15).
fn is_spatial_space(space: &SpaceIr) -> bool {
    space.projection.is_some() || frame_is_geometry(&space.frame)
}

/// The `(longitude, latitude)` column names of a projected `long * lat` space,
/// for point/line overlays sharing a basemap's spatial scale.
fn lonlat_columns(frame: &FrameIr) -> Option<(String, String)> {
    if let FrameIr::Cartesian(axes) = frame {
        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) = (axes.first(), axes.get(1)) {
            return Some((x.name.clone(), y.name.clone()));
        }
    }
    None
}

/// An accumulating projected bounding box.
#[derive(Default)]
struct Bbox {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    seen: bool,
}

impl Bbox {
    fn add(&mut self, x: f64, y: f64) {
        if self.seen {
            self.min_x = self.min_x.min(x);
            self.min_y = self.min_y.min(y);
            self.max_x = self.max_x.max(x);
            self.max_y = self.max_y.max(y);
        } else {
            (self.min_x, self.min_y, self.max_x, self.max_y) = (x, y, x, y);
            self.seen = true;
        }
    }

    fn finish(self) -> Option<(f64, f64, f64, f64)> {
        self.seen
            .then_some((self.min_x, self.min_y, self.max_x, self.max_y))
    }
}

/// The shared projection and projected bounding box across all spatial spaces
/// (spec §16.15, §17.5).
struct SpatialPlan {
    proj_name: Option<String>,
    bbox: (f64, f64, f64, f64),
}

impl SpatialPlan {
    /// Build a spatial scale for one space against the shared fit, tagging the
    /// longitude/latitude columns for a projected `long * lat` overlay.
    fn scaled_space(&self, space: &SpaceIr, plot: Rect) -> Option<ScaledSpace> {
        let projection = Projection::resolve(self.proj_name.as_deref()).ok()?;
        let mut spatial = SpatialScale::fit(projection, self.bbox, plot);
        if let FrameIr::Vector(col) = &space.frame {
            if col.dtype == DataType::Geometry {
                spatial.geom_col = Some(col.name.clone());
            }
        }
        if let Some((lon, lat)) = lonlat_columns(&space.frame) {
            spatial.lon_col = Some(lon);
            spatial.lat_col = Some(lat);
        }
        Some(ScaledSpace::spatial(spatial))
    }
}

/// Resolve the shared spatial plan: one projection for all overlaid spatial
/// spaces (conflict is `E1803`, an invalid projection is `E1802`) and the union
/// of their projected bounding boxes so a basemap and overlay align.
fn build_spatial_plan(
    ir: &ChartIr,
    primary: &dyn Table,
    derived: &HashMap<String, DataFrame>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<SpatialPlan> {
    let spatial: Vec<&SpaceIr> = ir.spaces.iter().filter(|s| is_spatial_space(s)).collect();
    let first = *spatial.first()?;

    // All spatial spaces must agree on projection; `None` means the default
    // equirectangular projection.
    let effective = |s: &SpaceIr| {
        s.projection
            .clone()
            .unwrap_or_else(|| "equirectangular".into())
    };
    let agreed = effective(first);
    if spatial.iter().any(|s| effective(s) != agreed) {
        diagnostics.push(Diagnostic::error(
            "E1803",
            "overlaid spaces declare conflicting projections; \
             all spatial spaces must use the same projection",
            first.span,
        ));
    }

    let proj_name = first.projection.clone();
    let projection = match Projection::resolve(proj_name.as_deref()) {
        Ok(projection) => projection,
        Err(message) => {
            diagnostics.push(Diagnostic::error(
                "E1802",
                format!("invalid or unknown projection: {message}"),
                first.span,
            ));
            return None;
        }
    };

    let mut bbox = Bbox::default();
    for space in &spatial {
        let table = active_table(&space.data, primary, derived);
        accumulate_space_bbox(space, table, &projection, &mut bbox);
    }
    Some(SpatialPlan {
        proj_name,
        bbox: bbox.finish()?,
    })
}

/// Project every coordinate a space contributes (geometry vertices, or
/// `long * lat` points) into the accumulating projected bounding box.
fn accumulate_space_bbox(
    space: &SpaceIr,
    table: &dyn Table,
    projection: &Projection,
    bbox: &mut Bbox,
) {
    if let FrameIr::Vector(col) = &space.frame {
        if col.dtype == DataType::Geometry {
            for row in 0..table.row_count() {
                if let Some(DataValueRef::Geometry(geometry)) = table.value(&col.name, row) {
                    for_each_coord(geometry, &mut |lon, lat| {
                        if let Some((x, y)) = projection.project(lon, lat) {
                            bbox.add(x, y);
                        }
                    });
                }
            }
            return;
        }
    }
    if let Some((lon_col, lat_col)) = lonlat_columns(&space.frame) {
        for row in 0..table.row_count() {
            if let (Some(lon), Some(lat)) = (
                cell_f64(table, &lon_col, row),
                cell_f64(table, &lat_col, row),
            ) {
                if let Some((x, y)) = projection.project(lon, lat) {
                    bbox.add(x, y);
                }
            }
        }
    }
}

fn active_table<'t>(
    data: &SpaceDataRef,
    primary: &'t dyn Table,
    derived: &'t HashMap<String, DataFrame>,
) -> &'t dyn Table {
    match data {
        SpaceDataRef::Primary => primary,
        // Named tables are seeded into the same map as derived tables, so both
        // resolve the same way (spec §10.x).
        SpaceDataRef::Derived(name) | SpaceDataRef::Table(name) => derived
            .get(name)
            .map(|d| d as &dyn Table)
            .unwrap_or(primary),
    }
}

fn compute_derived(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
) -> HashMap<String, DataFrame> {
    // Seed with the chart's named CSV tables; derived stats may read from them
    // and `SpaceDataRef::Table` resolves through this same map.
    let mut derived: HashMap<String, DataFrame> = named_tables.clone();
    for d in &ir.derived_tables {
        let frame = {
            let source = active_table(&d.data, primary, &derived);
            match d.stat.kind {
                StatKind::Bin => {
                    if let FrameIr::Vector(col) = &d.stat.input {
                        let bins = numeric_setting(&d.stat.settings, "bins")
                            .filter(|n| *n >= 1.0)
                            .map(|n| n.round() as usize)
                            .unwrap_or(30);
                        let options = stats::BinOptions {
                            bins,
                            bin_width: numeric_setting(&d.stat.settings, "binWidth")
                                .filter(|n| *n > 0.0),
                            boundary: numeric_setting(&d.stat.settings, "boundary"),
                            closed: closed_setting(&d.stat.settings),
                        };
                        Some(stats::bin_with_options(source, &col.name, options))
                    } else {
                        None
                    }
                }
                StatKind::Bin2D => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) =
                            (cols.first(), cols.get(1))
                        {
                            let bins = numeric_setting(&d.stat.settings, "bins")
                                .filter(|n| *n >= 1.0)
                                .map(|n| n.round() as usize)
                                .unwrap_or(30);
                            Some(stats::bin2d(
                                source,
                                &x.name,
                                &y.name,
                                stats::Bin2DOptions { bins },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatKind::HexBin => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) =
                            (cols.first(), cols.get(1))
                        {
                            let bins = numeric_setting(&d.stat.settings, "bins")
                                .filter(|n| *n >= 1.0)
                                .map(|n| n.round() as usize)
                                .unwrap_or(30);
                            Some(stats::hexbin_frame(
                                source,
                                &x.name,
                                &y.name,
                                stats::Bin2DOptions { bins },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                StatKind::Count => {
                    let mut group_cols: Vec<&str> = Vec::new();
                    match &d.stat.input {
                        FrameIr::Vector(col) => group_cols.push(&col.name),
                        FrameIr::Nested { outer, inner } => {
                            if let (FrameIr::Vector(o), FrameIr::Vector(i)) =
                                (outer.as_ref(), inner.as_ref())
                            {
                                group_cols.push(&o.name);
                                group_cols.push(&i.name);
                            }
                        }
                        _ => {}
                    }
                    if group_cols.is_empty() {
                        None
                    } else {
                        Some(stats::count_by(source, &group_cols))
                    }
                }
                StatKind::Density => {
                    if let FrameIr::Vector(col) = &d.stat.input {
                        let options = stats::DensityOptions {
                            bandwidth: numeric_setting(&d.stat.settings, "bandwidth")
                                .filter(|n| *n > 0.0),
                            grid_points: numeric_setting(&d.stat.settings, "n")
                                .filter(|n| *n >= 2.0)
                                .map(|n| n.round() as usize)
                                .unwrap_or(256),
                        };
                        Some(stats::density(source, &col.name, options))
                    } else {
                        None
                    }
                }
                StatKind::Smooth => {
                    if let FrameIr::Cartesian(cols) = &d.stat.input {
                        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) =
                            (cols.first(), cols.get(1))
                        {
                            Some(stats::smooth_lm(source, &x.name, &y.name))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            }
        };
        if let Some(frame) = frame {
            derived.insert(d.name.clone(), frame);
        }
    }
    derived
}

fn numeric_setting(settings: &[Setting], name: &str) -> Option<f64> {
    settings
        .iter()
        .find(|setting| setting.name == name)
        .and_then(|setting| match setting.value {
            SettingValue::Number(value) if value.is_finite() => Some(value),
            _ => None,
        })
}

fn closed_setting(settings: &[Setting]) -> stats::BinClosed {
    settings
        .iter()
        .find(|setting| setting.name == "closed")
        .and_then(|setting| match &setting.value {
            SettingValue::String(value) if value == "right" => Some(stats::BinClosed::Right),
            SettingValue::String(value) if value == "left" => Some(stats::BinClosed::Left),
            _ => None,
        })
        .unwrap_or(stats::BinClosed::Left)
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
        let hints = train_space_domains(frame, table, &space.geometries);
        if let Some(scaled) =
            ScaledSpace::build(frame, table, x_range, y_range, &hints, &space_scales)
        {
            max_label_width =
                max_label_width.max(guide::max_y_tick_label_width(&scaled, theme.font_size));
        }
    }
    if max_label_width <= 0.0 {
        return 0.0;
    }
    (guide::y_axis_left_margin(max_label_width, theme.font_size) - MARGIN_LEFT).max(0.0)
}

fn chart_text_reserve(ir: &ChartIr, theme: &Theme) -> (f64, f64) {
    let mut top = 0.0;
    if ir.title.is_some() {
        top += theme.title_size + 8.0;
    }
    if ir.subtitle.is_some() {
        top += theme.font_size + 4.0;
    }
    let bottom = if ir.caption.is_some() {
        theme.font_size + 8.0
    } else {
        0.0
    };
    (top, bottom)
}

fn chart_desc(ir: &ChartIr) -> Option<String> {
    match (&ir.subtitle, &ir.caption) {
        (Some(subtitle), Some(caption)) => Some(format!("{subtitle}\n{caption}")),
        (Some(subtitle), None) => Some(subtitle.clone()),
        (None, Some(caption)) => Some(caption.clone()),
        (None, None) => None,
    }
}

fn render_chart_text(
    w: &mut SvgWriter,
    ir: &ChartIr,
    width: f64,
    height: f64,
    layout: &Layout,
    theme: &Theme,
) {
    let x = layout.plot.x;
    let mut y = 24.0;
    if let Some(title) = &ir.title {
        w.line(&format!(
            "<text class=\"algraf-title\" x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"600\" fill=\"{}\">{}</text>",
            num(x),
            num(y),
            escape_attr(&theme.font_family),
            num(theme.title_size),
            escape_attr(&theme.text_color),
            escape_text(title),
        ));
        y += theme.title_size + 8.0;
    }
    if let Some(subtitle) = &ir.subtitle {
        w.line(&format!(
            "<text class=\"algraf-subtitle\" x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
            num(x),
            num(y),
            escape_attr(&theme.font_family),
            num(theme.font_size),
            escape_attr(&theme.text_color),
            escape_text(subtitle),
        ));
    }
    if let Some(caption) = &ir.caption {
        w.line(&format!(
            "<text class=\"algraf-caption\" x=\"{}\" y=\"{}\" text-anchor=\"end\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
            num(width - 16.0),
            num(height - 12.0),
            escape_attr(&theme.font_family),
            num(theme.font_size),
            escape_attr(&theme.text_color),
            escape_text(caption),
        ));
    }
}

/// Collect deduplicated fill/stroke legends across all spaces (spec §19.5).
///
/// When `include_fill` is false, fill legends are suppressed
/// (e.g. `Guide(fill: null)` from spec §19.6).
fn collect_legends(
    ir: &ChartIr,
    primary: &dyn Table,
    derived: &HashMap<String, DataFrame>,
    _layout: &Layout,
) -> Vec<Legend> {
    // Candidate legends paired with the aesthetic that produced them, so a
    // fill legend and a stroke legend over the same column can be merged below.
    let mut candidates: Vec<(&'static str, Legend)> = Vec::new();
    for space in &ir.spaces {
        let guides = ir.guides.with_overrides(&space.guides);
        if !guides.legend {
            continue;
        }
        let scales = merged_scales(&ir.scales, &space.scales);
        let table = active_table(&space.data, primary, derived);
        for geo in &space.geometries {
            for aesthetic in ["fill", "stroke"] {
                if aesthetic == "fill" && !guides.fill_legend {
                    continue;
                }
                if aesthetic == "stroke" && !guides.stroke_legend {
                    continue;
                }
                if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == aesthetic) {
                    let spec = color_spec(geo, aesthetic, table, &scales);
                    // A `Scale(<aesthetic>: col, label: "...")` overrides the
                    // column-derived legend title (spec §16.13).
                    let title = scale_label(&scales, aesthetic)
                        .unwrap_or_else(|| crate::svg::display_label(&mapping.column.name));
                    if let Some(legend) = spec.legend(&title) {
                        if !candidates
                            .iter()
                            .any(|(a, l)| *a == aesthetic && l.title == legend.title)
                        {
                            candidates.push((aesthetic, legend));
                        }
                    }
                }
            }
            // `HexBin` is rendered by a bespoke geometry rather than desugared
            // to a fill-mapped `Rect` (as `Bin2D` is), so it has no `fill`
            // mapping for the loop above to find. Synthesize the same continuous
            // count legend here, over the binned count domain (spec §19.5).
            if geo.kind == GeometryKind::HexBin && guides.fill_legend {
                if let Some(legend) = hexbin_count_legend(geo, &space.frame, table, &scales) {
                    if !candidates
                        .iter()
                        .any(|(a, l)| *a == "fill" && l.title == legend.title)
                    {
                        candidates.push(("fill", legend));
                    }
                }
            }
        }
    }
    merge_fill_stroke_legends(candidates)
}

/// Build the continuous `count` legend for a bespoke `HexBin` geometry, unless
/// `fill` is set to a constant color. The count domain is derived by running
/// the same binning the renderer uses, so the legend's swatch colors match the
/// rendered hexagons.
fn hexbin_count_legend(
    geo: &GeometryIr,
    frame: &FrameIr,
    table: &dyn Table,
    scales: &[ScaleIr],
) -> Option<Legend> {
    if geo
        .settings
        .iter()
        .any(|s| s.name == "fill" && matches!(s.value, SettingValue::String(_)))
    {
        return None;
    }
    let FrameIr::Cartesian(axes) = frame else {
        return None;
    };
    let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) = (axes.first(), axes.get(1)) else {
        return None;
    };
    let bins = geo
        .settings
        .iter()
        .find(|s| s.name == "bins")
        .and_then(|s| match s.value {
            SettingValue::Number(n) if n >= 1.0 => Some(n.round() as usize),
            _ => None,
        })
        .unwrap_or(30);
    let cells = stats::hexbin(table, &x.name, &y.name, stats::Bin2DOptions { bins });
    let min = cells.iter().map(|c| c.count).min()? as f64;
    let max = cells.iter().map(|c| c.count).max()? as f64;
    let stops = crate::theme::CONTINUOUS_GRADIENT
        .iter()
        .map(|stop| (*stop).to_string())
        .collect();
    let spec = ColorSpec::Gradient {
        col: "count".to_string(),
        min,
        max,
        stops,
    };
    let title = scale_label(scales, "fill").unwrap_or_else(|| "count".to_string());
    spec.legend(&title)
}

/// The explicit `label` of a `fill`/`stroke` aesthetic scale, if declared.
fn scale_label(scales: &[ScaleIr], aesthetic: &str) -> Option<String> {
    scales.iter().find_map(|scale| match &scale.target {
        ScaleTargetIr::Aesthetic { aesthetic: a, .. } if a == aesthetic => scale.label.clone(),
        _ => None,
    })
}

/// Merge a `fill` legend and a `stroke` legend that share a title and have
/// compatible (identical, discrete) domains into a single legend whose swatches
/// show both colors (spec §19.7). Non-mergeable candidates pass through
/// unchanged, deduplicated by title with the first occurrence winning.
fn merge_fill_stroke_legends(candidates: Vec<(&'static str, Legend)>) -> Vec<Legend> {
    let mut out: Vec<Legend> = Vec::new();
    for (aesthetic, legend) in candidates {
        let Some(existing) = out.iter_mut().find(|l| l.title == legend.title) else {
            out.push(legend);
            continue;
        };

        // A fill/stroke pair over the same column with identical discrete entry
        // labels merges into one swatch set showing both colors. Only the first
        // unmerged base accepts a partner.
        let labels_match = existing.kind == LegendKind::Discrete
            && legend.kind == LegendKind::Discrete
            && existing.entries.len() == legend.entries.len()
            && existing
                .entries
                .iter()
                .zip(&legend.entries)
                .all(|(a, b)| a.0 == b.0);
        let new_colors: Vec<String> = legend.entries.iter().map(|(_, c)| c.clone()).collect();
        if labels_match && existing.stroke_entries.is_empty() {
            if aesthetic == "stroke" {
                // Existing is the fill base; this stroke legend adds outlines.
                existing.stroke_entries = new_colors;
            } else {
                // Existing was a stroke-only base seen first; promote the fill
                // colors to the swatch face and demote the strokes to outlines.
                existing.stroke_entries = existing.entries.iter().map(|(_, c)| c.clone()).collect();
                for (entry, color) in existing.entries.iter_mut().zip(new_colors) {
                    entry.1 = color;
                }
            }
        }
        // Otherwise the title already has a legend: keep the first.
    }
    out
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
        if facet_frame(&space.frame).is_some() {
            continue;
        }
        let Some(axis_frame) = frame_axis(&space.frame, axis) else {
            continue;
        };
        let table = active_table(&space.data, primary, derived);
        accumulate_axis_extent(axis_frame, table, &mut extent);
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

fn frame_axis(frame: &FrameIr, axis: AxisSelectorIr) -> Option<&FrameIr> {
    match (frame, axis) {
        (FrameIr::Cartesian(axes), AxisSelectorIr::X) => axes.first(),
        (FrameIr::Cartesian(axes), AxisSelectorIr::Y) => axes.get(1),
        (_, AxisSelectorIr::X) => Some(frame),
        (_, AxisSelectorIr::Y) => None,
    }
}

fn vector_column(frame: &FrameIr) -> Option<&ColumnRef> {
    match frame {
        FrameIr::Vector(column) => Some(column),
        _ => None,
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

fn for_each_unique_facet_panel<'t>(panels: &'t [Panel<'t>], mut f: impl FnMut(&'t Panel<'t>)) {
    let mut seen = Vec::new();
    for panel in panels {
        let Some(index) = panel.facet_index else {
            continue;
        };
        if seen.contains(&index) {
            continue;
        }
        seen.push(index);
        f(panel);
    }
}

fn rect_fill(x: f64, y: f64, w: f64, h: f64, color: &str, class: &str) -> String {
    format!(
        "<rect class=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" />",
        class,
        num(x),
        num(y),
        num(w),
        num(h),
        escape_attr(color),
    )
}
