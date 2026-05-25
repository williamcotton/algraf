# Algraf v0.17.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_16_PLAN.md`](V0_16_PLAN.md)
Follow-on plan: [`V0_18_PLAN.md`](V0_18_PLAN.md)

## Purpose

This document defines the intended v0.17.0 release shape: tightening the render
execution boundary after driver I/O, diagnostics, and schema planning have been
cleaned up.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.17.0 is a **render execution boundary** release: prepare the renderer for
future backend and lazy-data work without adding a new backend, output format,
interactive surface, or data engine.

The release should complete the refactor-only runway before new language
features are reconsidered. SVG output from `examples/generate.sh` is the guard
rail: this plan is successful only if the renderer becomes easier to extend
while the checked-in examples remain unchanged.

## Current Debt Surface

The deferred-item audit found:

- v0.11 intentionally deferred a guide-focused module split.
- v0.11 also deferred a full SVG DOM or retained scene graph.
- v0.13 deferred lazy data engines and pluggable render backends.
- `algraf-render` is modularized, but SVG emission remains the only backend
  shape the crate can express.
- Stats, scales, domains, guide planning, and SVG emission are easier to read
  than before, but the boundary between "planned visual scene" and "SVG bytes" is
  still mostly implicit.

## Scope Rules

- No new rendering features.
- No new output formats, raster backend changes, Canvas/WebGL backend, DOM scene
  graph, interactivity, or animation.
- SVG output should remain byte-for-byte identical for current checked-in
  examples.
- Existing `render` and `render_with_tables` callers keep working.
- Do not change semantic IR or source syntax.
- Do not introduce lazy data materialization or a Polars backend in this release.
- Any SVG whitespace, attribute order, precision, or class-name change must be
  intentional, reviewed, and documented.

## Capstone Acceptance Target

The capstone is render boundary cleanup with no output drift:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test -p algraf-render
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

`git diff -- examples` must be empty. Running `examples/generate.sh` should not
change what happens for any checked-in example.

## Design Decisions (settled)

1. **Keep SVG as the only backend.** A backend seam can exist, but it should have
   exactly one implementation in v0.17.0.
2. **Prefer explicit render planning over retained DOM.** Do not add a scene
   graph unless a concrete backend needs it.
3. **Finish guide cleanup before new visuals.** Axes and legends are core output;
   their planning and emission should be clear before adding features.
4. **Keep data materialization eager.** Lazy execution is a later release.
5. **Snapshot equivalence beats theoretical purity.** Refactors stop if they
   cannot preserve current SVG without a deliberate spec change.

## v0.17.0 Must

### 1. Render model boundary audit

Status: Done. The planning/emission boundary is documented in spec §24.6 and in
`algraf-render`'s crate-level docs (`lib.rs`), with module docs on `render.rs`,
`render/backend.rs`, `render/derived.rs`, and `guide/`. The render scene is the
named intermediate; `Panel`, `RenderScene`, and the stat/derived frames stay
private to the crate. No API or SVG behavior changed.

Acceptance criteria:

- Document the current boundary between semantic IR, loaded data, derived data,
  trained scales, layout, guides, legends, geometry emission, and SVG document
  emission.
- Identify which intermediate structures are stable enough to name and which
  should remain private implementation details.
- Add or update module-level docs in `algraf-render` for the final architecture.
- No API or SVG behavior changes are required for the audit itself.

### 2. Private backend seam for SVG emission

Status: Done. `render/backend.rs` introduces a private `RenderScene` plus a
private `RenderBackend` trait whose sole implementation is `SvgBackend`.
`render_with_tables` now builds a scene in the planning half and hands it to the
backend in the emission half. Public `render`/`render_with_tables` signatures and
result types are unchanged, escaping/number formatting/class names/ordering are
untouched, and no plugin API is exposed. `git diff -- examples` is empty.

Acceptance criteria:

- Introduce a private trait, enum, or facade that separates planned render
  operations from SVG string writing where doing so reduces coupling.
- The only implementation is the current SVG backend.
- Existing public render functions return the same result types.
- Escaping, number formatting, class naming, clip-path IDs, metadata, and
  deterministic ordering remain unchanged.
- The seam does not expose a plugin API.

### 3. Guide planning and module split

Status: Done. `guide.rs` is now a `guide/` module: `plan.rs` holds the layout
math (tick-label width estimation, y-axis margin reservation, tick anchoring) and
`emit.rs` holds the SVG writers (grid, axes, facet strips, legends). Tick
generation, label suppression, legend merging, ordering, and theme use are
unchanged. Coverage spans axis labels, suppressed titles, categorical legends,
continuous legends, and merged fill/stroke legends, plus a new guard
(`test_wide_y_tick_labels_reserve_more_left_margin`) for the margin-reservation
path.

Acceptance criteria:

- Complete the guide-focused cleanup deferred from v0.11.
- Split guide code into focused planning and SVG emission modules if the current
  shape still mixes axis/legend decisions with low-level writing.
- Axis tick generation, label suppression, legend merging, guide ordering, and
  theme use remain unchanged.
- Tests cover at least axis labels, suppressed guide labels, categorical legends,
  continuous legends, and merged fill/stroke legends.

### 4. Data/stat execution boundary cleanup

Status: Done. `render/derived.rs` is documented as the derived-table execution
boundary: stats run once, eagerly, against inputs resolved through the `Table`
trait and materialize into owned `DataFrame`s. An audit confirmed planning code
reads loaded data only through `&dyn Table`; the only concrete `DataFrame` is the
owned stat output. Category ordering, missing-value handling, warnings, and SVG
determinism are unchanged.

Acceptance criteria:

- Clarify where derived-table execution, geometry-local stats, and scale training
  consume loaded data.
- Existing stat execution continues to use the current eager data model.
- Renderer helpers that only need table access use the existing dataframe/table
  abstraction rather than concrete internals where practical.
- No change to category ordering, missing-value behavior, warning behavior, or
  SVG determinism.

### 5. Render equivalence guard coverage

Status: Done. The 79 existing render snapshots still pass, plus a new guard for
the y-axis margin-reservation path. `./examples/generate.sh` produces an empty
`git diff -- examples`. No non-example SVG serialization changed.

Acceptance criteria:

- Add focused render tests before moving guide/backend internals if snapshot
  coverage is thin.
- Existing render snapshots continue to pass.
- `./examples/generate.sh` produces an empty `git diff -- examples`.
- Any unavoidable non-example SVG serialization changes require explicit review
  and tests, but should be avoided for this release.

### 6. Spec, plan, and example hygiene

Status: Done. Workspace and VS Code versions are bumped to `0.17.0`. Spec §18.7,
§19, §23, and the new §24.6 document the planning/emission boundary and the
single private backend. This plan records each item's outcome. Examples were
regenerated; `git diff -- examples` is empty.

Acceptance criteria:

- Workspace version is bumped to `0.17.0` when the release branch is ready.
- Spec §18, §19, §23, and §24 are updated only for intended architecture
  clarifications.
- This plan is updated as each item completes, is rejected, or moves scope.
- Examples are regenerated with `./examples/generate.sh`; `git diff -- examples`
  must be empty.

## v0.17.0 Should

### Backend extension note

Status: Done (note below).

A second backend would implement the private `RenderBackend` trait and consume a
`RenderScene`. The scene already carries resolved layout rectangles, trained
scales (via each `Panel`'s `ScaledSpace`), legends, per-panel geometry, and
theme, so a raster/canvas backend would not need to re-run planning. What it
would still need:

- **A primitive vocabulary.** The SVG backend writes element strings directly
  (`<rect>`, `<line>`, `<text>`, paths). A non-SVG backend needs those calls
  expressed as backend-neutral primitives (fill rect, stroke polyline, draw
  text with anchor/rotation, clip region) rather than pre-formatted SVG. Today
  geometry and guide emission still produce SVG strings; promoting them to a
  primitive interface is the main remaining work and is deliberately deferred.
- **Text metrics.** Layout reserves space using a coarse `0.6 * font_size`
  glyph estimate (`guide::plan::estimate_text_width`). A raster backend with
  real font metrics would want the estimate and the emission to agree.
- **Color/gradient handling.** Continuous fills currently resolve to discrete
  swatch colors; a backend wanting smooth gradients needs the stop list, not the
  sampled colors.
- **Determinism contract.** Any backend must preserve the deterministic ordering
  and number formatting rules of §18.

No second backend is added in this release.

### Render performance inventory

Status: Done (note below).

Bottlenecks visible after the boundary cleanup, all consequences of the eager
data model and string-based SVG emission:

- **Repeated table scans during planning.** Domain training, shared-axis extent
  computation, legend collection, and the provisional-layout y-label measurement
  each iterate the same tables; `build_render_plan` also builds a provisional
  layout before the real one. Faceting re-scans per category for row membership.
- **Owned derived frames.** `compute_derived` materializes every stat result
  into an owned `DataFrame` up front, even when a space is later not laid out.
- **String-concatenation SVG.** Emission builds the document by pushing
  formatted strings into one growing buffer; large charts (many marks, hexbin,
  dense facets) are dominated by `format!` and allocation, and the output buffer
  grows unbounded with mark count.
- **No sampling or streaming.** Every row becomes a mark; there is no decimation
  for high-cardinality scatter/line layers.

Lazy execution, sampling, and a primitive-based emitter are deferred to a later
release; this is an inventory only.

## Explicitly Deferred Past v0.17.0

- Canvas, WebGL, raster, retained-DOM, or plugin render backends.
- Interactive or animated output.
- Lazy data engine or renderer-delayed data materialization.
- Polars backend.
- Full data-frame cache.
- Streaming or million-row rendering architecture.
- New source syntax, geometries, stats, scales, projections, or data formats.

## Optional-Item Audit

### Promote In v0.17.0 (Must)

- Render model boundary audit.
- Private backend seam for SVG emission.
- Guide planning and module split.
- Data/stat execution boundary cleanup.
- Render equivalence guard coverage.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- Backend extension note.
- Render performance inventory.

### Keep Deferred

- New output backends, retained DOM, interactivity, lazy data, Polars, streaming,
  and new user-facing capabilities.

## Promotion Workflow

1. Add guard tests for guide and render cases where snapshots are thin.
2. Document the current render model boundary.
3. Introduce the private SVG backend seam.
4. Split guide planning and emission while preserving SVG output.
5. Clarify data/stat execution boundaries.
6. Run render tests, workspace tests, `./examples/generate.sh`, and require an
   empty `git diff -- examples`.
