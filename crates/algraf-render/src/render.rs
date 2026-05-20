//! Render orchestration: IR + data to a deterministic SVG string
//! (spec §24.4, §24.5, §18).

use std::collections::HashMap;

use algraf_core::Diagnostic;
use algraf_data::{DataFrame, Table};
use algraf_semantics::{ChartIr, FrameIr, GeometryIr, SettingValue, SpaceDataRef, StatKind};

use crate::aes::{color_spec, Legend};
use crate::error::RenderError;
use crate::guide;
use crate::layout::Layout;
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

    // Pre-build scaled spaces so legend/axis presence is known before layout.
    struct Panel<'t> {
        table: &'t dyn Table,
        scaled: ScaledSpace,
        geometries: &'t [GeometryIr],
    }

    let has_axes = theme.axes;
    // A first pass with a provisional layout to discover legends.
    let provisional = Layout::compute(width, height, false, has_axes);
    let legends = collect_legends(ir, primary, &derived, &provisional);
    let layout = Layout::compute(width, height, !legends.is_empty(), has_axes);

    let x_range = (layout.plot.x, layout.plot.right());
    let y_range = (layout.plot.bottom(), layout.plot.y); // inverted for SVG

    let mut panels = Vec::new();
    for space in &ir.spaces {
        let table = active_table(&space.data, primary, &derived);
        match ScaledSpace::build(&space.frame, table, x_range, y_range) {
            Some(scaled) => panels.push(Panel {
                table,
                scaled,
                geometries: &space.geometries,
            }),
            None => diagnostics.push(Diagnostic::warning(
                "R0003",
                "this space could not be laid out (faceting is not yet supported)",
                space.span,
            )),
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
    w.line(&rect_fill(
        layout.plot.x,
        layout.plot.y,
        layout.plot.width,
        layout.plot.height,
        &theme.plot_background,
        "algraf-plot-area",
    ));

    // Grid (from the first laid-out panel).
    if let Some(first) = panels.first() {
        guide::render_grid(&mut w, &first.scaled, layout.plot, theme);
    }

    // Data layers in source order (spec §18.3).
    for panel in &panels {
        for geo in panel.geometries {
            crate::geom::render(
                &mut w,
                geo,
                &panel.scaled,
                panel.table,
                layout.plot,
                theme,
                &mut diagnostics,
            );
        }
    }

    // Axes (from the first panel) and legends.
    if has_axes {
        if let Some(first) = panels.first() {
            guide::render_axes(&mut w, &first.scaled, layout.plot, theme);
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
        let bins = d
            .stat
            .settings
            .iter()
            .find(|s| s.name == "bins")
            .and_then(|s| match s.value {
                SettingValue::Number(n) => Some(n as usize),
                _ => None,
            })
            .unwrap_or(30);
        derived.insert(d.name.clone(), stats::bin(primary, &col.name, bins));
    }
    derived
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
                    if let Some(legend) = spec.legend(&mapping.column.name) {
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
