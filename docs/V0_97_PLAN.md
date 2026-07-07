# Algraf v0.97.0 Plan

Status: Implemented
Target version: 0.97.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_96_PLAN.md`](V0_96_PLAN.md)
Follow-on plan: [`V0_98_PLAN.md`](V0_98_PLAN.md)
Roadmap theme: Extract render geometry and domain invariants once.

## Purpose

Algraf v0.97 should reduce medium-sized duplication in the renderer where
copied code currently protects subtle visual invariants. The main targets are
distribution geometries and domain-resolution pipelines.

This release should be behavior preserving. The payoff is not just fewer lines:
the extracted helpers should become the single place where ordering, density,
and domain-pipeline invariants are documented and tested.

## Release Thesis

v0.97.0 is a **render invariant consolidation** release. The renderer has good
backend architecture, but several geometry and domain helpers have grown by
copying careful code. This release should turn duplicated care into shared
construction:

- group collection preserves first-seen order once;
- density layout guards live once;
- domain hint, expansion, explicit-bound, and view-domain ordering lives once;
- shared guide-grid painting dispatch lives once.

No source-level language behavior should change.

## Current Debt Surface

- `crates/algraf-render/src/geom/distribution.rs` repeats group collection and
  density layout setup across `render_boxplot`, `render_violin`, and
  `render_sina`.
- `crates/algraf-render/src/geom/rect_tile.rs` has smaller duplicated property
  preambles for `Rect` and `Tile`.
- `crates/algraf-render/src/render/panels.rs` and related space-building code
  repeat numeric and temporal domain-resolution pipelines in union and vector
  paths.
- `render/document.rs` and `render/glyph_paint.rs` repeat grid-dispatch logic
  for panel and glyph painting.

## v0.97.0 Must

### Distribution Group Collection Helper

Status: Implemented.

Extract shared group collection for distribution geoms.

Acceptance criteria:

- A helper collects ordered group keys and row/value pairs for boxplot, violin,
  and sina rendering.
- First-seen group ordering remains unchanged.
- Missing, non-finite, or invalid values are filtered exactly as today.
- Orientation and value-column validation remain clear at call sites or move
  into a helper with equally clear diagnostics.
- Existing boxplot, violin, and sina output remains unchanged.

### Shared Density Layout For Violin And Sina

Status: Implemented.

Extract the duplicated density-curve layout preamble used by violin and sina.

Acceptance criteria:

- Shared layout output includes at least the group key, first row, density
  curve, maximum density, center, bandwidth, and half-width needed by each
  geometry.
- Guards such as `curve.len() < 2`, `max_density <= EPSILON`, and width
  clamping live in one place.
- Violin and sina keep ownership of their mark emission after the shared layout
  step.
- Tests or render snapshots cover at least one grouped and one ungrouped
  example for each geometry.

### Domain Pipeline Helpers

Status: Implemented.

Extract numeric and temporal domain-resolution pipeline helpers.

Acceptance criteria:

- The ordering of geometry hints, expansion, explicit domain bounds, and
  view-domain bounds is represented once for numeric axes.
- Temporal axes use one equivalent pipeline rather than duplicating the union
  path and helper path.
- The helpers carry comments referencing the relevant spec sections for domain
  ordering and view-domain behavior.
- Existing tests for explicit domains, null domain bounds, reverse axes, log/sqrt
  scales, temporal domains, and blended/unioned spaces continue to pass.
- Add a regression test if an uncovered branch is found during extraction.

## v0.97.0 Should

### `Rect`/`Tile` Property Preamble

Status: Implemented.

Extract the small duplicated property parsing preamble in `rect_tile.rs` if it
can be done without hiding geometry-specific behavior.

Acceptance criteria:

- Common fill/stroke/width/height setup lives once.
- Rect and Tile differences remain obvious at mark-emission time.
- Existing tile gutter and rect extent tests still pass.

### Panel Grid Painting Helper

Status: Implemented.

Share the polar/cartesian grid dispatch used by document and glyph painting.

Acceptance criteria:

- One helper paints panel grids before marks for both normal panels and glyph
  child panels.
- The helper does not introduce a new public rendering abstraction.
- Grid ordering relative to child guides and marks remains unchanged.

### Render Stats Grid Scaffolding

Status: Deferred.

Reviewed for v0.97.0. The remaining setup in `stats/bin.rs`,
`stats/density.rs`, and `stats/zfield.rs` is algorithm-specific rather than
identical scaffolding: histogram bin layout, 1D KDE grids, regular z-field
accumulation, and 2D KDE evaluation each carry different invariants. No shared
helper was extracted in this release.

If not handled in v0.93, consider extracting the repeated grid/accumulation
setup in `stats/bin.rs`, `stats/density.rs`, and `stats/zfield.rs`.

Acceptance criteria:

- Only extract identical scaffolding.
- Do not change binning, density, contour, or z-field algorithms.
- Keep deterministic ordering comments close to the shared helper.

## Explicitly Deferred Past v0.97.0

- New distribution geometries or density algorithms.
- Renderer performance rewrites beyond mechanically sharing existing work.
- Public render API changes; see [`V0_94_PLAN.md`](V0_94_PLAN.md).
- Semantics analyzer helper extraction; see [`V0_98_PLAN.md`](V0_98_PLAN.md).

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- Existing render snapshots and draw-list parity tests.
- Focused render tests for distribution geoms, explicit/null domains,
  view-domain clipping, temporal domains, and unioned/blended spaces.
- Manual image inspection only if examples or golden images change; the target
  for this release is no visual change.

## Promotion Workflow

1. Align version stamps for v0.97.0 when implementation begins.
2. Extract distribution group collection first and verify no output drift.
3. Extract violin/sina density layout and lock any subtle guard behavior.
4. Extract domain pipeline helpers with targeted tests.
5. Apply smaller grid or rect/tile cleanup only after the large invariants are
   stable.
6. Run the full required checks and mark statuses.
