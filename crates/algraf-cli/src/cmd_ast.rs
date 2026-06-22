//! `algraf ast` — print the parse tree (spec §22).

use algraf_syntax::{ast_json, parse};
use clap::Args;

use crate::error::CliError;
use crate::input::read_template_source;

#[derive(Args)]
pub(crate) struct AstArgs {
    pub(crate) input: Option<String>,
    #[arg(short = 'e', long = "eval", conflicts_with = "input")]
    pub(crate) eval: Option<String>,
    #[arg(long = "var")]
    pub(crate) vars: Vec<String>,
    #[arg(long)]
    pub(crate) json: bool,
}

pub(crate) fn ast_cmd(args: AstArgs) -> Result<(), CliError> {
    let (source, _) =
        read_template_source(args.input.as_deref(), args.eval.as_deref(), &args.vars)?;
    let root = parse(&source).syntax();
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&ast_json::node_to_json(&root)).unwrap()
        );
    } else {
        print!("{root:#?}");
    }
    Ok(())
}
