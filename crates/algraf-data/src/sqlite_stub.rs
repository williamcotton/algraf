//! SQLite stubs for builds without the `sql` feature (spec §10.12, §30).
//!
//! The browser/WASM runtime excludes the native `libsqlite3-sys` C library.
//! These stubs keep the same public signatures as [`crate::sqlite`] so every
//! caller (the driver's loading path, schema sampling) compiles unchanged; a
//! SQLite source in such a build fails with a clear diagnostic instead of a
//! link error or panic.

use std::path::Path;

use crate::csv::LoadResult;
use crate::error::DataError;
use crate::schema::ColumnDef;

const _: fn(&Path, &str) -> Result<LoadResult, DataError> = read_sqlite_path;
const _: fn(&Path, &str, usize) -> Result<Vec<ColumnDef>, DataError> = read_sqlite_schema_path;

fn unavailable() -> DataError {
    DataError::SqliteSafety(
        "SQLite data sources are not available in this build of Algraf".to_string(),
    )
}

/// Stub: SQLite loading is unavailable without the `sql` feature.
pub fn read_sqlite_path(_path: &Path, _query: &str) -> Result<LoadResult, DataError> {
    Err(unavailable())
}

/// Stub: SQLite schema sampling is unavailable without the `sql` feature.
pub fn read_sqlite_schema_path(
    _path: &Path,
    _query: &str,
    _sample: usize,
) -> Result<Vec<ColumnDef>, DataError> {
    Err(unavailable())
}
