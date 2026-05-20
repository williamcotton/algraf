//! Render orchestration: IR + data to a deterministic SVG string
//! (spec §24.4, §24.5, §18).

use std::collections::HashMap;

use algraf_core::Diagnostic;
use algraf_data::{DataFrame, Table};
use algraf_semantics::{
    ir::Setting, ChartIr, ColumnRef, FrameIr, GeometryIr, SettingValue, SpaceDataRef, StatKind,
};

use crate::aes::{color_spec, Legend};
use crate::domains::train_space_domains;
use crate::error::RenderError;
use crate::guide;
use crate::layout::{Layout, Rect};
use crate::scale::{categorical_domain, cell_category};
use crate::space::ScaledSpace;
use crate::stats;
use crate::svg::{escape_attr, num, SvgWriter};
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
}

/// Render a chart IR against its primary data table (spec §24.4).
pub fn render(
    ir: &ChartIr,
    primary: &dyn Table,
    theme: &Theme,
) -> Result<RenderResult, RenderError> {
    let mut diagnostics = Vec::new();

    // Compute derived tables (spec §24.1 step 12).
    let derived = compute_derived(ir, primary);

    let width = ir.width as f64;
    let height = ir.height as f64;

    let has_axes = theme.axes;
    // A first pass with a provisional layout to discover legends.
    let provisional = Layout::compute(width, height, false, has_axes);
    let legends = collect_legends(ir, primary, &derived, &provisional);
    let facet_panel_count = facet_panel_count(ir, primary, &derived);
    let layout = match facet_panel_count {
        Some(count) => Layout::compute_facets(
            width,
            height,
            !legends.is_empty(),
            has_axes,
            count,
            ir.layout.facet_columns,
        ),
        None => Layout::compute(width, height, !legends.is_empty(), has_axes),
    };

    let x_range = (layout.plot.x, layout.plot.right());
    let y_range = (layout.plot.bottom(), layout.plot.y); // inverted for SVG

    let mut panels = Vec::new();
    for space in &ir.spaces {
        let table = active_table(&space.data, primary, &derived);
        if let Some((plane, facet_col)) = facet_frame(&space.frame) {
            let domain_hints = train_space_domains(plane, table, &space.geometries);
            for (index, category) in facet_categories(table, &facet_col.name).iter().enumerate() {
                let Some(facet) = layout.facets.get(index) else {
                    continue;
                };
                let x_range = (facet.plot.x, facet.plot.right());
                let y_range = (facet.plot.bottom(), facet.plot.y);
                match ScaledSpace::build(plane, table, x_range, y_range, &domain_hints) {
                    Some(scaled) => panels.push(Panel {
                        table,
                        scaled,
                        geometries: &space.geometries,
                        plot: facet.plot,
                        rows: Some(facet_rows(table, &facet_col.name, category)),
                        label: Some(category.clone()),
                        facet_index: Some(index),
                    }),
                    None => diagnostics.push(Diagnostic::warning(
                        "R0003",
                        "this faceted space could not be laid out",
                        space.span,
                    )),
                }
            }
        } else {
            let domain_hints = train_space_domains(&space.frame, table, &space.geometries);
            match ScaledSpace::build(&space.frame, table, x_range, y_range, &domain_hints) {
                Some(scaled) => panels.push(Panel {
                    table,
                    scaled,
                    geometries: &space.geometries,
                    plot: layout.plot,
                    rows: None,
                    label: None,
                    facet_index: None,
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
    w.line(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" \
         viewBox=\"0 0 {} {}\" role=\"img\">",
        num(width),
        num(height),
        num(width),
        num(height),
    ));

    // Background.
    w.line(&rect_fill(
        0.0,
        0.0,
        width,
        height,
        &theme.background,
        "algraf-background",
    ));
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
        // Grid (from the first laid-out panel).
        if let Some(first) = panels.first() {
            guide::render_grid(&mut w, &first.scaled, first.plot, theme);
        }
    } else {
        for_each_unique_facet_panel(&panels, |panel| {
            guide::render_facet_label(
                &mut w,
                panel.label.as_deref().unwrap_or_default(),
                layout.facets[panel.facet_index.unwrap()].strip,
                theme,
            );
        });
        for_each_unique_facet_panel(&panels, |panel| {
            guide::render_grid(&mut w, &panel.scaled, panel.plot, theme);
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
                    theme,
                },
                &mut diagnostics,
            );
        }
    }

    // Axes (from the first panel) and legends.
    if has_axes {
        if layout.facets.is_empty() {
            if let Some(first) = panels.first() {
                guide::render_axes(&mut w, &first.scaled, first.plot, theme);
            }
        } else {
            for_each_unique_facet_panel(&panels, |panel| {
                guide::render_axes(&mut w, &panel.scaled, panel.plot, theme);
            });
        }
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

fn active_table<'t>(
    data: &SpaceDataRef,
    primary: &'t dyn Table,
    derived: &'t HashMap<String, DataFrame>,
) -> &'t dyn Table {
    match data {
        SpaceDataRef::Primary => primary,
        SpaceDataRef::Derived(name) => derived
            .get(name)
            .map(|d| d as &dyn Table)
            .unwrap_or(primary),
    }
}

fn compute_derived(ir: &ChartIr, primary: &dyn Table) -> HashMap<String, DataFrame> {
    let mut derived = HashMap::new();
    for d in &ir.derived_tables {
        if d.stat.kind != StatKind::Bin {
            continue;
        }
        let FrameIr::Vector(col) = &d.stat.input else {
            continue;
        };
        let bins = numeric_setting(&d.stat.settings, "bins")
            .filter(|n| *n >= 1.0)
            .map(|n| n.round() as usize)
            .unwrap_or(30);
        let options = stats::BinOptions {
            bins,
            bin_width: numeric_setting(&d.stat.settings, "binWidth").filter(|n| *n > 0.0),
            boundary: numeric_setting(&d.stat.settings, "boundary"),
        };
        derived.insert(
            d.name.clone(),
            stats::bin_with_options(primary, &col.name, options),
        );
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

/// Collect deduplicated fill/stroke legends across all spaces (spec §19.5).
fn collect_legends(
    ir: &ChartIr,
    primary: &dyn Table,
    derived: &HashMap<String, DataFrame>,
    _layout: &Layout,
) -> Vec<Legend> {
    let mut legends: Vec<Legend> = Vec::new();
    for space in &ir.spaces {
        let table = active_table(&space.data, primary, derived);
        for geo in &space.geometries {
            for aesthetic in ["fill", "stroke"] {
                if let Some(mapping) = geo.mappings.iter().find(|m| m.aesthetic == aesthetic) {
                    let spec = color_spec(geo, aesthetic, table);
                    let title = crate::svg::display_label(&mapping.column.name);
                    if let Some(legend) = spec.legend(&title) {
                        if !legends.iter().any(|l| l.title == legend.title) {
                            legends.push(legend);
                        }
                    }
                }
            }
        }
    }
    legends
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
