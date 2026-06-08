//! `algraf check` — parse and analyze without rendering (spec §22).

use std::path::PathBuf;

use algraf_data::Format;
use algraf_driver::{
    document_charts, prepare_chart_partial, PreparationReport, PrepareOptions, ReportPhase,
};
use algraf_syntax::parse;
use clap::Args;

use crate::cmd_render::DataFormatArg;
use crate::diagnostics;
use crate::error::CliError;
use crate::input::read_template_source;

#[derive(Args)]
pub(crate) struct CheckArgs {
    pub(crate) input: Option<String>,
    #[arg(short = 'e', long = "eval", conflicts_with = "input")]
    pub(crate) eval: Option<String>,
    #[arg(long)]
    pub(crate) base_dir: Option<PathBuf>,
    #[arg(long)]
    pub(crate) data: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) data_format: Option<DataFormatArg>,
    #[arg(long = "var")]
    pub(crate) vars: Vec<String>,
    #[arg(long)]
    pub(crate) json: bool,
    #[arg(long)]
    pub(crate) strict: bool,
}

pub(crate) fn check_cmd(args: CheckArgs) -> Result<(), CliError> {
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
