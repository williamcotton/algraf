use std::fmt;
use std::path::PathBuf;

use algraf_data::DataError;

/// Data loading context used to preserve caller-facing error messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadContext {
    Primary,
    Table { name: String },
}

impl fmt::Display for LoadContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadContext::Primary => f.write_str("data"),
            LoadContext::Table { name } => write!(f, "Table `{name}` data"),
        }
    }
}

/// Structured driver errors.
#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("{0}")]
    Usage(String),
    #[error("failed to load {context} {path}: {source}", path = .path.display())]
    Data {
        context: LoadContext,
        path: PathBuf,
        #[source]
        source: DataError,
    },
    #[error("{0}")]
    StdinRead(String),
    #[error("{0}")]
    StdinParse(String),
}
