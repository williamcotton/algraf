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

    /// Two data-source columns shared the same name.
    #[error("duplicate column name: {0:?}")]
    DuplicateColumn(String),

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

    /// A SQLite statement failed to parse or execute (spec §10.12).
    #[error("SQLite query error: {0}")]
    SqliteQuery(String),

    /// A SQLite statement violates Algraf's read-only/deterministic policy.
    #[error("SQLite safety error: {0}")]
    SqliteSafety(String),

    /// A SQLite result column used a storage type Algraf does not support.
    #[error("unsupported SQLite type in column {column:?}: {type_name}")]
    SqliteUnsupportedType {
        column: String,
        type_name: &'static str,
    },
}

/// A data inference warning (spec §10.3). Usually non-fatal, but a warning may
/// be marked `fatal` when the user opted into stricter handling (e.g.
/// `Parse(onError: "error")`); the driver promotes a fatal warning to a blocking
/// error diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataWarning {
    pub message: String,
    /// The column the warning relates to, if any.
    pub column: Option<String>,
    /// Whether this should block rendering rather than warn (spec §10.3).
    pub fatal: bool,
}

impl DataWarning {
    pub fn for_column(column: impl Into<String>, message: impl Into<String>) -> DataWarning {
        DataWarning {
            message: message.into(),
            column: Some(column.into()),
            fatal: false,
        }
    }

    /// A column warning that blocks rendering (`Parse(onError: "error")`).
    pub fn fatal_for_column(column: impl Into<String>, message: impl Into<String>) -> DataWarning {
        DataWarning {
            message: message.into(),
            column: Some(column.into()),
            fatal: true,
        }
    }
}
