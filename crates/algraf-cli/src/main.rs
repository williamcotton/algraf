//! The `algraf` binary: argument parsing, command dispatch, and I/O (spec §22).

mod astjson;
mod diagnostics;
mod error;
mod input;
mod png;

use std::path::PathBuf;
use std::process::ExitCode;

use algraf_data::{ColumnDef, DataType, Table};
use algraf_render::{render, Layout, Rect, Theme};
use algraf_semantics::{
    analyze, AestheticMapping, ChartIr, ColumnRef, DataSourceIr, DeriveIr, FrameIr, GeometryIr,
    GeometryKind, SettingValue, SpaceDataRef, SpaceIr, StatKind,
};
use algraf_syntax::{format, parse};
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};

use crate::error::CliError;
use crate::input::{
    extract_data_source, load_data, load_schema, read_source, AstData, SourceInput,
};

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
    /// Render a chart to SVG or PNG.
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

#[derive(Args)]
struct RenderArgs {
    /// Source file, or `-` for stdin.
    input: Option<String>,
    /// Output path. `.png` paths rasterize to PNG; all other paths write SVG.
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    width: Option<u32>,
    #[arg(long)]
    height: Option<u32>,
    #[arg(long)]
    base_dir: Option<PathBuf>,
    /// CSV data path, or `-` for stdin (overrides the chart's data argument).
    #[arg(long)]
    data: Option<String>,
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
    #[arg(long)]
    base_dir: Option<PathBuf>,
    #[arg(long)]
    data: Option<String>,
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
    #[arg(long)]
    base_dir: Option<PathBuf>,
    #[arg(long)]
    data: Option<String>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    sample_size: Option<usize>,
}

#[derive(Args)]
struct AstArgs {
    input: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct IrArgs {
    input: Option<String>,
    #[arg(long)]
    base_dir: Option<PathBuf>,
    #[arg(long)]
    data: Option<String>,
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
        Command::Lsp => Err(CliError::Internal(
            "the `lsp` command is not yet implemented in this build".to_string(),
        )),
    }
}

fn render_cmd(args: RenderArgs) -> Result<(), CliError> {
    let (source, input) = read_source(args.input.as_deref())?;
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

    let ast_data = extract_data_source(&root);
    let loaded = if matches!(ast_data, AstData::Missing) {
        None
    } else {
        Some(load_data(
            &ast_data,
            &input,
            args.base_dir.as_deref(),
            args.data.as_deref(),
        )?)
    };
    let schema = loaded
        .as_ref()
        .map(|l| l.frame.schema())
        .unwrap_or(&[] as &[ColumnDef]);

    let analysis = analyze(&root, schema);
    let mut diags = parsed.diagnostics().to_vec();
    diags.extend(analysis.diagnostics);

    if diagnostics::has_blocking(&diags, args.strict) {
        eprint!("{}", diagnostics::render_human(&source, &label, &diags));
        return Err(CliError::Diagnostics);
    }
    // Non-blocking diagnostics (warnings, hints) still print to stderr.
    if !diags.is_empty() {
        eprint!("{}", diagnostics::render_human(&source, &label, &diags));
    }

    let loaded = loaded.ok_or_else(|| {
        CliError::Internal("analysis allowed rendering with a missing data source".to_string())
    })?;
    for warning in &loaded.warnings {
        eprintln!("warning: {}", warning.message);
    }
    if args.strict && !loaded.warnings.is_empty() {
        return Err(CliError::Diagnostics);
    }

    let frame = loaded.frame;

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
    // applies space-local theme overrides from the IR on top of this base.
    let theme = args
        .theme
        .clone()
        .or_else(|| ir.theme.clone())
        .map(|name| Theme::by_name(&name))
        .unwrap_or_default();
    let cli_theme_override = args.theme.clone();

    let mut result = render(&ir, &frame, &theme, cli_theme_override.as_deref())
        .map_err(|e| CliError::Internal(e.to_string()))?;
    if !result.diagnostics.is_empty() {
        eprint!(
            "{}",
            diagnostics::render_human(&source, &label, &result.diagnostics)
        );
    }
    if diagnostics::has_blocking(&result.diagnostics, args.strict) {
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

    match args.output {
        Some(path) if is_png_path(&path) => png::write_png(result.svg.as_bytes(), &path)
            .map_err(|e| CliError::Io(format!("failed to write PNG {}: {e}", path.display())))?,
        Some(path) => std::fs::write(&path, result.svg)
            .map_err(|e| CliError::Io(format!("failed to write {}: {e}", path.display())))?,
        None => print!("{}", result.svg),
    }
    Ok(())
}

fn is_png_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
}

fn check_cmd(args: CheckArgs) -> Result<(), CliError> {
    let (source, input) = read_source(args.input.as_deref())?;
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

    let ast_data = extract_data_source(&root);
    let loaded = if matches!(ast_data, AstData::Missing) {
        None
    } else {
        Some(load_data(
            &ast_data,
            &input,
            args.base_dir.as_deref(),
            args.data.as_deref(),
        )?)
    };
    let schema = loaded
        .as_ref()
        .map(|l| l.frame.schema())
        .unwrap_or(&[] as &[ColumnDef]);

    let analysis = analyze(&root, schema);
    let mut diags = parsed.diagnostics().to_vec();
    diags.extend(analysis.diagnostics);

    if args.json {
        println!("{}", diagnostics::render_json(&source, &label, &diags));
    } else if diags.is_empty() {
        eprintln!("no problems found");
    } else {
        eprint!("{}", diagnostics::render_human(&source, &label, &diags));
    }
    if !args.json {
        if let Some(loaded) = &loaded {
            for warning in &loaded.warnings {
                eprintln!("warning: {}", warning.message);
            }
        }
    }

    let data_warnings_block = args.strict
        && loaded
            .as_ref()
            .is_some_and(|loaded| !loaded.warnings.is_empty());
    if diagnostics::has_blocking(&diags, args.strict) || data_warnings_block {
        Err(CliError::Diagnostics)
    } else {
        Ok(())
    }
}

fn format_cmd(args: FormatArgs) -> Result<(), CliError> {
    let (source, input) = read_source(args.input.as_deref())?;
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
    let (source, input) = read_source(args.input.as_deref())?;
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
    if matches!(ast_data, AstData::Missing) {
        let analysis = analyze(&root, &[]);
        let mut diags = parsed.diagnostics().to_vec();
        diags.extend(analysis.diagnostics);
        if !diags.is_empty() {
            eprint!("{}", diagnostics::render_human(&source, &label, &diags));
        }
        return Err(CliError::Diagnostics);
    }

    let schema = load_schema(
        &ast_data,
        &input,
        args.base_dir.as_deref(),
        args.data.as_deref(),
        args.sample_size,
    )?;

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
    let (source, _) = read_source(args.input.as_deref())?;
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
    let (source, input) = read_source(args.input.as_deref())?;
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

    let ast_data = extract_data_source(&root);
    let loaded = if matches!(ast_data, AstData::Missing) {
        None
    } else {
        Some(load_data(
            &ast_data,
            &input,
            args.base_dir.as_deref(),
            args.data.as_deref(),
        )?)
    };
    let schema = loaded
        .as_ref()
        .map(|l| l.frame.schema())
        .unwrap_or(&[] as &[ColumnDef]);
    let analysis = analyze(&root, schema);
    let mut diags = parsed.diagnostics().to_vec();
    diags.extend(analysis.diagnostics);

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

fn svg_num(value: f64) -> String {
    let mut out = format!("{value:.3}");
    while out.contains('.') && out.ends_with('0') {
        out.pop();
    }
    if out.ends_with('.') {
        out.pop();
    }
    if out == "-0" {
        "0".to_string()
    } else {
        out
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
        },
        "title": ir.title.as_deref(),
        "subtitle": ir.subtitle.as_deref(),
        "caption": ir.caption.as_deref(),
        "derivedTables": ir.derived_tables.iter().map(derive_json).collect::<Vec<_>>(),
        "spaces": ir.spaces.iter().map(space_json).collect::<Vec<_>>(),
    })
}

fn data_source_json(data_source: &DataSourceIr) -> Value {
    match data_source {
        DataSourceIr::Path(path) => json!({ "kind": "path", "path": path }),
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
            "settings": derive.stat.settings.iter().map(|s| {
                json!({ "name": s.name, "value": setting_value_json(&s.value) })
            }).collect::<Vec<_>>(),
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
        "geometries": space.geometries.iter().map(geometry_json).collect::<Vec<_>>(),
        "span": span_json(space.span),
    })
}

fn space_data_json(data: &SpaceDataRef) -> Value {
    match data {
        SpaceDataRef::Primary => json!({ "kind": "primary" }),
        SpaceDataRef::Derived(name) => json!({ "kind": "derived", "name": name }),
    }
}

fn geometry_json(geometry: &GeometryIr) -> Value {
    json!({
        "kind": geometry_kind_str(geometry.kind),
        "mappings": geometry.mappings.iter().map(mapping_json).collect::<Vec<_>>(),
        "settings": geometry.settings.iter().map(|s| {
            json!({ "name": s.name, "value": setting_value_json(&s.value) })
        }).collect::<Vec<_>>(),
        "span": span_json(geometry.span),
    })
}

fn mapping_json(mapping: &AestheticMapping) -> Value {
    json!({
        "aesthetic": mapping.aesthetic,
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

fn stat_kind_str(kind: StatKind) -> &'static str {
    match kind {
        StatKind::Bin => "Bin",
        StatKind::Count => "Count",
        StatKind::Smooth => "Smooth",
        StatKind::Boxplot => "Boxplot",
    }
}

fn geometry_kind_str(kind: GeometryKind) -> &'static str {
    match kind {
        GeometryKind::Point => "Point",
        GeometryKind::Line => "Line",
        GeometryKind::Bar => "Bar",
        GeometryKind::Rect => "Rect",
        GeometryKind::Histogram => "Histogram",
        GeometryKind::Smooth => "Smooth",
        GeometryKind::Boxplot => "Boxplot",
        GeometryKind::Violin => "Violin",
        GeometryKind::Ribbon => "Ribbon",
        GeometryKind::Tile => "Tile",
        GeometryKind::HLine => "HLine",
        GeometryKind::VLine => "VLine",
        GeometryKind::Rug => "Rug",
        GeometryKind::Area => "Area",
        GeometryKind::Text => "Text",
        GeometryKind::Segment => "Segment",
    }
}

fn dtype_str(dtype: DataType) -> &'static str {
    match dtype {
        DataType::Boolean => "boolean",
        DataType::Integer => "integer",
        DataType::Float => "float",
        DataType::Temporal => "temporal",
        DataType::String => "string",
        DataType::Mixed => "mixed",
        DataType::Unknown => "unknown",
    }
}
