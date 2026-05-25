# Runtime Cache Policy

This note records the v0.19.0 cache boundary. It is descriptive policy, not a
new user feature.

## Cache Kinds

**Schema cache**

- Stores resolved schemas and stable load-error `(code, message)` pairs.
- Keyed by `DataSourceKey`: normalized resolved path plus explicit source
  constructor format.
- Validated by `SourceFingerprint`: file length, modified time, and optional
  content hash.
- Owned by `algraf-driver`; the LSP uses `InMemorySchemaCache`, while one-shot
  CLI commands use fresh loading unless they explicitly opt into a cache.

**Full-frame cache**

- Would store loaded `DataFrame` values or a future table-engine equivalent.
- Not implemented in v0.19.0. There is no current caller that reuses full frames
  without risking stale render behavior or larger editor memory use.
- If promoted later, it must reuse `DataSourceKey` and `SourceFingerprint`, keep
  named-table and primary frames distinct, and preserve data warnings.

**Render-result cache**

- Would store final SVG or a planned render scene.
- Not implemented in v0.19.0. Render output depends on source text, data, theme,
  CLI flags, dimensions, and future output-backend choices; caching it now would
  add invalidation surface without a caller.

**Persistent cache**

- Would survive process restarts on disk.
- Not implemented in v0.19.0. Persistent storage needs an explicit storage
  location, invalidation format, versioning, and privacy policy.

## v0.19.0 Decision

Only the existing schema cache ships. Full-frame, render-result, and persistent
caches remain deferred because the current CLI performs one-shot renders and the
LSP only needs schema-first validation for responsive analysis.
