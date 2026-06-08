//! `algraf ir` — print the semantic IR (spec §22).

use std::path::PathBuf;

use algraf_data::Format;
use algraf_driver::{
    document_charts, prepare_chart, PreparationReport, PrepareOptions, ReportPhase,
};
use algraf_syntax::parse;
use clap::Args;

use crate::cmd_render::DataFormatArg;
use crate::diagnostics;
use crate::error::CliError;
use crate::input::{driver_error, read_template_source};
use crate::ir_json::ir_to_json;

#[derive(Args)]
pub(crate) struct IrArgs {
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
}

pub(crate) fn ir_cmd(args: IrArgs) -> Result<(), CliError> {
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
