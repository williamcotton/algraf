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

    /// A JSON document failed to parse (spec §10.2).
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// An NDJSON line failed to parse (spec §10.2).
    #[error("JSON parse error on line {line}: {source}")]
    NdJson {
        line: usize,
        source: serde_json::Error,
    },

    /// The top-level JSON value was not an array of row objects (spec §10.2).
    #[error("JSON data must be an array of row objects")]
    JsonNotArray,

    /// A JSON array element was not an object (spec §10.2).
    #[error("JSON row {index} is not an object")]
    JsonRowNotObject { index: usize },

    /// An NDJSON line was valid JSON but not an object (spec §10.2).
    #[error("NDJSON line {line} is not an object")]
    NdJsonRowNotObject { line: usize },

    /// A GeoJSON or shapefile document failed to parse, or contained a geometry
    /// type the loader does not support (spec §10.11; diagnostic `E1805`).
    #[error("geospatial parse error: {0}")]
    Geo(String),
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
