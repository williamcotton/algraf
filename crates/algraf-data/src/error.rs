//! Data loading errors and warnings (spec §10.2, §10.3).

/// A fatal error while reading tabular data.
#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The CSV had no header row (headerless CSV is rejected; spec §10.2).
    #[error("CSV has no header row")]
    MissingHeader,

    /// Two columns shared the same header name.
    #[error("duplicate column header: {0:?}")]
    DuplicateHeader(String),
}

/// A non-fatal data inference warning (spec §10.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataWarning {
    pub message: String,
    /// The column the warning relates to, if any.
    pub column: Option<String>,
}

impl DataWarning {
    pub fn for_column(column: impl Into<String>, message: impl Into<String>) -> DataWarning {
        DataWarning {
            message: message.into(),
            column: Some(column.into()),
        }
    }
}
