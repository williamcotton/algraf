//! `algraf format` — canonical source formatting (spec §22).

use algraf_driver::SourceInput;
use algraf_syntax::format;
use clap::Args;

use crate::error::CliError;
use crate::input::read_source;

#[derive(Args)]
pub(crate) struct FormatArgs {
    pub(crate) input: Option<String>,
    /// Overwrite the input file in place.
    #[arg(long)]
    pub(crate) write: bool,
}

pub(crate) fn format_cmd(args: FormatArgs) -> Result<(), CliError> {
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
