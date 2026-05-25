# Algraf v0.13.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_12_PLAN.md`](V0_12_PLAN.md)

## Purpose

This document defines the intended v0.13.0 release shape: cleaning up the shared
driver after the v0.9 through v0.12 modularization work moved source resolution,
data loading, semantic preparation, rendering, and editor behavior into clearer
crate boundaries.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.13.0 is a **driver cleanup and preparation** release: simplify
`algraf-driver` now that CLI and LSP usage has settled, reduce repeated context
plumbing, centralize source/path resolution, and make the driver easier to
evolve toward future VFS or data-loading abstractions without committing to
those larger changes yet.

This release should not add language features, editor features, render features,
or data formats. The goal is to pay down the natural debt left after extracting
the driver crate: orchestration logic is now centralized, but the internal
structure still looks like first-pass extraction code in places.

## Current Debt Surface

The refactor survey found:

- `crates/algraf-driver/src/lib.rs` owns the right responsibilities, but it is a
  single large module with source resolution, data/schema loading, named table
  preparation, error formatting, stdin handling, and tests interleaved.
- The same contextual values are passed through many helper functions:
  `SourceInput`, `base_dir`, `data_override`, and `multi_chart`.
- Path and source resolution behavior is spread across several helpers, making
  precedence rules harder to audit as CLI and LSP use cases evolve.
- Data loading and schema loading contain similar path/format dispatch logic.
- `DriverError` is manually formatted even though the workspace already depends
  on `thiserror`.
- Some broader architectural ideas are attractive for later, but are too large
  for this release without a concrete performance or product need.

## Scope Rules

- No user-facing behavior change is required. CLI output, LSP behavior, parser
  behavior, semantic behavior, render output, and data loading semantics should
  remain stable unless a test exposes real drift that should be documented.
- Keep existing public driver wrapper functions available for CLI and LSP
  compatibility.
- The driver must continue to avoid dependencies on CLI, LSP, or render crates.
- Do not introduce a VFS, async API, query database, or new caching policy in
  this release.
- Do not make contextual source constructors generic runtime strings.
  `GeoJson(...)` and `Shapefile(...)` remain language syntax unless a future
  spec change deliberately revisits the source model.
- If behavior changes intentionally, update the spec in the same implementation
  change. If the work is purely internal, the spec may remain unchanged.

## Capstone Acceptance Target

The capstone is driver cleanup without behavior drift:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Current checked-in examples should remain the visual regression baseline. If an
implementation of this plan changes rendered examples, treat that as unexpected
unless the behavior change is explicitly promoted into the spec and release
scope.

## Design Decisions (settled)

1. **Preserve the public driver surface.** Add cleaner internal structure first;
   do not force CLI and LSP through a breaking API migration.
2. **Use a driver environment internally.** Bundle repeated source/load context
   into one internal value so future options do not widen every helper
   signature.
3. **Centralize path resolution before adding abstraction.** Make current path
   behavior easy to understand before considering VFS or fetcher traits.
4. **Prefer small, testable refactors.** Loader unification is welcome only when
   it improves readability.
5. **Defer speculative architecture.** VFS, async, caching, demand-driven
   compilation, lazy data engines, and render backend plugins need their own
   product or performance justification.

## v0.13.0 Must

### 1. Driver environment

Status: Completed.

Acceptance criteria:

- `algraf-driver` has an internal context type, such as `DriverEnv<'a>`, that
  bundles the repeated driver inputs:
  - `source_input: &'a SourceInput`;
  - `base_dir: Option<&'a Path>`;
  - `data_override: Option<&'a str>`;
  - `multi_chart: bool`.
- Public entry points continue to accept the existing argument shapes where CLI
  and LSP already depend on them.
- `PrepareOptions` either converts into the new internal context or becomes the
  public construction point for it without requiring caller churn.
- The new context does not own loaded data, cache state, file system handles, or
  renderer-specific behavior.

### 2. Source and path resolution consolidation

Status: Completed.

Acceptance criteria:

- Source base directory resolution, relative path resolution, chart data source
  resolution, document-level source resolution, named table source resolution,
  and `data_override` handling are centralized behind the driver context or a
  small private resolver owned by the driver.
- Current precedence is preserved:
  - explicit `base_dir` takes precedence for relative paths;
  - otherwise a path source resolves relative to the source file parent;
  - stdin has no implicit file parent;
  - `data_override` overrides the primary data source where current behavior
    allows it;
  - named table sources resolve from the same source/base context as today.
- Focused tests cover relative paths, absolute paths, stdin source context,
  explicit `base_dir`, `data_override`, chart-level data sources, and named table
  sources.
- Public helper functions such as path/source resolution wrappers remain
  available unless all current callers are migrated in the same release without
  widening scope.

### 3. Driver error cleanup

Status: Completed.

Acceptance criteria:

- `DriverError` uses `thiserror` for `Display` and `Error` implementations.
- Existing error variants remain structured enough for CLI and LSP mapping.
- User-facing wording stays as close as practical to current output.
- LSP diagnostic-code mapping for driver/schema errors remains stable.
- Tests or snapshots that assert error strings are updated only for deliberate,
  documented wording improvements.

### 4. Loader duplication cleanup

Status: Completed.

Acceptance criteria:

- Path/format dispatch shared by data and schema loading is reduced where doing
  so makes the code easier to audit.
- Explicit-format and inferred-format paths remain tested.
- `LoadContext` continues to distinguish primary data from named table data in
  errors.
- Stdin loading remains explicit and CSV-only unless a future release changes
  stdin format semantics.
- The refactor does not introduce generic loader abstractions that obscure the
  two real operations the driver performs today: loading data frames and loading
  schemas.

### 5. Driver test coverage

Status: Completed.

Acceptance criteria:

- Driver tests cover the public wrappers as well as the new internal resolution
  path.
- Existing CLI and LSP integration expectations continue to pass through the
  driver without local behavior forks.
- Tests avoid relying on global process state except where stdin behavior itself
  is being exercised.
- Coverage is added near the driver code rather than by expanding unrelated CLI
  or LSP test suites.

### 6. Spec, plan, and release hygiene

Status: Completed.

Acceptance criteria:

- Workspace version is bumped to `0.13.0` when the release branch is ready.
- This plan is updated as each item completes, is rejected, or moves scope.
- [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is updated only for intentional behavior or
  responsibility changes.
- Examples remain unchanged unless an intentional promoted behavior change
  requires regeneration and documentation.

## v0.13.0 Should

### Private preparation boundary

Status: Completed.

If it falls out naturally from the driver environment work, introduce a small
private resolved-input or preparation structure that separates "resolve what will
be loaded" from "load data and analyze". This should remain private in v0.13.0;
do not expose a public execution-plan API until there is a real caller for it.

### Driver module split

Status: Completed.

If `algraf-driver/src/lib.rs` remains difficult to navigate after the Must
items, split private implementation into focused modules such as resolution,
loading, errors, and tests. Keep the public crate API stable from the caller's
perspective.

### LSP local-helper audit

Status: Completed.

Review LSP source/schema/preview code for helper logic that can be deleted after
driver resolution is clearer. Keep the LSP's existing schema cache policy unless
there is evidence that a driver-level cache is needed.

The audit found that LSP schema caching still belongs in `algraf-lsp`; the
remaining source/schema/preview helpers already call driver resolution wrappers,
so no local behavior fork needed deletion in this release.

## Explicitly Deferred Past v0.13.0

- VFS or injected file system abstraction.
- Async driver APIs or async data loading.
- Driver-level schema/data cache.
- Query-driven or `salsa`-style compilation.
- Lazy data engine or renderer-delayed data materialization.
- Pluggable render backends.
- Generic runtime source-format constructors replacing `GeoJson(...)` and
  `Shapefile(...)`.
- New CLI flags, LSP features, data formats, chart syntax, or renderer features.

## Optional-Item Audit

### Promote In v0.13.0 (Must)

- Driver environment.
- Source and path resolution consolidation.
- Driver error cleanup.
- Loader duplication cleanup.
- Driver test coverage.
- Spec, plan, and release hygiene.

### Consider If Capacity Allows (Should)

- Private preparation boundary.
- Driver module split.
- LSP local-helper audit.

### Keep Deferred

- VFS, async, caching, query-driven compilation, lazy data engines, and backend
  plugins.
- Source-format language changes.
- New user-facing capabilities.

## Promotion Workflow

1. Add driver tests around current source/path/data behavior before moving
   internals.
2. Introduce the internal driver environment while preserving public wrappers.
3. Move source and path resolution behind the environment.
4. Convert `DriverError` to `thiserror`.
5. Reduce loader duplication where the resulting code stays explicit.
6. Audit CLI and LSP call sites for compatibility and removable local helpers.
7. Run formatter, clippy, and workspace tests.
8. Update this plan's `Status:` lines as items land, defer, or are rejected.
