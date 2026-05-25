# Algraf v0.16.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_15_PLAN.md`](V0_15_PLAN.md)
Follow-on plan: [`V0_17_PLAN.md`](V0_17_PLAN.md)

## Purpose

This document defines the intended v0.16.0 release shape: making schema caching
and phase separation explicit after driver I/O and diagnostic reporting have
clear seams.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.16.0 is a **schema cache and compilation-phase boundary** release: move the
current LSP-only schema-cache policy toward a shared, metadata-aware driver
service and separate "resolve/plan" work from "load/analyze/render" work.

This is still a behavior-preserving refactor. It should make editor analysis
more predictable and future incremental work easier, but it should not introduce
`salsa`, full data-frame caching, render caching, async driver APIs, or new
language/runtime features.

## Current Debt Surface

The deferred-item audit found:

- v0.13 deferred driver-level schema/data cache and query-driven compilation.
- The LSP has a local `DataSourceKey` keyed by path and explicit format, but it
  does not yet include file metadata, size, or content hash.
- Primary schema and named-table schema resolution share patterns but are still
  locally orchestrated in `crates/algraf-lsp/src/backend.rs`.
- Preview rendering always performs full data loading through strict
  preparation, even when schema analysis could reuse prior schema information.
- The driver has private resolved-input state, but no stable plan object that
  callers can inspect before executing loads.

## Scope Rules

- No source-language changes.
- No rendered output changes.
- No full `DataFrame` cache in this release.
- No persistent on-disk cache.
- No `salsa` or demand-driven query database.
- No async driver API requirement.
- Cache invalidation must be conservative: when metadata is unavailable or
  ambiguous, reload.
- CLI render should continue to perform fresh one-shot loads unless a local
  per-command cache is proven behavior-neutral and useful.

## Capstone Acceptance Target

The capstone is cache-aware analysis with no output drift:

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

1. **Cache schemas, not frames.** v0.16.0 targets lightweight editor validation;
   full data caching remains deferred.
2. **Plan before executing.** A chart data plan should identify dependencies,
   formats, and table names before any expensive load starts.
3. **Invalidate by source identity plus fingerprint.** Path/format alone is not
   enough once data files can change under the editor.
4. **Keep cache policy injectable.** CLI, LSP, tests, and future WASM callers may
   want different cache lifetimes.
5. **Do not introduce a query framework yet.** The shape may resemble future
   demand-driven compilation, but the release should stay ordinary Rust.

## v0.16.0 Must

### 1. Shared data source key and fingerprint

Status: Done. `DataSourceKey` and `SourceFingerprint` live in
`crates/algraf-driver/src/cache.rs`, shared by driver and LSP. The key carries
the normalized path plus explicit format policy; the fingerprint carries size,
last-modified time, and an optional content hash. Tests in `algraf-driver`
cover path normalization, explicit format differences, and missing/changed
metadata.

Acceptance criteria:

- Move or recreate `DataSourceKey` outside `algraf-lsp` so driver and LSP can
  share it.
- The key includes resolved source identity and explicit format policy.
- A separate fingerprint can include last-modified timestamp, file size, and
  optional content hash when available from the v0.14 I/O provider.
- Tests cover path normalization, explicit format differences, missing metadata,
  and changed metadata.
- Fingerprinting does not change source resolution behavior.

### 2. Driver schema cache service

Status: Done. The `SchemaCache` trait, `InMemorySchemaCache`, `NoSchemaCache`,
and `resolve_schema_cached` store schemas and cached `(code, message)` errors —
never frames — and invalidate by fingerprint. Missing, unreadable, malformed,
and successful results stay distinct; the cache is injectable so CLI render can
opt out with `NoSchemaCache`.

Acceptance criteria:

- Add a small schema-cache service, trait, or helper owned by the driver or a
  shared crate.
- The cache stores schemas and load errors, not full frames.
- Cache entries are invalidated when fingerprints change.
- Missing files, unreadable files, malformed data, and successful schemas remain
  distinguishable.
- LSP can use the cache for primary and named-table schemas without local
  duplicate load policy.
- The cache is optional; callers can choose a no-cache implementation.

### 3. Public chart data plan

Status: Done. The former private `ResolvedChartInputs` is now the public
`ChartDataPlan`, built by `plan_chart_data` without loading bytes. It records the
primary location, named table locations, explicit formats, the primary source
span, and a dependency inventory; `prepare_chart` loads from it, and
`data_dependencies` remains as a compatibility wrapper.

Acceptance criteria:

- Promote the useful parts of the private resolved-input structure into a stable
  driver type such as `ChartDataPlan`.
- The plan records primary source location, named table locations, explicit
  formats, source spans where known, and dependency inventory.
- Plan construction performs source/path validation but does not load data
  bytes.
- Loading and schema resolution execute from the plan.
- Existing public helpers remain as compatibility wrappers.

### 4. LSP cache migration

Status: Done. The backend holds an `Arc<InMemorySchemaCache>`; both primary and
named-table schema resolution call `resolve_schema_cached`, so they share one
fingerprint-validated path. Diagnostic codes and messages are unchanged (the
driver still owns the mapping), and cached schemas now invalidate when the
underlying file changes.

Acceptance criteria:

- LSP primary schema resolution uses the shared key/fingerprint/cache service.
- LSP named-table schema resolution uses the same cache path as primary schema
  resolution.
- Current LSP diagnostics for missing data and unavailable schemas keep their
  codes and messages unless a deliberate correction is documented.
- Cached schemas invalidate when the underlying file changes.
- Completion and hover behavior remain stable except for avoiding stale schema
  results.

### 5. Phase-boundary tests

Status: Done. Driver tests prove `plan_chart_data` records dependencies without
loading; cache tests prove unchanged sources reuse schemas and changed sources
reload (counting I/O), and that error kinds stay distinct and are never served
stale. LSP tests cover primary and named-table cache invalidation. CLI one-shot
behavior is exercised by the existing `algraf-cli` tests and example generation
stays byte-for-byte stable.

Acceptance criteria:

- Driver tests prove plan construction does not load data.
- Cache tests prove unchanged files reuse schemas and changed files reload.
- LSP tests cover primary and named-table cache invalidation.
- CLI tests prove one-shot render/check/schema/ir behavior remains unchanged.
- Example generation remains byte-for-byte stable.

### 6. Spec, plan, and example hygiene

Status: Done. Workspace and VS Code versions are bumped to `0.16.0`. Spec §10.9
now describes the driver-owned, fingerprint-validated cache; §21.3 reflects the
`Arc<InMemorySchemaCache>` backend field; §23.2 lists the chart data plan and
schema cache service under the driver's responsibilities. Examples were
regenerated with `./examples/generate.sh` and `git diff -- examples` is empty.

Acceptance criteria:

- Workspace version is bumped to `0.16.0` when the release branch is ready.
- Spec §10.9, §21.3, §21.4, §23, and §24 are updated for cache responsibility
  clarifications if the implementation changes the intended architecture.
- This plan is updated as each item completes, is rejected, or moves scope.
- Examples are regenerated with `./examples/generate.sh`; `git diff -- examples`
  must be empty.

## v0.16.0 Should

### Analysis reuse audit

Status: Deferred to a later release. Schema caching is now shared, but a formal
audit of remaining repeated LSP analysis work was not undertaken this release.
The most visible remaining duplication: `analyze_document` re-parses and
re-resolves named-table schemas on every change even when only unrelated text
changed, and `did_change` always reanalyzes the whole document.

### Lightweight benchmarks

Status: Deferred to a later release. The project still has no stable benchmark
harness, so adding latency tests now would risk machine-specific CI flakiness.

## Explicitly Deferred Past v0.16.0

- Full data-frame cache.
- Render preview SVG cache.
- Persistent disk cache.
- Query-driven or `salsa`-style compilation.
- Incremental text synchronization.
- Async driver APIs.
- Lazy data engine or renderer-delayed materialization.
- New data formats, source syntax, CLI flags, LSP features, or render behavior.

## Optional-Item Audit

### Promote In v0.16.0 (Must)

- Shared data source key and fingerprint.
- Driver schema cache service.
- Public chart data plan.
- LSP cache migration.
- Phase-boundary tests.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- Analysis reuse audit.
- Lightweight benchmarks.

### Keep Deferred

- Full data caching, render caching, query databases, async APIs, incremental
  sync, lazy execution, and new user-facing capabilities.

## Promotion Workflow

1. Add guard tests for current LSP schema diagnostics and CLI one-shot loads.
2. Extract shared data source key and fingerprinting.
3. Add the optional schema-cache service.
4. Promote a public chart data plan.
5. Migrate LSP primary and named-table schema resolution to the shared cache.
6. Add invalidation and no-load plan tests.
7. Run formatter, clippy, workspace tests, `./examples/generate.sh`, and require
   an empty `git diff -- examples`.
