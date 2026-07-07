//! `algraf schema` — print the resolved data schema (spec §22).

use algraf_data::Format;
use algraf_driver::{driver_error_diagnostic, extract_data_source, load_schema, DriverError};
use algraf_semantics::analyze;
use algraf_syntax::parse;
use clap::Args;
use serde_json::{json, Value};

use crate::cmd_source::SourceArgs;
use crate::diagnostics;
use crate::error::CliError;
use crate::input::{driver_error, read_template_source};
use crate::ir_json::dtype_str;

#[derive(Args)]
pub(crate) struct SchemaArgs {
    #[command(flatten)]
    pub(crate) source: SourceArgs,
    #[arg(long)]
    pub(crate) json: bool,
    #[arg(long)]
    pub(crate) sample_size: Option<usize>,
}

pub(crate) fn schema_cmd(args: SchemaArgs) -> Result<(), CliError> {
    let (source, input) = read_template_source(
        args.source.input.as_deref(),
        args.source.eval.as_deref(),
        &args.source.vars,
    )?;
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
        args.source.base_dir.as_deref(),
        args.source.data.as_deref(),
        args.source.data_format.map(Format::from),
        args.sample_size,
    ) {
        Ok(schema) => schema,
        Err(
            err @ (DriverError::Data { .. }
            | DriverError::StdinRead(_)
            | DriverError::StdinData { .. }
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
