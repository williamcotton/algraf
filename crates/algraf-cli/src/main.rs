//! The `algraf` binary: argument parsing, command dispatch, and I/O (spec §22).
//!
//! `main.rs` is a thin dispatcher. Each subcommand lives in its own sibling
//! module, alongside shared helpers:
//!
//! - `cmd_render` — `render`: chart -> SVG, draw-list JSON, or raster PNG.
//! - `cmd_check` — `check`: parse + analyze without rendering.
//! - `cmd_format` — `format`: canonical source formatting.
//! - `cmd_schema` — `schema`: print the resolved data schema.
//! - `cmd_ast` — `ast`: print the parse tree.
//! - `cmd_ir` — `ir`: print the semantic IR.
//! - `cmd_lsp` — `lsp`: run the language server over stdio.
//! - `cmd_init` — `init`: create project-level agent guidance files.
//! - `input` — source reading + `--var` template expansion.
//! - `io` — output-path resolution and the render-output writer.
//! - `svg_debug` — `--debug-layout` and `--emit-metadata` SVG augmentation.
//! - `ir_json` — JSON serialization for `ir --json` and `schema --json`.
//! - `diagnostics`, `error`, `png` — shared low-level helpers.

mod cmd_ast;
mod cmd_check;
mod cmd_format;
mod cmd_init;
mod cmd_ir;
mod cmd_lsp;
mod cmd_render;
mod cmd_schema;
mod cmd_source;
mod diagnostics;
mod error;
mod input;
mod io;
mod ir_json;
mod png;
mod svg_debug;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::cmd_ast::{ast_cmd, AstArgs};
use crate::cmd_check::{check_cmd, CheckArgs};
use crate::cmd_format::{format_cmd, FormatArgs};
use crate::cmd_init::{init_cmd, InitArgs};
use crate::cmd_ir::{ir_cmd, IrArgs};
use crate::cmd_lsp::lsp_cmd;
use crate::cmd_render::{render_cmd, RenderArgs};
use crate::cmd_schema::{schema_cmd, SchemaArgs};
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
    /// Create project-level agent guidance files.
    Init(InitArgs),
    /// Run the language server over stdio.
    Lsp,
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
        Command::Init(args) => init_cmd(args),
        Command::Lsp => lsp_cmd(),
    }
}
