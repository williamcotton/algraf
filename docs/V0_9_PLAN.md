# Algraf v0.9.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_8_PLAN.md`](V0_8_PLAN.md)
Follow-on plans: [`V0_10_PLAN.md`](V0_10_PLAN.md),
[`V0_11_PLAN.md`](V0_11_PLAN.md), [`V0_12_PLAN.md`](V0_12_PLAN.md)

## Purpose

This document defines the intended v0.9.0 release shape: unifying Algraf's
compiler/data-loading pipeline before deeper internal refactors.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when syntax, diagnostics, tests, and
examples land together.

## Release Thesis

v0.9.0 is a **pipeline unification** release: make CLI and LSP use one shared
source-expression, data-loading, schema-loading, analysis, and render-preparation
path.

The release deliberately avoids the bigger analyzer and renderer surgeries. It
does not split `analyzer.rs`, redesign stat options, introduce diagnostic-code
enums, or reorganize geometry rendering. Those are follow-on releases. v0.9.0
creates the foundation they need: one driver boundary and one source model, so
later moves do not preserve duplicated orchestration in nicer files.

The desired outcome is no source-language or generated-output change. Because
this roadmap is still pre-release, CLI/LSP surface details may change when the
centralized driver gives a cleaner design, but those changes must be explicit in
tests and spec updates rather than accidental drift. The concrete payoff is
removing drift between:

- `crates/algraf-cli/src/input.rs`;
- `crates/algraf-cli/src/main.rs`;
- `crates/algraf-lsp/src/lib.rs` schema resolution;
- `crates/algraf-lsp/src/lib.rs` preview rendering;
- `crates/algraf-semantics/src/analyzer.rs` source-expression recognition.

## Current Debt Surface

The refactor notes found several source/pipeline duplications that should be
fixed first:

- `strip_string` exists independently in CLI and LSP, while semantics has the
  same algorithm under `string_value`.
- source-constructor extraction for `GeoJson(...)` and `Shapefile(...)` is
  repeated in CLI, LSP, and semantics.
- LSP preview still has its own named-table loader and currently handles fewer
  source forms than the newer schema-resolution path.
- CLI commands perform parse, load, analyze, theme, render, warning, and
  diagnostics orchestration directly.
- LSP has a private `node_span` copy even though the same helper exists in
  semantics.

These are the safest high-payoff cleanups because they mostly move boundary code
without changing the language, analyzer, or renderer internals.

## Scope Rules

- No new Algraf source syntax, geometries, stats, data formats, projections, or
  output formats.
- CLI and LSP diagnostic code, severity, or span changes are allowed only when a
  test and any relevant spec update document an intentional design correction.
- Rendered SVG/PNG examples should remain byte-for-byte unchanged.
- The driver must not depend on CLI or LSP crates.
- CLI remains responsible for command parsing, terminal rendering of
  diagnostics, output paths, PNG conversion, and exit codes.
- LSP remains responsible for protocol types, editor caches, and request
  handling.
- Keep dependencies pure-Rust and offline; do not introduce async, networking,
  database drivers, or service processes.

## Capstone Acceptance Target

Run the full no-regression pipeline:

```bash
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

The current checked-in examples are the visual regression baseline:
`git diff -- examples` must be empty after regeneration.

## Design Decisions (settled)

1. **Create a reusable driver boundary.** Add `algraf-driver` or an equivalent
   non-UI module. Do not continue growing CLI helper APIs.
2. **Represent sources once.** `Chart(data:)` and `Table name = <source>` should
   share a typed source-expression model.
3. **Move shared literal/source helpers down.** Literal unescaping and path/source
   extraction should live in a crate that CLI, LSP, and semantics can all use.
4. **Migrate callers before splitting monoliths.** The LSP and CLI should first
   use the driver. File/module splits come later, after duplicate behavior is
   gone.

## v0.9.0 Must

### 1. Shared literal and source helpers

Status: Not started.

Acceptance criteria:

- One canonical double-quoted string literal unescape function is exposed from a
  shared crate (`algraf-syntax` is the likely home because it owns token/AST
  interpretation). CLI, LSP, and semantics stop carrying local copies.
- Backtick quoted-identifier unescaping remains distinct but is adjacent enough
  to make future escape-rule changes obvious.
- `node_span` is exposed from a shared crate or reused from one existing helper;
  LSP no longer has a private reimplementation.
- Source constructor names (`GeoJson`, `Shapefile`) are represented in one shared
  place rather than manually matched in three crates.
- Tests cover escape handling and source-constructor extraction for the cases
  already supported by examples.

### 2. Shared source-expression model

Status: Not started.

Create a typed model for Algraf source expressions.

Minimum shape:

```rust
enum SourceExpr {
    Path { path: String, format: Option<Format> },
    Stdin,
    Missing,
    Invalid,
}
```

Acceptance criteria:

- `Chart(data:)` and chart-scoped `Table` declarations use the same extractor.
- Plain string paths select format by extension.
- `GeoJson("...")` and `Shapefile("...")` set explicit formats.
- `stdin` is represented only where allowed by the current language rules.
- Invalid/missing sources preserve the existing semantic diagnostics.
- Extraction has unit tests for primary data, named tables, constructors,
  malformed constructors, and missing values.

### 3. Driver crate/module

Status: Not started.

Create the reusable non-UI pipeline used by CLI and LSP.

Acceptance criteria:

- The driver exposes typed APIs for:
  - parsing source and returning parse diagnostics;
  - extracting chart data sources and table declarations;
  - resolving paths relative to a source file, stdin input, or `--base-dir`;
  - applying the CLI `--data` override rules;
  - loading full data frames;
  - loading sampled schemas;
  - loading named table frames/schemas;
  - analyzing one chart or every chart in a document;
  - preparing SVG render input.
- The driver owns stdin conflict rules, multi-chart source handling, and named
  table loading policy.
- The driver returns structured errors/diagnostics; it does not print, write
  files, choose output paths, rasterize PNGs, or speak LSP.
- Driver tests cover CSV, TSV, JSON, NDJSON, GeoJSON, Shapefile, named tables,
  source constructors, missing files, unreadable/malformed data, multi-chart
  documents, and stdin conflicts.

### 4. CLI migration to driver

Status: Not started.

Acceptance criteria:

- `algraf render`, `check`, `schema`, and `ir` delegate parse/load/analyze/render
  preparation to the driver.
- CLI command parsing, human/JSON diagnostic rendering, output path expansion,
  PNG writing, `--strict`, `--debug-layout`, `--emit-metadata`, and exit codes
  stay in the CLI crate.
- `crates/algraf-cli/src/input.rs` is deleted or reduced to thin CLI-specific
  adapters.
- CLI tests continue to pass and include at least one named geospatial table
  case to prove source constructors survive the migration.

### 5. LSP schema/preview migration to driver

Status: Not started.

Acceptance criteria:

- LSP schema resolution uses driver source extraction and schema loading.
- LSP named-table schema resolution uses the same table extraction as the CLI.
- LSP preview rendering uses the driver for primary data and named table frames.
- LSP preview supports every named table source form the CLI supports, including
  `GeoJson(...)` and `Shapefile(...)`.
- LSP may keep its cache policy, but cached values are keyed around driver
  results rather than local parsing rules.
- Local LSP copies of `strip_string`, `source_constructor_path`, and preview
  named-table loading are removed.
- Existing LSP tests pass; add focused tests for source constructors and preview
  table parity.

### 6. Spec, version, and example hygiene

Status: Not started.

Acceptance criteria:

- `Cargo.toml` workspace version and `editors/vscode/package.json` are bumped to
  `0.9.0` when the release branch is ready.
- Spec §30.4 lists this release and the follow-on refactor releases.
- If a new `algraf-driver` crate is introduced, spec §23, the README workspace
  layout, and the workspace-layout table in `CLAUDE.md` are updated to describe
  the eighth crate and its dependency direction in the same change. If the driver
  ships as a module inside an existing crate instead, that decision is recorded
  here so the "seven crates" documentation stays accurate.
- User-visible behavior clarified by driver centralization is promoted into the
  relevant normative spec section before the release is marked complete.
- README active-plan references point to this staged refactor roadmap.
- Examples are regenerated with `./examples/generate.sh`; `git diff -- examples`
  must be empty for current checked-in examples.
- This document is updated as each item completes, is rejected, or moves scope.

## v0.9.0 Should

### Source diagnostics normalization

Status: Not started.

If the driver exposes enough structure, normalize data-loading diagnostics for
CLI and LSP so missing/unreadable/malformed data paths use one code mapping.
Keep any diagnostic-code changes explicit: tests and spec updates should show the
old drift and the new centralized mapping.

### Minimal LSP file split

Status: Not started.

After driver migration, optionally split only the LSP source/schema/preview
pieces into separate modules. Full LSP modularization is v0.12 scope.

## Explicitly Deferred Past v0.9.0

- Semantic analyzer module split, argument helper cleanup, typed stat options,
  and high-level geometry lowering cleanup: [`V0_10_PLAN.md`](V0_10_PLAN.md).
- Render planner, geometry renderer, SVG writer, and render helper cleanup:
  [`V0_11_PLAN.md`](V0_11_PLAN.md).
- LSP module split, diagnostic code registry, and parser recovery cleanup:
  [`V0_12_PLAN.md`](V0_12_PLAN.md).
- New language features, output formats, backends, network sources, plugins,
  interactive output, and dataframe backend swaps.

## Optional-Item Audit

### Promote In v0.9.0 (Must)

- Shared literal/source helpers.
- Shared source-expression model.
- Driver crate/module.
- CLI migration to driver.
- LSP schema/preview migration to driver.
- Spec, version, and example hygiene.

### Consider If Capacity Allows (Should)

- Source diagnostics normalization.
- Minimal LSP file split around source/schema/preview.

### Keep Deferred

- Analyzer, renderer, full LSP, parser, and diagnostic-registry cleanup.
- New user-visible language/runtime features.

## Promotion Workflow

1. Add driver tests around current CLI/LSP source behavior before deleting local
   loaders.
2. Move shared literal/source helpers first.
3. Introduce the driver behind existing CLI/LSP behavior.
4. Migrate one CLI command at a time.
5. Migrate LSP schema and preview paths.
6. Remove duplicated helper functions only after their callers are migrated.
7. Run `cargo test --workspace`, `./examples/generate.sh`, and
   `git diff -- examples`; the examples diff must be empty before marking any
   Must item complete.
