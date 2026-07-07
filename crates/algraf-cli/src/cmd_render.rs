//! `algraf render` — render a chart to SVG, PNG, or a draw-list JSON
//! (spec §22, §24).

use std::collections::HashMap;
use std::path::PathBuf;

use algraf_data::Format;
use algraf_driver::{
    document_charts, driver_error_diagnostic, prepare_chart, DriverError, LoadContext, OsDriverIo,
    PreparationReport, PrepareOptions, ReportPhase, SourceInput,
};
use algraf_render::{
    load_image_assets_with_io, render, render_draw_list, render_interactive, render_raster,
    ImageAssets, RenderLimits, RenderOptions, Theme,
};
use algraf_semantics::ChartIr;
use algraf_syntax::ast::ChartBlock;
use algraf_syntax::parse;
use clap::Args;

use crate::cmd_source::SourceArgs;
use crate::diagnostics;
use crate::error::CliError;
use crate::input::{driver_error, read_template_source};
use crate::io::{should_write_metadata, write_outputs};
use crate::png;
use crate::svg_debug::augment_svg;

/// Output backend for `render` (spec §24.6).
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum RenderFormat {
    /// Deterministic SVG (the canonical backend). `.png` outputs rasterize it.
    Svg,
    /// Write deterministic SVG plus a JSON interaction metadata sidecar.
    #[value(name = "svg+json")]
    SvgJson,
    /// A serializable, Canvas-drawable JSON draw list of scene primitives.
    DrawList,
    /// A PNG rasterized directly from the draw-list scene model (no SVG, no
    /// system fonts). Shapes only; text glyphs are not rendered (spec §24.6).
    Raster,
}

impl RenderFormat {
    pub(crate) fn writes_svg(self) -> bool {
        matches!(self, RenderFormat::Svg | RenderFormat::SvgJson)
    }

    pub(crate) fn writes_metadata(self) -> bool {
        matches!(self, RenderFormat::SvgJson)
    }
}

#[derive(Args)]
pub(crate) struct RenderArgs {
    #[command(flatten)]
    pub(crate) source: SourceArgs,
    /// Output path. With `--format svg`, `.png` paths rasterize the SVG and all
    /// other paths write SVG; with `--format svg+json`, this is the SVG path or
    /// base path; with `--format draw-list`, paths write JSON; with `--format
    /// raster`, paths write a PNG drawn from the scene model.
    #[arg(long)]
    pub(crate) output: Option<PathBuf>,
    /// Output backend: `svg` (default), `svg+json`, `draw-list` JSON, or
    /// `raster` PNG drawn from the scene model (spec §24.6).
    #[arg(long, value_enum, default_value_t = RenderFormat::Svg)]
    pub(crate) format: RenderFormat,
    /// Embed the fixed, audited interactive runtime in SVG output (spec §29.3):
    /// tooltip-on-hover, highlight-on-hover, and plot crosshairs from inert
    /// metadata plus rendered axes. Only affects `--format svg`; static SVG
    /// stays script-free without it.
    #[arg(long)]
    pub(crate) interactive: bool,
    /// Write the JSON interaction metadata sidecar to this path.
    #[arg(long)]
    pub(crate) metadata: Option<PathBuf>,
    #[arg(long)]
    pub(crate) width: Option<u32>,
    #[arg(long)]
    pub(crate) height: Option<u32>,
    /// PNG raster scale. The SVG viewport stays at --width/--height.
    #[arg(long, default_value_t = png::DEFAULT_PNG_SCALE)]
    pub(crate) png_scale: f32,
    /// PNG physical DPI metadata. Defaults to 96 * --png-scale.
    #[arg(long)]
    pub(crate) png_dpi: Option<u32>,
    #[arg(long)]
    pub(crate) theme: Option<String>,
    /// Treat warnings as errors.
    #[arg(long)]
    pub(crate) strict: bool,
    #[arg(long)]
    pub(crate) debug_layout: bool,
    #[arg(long)]
    pub(crate) emit_metadata: bool,
    /// Maximum raw marks a single layer may emit before rendering fails.
    #[arg(long)]
    pub(crate) mark_budget: Option<usize>,
    /// Disable the raw-mark budget guardrail.
    #[arg(long)]
    pub(crate) allow_large_render: bool,
}

/// One rendered chart's primary output plus an optional sidecar.
pub(crate) struct RenderOutput {
    pub(crate) primary: RenderOutputData,
    pub(crate) metadata_json: Option<String>,
}

/// One rendered chart's primary output: text (SVG/JSON) or binary (raster PNG).
pub(crate) enum RenderOutputData {
    Text(String),
    Bytes(Vec<u8>),
}

pub(crate) fn render_cmd(args: RenderArgs) -> Result<(), CliError> {
    if args.format == RenderFormat::SvgJson && args.output.is_none() {
        return Err(CliError::Usage(
            "`--format svg+json` writes two files; pass --output".to_string(),
        ));
    }
    let (source, input) = read_template_source(
        args.source.input.as_deref(),
        args.source.eval.as_deref(),
        &args.source.vars,
    )?;
    let parsed = parse(&source);
    let root = parsed.syntax();
    let label = input.label();

    if diagnostics::has_blocking(parsed.diagnostics(), args.strict) {
        eprint!(
            "{}",
            diagnostics::render_human(&source, &label, parsed.diagnostics())
        );
        return Err(CliError::Diagnostics);
    }
    // Document-level parse diagnostics (warnings, hints) print once.
    if !parsed.diagnostics().is_empty() {
        eprint!(
            "{}",
            diagnostics::render_human(&source, &label, parsed.diagnostics())
        );
    }

    // A document may contain more than one chart (spec §7.1); each renders
    // independently against its own data source.
    let charts = document_charts(&root);
    if charts.is_empty() {
        return Err(CliError::Usage(
            "no Chart block to render; add Chart(data: \"file.csv\") { ... }".to_string(),
        ));
    }
    let multi = charts.len() > 1;
    if multi && args.output.is_none() {
        return Err(CliError::Usage(format!(
            "document has {} charts; pass --output to write one file per chart",
            charts.len()
        )));
    }

    // Render every chart before writing, so a failure leaves no partial output.
    let mut outputs = Vec::with_capacity(charts.len());
    for chart in &charts {
        outputs.push(render_chart_output(
            chart, &args, &input, &source, &label, multi,
        )?);
    }

    write_outputs(&args, outputs, multi)
}

/// Resolve, analyze, and render one chart to the requested output format.
fn render_chart_output(
    chart: &ChartBlock,
    args: &RenderArgs,
    input: &SourceInput,
    source: &str,
    label: &str,
    multi: bool,
) -> Result<RenderOutput, CliError> {
    match args.format {
        RenderFormat::Svg | RenderFormat::SvgJson => {
            render_chart_svg(chart, args, input, source, label, multi)
        }
        RenderFormat::DrawList => render_chart_draw_list(chart, args, input, source, label, multi),
        RenderFormat::Raster => render_chart_raster(chart, args, input, source, label, multi),
    }
}

fn render_limits(args: &RenderArgs) -> RenderLimits {
    RenderLimits {
        mark_budget: if args.allow_large_render {
            None
        } else {
            args.mark_budget.or(RenderLimits::default().mark_budget)
        },
    }
}

fn render_options<'a>(
    named_frames: &'a HashMap<String, algraf_data::DataFrame>,
    assets: &'a ImageAssets,
    args: &'a RenderArgs,
    cli_theme_override: Option<&'a str>,
) -> RenderOptions<'a> {
    RenderOptions::default()
        .with_named_tables(named_frames)
        .with_image_assets(assets)
        .with_limits(render_limits(args))
        .with_cli_theme_override(cli_theme_override)
}

fn finish_render_report(
    report: &mut PreparationReport,
    render_diagnostics: &[algraf_core::Diagnostic],
    args: &RenderArgs,
    source: &str,
    label: &str,
) -> Result<(), CliError> {
    report.extend(ReportPhase::Render, render_diagnostics.iter().cloned());
    let render_diags = report.diagnostics();
    if !render_diags.is_empty() {
        eprint!(
            "{}",
            diagnostics::render_human(source, label, &render_diags)
        );
    }
    if diagnostics::has_blocking(&render_diags, args.strict) {
        Err(CliError::Diagnostics)
    } else {
        Ok(())
    }
}

/// Everything needed to drive a render backend for one chart, with all parse,
/// data, and semantic diagnostics already reported. The shared `report` carries
/// data warnings so each backend can append its own render diagnostics.
struct RenderInputs {
    ir: ChartIr,
    frame: algraf_data::DataFrame,
    named_frames: HashMap<String, algraf_data::DataFrame>,
    assets: ImageAssets,
    theme: Theme,
    cli_theme_override: Option<String>,
    report: PreparationReport,
}

/// Resolve, analyze, and render one chart block to an SVG string (spec §7.1).
fn render_chart_svg(
    chart: &ChartBlock,
    args: &RenderArgs,
    input: &SourceInput,
    source: &str,
    label: &str,
    multi: bool,
) -> Result<RenderOutput, CliError> {
    let RenderInputs {
        ir,
        frame,
        named_frames,
        assets,
        theme,
        cli_theme_override,
        mut report,
    } = prepare_render_inputs(chart, args, input, source, label, multi)?;

    let options = render_options(&named_frames, &assets, args, cli_theme_override.as_deref());
    let mut result = if args.interactive {
        render_interactive(&ir, &frame, &theme, options)
    } else {
        render(&ir, &frame, &theme, options)
    }
    .map_err(|e| CliError::Internal(e.to_string()))?;
    finish_render_report(&mut report, &result.diagnostics, args, source, label)?;

    if args.debug_layout || args.emit_metadata {
        let layout = result.layout.clone();
        augment_svg(
            &mut result.svg,
            &ir,
            &theme,
            &layout,
            result.diagnostics.len(),
            args.debug_layout,
            args.emit_metadata,
        );
    }
    let metadata_json = should_write_metadata(args).then(|| result.metadata.to_json());
    Ok(RenderOutput {
        primary: RenderOutputData::Text(result.svg),
        metadata_json,
    })
}

/// Render one chart block to a draw-list JSON string (spec §24.6).
fn render_chart_draw_list(
    chart: &ChartBlock,
    args: &RenderArgs,
    input: &SourceInput,
    source: &str,
    label: &str,
    multi: bool,
) -> Result<RenderOutput, CliError> {
    let RenderInputs {
        ir,
        frame,
        named_frames,
        assets,
        theme,
        cli_theme_override,
        mut report,
    } = prepare_render_inputs(chart, args, input, source, label, multi)?;

    let result = render_draw_list(
        &ir,
        &frame,
        &theme,
        render_options(&named_frames, &assets, args, cli_theme_override.as_deref()),
    )
    .map_err(|e| CliError::Internal(e.to_string()))?;
    finish_render_report(&mut report, &result.diagnostics, args, source, label)?;
    let metadata_json = should_write_metadata(args).then(|| result.metadata.to_json());
    Ok(RenderOutput {
        primary: RenderOutputData::Text(result.draw_list.to_json()),
        metadata_json,
    })
}

/// Render one chart block to PNG bytes through the render-model raster backend
/// (spec §24.6). Honors `--png-scale`/`--png-dpi` like the SVG PNG path.
fn render_chart_raster(
    chart: &ChartBlock,
    args: &RenderArgs,
    input: &SourceInput,
    source: &str,
    label: &str,
    multi: bool,
) -> Result<RenderOutput, CliError> {
    let png_options =
        png::PngOptions::new(args.png_scale, args.png_dpi).map_err(CliError::Usage)?;
    let RenderInputs {
        ir,
        frame,
        named_frames,
        assets,
        theme,
        cli_theme_override,
        mut report,
    } = prepare_render_inputs(chart, args, input, source, label, multi)?;

    let result = render_raster(
        &ir,
        &frame,
        &theme,
        render_options(&named_frames, &assets, args, cli_theme_override.as_deref()),
        png_options.scale(),
    )
    .map_err(|e| CliError::Internal(e.to_string()))?;
    finish_render_report(&mut report, &result.diagnostics, args, source, label)?;
    let bytes = png::encode_pixmap(result.image.pixmap(), png_options.dpi())
        .map_err(|e| CliError::Internal(format!("PNG encoding failed: {e}")))?;
    let metadata_json = should_write_metadata(args).then(|| result.metadata.to_json());
    Ok(RenderOutput {
        primary: RenderOutputData::Bytes(bytes),
        metadata_json,
    })
}

/// Shared preparation for both render backends: load data, analyze, resolve the
/// IR and theme, and report all non-render diagnostics (spec §7.1).
fn prepare_render_inputs(
    chart: &ChartBlock,
    args: &RenderArgs,
    input: &SourceInput,
    source: &str,
    label: &str,
    multi: bool,
) -> Result<RenderInputs, CliError> {
    let algraf_driver::PreparedChart {
        mut primary,
        named_tables,
        analysis,
        ..
    } = match prepare_chart(
        chart,
        PrepareOptions {
            source_input: input,
            base_dir: args.source.base_dir.as_deref(),
            data_override: args.source.data.as_deref(),
            data_format_override: args.source.data_format.map(Format::from),
            multi_chart: multi,
        },
    ) {
        Ok(prepared) => prepared,
        Err(
            err @ (DriverError::Data { .. }
            | DriverError::StdinRead(_)
            | DriverError::StdinData { .. }
            | DriverError::StdinParse(_)),
        ) => {
            let source_expr = algraf_driver::extract_chart_data_source(chart);
            let span = source_expr
                .span()
                .unwrap_or_else(|| algraf_core::Span::new(0, 0));
            let diagnostic = driver_error_diagnostic(&err, span);
            eprint!(
                "{}",
                diagnostics::render_human(source, label, &[diagnostic])
            );
            return Err(CliError::Diagnostics);
        }
        Err(err) => return Err(driver_error(err)),
    };
    let diags = analysis.diagnostics;
    if diagnostics::has_blocking(&diags, args.strict) {
        eprint!("{}", diagnostics::render_human(source, label, &diags));
        return Err(CliError::Diagnostics);
    }
    if !diags.is_empty() {
        eprint!("{}", diagnostics::render_human(source, label, &diags));
    }

    let loaded = primary.take().ok_or_else(|| {
        CliError::Internal("analysis allowed rendering with a missing data source".to_string())
    })?;
    // Collect data warnings with their table/source context (spec §10.3); they
    // print as plain `warning:` lines because they carry no source span.
    let mut report = PreparationReport::new();
    report.push_data_warnings(&LoadContext::Primary, None, &loaded.warnings);
    for table in &named_tables {
        report.push_data_warnings(
            &LoadContext::Table {
                name: table.name.clone(),
            },
            Some(table.path.as_path()),
            &table.warnings,
        );
    }
    for warning in report.data_warnings() {
        eprintln!("warning: {}", warning.message());
    }
    if args.strict && report.has_data_warnings() {
        return Err(CliError::Diagnostics);
    }
    let frame = loaded.frame;
    let named_frames: HashMap<String, algraf_data::DataFrame> = named_tables
        .into_iter()
        .map(|t| (t.name, t.frame))
        .collect();

    let mut ir = analysis
        .ir
        .ok_or_else(|| CliError::Internal("analysis produced no IR".to_string()))?;
    if let Some(w) = args.width {
        ir.width = w;
    }
    if let Some(h) = args.height {
        ir.height = h;
    }

    // CLI --theme replaces the base theme (spec §22.3). The renderer still
    // applies space-local theme overrides from the IR on top of this base. A
    // source-level `Theme(...)` may carry override values, resolved here.
    let theme = match (&args.theme, &ir.theme) {
        (Some(name), _) => Theme::by_name(name),
        (None, Some(theme_ir)) => Theme::from_ir(theme_ir),
        (None, None) => Theme::default(),
    };
    let cli_theme_override = args.theme.clone();

    let image_assets = load_image_assets_with_io(
        &ir,
        &frame,
        &named_frames,
        input,
        args.source.base_dir.as_deref(),
        &OsDriverIo,
    );
    if !image_assets.diagnostics.is_empty() {
        let mut asset_report = PreparationReport::new();
        asset_report.extend(ReportPhase::Render, image_assets.diagnostics);
        let asset_diags = asset_report.diagnostics();
        eprint!("{}", diagnostics::render_human(source, label, &asset_diags));
        return Err(CliError::Diagnostics);
    }

    Ok(RenderInputs {
        ir,
        frame,
        named_frames,
        assets: image_assets.assets,
        theme,
        cli_theme_override,
        report,
    })
}
