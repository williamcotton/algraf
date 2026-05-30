use std::collections::HashMap;

use algraf_core::{codes, Diagnostic};
use algraf_data::{DataFrame, Table};
use algraf_semantics::{
    AxisSelectorIr, ChartIr, ColumnRef, CoordsIr, FrameIr, GeometryIr, GuideIr, ScaleIr,
};

use crate::domains::{train_space_domains, AxisDomainHints};
use crate::guide;
use crate::helpers::frame_axis;
use crate::layout::{Layout, Margins, Rect, MARGIN_BOTTOM, MARGIN_LEFT};
use crate::scale::{categorical_domain, cell_category, numeric_domain, temporal_domain};
use crate::space::ScaledSpace;
use crate::theme::Theme;

use super::common::{merged_scales, resolve_space_theme, validate_scale_configs};
use super::derived::active_table;
use super::legend::collect_legends;
use super::spatial::{build_spatial_plan, is_spatial_space};

pub(super) struct Panel<'t> {
    pub(super) table: &'t dyn Table,
    pub(super) scaled: ScaledSpace,
    pub(super) geometries: &'t [GeometryIr],
    pub(super) plot: Rect,
    pub(super) rows: Option<Vec<usize>>,
    pub(super) label: Option<String>,
    pub(super) facet_index: Option<usize>,
    pub(super) theme: Theme,
    pub(super) guides: GuideIr,
    pub(super) scales: Vec<ScaleIr>,
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

pub(super) fn build_render_plan<'t>(
    ir: &'t ChartIr,
    primary: &'t dyn Table,
    derived: &'t HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
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
    // A first pass with a provisional layout to discover legends.
    let provisional = Layout::compute_with_text(
        width,
        height,
        false,
        has_axes,
        top_extra,
        chart_bottom_extra,
        0.0,
        margins,
    );
    let guide_bottom_extra = if has_axes {
        x_label_bottom_extra(ir, primary, derived, &provisional, theme)
    } else {
        0.0
    };
    let bottom_extra = chart_bottom_extra + guide_bottom_extra;
    let legends = collect_legends(ir, primary, derived, theme);
    let left_extra = if has_axes {
        y_label_left_extra(ir, primary, derived, &provisional, theme)
    } else {
        0.0
    };
    let facet_panel_count = facet_panel_count(ir, primary, derived);
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
    let shared_x = shared_axis_extent(ir, primary, derived, AxisSelectorIr::X);
    let shared_y = shared_axis_extent(ir, primary, derived, AxisSelectorIr::Y);

    // Spatial (map) spaces share one projection and one projected bounding box
    // across overlaid layers, so a basemap and a point overlay align
    // (spec §16.15, §17.5). Resolve that plan once.
    let spatial_plan = build_spatial_plan(ir, primary, derived, diagnostics);

    let mut panels = Vec::new();
    for space in &ir.spaces {
        let table = active_table(&space.data, primary, derived);
        let panel_theme = resolve_space_theme(theme, space.theme.as_ref(), cli_theme_override);
        let space_guides = ir.guides.with_overrides(&space.guides);
        let space_scales = merged_scales(&ir.scales, &space.scales);
        validate_scale_configs(&space.frame, &space_scales, space.span, diagnostics);
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
                        codes::R0003,
                        "this faceted space could not be laid out",
                        space.span,
                    )),
                }
            }
        } else {
            let mut domain_hints = train_space_domains(&space.frame, table, &space.geometries);
            // Polar spaces are self-contained (one circular plot); Cartesian
            // axis-sharing across overlaid spaces does not apply (spec §16.16).
            let scaled = if let CoordsIr::Polar {
                theta,
                inner_radius,
                start_angle,
                direction,
            } = space.coords
            {
                ScaledSpace::build_polar(
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
                )
            } else {
                shared_x.apply(&mut domain_hints.x);
                shared_y.apply(&mut domain_hints.y);
                ScaledSpace::build(
                    &space.frame,
                    table,
                    x_range,
                    y_range,
                    &domain_hints,
                    &space_scales,
                )
            };
            match scaled {
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
                    codes::R0003,
                    "this space could not be laid out",
                    space.span,
                )),
            }
        }
    }

    RenderPlan {
        layout,
        legends,
        panels,
    }
}

pub(super) fn panel_slots<'a>(layout: &'a Layout, panels: &'a [Panel<'a>]) -> Vec<PanelSlot<'a>> {
    if layout.facets.is_empty() {
        return vec![PanelSlot {
            plot: layout.plot,
            strip: None,
            label: None,
            facet_index: None,
            panel: panels.first(),
        }];
    }

    layout
        .facets
        .iter()
        .enumerate()
        .map(|(index, facet)| {
            let panel = panels.iter().find(|panel| panel.facet_index == Some(index));
            PanelSlot {
                plot: facet.plot,
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
        let hints = train_space_domains(frame, table, &space.geometries);
        if let Some(scaled) =
            ScaledSpace::build(frame, table, x_range, y_range, &hints, &space_scales)
        {
            let guides = ir.guides.with_overrides(&space.guides);
            max_label_height = max_label_height.max(guide::max_x_tick_label_height(
                &scaled,
                theme.font_size,
                guides.x_time_format.as_ref(),
                guides.x_tick_label_angle,
            ));
        }
    }
    if max_label_height <= 0.0 {
        return 0.0;
    }
    (guide::x_axis_bottom_margin(max_label_height, theme.font_size) - MARGIN_BOTTOM).max(0.0)
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
            let guides = ir.guides.with_overrides(&space.guides);
            max_label_width = max_label_width.max(guide::max_y_tick_label_width(
                &scaled,
                theme.font_size,
                guides.y_time_format.as_ref(),
                guides.y_tick_label_angle,
            ));
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

    fn add_hints(&mut self, hints: &AxisDomainHints) {
        if let Some((min, max)) = hints.numeric_extent() {
            self.add_numeric(min, max);
        }
        if let Some((min, max)) = hints.temporal_extent() {
            self.add_temporal(min, max);
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
        let hints = train_space_domains(&space.frame, table, &space.geometries);
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
