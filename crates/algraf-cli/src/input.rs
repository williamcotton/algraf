//! Source reading helpers shared by every subcommand.
//!
//! Each subcommand needs to turn an `Option<&str>` argument (a path, `-` for
//! stdin, or absent) plus an optional inline `--eval` string into the actual
//! source text and a `SourceInput` label used for diagnostics. Variable
//! substitution via repeated `--var key=value` flags is applied here as well.

use std::io::Read;
use std::path::PathBuf;

use algraf_driver::{expand_variables, parse_variable_assignments, DriverError, SourceInput};

use crate::error::CliError;

/// Read Algraf source from an inline string or a path argument (`-` or absent
/// means stdin when no inline source is supplied).
pub(crate) fn read_source(
    arg: Option<&str>,
    eval: Option<&str>,
) -> Result<(String, SourceInput), CliError> {
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

pub(crate) fn read_template_source(
    arg: Option<&str>,
    eval: Option<&str>,
    vars: &[String],
) -> Result<(String, SourceInput), CliError> {
    let (source, input) = read_source(arg, eval)?;
    let variables = parse_variable_assignments(vars).map_err(driver_error)?;
    expand_variables(&source, &variables)
        .map(|expanded| (expanded, input))
        .map_err(driver_error)
}

pub(crate) fn driver_error(err: DriverError) -> CliError {
    match err {
        DriverError::Usage(message) => CliError::Usage(message),
        DriverError::Data { .. }
        | DriverError::StdinRead(_)
        | DriverError::StdinData { .. }
        | DriverError::StdinParse(_) => CliError::Io(err.to_string()),
    }
}
