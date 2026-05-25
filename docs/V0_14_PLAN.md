# Algraf v0.14.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_13_PLAN.md`](V0_13_PLAN.md)
Follow-on plan: [`V0_15_PLAN.md`](V0_15_PLAN.md)

## Purpose

This document defines the intended v0.14.0 release shape: adding a narrow
driver I/O boundary after v0.13 cleaned up source and path resolution.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.14.0 is a **driver I/O seam** release: keep the current language, CLI, LSP,
data formats, and rendered output stable while decoupling `algraf-driver` from
hard-coded process and disk reads.

The goal is not to add network sources, async loading, SQL, WASM, or a new cache
policy. The goal is to make the existing behavior injectable and testable: the
CLI continues to use the operating system, while future editor or WASM work can
provide an in-memory file system without changing the source language.

## Current Debt Surface

The deferred-item audit found:

- v0.13 explicitly deferred a VFS or injected file system abstraction.
- `crates/algraf-driver/src/loading.rs` still reads from `std::io::stdin()` and
  delegates path loading directly to `algraf_data::read_path` /
  `read_schema_path`.
- LSP analysis uses the driver for resolution, but schema reads still assume
  resolved physical paths.
- Tests still need temporary directories for cases that are really about driver
  resolution and loader dispatch rather than the host file system.
- Shapefile loading has sidecar-file behavior (`.dbf`, `.shx`, `.prj`, `.cpg`)
  that any I/O seam must preserve exactly.

## Scope Rules

- No source-language changes.
- No new data formats, source constructors, network access, command execution,
  SQL, async APIs, or caching policy.
- Existing public functions such as `load_path`, `load_schema_path`, and
  `prepare_chart` continue to use the operating-system implementation by
  default.
- CLI, LSP, and example rendering behavior should remain stable.
- Shapefile sidecar resolution must keep the current path-relative behavior.
- If any implementation of this plan changes generated examples, treat that as
  a bug unless a separate spec update deliberately promotes a behavior change.

## Capstone Acceptance Target

The capstone is injectable I/O with no output drift:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

`git diff -- examples` must be empty. Running `examples/generate.sh` should not
change what happens for any checked-in example.

## Design Decisions (settled)

1. **Inject bytes, not source syntax.** The language still names paths,
   `stdin`, `GeoJson(...)`, and `Shapefile(...)` exactly as before.
2. **Keep the OS path as the compatibility facade.** Existing callers use the
   same public helpers; new injectable APIs sit beside or under them.
3. **Handle shapefile sidecars deliberately.** Do not design an I/O trait that
   works only for single-file formats while quietly regressing shapefiles.
4. **Stay synchronous.** Async belongs to a later release after the blocking I/O
   boundary is explicit and tested.
5. **Do not move caching into v0.14.** Cache keys and invalidation are v0.16
   scope.

## v0.14.0 Must

### 1. Driver I/O trait and OS implementation

Status: Implemented. `algraf-driver` now exposes `DriverIo`, `OsDriverIo`,
path metadata, and a shapefile sidecar bundle type. OS-backed public wrappers
remain the default, while injectable APIs sit beside them.

Acceptance criteria:

- `algraf-driver` defines a narrow internal or public trait such as
  `DriverIo`, `FileSystem`, or `DataSourceReader` that can read the bytes needed
  by existing data sources.
- The trait can represent:
  - ordinary path reads;
  - stdin reads;
  - path metadata if needed for error reporting or future cache keys;
  - shapefile sidecar reads or an equivalent bundle abstraction.
- An `OsDriverIo` implementation preserves current disk and stdin behavior.
- The default public driver entry points use `OsDriverIo` so CLI and LSP callers
  do not need an immediate migration.
- The trait does not include network, environment-variable, process, or async
  operations.

### 2. Data reader split behind existing path APIs

Status: Implemented. `algraf-data` keeps existing path APIs and now exposes
byte-slice readers for single-file formats plus an in-memory shapefile sidecar
bundle reader. `OsDriverIo` preserves path-backed shapefile loading behavior.

Acceptance criteria:

- `algraf-data` keeps current path-oriented functions for compatibility.
- Single-file formats (`csv`, `tsv`, `json`, `ndjson`, `geojson`) have reader or
  byte-slice entry points that the injected driver I/O path can call without
  re-opening files internally.
- Shapefile loading either gains a sidecar-bundle reader or remains path-backed
  through a clearly documented `OsDriverIo` adapter; in either case, the current
  checked-in shapefile fixtures load identically.
- Format inference by extension and explicit format selection preserve current
  errors and diagnostics.
- Data-warning collection is unchanged.

### 3. Thread I/O through driver preparation

Status: Implemented. Driver loading and `prepare_chart_with_io` accept an
injected provider; existing `prepare_chart`, `load_path`, `load_schema_path`,
and related helpers keep using `OsDriverIo`.

Acceptance criteria:

- `PrepareOptions` or an adjacent builder can carry the chosen I/O provider.
- Internal loading helpers use the provider instead of directly calling
  `std::io::stdin()` or path readers.
- Public wrappers remain available and use the OS provider.
- `SourceInput`, `base_dir`, `data_override`, and named-table resolution keep
  the v0.13 precedence rules.
- Stdin conflict errors keep their current wording unless tests and docs record
  a deliberate improvement.

### 4. In-memory driver tests

Status: Implemented. Driver tests cover in-memory CSV, TSV, JSON, NDJSON,
GeoJSON, named tables, schema loading, primary loading, data overrides, stdin,
and shapefile bundles. Disk-backed shapefile fixture tests remain.

Acceptance criteria:

- Driver tests cover CSV, TSV, JSON, NDJSON, GeoJSON, named tables, schema
  loading, primary loading, and data overrides through an in-memory I/O provider
  where practical.
- Disk-backed tests remain for actual OS path behavior and shapefile sidecars.
- Tests prove injected I/O produces the same frames, schemas, warnings, and
  driver errors as OS-backed loading for equivalent inputs.
- Temporary-directory tests are reduced where they only exist to put bytes behind
  a path.

### 5. Data dependency inventory

Status: Implemented. `data_dependencies` centralizes one chart's resolved
path-backed primary and named-table dependencies, and LSP preview data-path
reporting now uses it.

Acceptance criteria:

- The driver exposes or internally centralizes the list of resolved data
  dependencies for one chart: primary source plus valid named table sources.
- LSP preview data-path reporting can reuse this inventory without duplicating
  resolution logic.
- The inventory reports the same path strings the LSP reports today for OS
  sources.
- Dependency reporting does not imply watching, caching, network access, or lazy
  loading in this release.

### 6. Spec, plan, and example hygiene

Status: Implemented. Workspace and VS Code package versions are bumped to
`0.14.0`; spec §10, §21, and §23 document the I/O boundary and dependency
inventory. Example regeneration produced no checked-in example drift.

Acceptance criteria:

- Workspace version is bumped to `0.14.0` when the release branch is ready.
- Spec §23 is updated only if the driver I/O boundary becomes part of the
  intended crate architecture.
- This plan is updated as each item completes, is rejected, or moves scope.
- Examples are regenerated with `./examples/generate.sh`; `git diff -- examples`
  must be empty.

## v0.14.0 Should

### WASM-readiness audit

Status: Implemented. Remaining OS-only use after this release is limited to
compatibility callers and product surfaces: CLI source reads, output writes,
example generation, LSP URI-to-path schema/cache reads, and the OS adapter used
by default wrappers. No `wasm32-unknown-unknown` port is attempted.

Document the remaining OS-only dependencies after the I/O seam lands. This is
an audit only; v0.14.0 should not attempt a `wasm32-unknown-unknown` port.

### LSP unsaved-buffer spike

Status: Implemented. The in-memory driver tests use an unsaved `/mem/chart.ag`
source input with data served entirely by an injected provider, proving the
driver seam can support unsaved-buffer data dependencies without protocol
changes.

Add one small test or prototype proving an injected source/data provider can
serve an unsaved in-memory data dependency. Do not change editor behavior unless
the implementation falls out with no public protocol or diagnostic changes.

## Explicitly Deferred Past v0.14.0

- Async driver APIs or async data loading.
- Driver-level schema/data cache.
- Query-driven or `salsa`-style compilation.
- Lazy data engine or renderer-delayed data materialization.
- Network, URL, command, SQL, or environment-variable data sources.
- Generic runtime source-format constructors replacing `GeoJson(...)` and
  `Shapefile(...)`.
- WASM runtime support.
- New CLI flags, LSP features, data formats, chart syntax, or renderer features.

## Optional-Item Audit

### Promote In v0.14.0 (Must)

- Driver I/O trait and OS implementation.
- Data reader split behind existing path APIs.
- I/O provider threading through preparation.
- In-memory driver tests.
- Data dependency inventory.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- WASM-readiness audit.
- LSP unsaved-buffer spike.

### Keep Deferred

- Async, caching, query-driven compilation, lazy data engines, network sources,
  SQL, WASM runtime, and new user-facing capabilities.

## Promotion Workflow

1. Add guard tests around current OS-backed data and schema loading behavior.
2. Add the driver I/O trait and OS implementation without changing public
   wrappers.
3. Split data readers where needed so injected bytes use the same parsers.
4. Thread the provider through driver preparation and named-table loading.
5. Move dependency inventory behind the driver and update LSP preview to reuse
   it if doing so is behavior-preserving.
6. Add in-memory loading tests.
7. Run formatter, clippy, workspace tests, `./examples/generate.sh`, and require
   an empty `git diff -- examples`.
