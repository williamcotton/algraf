# Algraf v0.35.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_34_PLAN.md`](V0_34_PLAN.md)
Follow-on plan: [`V0_36_PLAN.md`](V0_36_PLAN.md)
Prior art: [`ARCHITECTURE_REVIEW.md`](ARCHITECTURE_REVIEW.md) (v0.34 architecture
assessment that sourced this plan's work items).
Roadmap theme: internal architecture hardening before the ggplot2-comparability
feature roadmap (v0.36–v0.42) lands more stats, geometries, and diagnostics.

## Purpose

This release is an **internal health release**, not a language release. It pays
down the architectural debt identified in the v0.34 architecture review so that
the seven feature releases that follow (v0.36–v0.42) can add stats, geometries,
properties, and diagnostics onto decomposed modules and generated registries
instead of onto god-files and hand-maintained triple-entry tables.

Algraf has a strong tradition of dedicated hardening releases — v0.10
(analyzer modularization), v0.11 (renderer modularization), v0.12 (parser
cleanup), v0.13 (driver cleanup), v0.17 (render execution boundary), and v0.18
(semantic surface hardening). v0.35 continues that tradition. The work here is
sequenced deliberately *before* the feature roadmap because several items are
prerequisites:

- The feature roadmap's stats-heavy releases (**v0.38** z-field statistics and
  **v0.39** model/summary stats) would otherwise pour contour, 2D-density,
  summary-grid, ECDF, QQ, and summary-bin code into the already 1,706-LOC
  `render/stats.rs`.
- Every feature release adds geometries, properties, and diagnostics, each of
  which today requires hand-edits across ~7 registry sites and the diagnostic
  triple-entry. Generating those registries first makes each later release
  smaller and harder to get wrong.

## Release Thesis

v0.35.0 is the **architecture hardening** release. Its success criterion is
*behavioral invisibility*: decomposing modules, generating registries, and
adding test coverage MUST NOT change any rendered SVG, draw-list JSON, raster
output, interaction sidecar, diagnostic code, or LSP response. The only intended
external change is additive LSP/diagnostic guidance explicitly promoted in the
Should items below.

This is the inverse discipline of the feature plans: where they require
*new sugar to be byte-for-byte identical to its primitive lowering*, this release
requires *refactored internals to be byte-for-byte identical to the v0.34
baseline*.

## Scope Rules

- **No behavior change in Must items.** Each refactor MUST be covered by an
  output-equivalence check against the v0.34 baseline (existing snapshot/insta
  tests plus the full example corpus via `./examples/generate.sh` with an empty
  `git diff -- examples`).
- **No new language surface.** No new geometries, properties, stats, scales,
  themes, or CLI flags. Behavior-changing items (new diagnostics, new LSP hover
  content) are confined to the Should section and each must reserve spec text /
  diagnostic codes before implementation per the normal promotion workflow.
- **Generated registries are the single source of truth.** Where a macro or
  build step replaces a hand-maintained table, the generated output MUST match
  the current table exactly (assert equality in a migration test before deleting
  the hand-written form).
- **Determinism is a tested contract, not a convention.** New determinism tests
  must fail if a stat's output ordering becomes input-order- or hash-dependent.
- **Module splits preserve public APIs.** Re-exports MUST keep each crate's
  existing `pub use` surface stable so downstream crates and the spec's §23.2
  module-boundary description stay accurate.

## Source of Work Items

Every item below maps to a finding in
[`ARCHITECTURE_REVIEW.md`](ARCHITECTURE_REVIEW.md) §3–§5. The three documentation
drifts called out in review §5 Tier 1 (CLAUDE.md crate count, spec status line)
have already been fixed and are not repeated here.

## v0.35.0 Must

### 1. Decompose `render/stats.rs` and encode the determinism contract

Status: Implemented.
Review ref: §3.5 (god-module), §4 (determinism enforcement). Unblocks v0.38 and
v0.39.

- Split `crates/algraf-render/src/stats.rs` (1,706 LOC) into a `stats/` module
  tree, e.g. `stats/bin.rs` (1D/2D/hex/temporal binning), `stats/density.rs`
  (KDE), `stats/smooth.rs` (LOESS), `stats/summary.rs` (count, quantiles), and
  `stats/util.rs` (shared numeric helpers, sorting).
- Keep the crate's `stats` API and all call sites unchanged; this is a
  file-organization change only.
- Encode "output rows are produced in a stable, input-order-independent order"
  as an explicit contract at the module boundary (a small helper or newtype that
  every stat funnels its output through), so a new stat cannot silently skip
  sorting.
- Verify byte-identical stat output and rendered examples against the v0.34
  baseline.

### 2. Add a per-stat determinism test harness

Status: Implemented.
Review ref: §3.5, §4 (spec §18.12 is mandated but only LOESS is tested).

- Add tests that run each stat twice — including against shuffled input rows —
  and assert identical output: `Bin`, `Bin2D`, `HexBin`, temporal/calendar bins,
  `Density`, `Count`, and boxplot quantiles, joining the existing LOESS test.
- Place these in `crates/algraf-render/tests/` per the spec §27 testing
  categories; document the new "determinism" test category in spec §27.
- This harness is the regression net that makes the v0.38/v0.39 stat additions
  safe; it must exist before those releases add stats.

### 3. Generate the diagnostic-code registry

Status: Implemented.
Review ref: §3.1 (manual triple-entry across the `codes` module, `all_codes()`,
and spec §26).

- Replace the hand-maintained constant list + `all_codes()` array in
  `crates/algraf-core/src/diagnostic.rs` with a single declarative source (e.g.
  a `register_codes!` macro) that emits both the `codes::*` constants and the
  `all_codes()` slice from one place.
- Preserve the existing wire strings exactly; the three existing registry tests
  (`registered_codes_are_unique_and_well_formed`,
  `spec_diagnostic_catalog_is_registered`,
  `production_sources_use_registered_constants`) MUST still pass unchanged.
- No diagnostic codes are added, removed, or renumbered in this release.

### 4. Compile-check the property / geometry registry

Status: Implemented.
Review ref: §3.4 (PropertyKey/GeometryKind vocabulary hand-maintained in ~7
sites with no compile-time agreement check). Unblocks every feature release that
adds a geometry or property.

- Make the `PropertyKey` and `GeometryKind` spellings, their `as_str` /
  `display_name` / `css_class` mappings, the `PROPERTY_KEYS` array, and the
  `registry.rs` prop-spec names derive from one source (proc-macro, `build.rs`,
  or at minimum a test that asserts the cross-site agreement).
- The goal is that a future geometry or property whose registry entry disagrees
  with its enum spelling fails to compile (or fails a focused test), rather than
  surfacing as a runtime mismatch.
- Generated/derived output MUST equal the current tables before the hand-written
  forms are removed.

### 5. Factor the histogram family in `semantics/lowering.rs`

Status: Implemented.
Review ref: §3.4 (lowering.rs is the 1,039-LOC complexity hotspot with three
near-parallel histogram desugarings).

- Extract the shared "build a `Bin` `DeriveIr`, synthesize `bin_start`/`count`,
  emit a `Rect`" pattern behind one builder used by `desugar_histogram`,
  `blended_histogram`, `grouped_histogram`, and the freq-poly / bin2d paths.
- IR output MUST be unchanged: the analyzer's `ChartIr` for every existing
  example/fixture stays byte-identical.
- This removes the duplication the v0.36 (primitive transforms) and v0.37
  (interval transforms) releases would otherwise copy again.

### 6. Split `syntax/parser.rs`

Status: Implemented.
Review ref: §3.2 (1,188-LOC file mixing cursor, tree-building, block/value/
algebra parsing, and post-parse validation).

- Split along the seams the test suite already implies: token cursor,
  tree-building primitives, block/declaration parsing, value parsing, algebra
  (Pratt) parsing, and the post-parse validator (`validate_source_header`,
  gated-constructor checks).
- Keep `parse`, `parse_algebra`, and the `Parse` API unchanged; all existing
  `algraf-syntax` tests (lexer, block, algebra, resilience, formatter) MUST pass
  without modification.

## v0.35.0 Should

### Decompose `render/space.rs` and share polar helpers

Status: Implemented.
Review ref: §3.5 (1,180-LOC module bundling axis training, temporal formatting,
polar math, nested-band algebra, perimeter-label estimation; polar variants
re-implemented per geometry).

- Extract `space/polar.rs` and `space/temporal.rs`, leaving a focused
  axis-training core, and share one polar rendering helper instead of the
  separate implementations in `geom/bar.rs`, `geom/line.rs`, and
  `geom/rect_tile.rs`.
- Output equivalence required; defer if the polar/temporal extraction risks
  visual drift that cannot be fully snapshot-verified this cycle.

### Tidy the driver IO surface

Status: Implemented.
Review ref: §3.6 (sync/async loader duplication where no caller uses the async
path; temporal-policy threaded through 5–6 positional params).

- Either delete the unused `*_with_async_io` loading surface until a consumer
  needs it, or consolidate it now that `async fn` in traits is available.
- Bundle the temporal-parse-policy parameters into a small `DataLoadingContext`
  struct to shorten the loader signatures.
- Public driver API changes must keep the CLI, LSP, and WASM call sites
  compiling and behaving identically.

### Decouple LSP analysis from document management

Status: Implemented.
Review ref: §3.7 (`upsert_document` couples parse + schema + analyze + insert +
publish; first-chart-only table resolution is undocumented).

- Split the pure analysis step from `update_document` so analysis can be skipped
  on no-op edits and tested in isolation.
- Either resolve named-table schemas for all charts in a document or document the
  first-chart-only limitation explicitly in spec §21.

### Guard the SQLite stub/real parity and surface inferred types

Status: Implemented.
Review ref: §3.3 (no compile-time parity between `sqlite.rs` and
`sqlite_stub.rs`; silent `Mixed → String` inference is invisible to authors).

- Add a parity guard (shared trait or at minimum a cross-linking test/comment)
  so the `sql`-off stub cannot drift from the real module's signatures.
- Consider surfacing inferred column type and sample values in LSP hover so a
  silent `Mixed → String` fallback is visible. This is a behavior change to
  spec §21 and MUST reserve the spec text before implementation.

### CLI output-path cleanup

Status: Implemented.
Review ref: §3.8 (monolithic `render_cmd` / `prepare_render_inputs`).

- Extract a `write_outputs` helper and a small backend abstraction over the
  SVG / draw-list / raster output forms to make the render command testable.
  Behavior and output bytes unchanged.

## Explicitly Deferred Past v0.35.0

- Geometry input validation (NaN/Inf coordinates, ring winding) for
  GeoJSON/TopoJSON/Shapefile loaders. This adds new diagnostics, so it belongs
  with a data/geo feature release and MUST reserve a diagnostic code in spec §26
  first; it is a natural companion to the v0.38 z-field grid-validation work.
- Any new geometry, stat, scale, theme, or CLI surface — those resume at v0.36.
- Persistent / full-frame / render-result caching (already deferred by
  [`CACHE_POLICY.md`](CACHE_POLICY.md)).
- A trait-based pluggable dataframe backend (Polars/DuckDB). The `Table`
  boundary already permits it; actually adding one is out of scope here.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples   # MUST be empty: refactors are behavior-invisible
```

The empty `git diff -- examples` is the load-bearing check for this release: it
proves the decomposition and registry generation did not alter any rendered
output.

## Promotion Workflow

1. For each Must item, land the refactor behind the existing tests plus a
   migration/equivalence check that asserts identical output to the v0.34
   baseline before removing any hand-written table.
2. Update spec §23.2 (module boundaries) and §27 (testing strategy) to describe
   the new `stats/` and `space/` module layout and the determinism test
   category. These are descriptive updates that match the implementation; they
   do not add normative behavior.
3. For any Should item that changes behavior (LSP hover, new diagnostics),
   reserve the spec text / diagnostic code first, then implement, test, and add
   an example.
4. Run the required checks; the empty examples diff is mandatory for every Must
   item.
