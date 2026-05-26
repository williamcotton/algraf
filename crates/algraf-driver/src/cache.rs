//! Shared schema-cache primitives (spec §10.9).
//!
//! The driver owns a small, schema-only cache so the LSP, tests, and future
//! callers share one policy for keying and invalidation. The cache stores
//! resolved schemas and load errors — never full data frames — and invalidates
//! conservatively: an entry is reused only when a fresh source fingerprint
//! matches the one observed when the entry was stored. When metadata is
//! unavailable the entry is never reused (the source is reloaded), so a missing
//! or unreadable file is re-examined every time rather than served stale.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use algraf_core::DiagnosticCode;
use algraf_data::{ColumnDef, Format};

use crate::error::LoadContext;
use crate::io::{DriverIo, DriverPathMetadata};
use crate::loading::{load_schema_path_with_io, load_sqlite_schema_with_io};
use crate::report::driver_error_code_message;

/// Schema cache key: a normalized source path plus any explicit
/// source-constructor format policy (spec §10.9).
///
/// The path is normalized lexically (without touching the filesystem) so that
/// equivalent spellings — `a/./b`, `a/b`, `a/c/../b` — share one cache slot.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DataSourceKey {
    path: PathBuf,
    format: Option<Format>,
    query: Option<String>,
}

impl DataSourceKey {
    /// Build a key from a resolved path and explicit format policy.
    pub fn new(path: impl Into<PathBuf>, format: Option<Format>) -> DataSourceKey {
        DataSourceKey {
            path: normalize_path(&path.into()),
            format,
            query: None,
        }
    }

    /// Build a key for a SQLite database path plus query.
    pub fn sqlite(path: impl Into<PathBuf>, query: impl Into<String>) -> DataSourceKey {
        DataSourceKey {
            path: normalize_path(&path.into()),
            format: None,
            query: Some(query.into()),
        }
    }

    /// The normalized source path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The explicit format policy, if a source constructor named one.
    pub fn format(&self) -> Option<Format> {
        self.format
    }

    /// The SQL query for SQLite sources, if any.
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }
}

/// Lexically normalize a path: drop `.` components and resolve `..` against a
/// preceding normal component, without consulting the filesystem (so it works
/// for paths whose target does not exist).
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                match out.components().next_back() {
                    Some(Component::Normal(_)) => {
                        out.pop();
                    }
                    // Keep `..` when there is nothing normal to pop and we are
                    // not anchored at a root/prefix.
                    Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                    _ => out.push(".."),
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// A lightweight identity for the bytes behind a source path (spec §10.9).
///
/// Two fingerprints are equal only when every observed field matches. A `len`
/// or `modified` change therefore invalidates a cached schema. The content hash
/// is optional and left `None` by the metadata-only path; it exists so a future
/// hashing provider can tighten invalidation without an API change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFingerprint {
    pub len: u64,
    pub modified: Option<SystemTime>,
    pub content_hash: Option<u64>,
}

impl SourceFingerprint {
    /// Build a fingerprint from path metadata.
    pub fn from_metadata(metadata: DriverPathMetadata) -> SourceFingerprint {
        SourceFingerprint {
            len: metadata.len,
            modified: metadata.modified,
            content_hash: None,
        }
    }

    /// Attach an optional content hash.
    pub fn with_content_hash(mut self, hash: u64) -> SourceFingerprint {
        self.content_hash = Some(hash);
        self
    }
}

/// Fingerprint a path through an I/O provider, returning `None` when metadata is
/// unavailable (for example a missing or unreadable file). A `None` fingerprint
/// forces a reload, never a stale cache hit.
pub fn fingerprint_path(io: &dyn DriverIo, path: &Path) -> Option<SourceFingerprint> {
    io.metadata(path).ok().map(SourceFingerprint::from_metadata)
}

/// A cached schema resolution: either a sampled schema or the diagnostic a load
/// failure produced. Errors are cached as their stable `(code, message)` pair so
/// missing, unreadable, and malformed sources stay distinguishable (spec §10.9).
#[derive(Debug, Clone)]
pub enum CachedSchema {
    Ready(Vec<ColumnDef>),
    Error {
        code: DiagnosticCode,
        message: String,
    },
}

/// A schema cache keyed by [`DataSourceKey`] and validated by
/// [`SourceFingerprint`].
///
/// Implementations decide storage and lifetime. `get` MUST return an entry only
/// when it is still valid for the supplied fingerprint; conservative
/// implementations treat a `None` fingerprint, or any fingerprint mismatch, as a
/// miss.
pub trait SchemaCache {
    /// Return a cached schema if one is present and valid for `fingerprint`.
    fn get(
        &self,
        key: &DataSourceKey,
        fingerprint: Option<&SourceFingerprint>,
    ) -> Option<CachedSchema>;

    /// Store a resolution under `key` with the fingerprint observed at load time.
    fn put(&self, key: DataSourceKey, fingerprint: Option<SourceFingerprint>, schema: CachedSchema);
}

/// An in-memory, fingerprint-validated schema cache safe for concurrent use.
#[derive(Debug, Default)]
pub struct InMemorySchemaCache {
    entries: Mutex<HashMap<DataSourceKey, CacheEntry>>,
}

#[derive(Debug)]
struct CacheEntry {
    fingerprint: Option<SourceFingerprint>,
    schema: CachedSchema,
}

impl InMemorySchemaCache {
    pub fn new() -> InMemorySchemaCache {
        InMemorySchemaCache::default()
    }

    /// Number of cached entries (test/diagnostic helper).
    pub fn len(&self) -> usize {
        self.entries.lock().expect("schema cache poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SchemaCache for InMemorySchemaCache {
    fn get(
        &self,
        key: &DataSourceKey,
        fingerprint: Option<&SourceFingerprint>,
    ) -> Option<CachedSchema> {
        let entries = self.entries.lock().expect("schema cache poisoned");
        let entry = entries.get(key)?;
        // Reuse only when both fingerprints are present and equal. A `None` on
        // either side is ambiguous, so we force a reload (spec §10.9).
        match (entry.fingerprint.as_ref(), fingerprint) {
            (Some(stored), Some(current)) if stored == current => Some(entry.schema.clone()),
            _ => None,
        }
    }

    fn put(
        &self,
        key: DataSourceKey,
        fingerprint: Option<SourceFingerprint>,
        schema: CachedSchema,
    ) {
        self.entries.lock().expect("schema cache poisoned").insert(
            key,
            CacheEntry {
                fingerprint,
                schema,
            },
        );
    }
}

/// A cache that never stores anything; every lookup misses. Callers that want
/// fresh one-shot loads (for example CLI render) use this implementation.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSchemaCache;

impl SchemaCache for NoSchemaCache {
    fn get(
        &self,
        _key: &DataSourceKey,
        _fingerprint: Option<&SourceFingerprint>,
    ) -> Option<CachedSchema> {
        None
    }

    fn put(
        &self,
        _key: DataSourceKey,
        _fingerprint: Option<SourceFingerprint>,
        _schema: CachedSchema,
    ) {
    }
}

/// Resolve a path's schema through a cache.
///
/// On a valid cache hit the stored schema (or cached error) is returned without
/// touching the file. On a miss the schema is loaded through `io`, stored under
/// the freshly observed fingerprint, and returned. Load failures are cached as
/// [`CachedSchema::Error`] with the same stable diagnostic code and message the
/// CLI would emit (spec §23.4).
pub fn resolve_schema_cached(
    cache: &dyn SchemaCache,
    io: &dyn DriverIo,
    path: &Path,
    format: Option<Format>,
    sample_size: usize,
    context: LoadContext,
) -> CachedSchema {
    let key = DataSourceKey::new(path, format);
    let fingerprint = fingerprint_path(io, path);

    if let Some(cached) = cache.get(&key, fingerprint.as_ref()) {
        return cached;
    }

    let schema = match load_schema_path_with_io(path, format, sample_size, context, io) {
        Ok(schema) => CachedSchema::Ready(schema),
        Err(err) => {
            let (code, message) = driver_error_code_message(&err);
            CachedSchema::Error { code, message }
        }
    };
    cache.put(key, fingerprint, schema.clone());
    schema
}

/// Resolve a SQLite query schema through a cache.
pub fn resolve_sqlite_schema_cached(
    cache: &dyn SchemaCache,
    io: &dyn DriverIo,
    path: &Path,
    query: &str,
    sample_size: usize,
    context: LoadContext,
) -> CachedSchema {
    let key = DataSourceKey::sqlite(path, query);
    let fingerprint = fingerprint_path(io, path);

    if let Some(cached) = cache.get(&key, fingerprint.as_ref()) {
        return cached;
    }

    let schema = match load_sqlite_schema_with_io(path, query, sample_size, context, io) {
        Ok(schema) => CachedSchema::Ready(schema),
        Err(err) => {
            let (code, message) = driver_error_code_message(&err);
            CachedSchema::Error { code, message }
        }
    };
    cache.put(key, fingerprint, schema.clone());
    schema
}
