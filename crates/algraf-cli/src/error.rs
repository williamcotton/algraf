//! CLI errors mapped to exit codes (spec §22.9).

use std::fmt;

/// A top-level CLI error carrying the process exit code.
#[derive(Debug)]
pub enum CliError {
    /// Diagnostic errors were reported (exit 1).
    Diagnostics,
    /// CLI usage error (exit 2).
    Usage(String),
    /// I/O error (exit 3).
    Io(String),
    /// Internal error / unimplemented (exit 4).
    Internal(String),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Diagnostics => 1,
            CliError::Usage(_) => 2,
            CliError::Io(_) => 3,
            CliError::Internal(_) => 4,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Diagnostics => write!(f, "errors were reported"),
            CliError::Usage(m) => write!(f, "usage error: {m}"),
            CliError::Io(m) => write!(f, "{m}"),
            CliError::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}
