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

mod assets;
mod backend;
mod common;
mod derived;
mod document;
mod draw_list;
mod glyph_paint;
mod glyph_plan;
mod legend;
mod metadata;
mod panel_space;
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

pub use assets::{load_image_assets_with_io, ImageAsset, ImageAssetLoadResult, ImageAssets};
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

/// Optional render context shared by all render backends.
///
/// Embedders normally use `named_tables`, `image_assets`, and `limits` to pass
/// already-loaded data and resource state across the driver/render boundary
/// without cloning large values. `cli_theme_override` represents CLI-only
/// semantics: `algraf render --theme` is the strongest theme source and
/// replaces space-local theme overrides as well as the base chart theme.
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderOptions<'a> {
    pub named_tables: Option<&'a HashMap<String, DataFrame>>,
    pub image_assets: Option<&'a ImageAssets>,
    pub limits: RenderLimits,
    pub cli_theme_override: Option<&'a str>,
}

impl<'a> RenderOptions<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_named_tables(mut self, named_tables: &'a HashMap<String, DataFrame>) -> Self {
        self.named_tables = Some(named_tables);
        self
    }

    pub fn with_image_assets(mut self, image_assets: &'a ImageAssets) -> Self {
        self.image_assets = Some(image_assets);
        self
    }

    pub fn with_limits(mut self, limits: RenderLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_cli_theme_override(mut self, cli_theme_override: Option<&'a str>) -> Self {
        self.cli_theme_override = cli_theme_override;
        self
    }
}

impl<'a> From<Option<&'a str>> for RenderOptions<'a> {
    fn from(cli_theme_override: Option<&'a str>) -> Self {
        RenderOptions::default().with_cli_theme_override(cli_theme_override)
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

/// Render a chart IR to static SVG (spec §24.4).
///
/// `theme` is the base (chart-level) theme already resolved by the caller.
/// `options` carries optional named tables, image assets, render limits, and
/// the CLI-only strongest theme override.
pub fn render<'a>(
    ir: &ChartIr,
    primary: &'a dyn Table,
    theme: &Theme,
    options: impl Into<RenderOptions<'a>>,
) -> Result<RenderResult, RenderError> {
    render_svg_with_options(ir, primary, theme, options.into(), false)
}

/// Render a chart IR against its primary table plus chart-scoped named tables
/// (spec §10.x). `named_tables` maps each `Table name = "..."` declaration's
/// name to its loaded frame; the caller loads them at the I/O boundary.
#[deprecated(
    since = "0.94.0",
    note = "use render with RenderOptions::with_named_tables"
)]
pub fn render_with_tables(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
) -> Result<RenderResult, RenderError> {
    render(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_cli_theme_override(cli_theme_override),
    )
}

#[deprecated(
    since = "0.94.0",
    note = "use render with RenderOptions::with_named_tables and RenderOptions::with_limits"
)]
pub fn render_with_tables_and_limits(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: RenderLimits,
) -> Result<RenderResult, RenderError> {
    render(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_limits(limits)
            .with_cli_theme_override(cli_theme_override),
    )
}

#[deprecated(
    since = "0.94.0",
    note = "use render with RenderOptions::with_named_tables and RenderOptions::with_image_assets"
)]
pub fn render_with_tables_and_assets(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    assets: &ImageAssets,
) -> Result<RenderResult, RenderError> {
    render(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_image_assets(assets)
            .with_cli_theme_override(cli_theme_override),
    )
}

#[deprecated(
    since = "0.94.0",
    note = "use render with RenderOptions::with_named_tables, RenderOptions::with_image_assets, and RenderOptions::with_limits"
)]
pub fn render_with_tables_and_assets_and_limits(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    assets: &ImageAssets,
    limits: RenderLimits,
) -> Result<RenderResult, RenderError> {
    render(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_image_assets(assets)
            .with_limits(limits)
            .with_cli_theme_override(cli_theme_override),
    )
}

/// Render a chart IR to SVG with the opt-in interactive runtime embedded
/// (spec §29.3). The chart body is byte-for-byte identical to [`render`]; the
/// only difference is the single fixed, audited `<script>` appended before
/// `</svg>`. Static affordances (`<title>`, `data-algraf-highlight`) and
/// ordinary plot/axis elements are present either way; the script interprets
/// them for tooltips, highlighting, and crosshair value readouts.
pub fn render_interactive<'a>(
    ir: &ChartIr,
    primary: &'a dyn Table,
    theme: &Theme,
    options: impl Into<RenderOptions<'a>>,
) -> Result<RenderResult, RenderError> {
    render_svg_with_options(ir, primary, theme, options.into(), true)
}

/// Interactive counterpart of [`render_with_tables`].
#[deprecated(
    since = "0.94.0",
    note = "use render_interactive with RenderOptions::with_named_tables"
)]
pub fn render_interactive_with_tables(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
) -> Result<RenderResult, RenderError> {
    render_interactive(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_cli_theme_override(cli_theme_override),
    )
}

#[deprecated(
    since = "0.94.0",
    note = "use render_interactive with RenderOptions::with_named_tables and RenderOptions::with_limits"
)]
pub fn render_interactive_with_tables_and_limits(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: RenderLimits,
) -> Result<RenderResult, RenderError> {
    render_interactive(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_limits(limits)
            .with_cli_theme_override(cli_theme_override),
    )
}

#[deprecated(
    since = "0.94.0",
    note = "use render_interactive with RenderOptions::with_named_tables, RenderOptions::with_image_assets, and RenderOptions::with_limits"
)]
pub fn render_interactive_with_tables_and_assets_and_limits(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    assets: &ImageAssets,
    limits: RenderLimits,
) -> Result<RenderResult, RenderError> {
    render_interactive(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_image_assets(assets)
            .with_limits(limits)
            .with_cli_theme_override(cli_theme_override),
    )
}

#[allow(clippy::too_many_arguments)]
fn render_svg_with_options(
    ir: &ChartIr,
    primary: &dyn Table,
    theme: &Theme,
    options: RenderOptions<'_>,
    interactive: bool,
) -> Result<RenderResult, RenderError> {
    let (svg, diagnostics, layout, metadata) =
        render_with_backend(ir, primary, theme, options, SvgBackend { interactive })?;
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
pub fn render_draw_list<'a>(
    ir: &ChartIr,
    primary: &'a dyn Table,
    theme: &Theme,
    options: impl Into<RenderOptions<'a>>,
) -> Result<DrawListResult, RenderError> {
    let (draw_list, diagnostics, layout, metadata) =
        render_with_backend(ir, primary, theme, options.into(), DrawListBackend)?;
    Ok(DrawListResult {
        draw_list,
        diagnostics,
        layout,
        metadata,
    })
}

/// Draw-list counterpart of [`render_with_tables`].
#[deprecated(
    since = "0.94.0",
    note = "use render_draw_list with RenderOptions::with_named_tables"
)]
pub fn render_draw_list_with_tables(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
) -> Result<DrawListResult, RenderError> {
    render_draw_list(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_cli_theme_override(cli_theme_override),
    )
}

#[deprecated(
    since = "0.94.0",
    note = "use render_draw_list with RenderOptions::with_named_tables and RenderOptions::with_limits"
)]
pub fn render_draw_list_with_tables_and_limits(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: RenderLimits,
) -> Result<DrawListResult, RenderError> {
    render_draw_list(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_limits(limits)
            .with_cli_theme_override(cli_theme_override),
    )
}

#[deprecated(
    since = "0.94.0",
    note = "use render_draw_list with RenderOptions::with_named_tables, RenderOptions::with_image_assets, and RenderOptions::with_limits"
)]
pub fn render_draw_list_with_tables_and_assets_and_limits(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    assets: &ImageAssets,
    limits: RenderLimits,
) -> Result<DrawListResult, RenderError> {
    render_draw_list(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_image_assets(assets)
            .with_limits(limits)
            .with_cli_theme_override(cli_theme_override),
    )
}

/// Render a chart IR to a raster image through the render-model raster backend
/// (spec §24.6). `scale` multiplies the SVG viewport to the pixel grid. Unlike
/// the SVG-rasterizing PNG path, this draws from the planned scene's draw list.
pub fn render_raster<'a>(
    ir: &ChartIr,
    primary: &'a dyn Table,
    theme: &Theme,
    options: impl Into<RenderOptions<'a>>,
    scale: f32,
) -> Result<RasterResult, RenderError> {
    let (image, diagnostics, layout, metadata) =
        render_with_backend(ir, primary, theme, options.into(), RasterBackend { scale })?;
    Ok(RasterResult {
        image,
        diagnostics,
        layout,
        metadata,
    })
}

/// Raster counterpart of [`render_with_tables`].
#[deprecated(
    since = "0.94.0",
    note = "use render_raster with RenderOptions::with_named_tables"
)]
pub fn render_raster_with_tables(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    scale: f32,
) -> Result<RasterResult, RenderError> {
    render_raster(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_cli_theme_override(cli_theme_override),
        scale,
    )
}

#[deprecated(
    since = "0.94.0",
    note = "use render_raster with RenderOptions::with_named_tables and RenderOptions::with_limits"
)]
pub fn render_raster_with_tables_and_limits(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    limits: RenderLimits,
    scale: f32,
) -> Result<RasterResult, RenderError> {
    render_raster(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_limits(limits)
            .with_cli_theme_override(cli_theme_override),
        scale,
    )
}

#[deprecated(
    since = "0.94.0",
    note = "use render_raster with RenderOptions::with_named_tables, RenderOptions::with_image_assets, and RenderOptions::with_limits"
)]
#[allow(clippy::too_many_arguments)]
pub fn render_raster_with_tables_and_assets_and_limits(
    ir: &ChartIr,
    primary: &dyn Table,
    named_tables: &HashMap<String, DataFrame>,
    theme: &Theme,
    cli_theme_override: Option<&str>,
    assets: &ImageAssets,
    limits: RenderLimits,
    scale: f32,
) -> Result<RasterResult, RenderError> {
    render_raster(
        ir,
        primary,
        theme,
        RenderOptions::default()
            .with_named_tables(named_tables)
            .with_image_assets(assets)
            .with_limits(limits)
            .with_cli_theme_override(cli_theme_override),
        scale,
    )
}

/// Drive the shared planning pipeline and hand the resulting scene to `backend`
/// for emission (spec §24.6). Planning is identical across backends; only the
/// emission step differs.
fn render_with_backend<B: RenderBackend>(
    ir: &ChartIr,
    primary: &dyn Table,
    theme: &Theme,
    options: RenderOptions<'_>,
    backend: B,
) -> Result<(B::Output, Vec<Diagnostic>, Layout, InteractionMetadata), RenderError> {
    let empty_named_tables;
    let named_tables = match options.named_tables {
        Some(named_tables) => named_tables,
        None => {
            empty_named_tables = HashMap::new();
            &empty_named_tables
        }
    };
    let empty_assets;
    let assets = match options.image_assets {
        Some(assets) => assets,
        None => {
            empty_assets = ImageAssets::default();
            &empty_assets
        }
    };
    let limits = options.limits;
    let mut diagnostics = Vec::new();
    let derived = derived::compute_derived(ir, primary, named_tables);
    // Planning half: resolve everything to draw into a render scene.
    let plan = panels::build_render_plan(
        ir,
        primary,
        &derived,
        theme,
        options.cli_theme_override,
        assets,
        &limits,
        &mut diagnostics,
    );
    let scene = RenderScene {
        ir,
        layout: &plan.layout,
        legends: &plan.legends,
        panels: &plan.panels,
        theme,
        assets,
        limits: &limits,
    };
    let metadata = metadata::build_interaction_metadata(&scene);
    // Emission half: hand the scene to the chosen output backend.
    let output = backend.emit(&scene, &metadata, &mut diagnostics);

    Ok((output, diagnostics, plan.layout, metadata))
}
