//! SQLite loading (spec §10.12).
//!
//! SQLite sources are local, read-only database files queried through a single
//! deterministic statement. Results enter the same raw-string inference path as
//! CSV and JSON, so downstream crates continue to see only a [`DataFrame`].

use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::Path;
use std::ptr;

use libsqlite3_sys as ffi;

use crate::csv::{build, LoadResult};
use crate::error::DataError;
use crate::frame::Table;
use crate::schema::ColumnDef;

const _: fn(&Path, &str) -> Result<LoadResult, DataError> = read_sqlite_path;
const _: fn(&Path, &str, usize) -> Result<Vec<ColumnDef>, DataError> = read_sqlite_schema_path;

/// Fully load a local SQLite query result.
pub fn read_sqlite_path(path: &Path, query: &str) -> Result<LoadResult, DataError> {
    read_sqlite_path_limited(path, query, None).map(|(loaded, _)| loaded)
}

/// Infer a provisional SQLite schema from at most `sample` result rows.
pub fn read_sqlite_schema_path(
    path: &Path,
    query: &str,
    sample: usize,
) -> Result<Vec<ColumnDef>, DataError> {
    read_sqlite_path_limited(path, query, Some(sample))
        .map(|(loaded, _)| loaded.frame.schema().to_vec())
}

fn read_sqlite_path_limited(
    path: &Path,
    query: &str,
    limit: Option<usize>,
) -> Result<(LoadResult, usize), DataError> {
    validate_sql_query(query)?;
    // Preserve existing missing/unreadable-file diagnostics where the OS can
    // identify them before SQLite opens the database handle.
    std::fs::metadata(path)?;

    let connection = Connection::open_readonly(path)?;
    let statement = connection.prepare(query)?;
    statement.validate_readonly_result()?;
    let names = statement.column_names()?;
    ensure_unique_columns(&names)?;
    statement.load(names, limit)
}

fn validate_sql_query(query: &str) -> Result<(), DataError> {
    let Some(first) = first_sql_word(query) else {
        return Err(DataError::SqliteQuery("query is empty".to_string()));
    };
    if first != "select" && first != "with" {
        return Err(DataError::SqliteSafety(
            "query must be a SELECT or WITH statement".to_string(),
        ));
    }
    if !has_top_level_order_by(query) {
        return Err(DataError::SqliteSafety(
            "SQLite queries must include a top-level ORDER BY for deterministic rows".to_string(),
        ));
    }
    Ok(())
}

fn ensure_unique_columns(names: &[String]) -> Result<(), DataError> {
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(name.clone()) {
            return Err(DataError::DuplicateColumn(name.clone()));
        }
    }
    Ok(())
}

struct Connection {
    raw: *mut ffi::sqlite3,
}

impl Connection {
    fn open_readonly(path: &Path) -> Result<Connection, DataError> {
        let filename = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| {
            DataError::SqliteQuery("database path contains an interior NUL byte".to_string())
        })?;
        let mut raw = ptr::null_mut();
        let rc = unsafe {
            ffi::sqlite3_open_v2(
                filename.as_ptr(),
                &mut raw,
                ffi::SQLITE_OPEN_READONLY,
                ptr::null(),
            )
        };
        if rc != ffi::SQLITE_OK {
            let message = sqlite_error(raw);
            if !raw.is_null() {
                unsafe {
                    ffi::sqlite3_close(raw);
                }
            }
            return Err(DataError::SqliteQuery(message));
        }
        Ok(Connection { raw })
    }

    fn prepare(&self, query: &str) -> Result<Statement<'_>, DataError> {
        let sql = CString::new(query.as_bytes()).map_err(|_| {
            DataError::SqliteQuery("query contains an interior NUL byte".to_string())
        })?;
        let mut raw = ptr::null_mut();
        let mut tail: *const c_char = ptr::null();
        let rc =
            unsafe { ffi::sqlite3_prepare_v2(self.raw, sql.as_ptr(), -1, &mut raw, &mut tail) };
        if rc != ffi::SQLITE_OK {
            return Err(DataError::SqliteQuery(sqlite_error(self.raw)));
        }
        if raw.is_null() {
            return Err(DataError::SqliteQuery(
                "query did not contain a SQL statement".to_string(),
            ));
        }
        if !tail_is_empty(&sql, tail) {
            unsafe {
                ffi::sqlite3_finalize(raw);
            }
            return Err(DataError::SqliteSafety(
                "SQLite sources accept exactly one SQL statement".to_string(),
            ));
        }
        Ok(Statement {
            connection: self,
            raw,
        })
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                ffi::sqlite3_close(self.raw);
            }
        }
    }
}

struct Statement<'a> {
    connection: &'a Connection,
    raw: *mut ffi::sqlite3_stmt,
}

impl Statement<'_> {
    fn validate_readonly_result(&self) -> Result<(), DataError> {
        let readonly = unsafe { ffi::sqlite3_stmt_readonly(self.raw) != 0 };
        if !readonly {
            return Err(DataError::SqliteSafety(
                "SQLite source query must be read-only".to_string(),
            ));
        }
        if self.column_count() == 0 {
            return Err(DataError::SqliteSafety(
                "SQLite source query must return result columns".to_string(),
            ));
        }
        Ok(())
    }

    fn column_count(&self) -> usize {
        unsafe { ffi::sqlite3_column_count(self.raw).max(0) as usize }
    }

    fn column_names(&self) -> Result<Vec<String>, DataError> {
        let mut names = Vec::with_capacity(self.column_count());
        for index in 0..self.column_count() {
            let raw = unsafe { ffi::sqlite3_column_name(self.raw, index as c_int) };
            if raw.is_null() {
                return Err(DataError::SqliteQuery(
                    "SQLite did not return a column name".to_string(),
                ));
            }
            let name = unsafe { CStr::from_ptr(raw) }
                .to_str()
                .map_err(|_| {
                    DataError::SqliteQuery(
                        "SQLite returned a column name that is not valid UTF-8".to_string(),
                    )
                })?
                .to_string();
            names.push(name);
        }
        Ok(names)
    }

    fn load(
        &self,
        names: Vec<String>,
        limit: Option<usize>,
    ) -> Result<(LoadResult, usize), DataError> {
        let mut columns: Vec<Vec<String>> = vec![Vec::new(); names.len()];
        let mut rows = 0usize;
        loop {
            if limit.is_some_and(|max| rows >= max) {
                break;
            }
            let rc = unsafe { ffi::sqlite3_step(self.raw) };
            match rc {
                ffi::SQLITE_ROW => {
                    for (index, column) in columns.iter_mut().enumerate() {
                        column.push(self.cell_text(index)?);
                    }
                    rows += 1;
                }
                ffi::SQLITE_DONE => break,
                _ => return Err(DataError::SqliteQuery(sqlite_error(self.connection.raw))),
            }
        }
        Ok((build(names, columns), rows))
    }

    fn cell_text(&self, index: usize) -> Result<String, DataError> {
        let storage_type = unsafe { ffi::sqlite3_column_type(self.raw, index as c_int) };
        match storage_type {
            ffi::SQLITE_NULL => Ok(String::new()),
            ffi::SQLITE_INTEGER => {
                let value = unsafe { ffi::sqlite3_column_int64(self.raw, index as c_int) };
                Ok(value.to_string())
            }
            ffi::SQLITE_FLOAT => {
                let value = unsafe { ffi::sqlite3_column_double(self.raw, index as c_int) };
                Ok(value.to_string())
            }
            ffi::SQLITE_TEXT => {
                let ptr = unsafe { ffi::sqlite3_column_text(self.raw, index as c_int) };
                let len = unsafe { ffi::sqlite3_column_bytes(self.raw, index as c_int) };
                if ptr.is_null() {
                    return Ok(String::new());
                }
                let bytes = unsafe { std::slice::from_raw_parts(ptr, len.max(0) as usize) };
                String::from_utf8(bytes.to_vec()).map_err(|_| {
                    DataError::SqliteQuery(
                        "SQLite returned text that is not valid UTF-8".to_string(),
                    )
                })
            }
            ffi::SQLITE_BLOB => {
                let name = self
                    .column_names()
                    .ok()
                    .and_then(|names| names.get(index).cloned())
                    .unwrap_or_else(|| format!("#{index}"));
                Err(DataError::SqliteUnsupportedType {
                    column: name,
                    type_name: "blob",
                })
            }
            _ => Err(DataError::SqliteUnsupportedType {
                column: format!("#{index}"),
                type_name: "unknown",
            }),
        }
    }
}

impl Drop for Statement<'_> {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                ffi::sqlite3_finalize(self.raw);
            }
        }
    }
}

fn sqlite_error(db: *mut ffi::sqlite3) -> String {
    if db.is_null() {
        return "unable to open SQLite database".to_string();
    }
    let raw = unsafe { ffi::sqlite3_errmsg(db) };
    if raw.is_null() {
        "unknown SQLite error".to_string()
    } else {
        unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned()
    }
}

fn tail_is_empty(sql: &CString, tail: *const c_char) -> bool {
    if tail.is_null() {
        return true;
    }
    let start = sql.as_ptr();
    let total = sql.as_bytes_with_nul().len();
    let offset = unsafe { tail.offset_from(start) };
    if offset < 0 {
        return false;
    }
    let offset = offset as usize;
    if offset >= total {
        return true;
    }
    let bytes = &sql.as_bytes_with_nul()[offset..total - 1];
    sql_tail_is_empty(bytes)
}

fn sql_tail_is_empty(bytes: &[u8]) -> bool {
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' | b';' => i += 1,
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            _ => return false,
        }
    }
    true
}

fn first_sql_word(query: &str) -> Option<String> {
    sql_words(query).into_iter().next().map(|(_, word)| word)
}

fn has_top_level_order_by(query: &str) -> bool {
    let words = sql_words(query);
    words.windows(2).any(|pair| {
        matches!(
            pair,
            [(0, first), (0, second)] if first == "order" && second == "by"
        )
    })
}

fn sql_words(query: &str) -> Vec<(i32, String)> {
    let bytes = query.as_bytes();
    let mut words = Vec::new();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => i = skip_quoted(bytes, i, b'\''),
            b'"' => i = skip_quoted(bytes, i, b'"'),
            b'`' => i = skip_quoted(bytes, i, b'`'),
            b'[' => i = skip_bracket_identifier(bytes, i),
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            byte if is_word_start(byte) => {
                let start = i;
                i += 1;
                while i < bytes.len() && is_word_continue(bytes[i]) {
                    i += 1;
                }
                let word = query[start..i].to_ascii_lowercase();
                words.push((depth, word));
            }
            _ => i += 1,
        }
    }
    words
}

fn is_word_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_word_continue(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric()
}

fn skip_quoted(bytes: &[u8], mut i: usize, quote: u8) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            i += 1;
            if bytes.get(i) == Some(&quote) {
                i += 1;
                continue;
            }
            break;
        }
        i += 1;
    }
    i
}

fn skip_bracket_identifier(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b']' {
            return i + 1;
        }
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Table;
    use crate::schema::DataType;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db(test: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "algraf-sqlite-{test}-{}-{nanos}.db",
            std::process::id()
        ))
    }

    fn create_sales_db(path: &Path) {
        let filename = CString::new(path.to_string_lossy().as_bytes()).unwrap();
        let mut db = ptr::null_mut();
        let rc = unsafe {
            ffi::sqlite3_open_v2(
                filename.as_ptr(),
                &mut db,
                ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
                ptr::null(),
            )
        };
        assert_eq!(rc, ffi::SQLITE_OK, "{}", sqlite_error(db));
        let sql = CString::new(
            "CREATE TABLE sales(region TEXT, revenue INTEGER, margin REAL, active INTEGER);
             INSERT INTO sales VALUES
                ('North', 12, 0.2, 1),
                ('South', 9, 0.1, 0);",
        )
        .unwrap();
        let rc =
            unsafe { ffi::sqlite3_exec(db, sql.as_ptr(), None, ptr::null_mut(), ptr::null_mut()) };
        assert_eq!(rc, ffi::SQLITE_OK, "{}", sqlite_error(db));
        unsafe {
            ffi::sqlite3_close(db);
        }
    }

    #[test]
    fn order_by_scan_ignores_strings_comments_and_nested_order_by() {
        assert!(!has_top_level_order_by("SELECT 'ORDER BY' AS label"));
        assert!(!has_top_level_order_by(
            "SELECT value, row_number() OVER (ORDER BY value) AS rn FROM t"
        ));
        assert!(has_top_level_order_by(
            "SELECT value FROM t /* ORDER BY nope */ ORDER\nBY value"
        ));
    }

    #[test]
    fn tail_allows_only_empty_comments_and_semicolons() {
        assert!(sql_tail_is_empty(b"; -- ok\n /* ok */"));
        assert!(!sql_tail_is_empty(b"; SELECT 2"));
    }

    #[test]
    fn loads_sqlite_query_result() {
        let path = temp_db("load");
        create_sales_db(&path);

        let loaded = read_sqlite_path(
            &path,
            "SELECT region, revenue, margin FROM sales ORDER BY region",
        )
        .unwrap();

        assert_eq!(loaded.frame.row_count(), 2);
        assert_eq!(loaded.frame.schema()[0].name, "region");
        assert_eq!(loaded.frame.schema()[0].dtype, DataType::String);
        assert_eq!(loaded.frame.schema()[1].dtype, DataType::Integer);
        assert_eq!(loaded.frame.schema()[2].dtype, DataType::Float);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn samples_sqlite_schema_rows() {
        let path = temp_db("schema");
        create_sales_db(&path);

        let schema = read_sqlite_schema_path(
            &path,
            "SELECT region, revenue FROM sales ORDER BY region",
            1,
        )
        .unwrap();

        assert_eq!(schema.len(), 2);
        assert_eq!(schema[1].dtype, DataType::Integer);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_sqlite_without_order_by_or_with_multiple_statements() {
        let path = temp_db("safety");
        create_sales_db(&path);

        let no_order = read_sqlite_path(&path, "SELECT region FROM sales").unwrap_err();
        assert!(matches!(no_order, DataError::SqliteSafety(_)));

        let multi = read_sqlite_path(
            &path,
            "SELECT region FROM sales ORDER BY region; SELECT revenue FROM sales ORDER BY revenue",
        )
        .unwrap_err();
        assert!(matches!(multi, DataError::SqliteSafety(_)));
        let _ = std::fs::remove_file(path);
    }
}
