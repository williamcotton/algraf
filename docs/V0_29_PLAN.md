# Algraf v0.29.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_28_PLAN.md`](V0_28_PLAN.md)
Follow-on plan: [`V0_30_PLAN.md`](V0_30_PLAN.md)

## Purpose

This document defines the intended v0.29.0 release shape: finishing the output
backend work that [`V0_24_PLAN.md`](V0_24_PLAN.md) started. v0.24 promoted a
public-but-crate-private `RenderBackend` seam (spec §24.6) and shipped two
backends — the canonical `SvgBackend` and a `DrawListBackend` that covers a
*documented subset* of the chart. It deliberately carried forward the render-crate
raster backend, because both raster and WebGL are gated on the same missing
piece: a per-mark draw list with full geometry and guide parity.

This release closes that gap. It promotes the draw list from "canvas size,
background, plot panels, and chart titles" to a complete scene description —
per-datum geometry marks, axis ticks, gridlines, and legends — and then adds a
render-crate raster backend that draws from that scene rather than rasterizing
SVG bytes.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.29.0 is a **render-model completeness** release. SVG remains canonical and
byte-for-byte unchanged. The work is to make the planned `RenderScene` fully
describable as backend-neutral primitives, prove that completeness with a third
backend (raster) that consumes the same scene, and unblock the deferred WebGL
and interactivity threads that both depend on per-mark primitives.

The central decision is that the draw list becomes the *complete* intermediate
form. Once every SVG element the renderer emits has a corresponding draw-list
op, a raster backend, a future WebGL backend (v0.31+), and the interaction
metadata model (v0.30) can all be expressed against one model instead of
re-deriving layout from SVG strings.

## Current Debt Surface

The plan/spec/code audit found:

- Spec §24.6 documents the draw list as covering only canvas size, background,
  plot panels (with facet strips and labels), and chart title/subtitle/caption.
  Per-datum geometry marks, axis ticks, and gridlines are explicitly *not* part
  of the draw list yet.
- [`V0_24_PLAN.md`](V0_24_PLAN.md) item 2 (render-crate raster backend) is
  deferred with the note that it is "gated on the per-mark draw list — without
  per-datum primitives a raster backend cannot reproduce the chart body."
- The CLI PNG path (`--output *.png`) is documented as a compatibility wrapper
  that rasterizes the SVG backend's output (spec §22.3, §24.6). There is no
  render-model raster path.
- [`WEBGL_FEASIBILITY.md`](WEBGL_FEASIBILITY.md) names the per-mark draw list as
  the chief prerequisite for WebGL, ahead of batching, glyph-atlas text, and a
  clip-space transform.
- The draw-list backend tests only compare canvas dimensions, background, and
  plot-panel placement against the SVG layout. There is no per-mark equivalence
  test, so the two backends can silently diverge below the panel level.
- Draw-list ops are currently `rect` and `text` only. Geometry marks need at
  least line/polyline, polygon/path (areas, wedges, arcs from polar §16.16),
  and circle/marker primitives to reach parity.

## Scope Rules

- SVG output and existing CLI behavior remain byte-for-byte stable. The SVG
  backend code is the regression baseline and MUST NOT change.
- The backend set stays closed and compiled-in (spec §24.6). No plugin API and
  no externally supplied backends.
- The draw list stays inert data: no scripts, no embedded behavior.
- Determinism is non-negotiable: stable ordering, locale-independent number
  formatting (spec §18.8), no time/locale dependence.
- The raster backend draws from the `RenderScene`/draw list, not from rasterized
  SVG. The existing SVG-rasterizing PNG wrapper MAY remain as a compatibility
  path but MUST be clearly distinguished.
- A new draw-list op is added only when an SVG primitive needs it; the draw list
  describes *what* SVG already draws, it does not invent new chart surface.
- Geometry and guide code ask the scene for primitives; backends do not learn
  about geometries (spec §10.5, §24.6).

## Capstone Acceptance Target

The capstone is rendering a representative chart that exercises points, lines,
areas, bars, faceting, a legend, and polar arcs through all three backends and
proving cross-backend equivalence at the mark level:

```bash
# Same scene, three backends.
algraf render examples/faceted.ag --format svg        --output /tmp/faceted.svg
algraf render examples/faceted.ag --format draw-list  --output /tmp/faceted.json
algraf render examples/faceted.ag --format png        --output /tmp/faceted.png   # render-model raster
```

The draw list must contain a mark op for every datum the SVG renders, at
coordinates identical to the SVG backend; the render-model raster output must
match the SVG-rasterized baseline within a documented tolerance.

The release must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

Existing SVG examples remain the visual regression baseline.

## Design Decisions (settled)

1. **The draw list is the completeness contract.** Parity is defined as: every
   SVG element emitted for the chart body and guides has a corresponding
   draw-list op with identical coordinates and colors.
2. **Backends consume the scene, never the AST** (spec §24.6). Promoting parity
   does not move any layout or scale decision into a backend.
3. **Raster draws from the model.** The render-crate raster backend is a third
   `RenderBackend` implementation (`Output = RasterImage` or equivalent), not a
   wrapper around the SVG string.
4. **SVG stays the source of truth for appearance.** Where raster cannot match
   SVG exactly (antialiasing, sub-pixel text), the difference is documented as an
   equivalence limit, and the SVG backend defines the intended look.
5. **WebGL stays out of scope but unblocked.** This release makes the per-mark
   draw list real; the WebGL backend itself remains deferred to v0.31+.

## v0.29.0 Must

### 1. Per-mark draw-list parity

Status: Planned.

Acceptance criteria:

- Extend the draw-list op set with the primitives geometry emission needs:
  line/polyline, polygon, generic path (areas, ribbons, polar wedges and annular
  segments from spec §16.16), and circle/marker ops, alongside the existing
  `rect` and `text`.
- Every built-in geometry (`Point`, `Line`, `Step`, `Rect`, `Bar`, `Area`,
  `Ribbon`, `Tile`, `Smooth`, `Boxplot`, `Violin`, `Text`, `HLine`, `VLine`,
  `Segment`, `Rug`, `Geo`, `Graticule`, and the histogram/density/2D-bin
  desugarings) emits draw-list ops for every datum the SVG backend draws.
- Axis ticks, tick labels, gridlines, facet strips, and legends (swatches +
  labels) are present in the draw list.
- Coordinates, colors, opacity, and z-order match the SVG backend exactly for
  every covered element.
- Polar charts (pie, donut, coxcomb, radar, annular heatmap) emit arc/wedge ops,
  not approximating rectangles.
- The draw list remains inert, deterministic, and locale-independent.

### 2. Cross-backend equivalence tests

Status: Planned.

Acceptance criteria:

- Add tests that plan one scene and compare SVG emission against the draw list
  op-by-op for a representative example covering points, lines, areas, bars,
  faceting, a legend, and a polar chart.
- Tests assert mark counts match the data, and that coordinates/colors agree
  within exact equality for vector geometry and documented tolerance for text
  metrics if any.
- A regression test fails if a new SVG primitive is added without a
  corresponding draw-list op (parity guard).

### 3. Render-crate raster backend

Status: Planned.

Acceptance criteria:

- Add a render-crate raster backend implementing the §24.6 `RenderBackend` seam
  with a raster `Output`, drawing from the planned scene / draw list.
- Raster output uses the same theme semantics, colors, and layout as SVG.
- The backend is deterministic: identical input produces byte-identical output
  on a given platform; document any platform-dependent antialiasing.
- Raster pulls in no browser runtime and keeps CLI builds self-contained.
- Tests compare dimensions, background, and representative mark placement against
  the SVG layout, plus at least one full-image golden comparison within a
  documented tolerance.

### 4. CLI and output-selection wiring

Status: Planned.

Acceptance criteria:

- `--format` accepts `svg`, `draw-list`, and a render-model raster mode; document
  whether `png` selects the render-model raster path or remains the SVG-rasterizer
  wrapper, and keep one documented default.
- If the render-model raster path replaces the SVG-rasterizing PNG wrapper, the
  change in `--output *.png` behavior is documented and the previous wrapper is
  either retired or kept behind an explicit flag.
- Multi-chart output suffixing (spec §7.1) works for every format.
- Width/height/theme/scale/DPI precedence (spec §22.3) is unchanged for raster.

### 5. WebGL prerequisite groundwork (no WebGL dependency)

Status: Planned.

Acceptance criteria:

- Update [`WEBGL_FEASIBILITY.md`](WEBGL_FEASIBILITY.md) to mark the per-mark
  draw list prerequisite as satisfied and re-scope the remaining WebGL work
  (batching, glyph-atlas text, clip-space transform) against the now-complete
  scene.
- Do not add a WebGL dependency or backend in this release.

### 6. Spec, plan, and example hygiene

Status: Planned.

Acceptance criteria:

- Spec §24.6 is updated to describe the draw list as a complete scene
  description and to add the render-model raster backend to the closed backend
  set; the "per-datum marks deferred" language is removed.
- Spec §18 notes any shared primitive model between SVG emission and the draw
  list. Spec §22.3 documents the final `--format`/PNG behavior.
- Reserve any new render diagnostics in spec §26 (e.g. `R0005` for an
  unrepresentable scene element, finalized before implementation).
- Workspace `Cargo.toml` and `editors/vscode/package.json` are bumped to
  `0.29.0` when the release branch is ready.
- README documents the available output formats and their equivalence limits.
- Existing SVG examples regenerate without drift.

## v0.29.0 Should

### Headless raster snapshot tooling

Status: Planned.

Add a small, deterministic image-diff helper for raster golden tests so raster
regressions are caught without hand-inspecting PNGs. Keep it test-only and free
of network or system font dependencies.

### Draw-list schema documentation

Status: Planned.

Document the draw-list op schema (op kinds, fields, coordinate space, color
encoding) so a Canvas/WebGL/raster client can consume it without reading render
source. Versioned alongside the language version.

### Per-mark metadata hooks for v0.30

Status: Planned.

If cheap to do while touching every geometry's emission, attach a stable
per-mark identity (geometry index + datum index) to draw-list ops so the v0.30
interaction metadata model can reference marks without re-walking the scene. Do
not add interaction semantics here.

### Full-bleed margins for axis-free charts

Status: Done.

Let `marginTop`/`marginRight`/`marginBottom`/`marginLeft` override the no-axes
base padding outright (down to `0`) instead of only acting as a floor, so an
embedded `void`-themed sparkline can reach the viewport edge. Floor semantics are
unchanged when the chart has axes, and chart title/caption reserve remains a
floor on the sides that carry it. Spec §17.3 updated; covered by
`test_no_axes_margin_overrides_below_default` and the `sparkline` example.

## Explicitly Deferred Past v0.29.0

- A shipping WebGL backend (re-scoped here, implemented later).
- Interaction metadata, tooltips, highlights, selections (v0.30).
- Animated or retained-DOM output.
- Lazy or streaming data materialization.
- Plugin/third-party backends.

## Optional-Item Audit

### Promote In v0.29.0 (Must)

- Per-mark draw-list parity.
- Cross-backend equivalence tests.
- Render-crate raster backend.
- CLI and output-selection wiring.
- WebGL prerequisite groundwork.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- Headless raster snapshot tooling.
- Draw-list schema documentation.
- Per-mark metadata hooks for v0.30.

### Keep Deferred

- WebGL backend shipping, interactivity, animation, streaming data, and plugin
  backends.

## Promotion Workflow

1. Reserve any new render diagnostics in spec §26.
2. Extend the draw-list op set with line/polygon/path/marker primitives.
3. Emit per-mark draw-list ops from every geometry and guide, matching SVG.
4. Add the op-by-op parity guard test and the parity regression test.
5. Add the render-crate raster backend over the §24.6 seam.
6. Wire `--format`/PNG output and document the final precedence.
7. Re-scope `WEBGL_FEASIBILITY.md` and update spec §18/§22.3/§24.6.
8. Bump versions; regenerate examples; confirm zero SVG drift.
