//! Render orchestration: IR + data to a deterministic SVG string
//! (spec §24.4, §24.5, §24.6, §18).
//!
//! The pipeline has two halves separated by an explicit boundary:
//!
//! 1. **Planning** ([`derived`], [`panels`], [`spatial`], [`legend`], [`common`])
//!    turns the semantic [`ChartIr`] plus loaded data into a [`RenderScene`]:
//!    derived-table execution, guide/legend discovery, layout and panel planning,
//!    spatial projection fitting, and scale training all happen here. No output
//!    bytes are written in this half.
//! 2. **Emission** hands the scene to a [`RenderBackend`]. Three backends consume
//!    the same scene through one shared mark sink ([`crate::sink`]):
//!    [`SvgBackend`] produces deterministic SVG, [`DrawListBackend`] records a
//!    complete serializable [`DrawList`], and [`RasterBackend`] draws that list
//!    to a raster image. Because all three observe the same primitive calls,
//!    they agree on coordinates and colors by construction (spec §24.6).
//!
//! See [`backend`] for the seam itself.

mod backend;
mod common;
mod derived;
mod document;
mod draw_list;
mod inset;
mod inset_plan;
mod legend;
mod metadata;
mod panels;
mod raster;
mod row_table;
mod spatial;

use std::collections::HashMap;

use algraf_core::Diagnostic;
use algraf_data::{DataFrame, Table};
use algraf_semantics::ChartIr;

use crate::error::RenderError;
use crate::layout::Layout;
use crate::theme::Theme;

use backend::{RenderBackend, RenderScene, SvgBackend};
use draw_list::DrawListBackend;
use raster::RasterBackend;

pub use draw_list::{DrawList, DrawOp, DrawRole, TextAnchor};
pub use metadata::{
    InteractionAxes, InteractionAxis, InteractionChart, InteractionDomain, InteractionGroup,
    InteractionGroupValue, InteractionLegend, InteractionMark, InteractionMetadata,
    InteractionPlot, TooltipRow,
};
pub use raster::RasterImage;

/// Default per-layer budget for raw per-row mark emission.
pub const DEFAULT_MARK_BUDGET: usize = 100_000;

/// Render-time limits for static SVG/draw-list output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderLimits {
    pub mark_budget: Option<usize>,
}

impl Default for RenderLimits {
    fn default() -> Self {
        RenderLimits {
            mark_budget: Some(DEFAULT_MARK_BUDGET),
        }
    }
}

/// The result of rendering: an SVG document plus render diagnostics.
#[derive(Debug, Clone)]
pub struct RenderResult {
    pub svg: String,
    pub diagnostics: Vec<Diagnostic>,
    pub layout: Layout,
    pub metadata: InteractionMetadata,
}

/// The result of rendering through the draw-list backend (spec §24.6).
#[derive(Debug, Clone)]
pub struct DrawListResult {
    pub draw_list: DrawList,
    pub diagnostics: Vec<Diagnostic>,
    pub layout: Layout,
    pub metadata: InteractionMetadata,
}

/// The result of rendering through the render-model raster backend (spec §24.6).
#[derive(Debug)]
pub struct RasterResult {
    pub image: RasterImage,
    pub diagnostics: Vec<Diagnostic>,
    pub layout: Layout,
    pub metadata: InteractionMetadata,
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
    render_svg_with_tables(
        ir,
        primary,
        named_tables,
        theme,
        cli_theme_override,
        false,
        RenderLimits::default(),
    )
}

pub fn render_with_tables_and_limits(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: RenderLimits,
) -> Result<RenderResult, RenderError> {
    render_svg_with_tables(
        ir,
        primary,
        named_tables,
        theme,
        cli_theme_override,
        false,
        limits,
    )
}

/// Render a chart IR to SVG with the opt-in interactive runtime embedded
/// (spec §29.3). The chart body is byte-for-byte identical to [`render`]; the
/// only difference is the single fixed, audited `<script>` appended before
/// `</svg>`. Static affordances (`<title>`, `data-algraf-highlight`) and
/// ordinary plot/axis elements are present either way; the script interprets
/// them for tooltips, highlighting, and crosshair value readouts.
pub fn render_interactive(
    ir: &ChartIr,
    primary: &dyn Table,
    theme: &Theme,
    cli_theme_override: Option<&str>,
) -> Result<RenderResult, RenderError> {
    render_svg_with_tables(
        ir,
        primary,
        &HashMap::new(),
        theme,
        cli_theme_override,
        true,
        RenderLimits::default(),
    )
}

/// Interactive counterpart of [`render_with_tables`].
pub fn render_interactive_with_tables(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
) -> Result<RenderResult, RenderError> {
    render_svg_with_tables(
        ir,
        primary,
        named_tables,
        theme,
        cli_theme_override,
        true,
        RenderLimits::default(),
    )
}

pub fn render_interactive_with_tables_and_limits(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: RenderLimits,
) -> Result<RenderResult, RenderError> {
    render_svg_with_tables(
        ir,
        primary,
        named_tables,
        theme,
        cli_theme_override,
        true,
        limits,
    )
}

fn render_svg_with_tables(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    interactive: bool,
    limits: RenderLimits,
) -> Result<RenderResult, RenderError> {
    let (svg, diagnostics, layout, metadata) = render_with_backend(
        ir,
        primary,
        named_tables,
        theme,
        cli_theme_override,
        limits,
        SvgBackend { interactive },
    )?;
    Ok(RenderResult {
        svg,
        diagnostics,
        layout,
        metadata,
    })
}

/// Render a chart IR to a [`DrawList`] through the draw-list backend (spec §24.6).
///
/// This drives the same planning pipeline as [`render`] but emits a serializable,
/// Canvas-drawable frame description instead of SVG. See [`DrawList`] for the
/// documented equivalence limits relative to SVG output.
pub fn render_draw_list(
    ir: &ChartIr,
    primary: &dyn Table,
    theme: &Theme,
    cli_theme_override: Option<&str>,
) -> Result<DrawListResult, RenderError> {
    render_draw_list_with_tables(ir, primary, &HashMap::new(), theme, cli_theme_override)
}

/// Draw-list counterpart of [`render_with_tables`].
pub fn render_draw_list_with_tables(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
) -> Result<DrawListResult, RenderError> {
    render_draw_list_with_tables_and_limits(
        ir,
        primary,
        named_tables,
        theme,
        cli_theme_override,
        RenderLimits::default(),
    )
}

pub fn render_draw_list_with_tables_and_limits(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: RenderLimits,
) -> Result<DrawListResult, RenderError> {
    let (draw_list, diagnostics, layout, metadata) = render_with_backend(
        ir,
        primary,
        named_tables,
        theme,
        cli_theme_override,
        limits,
        DrawListBackend,
    )?;
    Ok(DrawListResult {
        draw_list,
        diagnostics,
        layout,
        metadata,
    })
}

/// Render a chart IR to a raster image through the render-model raster backend
/// (spec §24.6). `scale` multiplies the SVG viewport to the pixel grid. Unlike
/// the SVG-rasterizing PNG path, this draws from the planned scene's draw list.
pub fn render_raster(
    ir: &ChartIr,
    primary: &dyn Table,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    scale: f32,
) -> Result<RasterResult, RenderError> {
    render_raster_with_tables(
        ir,
        primary,
        &HashMap::new(),
        theme,
        cli_theme_override,
        scale,
    )
}

/// Raster counterpart of [`render_with_tables`].
pub fn render_raster_with_tables(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    scale: f32,
) -> Result<RasterResult, RenderError> {
    render_raster_with_tables_and_limits(
        ir,
        primary,
        named_tables,
        theme,
        cli_theme_override,
        RenderLimits::default(),
        scale,
    )
}

pub fn render_raster_with_tables_and_limits(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: RenderLimits,
    scale: f32,
) -> Result<RasterResult, RenderError> {
    let (image, diagnostics, layout, metadata) = render_with_backend(
        ir,
        primary,
        named_tables,
        theme,
        cli_theme_override,
        limits,
        RasterBackend { scale },
    )?;
    Ok(RasterResult {
        image,
        diagnostics,
        layout,
        metadata,
    })
}

/// Drive the shared planning pipeline and hand the resulting scene to `backend`
/// for emission (spec §24.6). Planning is identical across backends; only the
/// emission step differs.
fn render_with_backend<B: RenderBackend>(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: RenderLimits,
    backend: B,
) -> Result<(B::Output, Vec<Diagnostic>, Layout, InteractionMetadata), RenderError> {
    let mut diagnostics = Vec::new();
    let derived = derived::compute_derived(ir, primary, named_tables);
    // Planning half: resolve everything to draw into a render scene.
    let plan = panels::build_render_plan(
        ir,
        primary,
        &derived,
        theme,
        cli_theme_override,
        &limits,
        &mut diagnostics,
    );
    let scene = RenderScene {
        ir,
        layout: &plan.layout,
        legends: &plan.legends,
        panels: &plan.panels,
        theme,
        limits: &limits,
    };
    let metadata = metadata::build_interaction_metadata(&scene);
    // Emission half: hand the scene to the chosen output backend.
    let output = backend.emit(&scene, &metadata, &mut diagnostics);

    Ok((output, diagnostics, plan.layout, metadata))
}
