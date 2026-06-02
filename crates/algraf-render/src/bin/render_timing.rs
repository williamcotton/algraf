use std::collections::HashMap;
use std::env;
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::process;
use std::str::FromStr;
use std::time::{Duration, Instant};

use algraf_core::{Diagnostic, Severity};
use algraf_data::Format;
use algraf_driver::{
    document_charts, parse_source, prepare_chart_with_io, DriverIo, OsDriverIo, PrepareOptions,
    SourceInput,
};
use algraf_render::{
    load_image_assets_with_io, render_interactive_with_tables_and_assets_and_limits,
    render_with_tables_and_assets_and_limits, InMemoryDriverIo, RenderLimits, Theme,
};

const DEFAULT_ITERATIONS: usize = 100;
const DEFAULT_WARMUP: usize = 10;

fn main() {
    let args = match Args::parse(env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            eprintln!();
            eprintln!("{}", usage());
            process::exit(2);
        }
    };

    if let Err(err) = run(args) {
        eprintln!("render-timing: {err}");
        process::exit(1);
    }
}

#[derive(Debug)]
struct Args {
    source: PathBuf,
    input: Option<PathBuf>,
    data_format: Option<Format>,
    iterations: usize,
    warmup: usize,
    interactive: bool,
}

impl Args {
    fn parse(iter: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut source = None;
        let mut input = None;
        let mut data_format = None;
        let mut iterations = DEFAULT_ITERATIONS;
        let mut warmup = DEFAULT_WARMUP;
        let mut interactive = false;

        let mut iter = iter.peekable();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-h" | "--help" => {
                    println!("{}", usage());
                    process::exit(0);
                }
                "--input" => {
                    input = Some(PathBuf::from(value_after(&arg, iter.next())?));
                }
                "--data-format" => {
                    let value = value_after(&arg, iter.next())?;
                    data_format = Some(Format::from_str(&value).map_err(|message| {
                        format!("invalid --data-format `{value}`: {message}")
                    })?);
                }
                "--iterations" => {
                    iterations = parse_count(&arg, iter.next())?;
                }
                "--warmup" => {
                    warmup = parse_count(&arg, iter.next())?;
                }
                "--interactive" => {
                    interactive = true;
                }
                other if other.starts_with('-') => {
                    return Err(format!("unknown option `{other}`"));
                }
                path => {
                    if source.replace(PathBuf::from(path)).is_some() {
                        return Err("only one source path may be supplied".to_string());
                    }
                }
            }
        }

        let source = source.ok_or_else(|| "missing source path".to_string())?;
        if iterations == 0 {
            return Err("--iterations must be greater than zero".to_string());
        }

        Ok(Args {
            source,
            input,
            data_format,
            iterations,
            warmup,
            interactive,
        })
    }
}

fn value_after(option: &str, value: Option<String>) -> Result<String, String> {
    value.ok_or_else(|| format!("{option} requires a value"))
}

fn parse_count(option: &str, value: Option<String>) -> Result<usize, String> {
    let value = value_after(option, value)?;
    value
        .parse::<usize>()
        .map_err(|_| format!("{option} expects a positive integer, got `{value}`"))
}

fn usage() -> &'static str {
    "usage: render-timing <chart.ag> [--input data.json] [--data-format json] [--interactive] [--warmup n] [--iterations n]"
}

#[derive(Debug, Clone, Copy)]
struct Sample {
    parse: Duration,
    prepare: Duration,
    render: Duration,
    total: Duration,
    svg_bytes: usize,
}

fn run(args: Args) -> Result<(), String> {
    let source = fs::read_to_string(&args.source)
        .map_err(|err| format!("failed to read {}: {err}", args.source.display()))?;
    let source_input = SourceInput::Path(args.source.clone());
    let base_dir = args.source.parent();
    let input_bytes = match &args.input {
        Some(path) => Some(
            fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?,
        ),
        None => None,
    };

    let mut samples = Vec::with_capacity(args.iterations);
    let total_runs = args.warmup + args.iterations;
    for run_index in 0..total_runs {
        let io: Box<dyn DriverIo> = match &input_bytes {
            Some(bytes) => Box::new(InMemoryDriverIo::new(bytes.clone())),
            None => Box::new(OsDriverIo),
        };
        let sample = render_once(
            &source,
            &source_input,
            base_dir,
            args.data_format,
            args.interactive,
            io.as_ref(),
        )?;
        black_box(sample.svg_bytes);
        if run_index >= args.warmup {
            samples.push(sample);
        }
    }

    print_report(&args, &samples);
    Ok(())
}

fn render_once(
    source: &str,
    source_input: &SourceInput,
    base_dir: Option<&std::path::Path>,
    data_format: Option<Format>,
    interactive: bool,
    io: &dyn DriverIo,
) -> Result<Sample, String> {
    let total_start = Instant::now();

    let parse_start = Instant::now();
    let parsed = parse_source(source);
    fail_on_errors("parse", parsed.diagnostics())?;
    let root = parsed.syntax();
    let charts = document_charts(&root);
    if charts.len() != 1 {
        return Err(format!(
            "render-timing expects exactly one Chart block, found {}",
            charts.len()
        ));
    }
    let parse = parse_start.elapsed();

    let prepare_start = Instant::now();
    let prepared = prepare_chart_with_io(
        &charts[0],
        PrepareOptions {
            source_input,
            base_dir,
            data_override: None,
            data_format_override: data_format,
            multi_chart: false,
        },
        io,
    )
    .map_err(|err| format!("prepare failed: {err}"))?;
    fail_on_errors("semantic", &prepared.analysis.diagnostics)?;
    let prepare = prepare_start.elapsed();

    let render_start = Instant::now();
    let primary = prepared
        .primary
        .ok_or_else(|| "analysis allowed rendering with a missing data source".to_string())?;
    let mut ir = prepared
        .analysis
        .ir
        .ok_or_else(|| "analysis produced no IR".to_string())?;
    let theme = match &ir.theme {
        Some(theme_ir) => Theme::from_ir(theme_ir),
        None => Theme::default(),
    };
    let named_frames = prepared
        .named_tables
        .into_iter()
        .map(|table| (table.name, table.frame))
        .collect::<HashMap<_, _>>();
    let image_assets = load_image_assets_with_io(
        &ir,
        &primary.frame,
        &named_frames,
        source_input,
        base_dir,
        io,
    );
    fail_on_errors("image assets", &image_assets.diagnostics)?;

    // Keep dimensions stable if later callers decide to mutate the IR in this
    // harness; today this is just the analyzed chart dimensions.
    ir.width = black_box(ir.width);
    ir.height = black_box(ir.height);

    let result = if interactive {
        render_interactive_with_tables_and_assets_and_limits(
            &ir,
            &primary.frame,
            &named_frames,
            &theme,
            None,
            &image_assets.assets,
            RenderLimits::default(),
        )
    } else {
        render_with_tables_and_assets_and_limits(
            &ir,
            &primary.frame,
            &named_frames,
            &theme,
            None,
            &image_assets.assets,
            RenderLimits::default(),
        )
    }
    .map_err(|err| format!("render failed: {err}"))?;
    fail_on_errors("render", &result.diagnostics)?;
    let svg_bytes = result.svg.len();
    let render = render_start.elapsed();

    Ok(Sample {
        parse,
        prepare,
        render,
        total: total_start.elapsed(),
        svg_bytes,
    })
}

fn fail_on_errors(phase: &str, diagnostics: &[Diagnostic]) -> Result<(), String> {
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| format!("{} {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{phase} diagnostics blocked timing: {}",
            errors.join("; ")
        ))
    }
}

fn print_report(args: &Args, samples: &[Sample]) {
    let svg_bytes = samples.last().map(|sample| sample.svg_bytes).unwrap_or(0);
    println!("Algraf render timing");
    println!("source: {}", args.source.display());
    if let Some(input) = &args.input {
        println!("input: {}", input.display());
    }
    if let Some(format) = args.data_format {
        println!("data_format: {format:?}");
    }
    println!("interactive: {}", args.interactive);
    println!("warmup: {}", args.warmup);
    println!("iterations: {}", args.iterations);
    println!("svg_bytes: {svg_bytes}");
    println!();
    println!("phase\tmin_ms\tmedian_ms\tmean_ms\tp95_ms\tmax_ms");
    print_phase("parse", samples.iter().map(|sample| sample.parse));
    print_phase("prepare", samples.iter().map(|sample| sample.prepare));
    print_phase("render", samples.iter().map(|sample| sample.render));
    print_phase("total", samples.iter().map(|sample| sample.total));
}

fn print_phase(name: &str, values: impl Iterator<Item = Duration>) {
    let mut nanos = values
        .map(|duration| duration.as_nanos())
        .collect::<Vec<_>>();
    nanos.sort_unstable();
    let sum = nanos.iter().copied().sum::<u128>();
    let mean = sum as f64 / nanos.len() as f64;
    println!(
        "{name}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}",
        millis(nanos[0]),
        millis(percentile(&nanos, 50.0)),
        millis_f64(mean),
        millis(percentile(&nanos, 95.0)),
        millis(*nanos.last().unwrap())
    );
}

fn percentile(sorted: &[u128], percentile: f64) -> u128 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = (percentile / 100.0) * (sorted.len() - 1) as f64;
    sorted[rank.round() as usize]
}

fn millis(nanos: u128) -> f64 {
    millis_f64(nanos as f64)
}

fn millis_f64(nanos: f64) -> f64 {
    nanos / 1_000_000.0
}
