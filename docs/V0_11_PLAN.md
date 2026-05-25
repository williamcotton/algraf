# Algraf v0.11.0 Plan

Status: Complete
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_10_PLAN.md`](V0_10_PLAN.md)
Follow-on plan: [`V0_12_PLAN.md`](V0_12_PLAN.md)

## Purpose

This document defines the intended v0.11.0 release shape: modularizing the
renderer after the semantic IR boundary has been cleaned up.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when syntax, diagnostics, tests, and
examples land together.

## Release Thesis

v0.11.0 is a **renderer architecture** release: split render planning from SVG
emission, make geometry rendering easier to maintain, and reduce render-time
string/setting helper duplication.

The release assumes v0.10 has typed stat options, so render-time derived-table
execution no longer searches string settings. That makes it practical to split
`render.rs` and `geom.rs` without preserving avoidable weak boundaries.

## Current Debt Surface

The render crate is the largest source area:

- `crates/algraf-render/src/render.rs` is about 1,247 lines.
- `render_with_tables` is about 317 lines and owns derived data, layout, panels,
  shared scales, spatial planning, SVG shell emission, guide dispatch, and layer
  dispatch.
- `crates/algraf-render/src/geom.rs` is about 2,128 lines and owns every mark
  renderer, shared helpers, and SVG element strings.
- Render helper duplication exists in `domains.rs`, `render.rs`, and `geom.rs`
  (`bar_layout`, `frame_axis`, `vector_column`, `numeric_setting` variants).
- `GeometryRenderContext` exists, but most geometry functions still receive long
  loose argument lists.
- SVG output is escaped manually through helpers, but element construction still
  uses many raw `format!("<...")` calls.

## Scope Rules

- No new geometry behavior, stat behavior, scales, projections, data formats, or
  output formats.
- Existing `render` and `render_with_tables` callers keep working unless a
  call-site cleanup is deliberately included.
- Render diagnostic changes must be deliberate and tested; current checked-in
  example SVG output must remain byte-for-byte identical.
- Prefer byte-for-byte SVG equivalence. Any whitespace/attribute-order changes
  must be intentional, reviewed, and documented.
- Keep the renderer dependency-light. Do not introduce a retained SVG DOM unless
  there is a clear immediate use.

## Capstone Acceptance Target

The capstone is example-output equivalence:

```bash
cargo test -p algraf-render
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

The current checked-in examples are the visual regression baseline:
`git diff -- examples` must be empty after regeneration.

## Design Decisions (settled)

1. **Split planning before emission.** Make render planning explicit, then clean
   SVG emission.
2. **Use a lightweight SVG writer, not a full DOM.** Automatic escaping and
   stable element helpers are enough for this release.
3. **Split geometry by mark families.** Avoid one file per tiny helper, but move
   large mark groups out of `geom.rs`.
4. **Thread context consistently.** Geometry functions should accept
   `GeometryRenderContext` rather than repeated loose arguments.

## v0.11.0 Must

### 1. Render planning split

Status: Complete.

Acceptance criteria:

- `render_with_tables` becomes a thin facade over smaller modules.
- Derived-table execution lives in a dedicated render module.
- Panel planning, layout selection, shared axis extents, and facet handling are
  separated from SVG document emission.
- Spatial projection/bbox planning lives in its own module.
- Legend discovery and legend merging live in a dedicated module.
- Existing render diagnostics keep their codes and spans.

### 2. Unified panel iteration

Status: Complete.

Acceptance criteria:

- Introduce a render-local panel/slot iterator that yields one slot for an
  unfaceted plot and N slots for facets.
- Plot background, grid/facet-strip emission, and axis emission use the same
  panel iteration model instead of repeated `layout.facets.is_empty()` branches.
- Avoid changing the shared `Layout` shape until tests prove the render-local
  model is stable.
- Facet labels, rows, and panel indexes remain deterministic.

### 3. Renderer helper consolidation

Status: Complete.

Acceptance criteria:

- `bar_layout`, `frame_axis`, `vector_column`, and numeric setting helpers are
  consolidated where practical.
- Render helpers do not depend on CLI/LSP.
- CLI debug SVG number formatting reuses `algraf-render` formatting or moves
  behind a renderer helper.
- Scale/aesthetic lookup helpers in `aes.rs` use one shared
  `aesthetic_scale(...)` selector, with callers reading fields from the matched
  scale.

### 4. Geometry renderer split and context threading

Status: Complete.

Acceptance criteria:

- `geom.rs` is split into mark-family modules, for example:
  - `geom/mod.rs`;
  - `geom/common.rs`;
  - `geom/point.rs`;
  - `geom/line.rs`;
  - `geom/bar.rs`;
  - `geom/distribution.rs`;
  - `geom/rect_tile.rs`;
  - `geom/annotation.rs`;
  - `geom/text.rs`;
  - `geom/geo.rs`.
- Render dispatch remains internal to the render crate.
- Per-geometry functions use `GeometryRenderContext` consistently.
- The number of `too_many_arguments` allowances in render code is reduced.
- `GeometryKind` display/CSS-class mapping is centralized enough that render,
  CLI, and LSP are not all maintaining unrelated matches where avoidable.

### 5. Structured SVG writer helpers

Status: Complete.

Acceptance criteria:

- `SvgWriter` gains helpers for common element emission, attributes, text nodes,
  and groups.
- New SVG emission code should not manually concatenate attributes without an
  explicit reason.
- Escaping is automatic at the element-helper boundary.
- Existing SVG snapshots/examples are byte-for-byte identical. Renderer
  normalizations must not change current checked-in example output.

### 6. Spec, version, and example hygiene

Status: Complete.

Acceptance criteria:

- Workspace and VS Code extension versions are bumped to `0.11.0` when the
  release branch is ready.
- Any renderer behavior clarification is promoted into the spec before release.
  No behavior clarification was required for v0.11.0; the release is
  architecture-only.
- Examples are regenerated; `git diff -- examples` must be empty for current
  checked-in examples.
- This document is updated as each item completes, is rejected, or moves scope.

## v0.11.0 Should

### Render module documentation

Status: Complete.

Add short module-level docs explaining the render stages: derived data, domain
training, layout/panel planning, geometry emission, guides, legends, and final
SVG document assembly.

### Guide module split

Status: Deferred.

`guide.rs::render_axes` remains in scope for a later guide-focused cleanup; the
v0.11.0 panel iterator removed repeated render branches without requiring a guide
module split.

## Explicitly Deferred Past v0.11.0

- LSP monolith split, diagnostic code registry, and parser cleanup: v0.12.
- Full SVG DOM or retained scene graph.
- Interactive/animated output or raster backend changes.
- Full typed geometry-property IR.

## Optional-Item Audit

### Promote In v0.11.0 (Must)

- Render planning split.
- Unified panel iteration.
- Renderer helper consolidation.
- Geometry renderer split and context threading.
- Structured SVG writer helpers.
- Spec/version/example hygiene.

### Consider If Capacity Allows (Should)

- Render module documentation.
- Guide module split.

### Keep Deferred

- Tooling/diagnostics/parser cleanup.
- New rendering features.
- Full SVG DOM.

## Promotion Workflow

1. Add render behavior guard tests where snapshot coverage is thin.
2. Extract derived, spatial, legend, and panel planning modules.
3. Introduce unified panel iteration.
4. Consolidate render helper duplication.
5. Split geometry renderers and thread context.
6. Add structured SVG writer helpers.
7. Regenerate examples and require an empty `git diff -- examples`.
