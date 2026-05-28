# Algraf v0.24.0 Plan

Status: In progress (backend contract, draw-list/Canvas backend, and WebGL note
landed; raster, interaction metadata, and interactive preview carried forward)
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_23_PLAN.md`](V0_23_PLAN.md)
Follow-on plan: [`V0_25_PLAN.md`](V0_25_PLAN.md)

## Purpose

This document defines the intended v0.24.0 release shape: turning the render
backend seam prepared in v0.17 into actual additional output surfaces.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.24.0 is an **output backends and interactivity** release. SVG remains the
canonical backend, but Algraf should be able to target at least one non-SVG
render path through a deliberate backend contract.

The release distinguishes three things that earlier docs mention separately:
CLI PNG rasterization that already exists, backend-level raster output that does
not yet exist, and interactive/browser rendering that needs a richer render
model.

## Current Debt Surface

The plan/spec audit found:

- The spec mentions interactive output, raster output through a separate
  backend, animated SVG, Canvas, WebGL, and runtime interactivity as later work.
- v0.17 plans a private SVG backend seam but explicitly defers actual new
  backends.
- The CLI has PNG rasterization, but the render crate remains SVG-first.
- LSP preview uses inline SVG and is read-only.
- Large point maps/charts may eventually need Canvas or WebGL, but the current
  renderer has no public backend contract.

## Scope Rules

- SVG output and existing CLI behavior remain stable.
- No plugin API in this release.
- No arbitrary JavaScript execution in previews.
- Interactivity must be explicit, deterministic, and safe by default.
- A backend contract must be tested with at least two implementations before it
  becomes public.
- Do not add new data sources or chart syntax unless needed to describe
  interaction metadata.

## Capstone Acceptance Target

The capstone is rendering the same chart through SVG and one additional backend
with documented equivalence limits.

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

1. **SVG remains canonical.** Other backends must not redefine language
   semantics.
2. **Backend contracts consume render plans, not AST.** The parser and semantics
   crates do not know about Canvas, WebGL, or raster output.
3. **Interactivity is data/mark metadata.** It should not become arbitrary code
   execution.
4. **Security applies to output too.** Preview surfaces must not execute user
   scripts by default.

## v0.24.0 Must

### 1. Public render backend contract

Status: Implemented.

The `RenderBackend` seam is now generic over an associated `Output` type and is
documented in spec §24.6 as a closed, compiled-in set (not a plugin API). Two
backends implement it — `SvgBackend` (`Output = String`) and `DrawListBackend`
(`Output = DrawList`) — driven by a shared `render_with_backend` helper so the
planning half is identical across both. The contract consumes the planned
`RenderScene`, never the AST.

Acceptance criteria:

- Promote the private v0.17 SVG backend seam into a documented internal or
  public trait only after proving a second backend can use it. **Done** — kept
  crate-private (no plugin API per the scope rules) and documented in §24.6.
- The contract accepts planned render operations or a render model, not source
  AST nodes. **Done.**
- SVG escaping, number formatting, ordering, and accessibility behavior remain
  unchanged. **Done** — SVG emission code is untouched; examples regenerate
  without drift.
- Existing `render` and `render_with_tables` APIs keep compatibility wrappers.
  **Done** — both are unchanged; new `render_draw_list[_with_tables]` parallel
  them.

### 2. Render-crate raster backend

Status: Deferred to a follow-on release (carried forward).

A standalone render-crate raster path that draws from the render model (rather
than rasterizing SVG) is gated on the per-mark draw list — without per-datum
primitives a raster backend cannot reproduce the chart body. The CLI PNG path is
now explicitly documented as a compatibility wrapper around the SVG backend
(spec §24.6, §22.3): `--output *.png` rasterizes SVG and is unaffected by the
draw-list backend. The per-mark draw list (see item 3's deferred parity) is the
prerequisite; raster moves with it.

Acceptance criteria:

- Add a raster backend or render-crate raster path distinct from the current CLI
  PNG adapter. **Deferred** (needs per-mark primitives).
- Raster output uses the same render model and theme semantics as SVG.
  **Deferred.**
- CLI PNG output is either migrated to the shared backend or documented as a
  compatibility wrapper. **Done** — documented as a compatibility wrapper.
- Tests compare dimensions, background, and representative mark placement.
  **Partially done** via the draw-list backend tests (dimensions, background,
  plot-panel placement compared against the SVG layout).

### 3. Canvas backend prototype

Status: Implemented (documented subset).

A serializable, Canvas-drawable `DrawList` backend ships in `algraf-render`
(`render_draw_list`, `algraf render --format draw-list`). It consumes the planned
scene, requires no browser runtime for CLI builds, and emits a deterministic flat
list of `rect`/`text` ops a Canvas/raster/WebGL client can replay. Per the
acceptance allowance, it covers a documented subset first: canvas size,
background, plot panels (with facet strips/labels), and chart title/subtitle/
caption, with coordinates and colors identical to the SVG backend. Per-datum
geometry marks, axis ticks, and gridlines remain SVG-only; promoting that "full
guide/text parity" is the carried-forward follow-up (and the prerequisite for
items 2 and 6).

Acceptance criteria:

- Add a Canvas-oriented backend or a serializable draw-list that a Canvas client
  can consume. **Done.**
- The backend supports a documented subset first if full guide/text parity is
  too large. **Done** — subset documented in spec §24.6 and the module docs.
- Unsupported features produce clear diagnostics or fallback behavior. **Done**
  — the equivalence limits are documented; the subset omits marks rather than
  emitting partial/incorrect ones.
- The implementation does not require a browser runtime for CLI builds. **Done.**

### 4. Interaction metadata model

Status: Deferred to a follow-on release (carried forward).

Defining a safe, declarative source/IR model for tooltips, highlights, and
selections touches syntax → semantics → IR → render and is its own workstream; it
is not started in this pass. The backend contract landed here is the foundation
it will build on (interaction metadata would ride on the render scene and be
emitted by both backends). SVG output remains script-free and the draw list is
inert data, satisfying the safety scope rules in the meantime.

Acceptance criteria:

- Define a source and IR model for safe interactions such as tooltips,
  highlights, or selections.
- Interactions are declarative data/mark metadata, not executable source code.
- SVG output remains script-free unless an explicit opt-in is added.
- LSP preview remains read-only and script-safe by default.

### 5. Interactive preview path

Status: Deferred to a follow-on release (carried forward).

Depends on item 4 (interaction metadata); not started. LSP preview remains
read-only and script-safe by default, which satisfies the safety scope rules
until interactions exist.

### 6. WebGL feasibility note

Status: Implemented.

See [`WEBGL_FEASIBILITY.md`](WEBGL_FEASIBILITY.md). It documents what WebGL needs
beyond Canvas/raster (chiefly the per-mark draw list, plus batching, glyph-atlas
text, and a clip-space transform), identifies the marks/scales that benefit most
(high-cardinality points, heatmaps, dense lines, continuous color/size), and adds
no WebGL dependency.

Acceptance criteria:

- Document what WebGL would need beyond Canvas and raster backends. **Done.**
- Identify which marks and scales would benefit most. **Done.**
- Do not make WebGL a required dependency in this release. **Done.**

### 7. Spec, plan, and example hygiene

Status: Partially implemented.

Acceptance criteria:

- Workspace and VS Code versions are bumped to `0.24.0` when the release branch
  is ready. **Deferred** — the release is not complete (items 2, 4, 5 are
  carried forward), so the version stays at `0.23.0` for now.
- Spec §18, §21, §22, §23, §24, §29, and §30 are updated for promoted backend
  and interaction behavior. **Done for the shipped scope** — §24.6 documents the
  two-backend contract and draw-list semantics, §22.3 documents `--format`, and
  the §23 crate-layout note reflects the closed backend set. Interaction-related
  sections are untouched because interactions are deferred.
- README documents the available output modes and their equivalence limits.
  **Done.**
- Existing SVG examples regenerate without drift. **Done** — SVG emission is
  unchanged.

## v0.24.0 Should

### Animated SVG design

Status: Planned.

Design animation support for later versions, including determinism, accessibility
fallbacks, and snapshot strategy. Do not implement animation unless it can be
kept script-safe and testable.

### Browser/WASM playground design

Status: Planned.

Tie together the v0.19 WASM audit and this release's backend work into a design
for a browser playground. Do not require a WASM runtime for this release.

### URL-valued property policy

Status: Planned.

Define whether URL-valued properties are ever allowed for images, hyperlinks, or
tooltips, and how they interact with SVG injection, previews, and network
policy. Do not enable URL loading by default.

### Overlaid histogram blend follow-up

Status: Implemented.

Promote the carried-forward grouped-histogram example gap into a concrete
blend-based form: `Space((a + b)) { Histogram(...) }` bins each numeric column
over shared edges and overlays full-width series bars. Add dotted/dashed
reference-line styling as needed for the example. This is chart-surface polish
and does not affect the v0.24 backend contract work.

## Explicitly Deferred Past v0.24.0

- Plugin render backends.
- Arbitrary JavaScript or user-authored event handlers.
- Required WebGL backend.
- Browser playground productization unless promoted from the design note.

## Optional-Item Audit

### Promote In v0.24.0 (Must)

- Public render backend contract.
- Render-crate raster backend.
- Canvas backend prototype.
- Interaction metadata model.
- Interactive preview path.
- WebGL feasibility note.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- Animated SVG design.
- Browser/WASM playground design.
- URL-valued property policy.
- Overlaid histogram blend follow-up.

### Keep Deferred

- Plugin backends, arbitrary scripts, required WebGL, and browser product work.

## Promotion Workflow

1. Add render-model guard tests around SVG output.
2. Promote the backend contract only as far as the second backend requires.
3. Add raster backend and migrate or wrap CLI PNG output.
4. Add Canvas/draw-list prototype.
5. Define and implement safe interaction metadata.
6. Extend preview support while preserving static fallback.
7. Update specs and docs; run the full no-SVG-drift test suite.
