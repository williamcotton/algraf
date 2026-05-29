//! The `algraf` binary: argument parsing, command dispatch, and I/O (spec §22).

mod astjson;
mod diagnostics;
mod error;
mod png;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use algraf_data::{DataType, Format};
use algraf_driver::{
    document_charts, driver_error_diagnostic, expand_variables, extract_data_source, load_schema,
    parse_variable_assignments, prepare_chart, prepare_chart_partial, DriverError, LoadContext,
    PreparationReport, PrepareOptions, ReportPhase, SourceInput,
};
use algraf_render::{
    render_draw_list_with_tables, render_interactive_with_tables, render_raster_with_tables,
    render_with_tables, svg_num, Layout, Rect, Theme,
};
use algraf_semantics::{
    analyze, AestheticMapping, ChartIr, ColumnRef, DataSourceIr, DeriveIr, FrameIr, GeometryIr,
    GeometryKind, GradientIr, GuideOverridesIr, ScaleIr, ScaleTargetIr, ScaleTypeIr, SettingValue,
    SpaceDataRef, SpaceIr, StatKind, StatOptionsIr,
};
use algraf_syntax::ast::ChartBlock;
use algraf_syntax::{format, parse};
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};

use crate::error::CliError;

#[derive(Parser)]
#[command(
    name = "algraf",
    version,
    about = "Algraf: algebraic grammar-of-graphics"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a chart to SVG, PNG, or a draw-list JSON.
    Render(RenderArgs),
    /// Parse and analyze without rendering.
    Check(CheckArgs),
    /// Format source to canonical form.
    Format(FormatArgs),
    /// Print the resolved data schema.
    Schema(SchemaArgs),
    /// Print the parse tree.
    Ast(AstArgs),
    /// Print the semantic IR.
    Ir(IrArgs),
    /// Run the language server over stdio.
    Lsp,
}

/// Output backend for `render` (spec §24.6).
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum RenderFormat {
    /// Deterministic SVG (the canonical backend). `.png` outputs rasterize it.
    Svg,
    /// A serializable, Canvas-drawable JSON draw list of scene primitives.
    DrawList,
    /// A PNG rasterized directly from the draw-list scene model (no SVG, no
    /// system fonts). Shapes only; text glyphs are not rendered (spec §24.6).
    Raster,
}

/// Stream/data format override for caller-provided primary data.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum DataFormatArg {
    Csv,
    Tsv,
    Json,
    Ndjson,
    Geojson,
    Topojson,
}

impl From<DataFormatArg> for Format {
    fn from(value: DataFormatArg) -> Self {
        match value {
            DataFormatArg::Csv => Format::Csv,
            DataFormatArg::Tsv => Format::Tsv,
            DataFormatArg::Json => Format::Json,
            DataFormatArg::Ndjson => Format::NdJson,
            DataFormatArg::Geojson => Format::GeoJson,
            DataFormatArg::Topojson => Format::TopoJson,
        }
    }
}

#[derive(Args)]
struct RenderArgs {
    /// Source file, or `-` for stdin.
    input: Option<String>,
    /// Inline source text. Mutually exclusive with a source file or `-`.
    #[arg(short = 'e', long = "eval", conflicts_with = "input")]
    eval: Option<String>,
    /// Output path. With `--format svg`, `.png` paths rasterize the SVG and all
    /// other paths write SVG; with `--format draw-list`, paths write JSON; with
    /// `--format raster`, paths write a PNG drawn from the scene model.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Output backend: `svg` (default), `draw-list` JSON, or `raster` PNG drawn
    /// from the scene model (spec §24.6).
    #[arg(long, value_enum, default_value_t = RenderFormat::Svg)]
    format: RenderFormat,
    /// Embed the fixed, audited interactive runtime in SVG output (spec §29.3):
    /// tooltip-on-hover and highlight-on-hover from inert mark metadata. Only
    /// affects `--format svg`; static SVG stays script-free without it.
    #[arg(long)]
    interactive: bool,
    #[arg(long)]
    width: Option<u32>,
    #[arg(long)]
    height: Option<u32>,
    /// PNG raster scale. The SVG viewport stays at --width/--height.
    #[arg(long, default_value_t = png::DEFAULT_PNG_SCALE)]
    png_scale: f32,
    /// PNG physical DPI metadata. Defaults to 96 * --png-scale.
    #[arg(long)]
    png_dpi: Option<u32>,
    #[arg(long)]
    base_dir: Option<PathBuf>,
    /// CSV data path, or `-` for stdin (overrides the chart's data argument).
    #[arg(long)]
    data: Option<String>,
    /// Explicit format for caller-provided primary data or --data paths.
    #[arg(long, value_enum)]
    data_format: Option<DataFormatArg>,
    /// Raw source variable assignment, repeated as --var key=value.
    #[arg(long = "var")]
    vars: Vec<String>,
    #[arg(long)]
    theme: Option<String>,
    /// Treat warnings as errors.
    #[arg(long)]
    strict: bool,
    #[arg(long)]
    debug_layout: bool,
    #[arg(long)]
    emit_metadata: bool,
}

#[derive(Args)]
struct CheckArgs {
    input: Option<String>,
    #[arg(short = 'e', long = "eval", conflicts_with = "input")]
    eval: Option<String>,
    #[arg(long)]
    base_dir: Option<PathBuf>,
    #[arg(long)]
    data: Option<String>,
    #[arg(long, value_enum)]
    data_format: Option<DataFormatArg>,
    #[arg(long = "var")]
    vars: Vec<String>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    strict: bool,
}

#[derive(Args)]
struct FormatArgs {
    input: Option<String>,
    /// Overwrite the input file in place.
    #[arg(long)]
    write: bool,
}

#[derive(Args)]
struct SchemaArgs {
    input: Option<String>,
    #[arg(short = 'e', long = "eval", conflicts_with = "input")]
    eval: Option<String>,
    #[arg(long)]
    base_dir: Option<PathBuf>,
    #[arg(long)]
    data: Option<String>,
    #[arg(long, value_enum)]
    data_format: Option<DataFormatArg>,
    #[arg(long = "var")]
    vars: Vec<String>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    sample_size: Option<usize>,
}

#[derive(Args)]
struct AstArgs {
    input: Option<String>,
    #[arg(short = 'e', long = "eval", conflicts_with = "input")]
    eval: Option<String>,
    #[arg(long = "var")]
    vars: Vec<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct IrArgs {
    input: Option<String>,
    #[arg(short = 'e', long = "eval", conflicts_with = "input")]
    eval: Option<String>,
    #[arg(long)]
    base_dir: Option<PathBuf>,
    #[arg(long)]
    data: Option<String>,
    #[arg(long, value_enum)]
    data_format: Option<DataFormatArg>,
    #[arg(long = "var")]
    vars: Vec<String>,
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            if !matches!(err, CliError::Diagnostics) {
                eprintln!("algraf: {err}");
            }
            if let CliError::Internal(_) = err {
                eprintln!("this is a bug; please report it with the input that triggered it");
            }
            ExitCode::from(err.exit_code() as u8)
        }
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Render(args) => render_cmd(args),
        Command::Check(args) => check_cmd(args),
        Command::Format(args) => format_cmd(args),
        Command::Schema(args) => schema_cmd(args),
        Command::Ast(args) => ast_cmd(args),
        Command::Ir(args) => ir_cmd(args),
        Command::Lsp => algraf_lsp::run_stdio()
            .map_err(|e| CliError::Internal(format!("failed to start language server: {e}"))),
    }
}

/// Read Algraf source from an inline string or a path argument (`-` or absent
/// means stdin when no inline source is supplied).
fn read_source(arg: Option<&str>, eval: Option<&str>) -> Result<(String, SourceInput), CliError> {
    if let Some(source) = eval {
        return Ok((
            source.to_string(),
            SourceInput::Inline {
                label: "<eval>".to_string(),
            },
        ));
    }

    match arg {
        None | Some("-") => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| CliError::Io(format!("failed to read source from stdin: {e}")))?;
            Ok((text, SourceInput::Stdin))
        }
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| CliError::Io(format!("failed to read {path}: {e}")))?;
            Ok((text, SourceInput::Path(PathBuf::from(path))))
        }
    }
}

fn read_template_source(
    arg: Option<&str>,
    eval: Option<&str>,
    vars: &[String],
) -> Result<(String, SourceInput), CliError> {
    let (source, input) = read_source(arg, eval)?;
    let variables = parse_variable_assignments(vars).map_err(driver_error)?;
    if variables.is_empty() {
        Ok((source, input))
    } else {
        expand_variables(&source, &variables)
            .map(|expanded| (expanded, input))
            .map_err(driver_error)
    }
}

fn driver_error(err: DriverError) -> CliError {
    match err {
        DriverError::Usage(message) => CliError::Usage(message),
        DriverError::Data { .. } | DriverError::StdinRead(_) | DriverError::StdinParse(_) => {
            CliError::Io(err.to_string())
        }
    }
}

fn render_cmd(args: RenderArgs) -> Result<(), CliError> {
    let (source, input) =
        read_template_source(args.input.as_deref(), args.eval.as_deref(), &args.vars)?;
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

    for (idx, output) in outputs.into_iter().enumerate() {
        let path = chart_output_path(args.output.as_deref(), idx, multi);
        match output {
            // Render-model raster (and any future binary backend) writes bytes.
            RenderOutput::Bytes(bytes) => match path {
                Some(path) => std::fs::write(&path, bytes).map_err(|e| {
                    CliError::Io(format!("failed to write {}: {e}", path.display()))
                })?,
                None => std::io::stdout()
                    .write_all(&bytes)
                    .map_err(|e| CliError::Io(format!("failed to write stdout: {e}")))?,
            },
            RenderOutput::Text(text) => match path {
                // The canonical PNG path rasterizes the SVG backend's output.
                Some(path) if args.format == RenderFormat::Svg && is_png_path(&path) => {
                    let png_options = png::PngOptions::new(args.png_scale, args.png_dpi)
                        .map_err(CliError::Usage)?;
                    png::write_png(text.as_bytes(), &path, png_options).map_err(|e| {
                        CliError::Io(format!("failed to write PNG {}: {e}", path.display()))
                    })?;
                }
                Some(path) => std::fs::write(&path, text).map_err(|e| {
                    CliError::Io(format!("failed to write {}: {e}", path.display()))
                })?,
                None => print!("{text}"),
            },
        }
    }
    Ok(())
}

/// One rendered chart's output: text (SVG/JSON) or binary (raster PNG).
enum RenderOutput {
    Text(String),
    Bytes(Vec<u8>),
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
        RenderFormat::Svg => {
            render_chart_svg(chart, args, input, source, label, multi).map(RenderOutput::Text)
        }
        RenderFormat::DrawList => {
            render_chart_draw_list(chart, args, input, source, label, multi).map(RenderOutput::Text)
        }
        RenderFormat::Raster => {
            render_chart_raster(chart, args, input, source, label, multi).map(RenderOutput::Bytes)
        }
    }
}

/// Everything needed to drive a render backend for one chart, with all parse,
/// data, and semantic diagnostics already reported. The shared `report` carries
/// data warnings so each backend can append its own render diagnostics.
struct RenderInputs {
    ir: ChartIr,
    frame: algraf_data::DataFrame,
    named_frames: HashMap<String, algraf_data::DataFrame>,
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
) -> Result<String, CliError> {
    let RenderInputs {
        ir,
        frame,
        named_frames,
        theme,
        cli_theme_override,
        mut report,
    } = prepare_render_inputs(chart, args, input, source, label, multi)?;

    let render = if args.interactive {
        render_interactive_with_tables
    } else {
        render_with_tables
    };
    let mut result = render(
        &ir,
        &frame,
        &named_frames,
        &theme,
        cli_theme_override.as_deref(),
    )
    .map_err(|e| CliError::Internal(e.to_string()))?;
    // Append render diagnostics to the same report (spec §23.4); they carry
    // source spans, so they print through the diagnostic renderer.
    report.extend(ReportPhase::Render, result.diagnostics.iter().cloned());
    let render_diags = report.diagnostics();
    if !render_diags.is_empty() {
        eprint!(
            "{}",
            diagnostics::render_human(source, label, &render_diags)
        );
    }
    if diagnostics::has_blocking(&render_diags, args.strict) {
        return Err(CliError::Diagnostics);
    }

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
    Ok(result.svg)
}

/// Render one chart block to a draw-list JSON string (spec §24.6).
fn render_chart_draw_list(
    chart: &ChartBlock,
    args: &RenderArgs,
    input: &SourceInput,
    source: &str,
    label: &str,
    multi: bool,
) -> Result<String, CliError> {
    let RenderInputs {
        ir,
        frame,
        named_frames,
        theme,
        cli_theme_override,
        mut report,
    } = prepare_render_inputs(chart, args, input, source, label, multi)?;

    let result = render_draw_list_with_tables(
        &ir,
        &frame,
        &named_frames,
        &theme,
        cli_theme_override.as_deref(),
    )
    .map_err(|e| CliError::Internal(e.to_string()))?;
    report.extend(ReportPhase::Render, result.diagnostics.iter().cloned());
    let render_diags = report.diagnostics();
    if !render_diags.is_empty() {
        eprint!(
            "{}",
            diagnostics::render_human(source, label, &render_diags)
        );
    }
    if diagnostics::has_blocking(&render_diags, args.strict) {
        return Err(CliError::Diagnostics);
    }
    Ok(result.draw_list.to_json())
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
) -> Result<Vec<u8>, CliError> {
    let png_options =
        png::PngOptions::new(args.png_scale, args.png_dpi).map_err(CliError::Usage)?;
    let RenderInputs {
        ir,
        frame,
        named_frames,
        theme,
        cli_theme_override,
        mut report,
    } = prepare_render_inputs(chart, args, input, source, label, multi)?;

    let result = render_raster_with_tables(
        &ir,
        &frame,
        &named_frames,
        &theme,
        cli_theme_override.as_deref(),
        png_options.scale(),
    )
    .map_err(|e| CliError::Internal(e.to_string()))?;
    report.extend(ReportPhase::Render, result.diagnostics.iter().cloned());
    let render_diags = report.diagnostics();
    if !render_diags.is_empty() {
        eprint!(
            "{}",
            diagnostics::render_human(source, label, &render_diags)
        );
    }
    if diagnostics::has_blocking(&render_diags, args.strict) {
        return Err(CliError::Diagnostics);
    }
    png::encode_pixmap(result.image.pixmap(), png_options.dpi())
        .map_err(|e| CliError::Internal(format!("PNG encoding failed: {e}")))
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
            base_dir: args.base_dir.as_deref(),
            data_override: args.data.as_deref(),
            data_format_override: args.data_format.map(Format::from),
            multi_chart: multi,
        },
    ) {
        Ok(prepared) => prepared,
        Err(
            err @ (DriverError::Data { .. }
            | DriverError::StdinRead(_)
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

    Ok(RenderInputs {
        ir,
        frame,
        named_frames,
        theme,
        cli_theme_override,
        report,
    })
}

/// Output path for chart `idx` (0-based). With a single chart the `--output`
/// path is used verbatim; with multiple charts a 1-based `-{n}` suffix is
/// inserted before the extension (`out.svg` -> `out-1.svg`, `out-2.svg`).
fn chart_output_path(base: Option<&Path>, idx: usize, multi: bool) -> Option<PathBuf> {
    let base = base?;
    if !multi {
        return Some(base.to_path_buf());
    }
    let n = idx + 1;
    let stem = base.file_stem().and_then(|s| s.to_str()).unwrap_or("chart");
    let ext = base.extension().and_then(|s| s.to_str()).unwrap_or("svg");
    Some(base.with_file_name(format!("{stem}-{n}.{ext}")))
}

fn is_png_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
}

fn check_cmd(args: CheckArgs) -> Result<(), CliError> {
    let (source, input) =
        read_template_source(args.input.as_deref(), args.eval.as_deref(), &args.vars)?;
    let parsed = parse(&source);
    let root = parsed.syntax();
    let label = input.label();

    if diagnostics::has_blocking(parsed.diagnostics(), args.strict) {
        if args.json {
            println!(
                "{}",
                diagnostics::render_json(&source, &label, parsed.diagnostics())
            );
        } else {
            eprint!(
                "{}",
                diagnostics::render_human(&source, &label, parsed.diagnostics())
            );
        }
        return Err(CliError::Diagnostics);
    }

    // Validate every chart in the document independently (spec §7.1),
    // assembling parse, load, and semantic diagnostics plus data warnings into
    // one shared report (spec §23.4).
    let charts = document_charts(&root);
    let multi = charts.len() > 1;
    let mut report = PreparationReport::new();
    report.extend(ReportPhase::Parse, parsed.diagnostics().iter().cloned());
    for chart in &charts {
        let prepared = prepare_chart_partial(
            chart,
            PrepareOptions {
                source_input: &input,
                base_dir: args.base_dir.as_deref(),
                data_override: args.data.as_deref(),
                data_format_override: args.data_format.map(Format::from),
                multi_chart: multi,
            },
        );
        for (phase, diagnostic) in prepared.report.entries() {
            report.push(*phase, diagnostic.clone());
        }
        for warning in prepared.report.data_warnings() {
            report.push_data_warning(warning.clone());
        }
    }

    // Human output lists data warnings (which carry no source span) before the
    // spanned diagnostics; JSON output carries only the spanned diagnostics.
    if !args.json {
        for warning in report.data_warnings() {
            eprintln!("warning: {}", warning.message());
        }
    }

    let diags = report.diagnostics();
    if args.json {
        println!("{}", diagnostics::render_json(&source, &label, &diags));
    } else if diags.is_empty() {
        eprintln!("no problems found");
    } else {
        eprint!("{}", diagnostics::render_human(&source, &label, &diags));
    }

    if diagnostics::has_blocking(&diags, args.strict) || (args.strict && report.has_data_warnings())
    {
        Err(CliError::Diagnostics)
    } else {
        Ok(())
    }
}

fn format_cmd(args: FormatArgs) -> Result<(), CliError> {
    let (source, input) = read_source(args.input.as_deref(), None)?;
    let formatted = format(&source);
    match (&input, args.write) {
        (SourceInput::Path(path), true) => std::fs::write(path, formatted)
            .map_err(|e| CliError::Io(format!("failed to write {}: {e}", path.display())))?,
        (SourceInput::Stdin, true) => {
            return Err(CliError::Usage(
                "--write requires a file argument, not stdin".to_string(),
            ));
        }
        _ => print!("{formatted}"),
    }
    Ok(())
}

fn schema_cmd(args: SchemaArgs) -> Result<(), CliError> {
    let (source, input) =
        read_template_source(args.input.as_deref(), args.eval.as_deref(), &args.vars)?;
    let parsed = parse(&source);
    let root = parsed.syntax();
    let label = input.label();

    let parse_diags = parsed.diagnostics();
    if diagnostics::has_blocking(parse_diags, false) {
        eprint!(
            "{}",
            diagnostics::render_human(&source, &label, parse_diags)
        );
        return Err(CliError::Diagnostics);
    }

    let ast_data = extract_data_source(&root);
    if !ast_data.is_path() && !ast_data.is_stdin() {
        let analysis = analyze(&root, &[]);
        let mut diags = parsed.diagnostics().to_vec();
        diags.extend(analysis.diagnostics);
        if !diags.is_empty() {
            eprint!("{}", diagnostics::render_human(&source, &label, &diags));
        }
        return Err(CliError::Diagnostics);
    }

    let schema = match load_schema(
        &ast_data,
        &input,
        args.base_dir.as_deref(),
        args.data.as_deref(),
        args.data_format.map(Format::from),
        args.sample_size,
    ) {
        Ok(schema) => schema,
        Err(
            err @ (DriverError::Data { .. }
            | DriverError::StdinRead(_)
            | DriverError::StdinParse(_)),
        ) => {
            let span = ast_data
                .span()
                .unwrap_or_else(|| algraf_core::Span::new(0, 0));
            let diagnostic = driver_error_diagnostic(&err, span);
            eprint!(
                "{}",
                diagnostics::render_human(&source, &label, &[diagnostic])
            );
            return Err(CliError::Diagnostics);
        }
        Err(err) => return Err(driver_error(err)),
    };

    if args.json {
        let cols: Vec<Value> = schema
            .iter()
            .map(|c| {
                json!({
                    "name": c.name,
                    "type": dtype_str(c.dtype),
                    "nullable": c.nullable,
                    "examples": c.examples,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&Value::Array(cols)).unwrap()
        );
    } else {
        println!("{:<24} {:<10} nullable  examples", "column", "type");
        for c in &schema {
            println!(
                "{:<24} {:<10} {:<8}  {}",
                c.name,
                dtype_str(c.dtype),
                c.nullable,
                c.examples.join(", "),
            );
        }
    }
    Ok(())
}

fn ast_cmd(args: AstArgs) -> Result<(), CliError> {
    let (source, _) =
        read_template_source(args.input.as_deref(), args.eval.as_deref(), &args.vars)?;
    let root = parse(&source).syntax();
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&astjson::node_to_json(&root)).unwrap()
        );
    } else {
        print!("{root:#?}");
    }
    Ok(())
}

fn ir_cmd(args: IrArgs) -> Result<(), CliError> {
    let (source, input) =
        read_template_source(args.input.as_deref(), args.eval.as_deref(), &args.vars)?;
    let parsed = parse(&source);
    let root = parsed.syntax();
    let label = input.label();

    if diagnostics::has_blocking(parsed.diagnostics(), false) {
        eprint!(
            "{}",
            diagnostics::render_human(&source, &label, parsed.diagnostics())
        );
        return Err(CliError::Diagnostics);
    }

    let Some(chart) = document_charts(&root).into_iter().next() else {
        return Err(CliError::Usage(
            "no Chart block to analyze; add Chart(data: \"file.csv\") { ... }".to_string(),
        ));
    };
    let prepared = prepare_chart(
        &chart,
        PrepareOptions {
            source_input: &input,
            base_dir: args.base_dir.as_deref(),
            data_override: args.data.as_deref(),
            data_format_override: args.data_format.map(Format::from),
            multi_chart: false,
        },
    )
    .map_err(driver_error)?;
    let analysis = prepared.analysis;
    let mut report = PreparationReport::new();
    report.extend(ReportPhase::Parse, parsed.diagnostics().iter().cloned());
    report.extend(ReportPhase::Semantic, analysis.diagnostics);
    let diags = report.diagnostics();

    if !diags.is_empty() {
        eprint!("{}", diagnostics::render_human(&source, &label, &diags));
    }
    if diagnostics::has_blocking(&diags, false) {
        return Err(CliError::Diagnostics);
    }

    match analysis.ir {
        Some(ir) => {
            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&ir_to_json(&ir)).unwrap()
                );
            } else {
                println!("{ir:#?}");
            }
            Ok(())
        }
        None => Err(CliError::Internal("analysis produced no IR".to_string())),
    }
}

fn augment_svg(
    svg: &mut String,
    ir: &ChartIr,
    theme: &Theme,
    layout: &Layout,
    diagnostic_count: usize,
    debug_layout: bool,
    emit_metadata: bool,
) {
    let mut fragment = String::new();
    if emit_metadata {
        fragment.push_str(&format!(
            "<!-- algraf metadata: width={} height={} theme={} spaces={} diagnostics={} -->\n",
            ir.width,
            ir.height,
            theme.name,
            ir.spaces.len(),
            diagnostic_count,
        ));
    }
    if debug_layout {
        fragment.push_str(&debug_layout_svg(layout));
    }
    insert_before_svg_end(svg, &fragment);
}

fn debug_layout_svg(layout: &Layout) -> String {
    let mut out = String::new();
    out.push_str("<g class=\"algraf-debug-layout\" aria-hidden=\"true\">\n");
    out.push_str(&debug_rect("svg", layout.svg, "#d62728"));
    out.push_str(&debug_rect("plot", layout.plot, "#2ca02c"));
    for (index, facet) in layout.facets.iter().enumerate() {
        out.push_str(&debug_rect(
            &format!("facet-strip-{index}"),
            facet.strip,
            "#9467bd",
        ));
        out.push_str(&debug_rect(
            &format!("facet-plot-{index}"),
            facet.plot,
            "#17becf",
        ));
    }
    if let Some(legend) = layout.legend {
        out.push_str(&debug_rect("legend", legend, "#1f77b4"));
    }
    out.push_str("</g>\n");
    out
}

fn debug_rect(name: &str, rect: Rect, stroke: &str) -> String {
    format!(
        "<rect class=\"algraf-debug-{name}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"1\" stroke-dasharray=\"4 3\" />\n",
        svg_num(rect.x),
        svg_num(rect.y),
        svg_num(rect.width),
        svg_num(rect.height),
    )
}

fn insert_before_svg_end(svg: &mut String, fragment: &str) {
    if fragment.is_empty() {
        return;
    }
    if let Some(index) = svg.rfind("</svg>") {
        svg.insert_str(index, fragment);
    } else {
        svg.push_str(fragment);
    }
}

fn ir_to_json(ir: &ChartIr) -> Value {
    json!({
        "dataSource": data_source_json(&ir.data_source),
        "width": ir.width,
        "height": ir.height,
        "layout": {
            "facetColumns": ir.layout.facet_columns,
        },
        "guides": {
            "legend": ir.guides.legend,
            "fillLegend": ir.guides.fill_legend,
            "strokeLegend": ir.guides.stroke_legend,
            "grid": ir.guides.grid,
            "xLabel": ir.guides.x_label.as_deref(),
            "yLabel": ir.guides.y_label.as_deref(),
            "xTimeFormat": ir.guides.x_time_format.as_ref().map(|format| format.as_str()),
            "yTimeFormat": ir.guides.y_time_format.as_ref().map(|format| format.as_str()),
            "xTickLabelAngle": ir.guides.x_tick_label_angle,
            "yTickLabelAngle": ir.guides.y_tick_label_angle,
        },
        "scales": ir.scales.iter().map(scale_json).collect::<Vec<_>>(),
        "title": ir.title.as_deref(),
        "subtitle": ir.subtitle.as_deref(),
        "caption": ir.caption.as_deref(),
        "tables": ir.tables.iter().map(|t| json!({
            "name": t.name,
            "path": t.path,
            "query": t.query.as_deref(),
            "span": span_json(t.span),
        })).collect::<Vec<_>>(),
        "derivedTables": ir.derived_tables.iter().map(derive_json).collect::<Vec<_>>(),
        "spaces": ir.spaces.iter().map(space_json).collect::<Vec<_>>(),
    })
}

fn data_source_json(data_source: &DataSourceIr) -> Value {
    match data_source {
        DataSourceIr::Path(path) => json!({ "kind": "path", "path": path }),
        DataSourceIr::GeoJson(path) => json!({ "kind": "geojson", "path": path }),
        DataSourceIr::Shapefile(path) => json!({ "kind": "shapefile", "path": path }),
        DataSourceIr::Sqlite { path, query } => {
            json!({ "kind": "sqlite", "path": path, "query": query })
        }
        DataSourceIr::TopoJson { path, object } => {
            json!({ "kind": "topojson", "path": path, "object": object })
        }
        DataSourceIr::Stdin => json!({ "kind": "stdin" }),
        DataSourceIr::Missing => json!({ "kind": "missing" }),
    }
}

fn derive_json(derive: &DeriveIr) -> Value {
    json!({
        "name": derive.name,
        "stat": {
            "kind": stat_kind_str(derive.stat.kind),
            "input": frame_json(&derive.stat.input),
            "options": stat_options_json(&derive.stat.options),
            "span": span_json(derive.stat.span),
        },
        "outputSchema": derive.output_schema.iter().map(|c| {
            json!({ "name": c.name, "type": dtype_str(c.dtype) })
        }).collect::<Vec<_>>(),
        "span": span_json(derive.span),
    })
}

fn space_json(space: &SpaceIr) -> Value {
    json!({
        "data": space_data_json(&space.data),
        "frame": frame_json(&space.frame),
        "guides": guide_overrides_json(&space.guides),
        "scales": space.scales.iter().map(scale_json).collect::<Vec<_>>(),
        "geometries": space.geometries.iter().map(geometry_json).collect::<Vec<_>>(),
        "span": span_json(space.span),
    })
}

fn guide_overrides_json(guides: &GuideOverridesIr) -> Value {
    json!({
        "legend": guides.legend,
        "fillLegend": guides.fill_legend,
        "strokeLegend": guides.stroke_legend,
        "grid": guides.grid,
        "xLabel": guides.x_label.as_deref(),
        "yLabel": guides.y_label.as_deref(),
        "xTimeFormat": guides.x_time_format.as_ref().map(|format| format.as_str()),
        "yTimeFormat": guides.y_time_format.as_ref().map(|format| format.as_str()),
        "xTickLabelAngle": guides.x_tick_label_angle,
        "yTickLabelAngle": guides.y_tick_label_angle,
    })
}

fn scale_json(scale: &ScaleIr) -> Value {
    json!({
        "target": scale_target_json(&scale.target),
        "type": scale.scale_type.map(scale_type_str),
        "domain": scale.domain,
        "reverse": scale.reverse,
        "palette": scale.palette.as_deref(),
        "gradient": scale.gradient.as_ref().map(gradient_json),
        "span": span_json(scale.span),
    })
}

fn gradient_json(gradient: &GradientIr) -> Value {
    match gradient {
        GradientIr::Even(stops) => json!({
            "kind": "even",
            "stops": stops,
        }),
        GradientIr::Positioned(stops) => json!({
            "kind": "positioned",
            "stops": stops.iter().map(|stop| {
                json!({ "value": stop.value, "color": stop.color })
            }).collect::<Vec<_>>(),
        }),
    }
}

fn scale_target_json(target: &ScaleTargetIr) -> Value {
    match target {
        ScaleTargetIr::Axis(axis) => json!({
            "kind": "axis",
            "axis": axis.as_str(),
        }),
        ScaleTargetIr::Aesthetic { aesthetic, column } => json!({
            "kind": "aesthetic",
            "aesthetic": aesthetic,
            "column": column.as_ref().map(column_json),
        }),
    }
}

fn scale_type_str(scale_type: ScaleTypeIr) -> &'static str {
    scale_type.as_str()
}

fn space_data_json(data: &SpaceDataRef) -> Value {
    match data {
        SpaceDataRef::Primary => json!({ "kind": "primary" }),
        SpaceDataRef::Derived(name) => json!({ "kind": "derived", "name": name }),
        SpaceDataRef::Table(name) => json!({ "kind": "table", "name": name }),
    }
}

fn geometry_json(geometry: &GeometryIr) -> Value {
    json!({
        "kind": geometry_kind_str(geometry.kind),
        "mappings": geometry.mappings.iter().map(mapping_json).collect::<Vec<_>>(),
        "settings": geometry.settings.iter().map(|s| {
            json!({ "name": s.name.as_str(), "value": setting_value_json(&s.value) })
        }).collect::<Vec<_>>(),
        "interaction": interaction_json(&geometry.interaction),
        "span": span_json(geometry.span),
    })
}

fn interaction_json(interaction: &algraf_semantics::InteractionIr) -> Value {
    json!({
        "tooltip": interaction.tooltip.iter().map(column_json).collect::<Vec<_>>(),
        "highlight": interaction.highlight.as_ref().map(column_json),
    })
}

fn mapping_json(mapping: &AestheticMapping) -> Value {
    json!({
        "aesthetic": mapping.aesthetic.as_str(),
        "column": column_json(&mapping.column),
    })
}

fn frame_json(frame: &FrameIr) -> Value {
    match frame {
        FrameIr::Vector(column) => json!({ "kind": "vector", "column": column_json(column) }),
        FrameIr::Cartesian(parts) => {
            json!({ "kind": "cartesian", "terms": parts.iter().map(frame_json).collect::<Vec<_>>() })
        }
        FrameIr::Nested { outer, inner } => {
            json!({ "kind": "nested", "outer": frame_json(outer), "inner": frame_json(inner) })
        }
        FrameIr::Union(parts) => {
            json!({ "kind": "union", "terms": parts.iter().map(frame_json).collect::<Vec<_>>() })
        }
        FrameIr::Invalid => json!({ "kind": "invalid" }),
    }
}

fn column_json(column: &ColumnRef) -> Value {
    json!({
        "name": column.name,
        "type": dtype_str(column.dtype),
        "span": span_json(column.span),
    })
}

fn setting_value_json(value: &SettingValue) -> Value {
    match value {
        SettingValue::Number(n) => json!({ "kind": "number", "value": n }),
        SettingValue::String(s) => json!({ "kind": "string", "value": s }),
        SettingValue::Bool(b) => json!({ "kind": "bool", "value": b }),
        SettingValue::Null => json!({ "kind": "null" }),
        SettingValue::NumberArray(values) => json!({ "kind": "numberArray", "value": values }),
    }
}

fn span_json(span: algraf_core::Span) -> Value {
    json!({ "start": span.start, "end": span.end })
}

fn stat_options_json(options: &StatOptionsIr) -> Value {
    match options {
        StatOptionsIr::Bin {
            bins,
            bin_width,
            boundary,
            closed,
            interval,
        } => json!({
            "kind": "bin",
            "bins": bins,
            "binWidth": bin_width,
            "boundary": boundary,
            "closed": closed.as_str(),
            "interval": interval.map(|unit| unit.as_str()),
        }),
        StatOptionsIr::Bin2D { bins } => json!({ "kind": "bin2d", "bins": bins }),
        StatOptionsIr::HexBin { bins } => json!({ "kind": "hexbin", "bins": bins }),
        StatOptionsIr::Smooth { method, span, se } => json!({
            "kind": "smooth",
            "method": method.as_str(),
            "span": span,
            "se": se,
        }),
        StatOptionsIr::Density {
            bandwidth,
            grid_points,
        } => json!({
            "kind": "density",
            "bandwidth": bandwidth,
            "gridPoints": grid_points,
        }),
        StatOptionsIr::Count => json!({ "kind": "count" }),
        StatOptionsIr::Centroid => json!({ "kind": "centroid" }),
        StatOptionsIr::Simplify { tolerance } => {
            json!({ "kind": "simplify", "tolerance": tolerance })
        }
        StatOptionsIr::SpatialJoin { table, predicate } => json!({
            "kind": "spatialJoin",
            "table": table,
            "predicate": match predicate {
                algraf_semantics::SpatialPredicateIr::Within => "within",
            },
        }),
    }
}

fn stat_kind_str(kind: StatKind) -> &'static str {
    kind.display_name()
}

fn geometry_kind_str(kind: GeometryKind) -> &'static str {
    kind.display_name()
}

fn dtype_str(dtype: DataType) -> &'static str {
    match dtype {
        DataType::Boolean => "boolean",
        DataType::Integer => "integer",
        DataType::Float => "float",
        DataType::Temporal => "temporal",
        DataType::String => "string",
        DataType::Geometry => "geometry",
        DataType::Mixed => "mixed",
        DataType::Unknown => "unknown",
    }
}
