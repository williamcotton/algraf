# Algraf v0.24.0 Plan

Status: Planned
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

Status: Planned.

Acceptance criteria:

- Promote the private v0.17 SVG backend seam into a documented internal or
  public trait only after proving a second backend can use it.
- The contract accepts planned render operations or a render model, not source
  AST nodes.
- SVG escaping, number formatting, ordering, and accessibility behavior remain
  unchanged.
- Existing `render` and `render_with_tables` APIs keep compatibility wrappers.

### 2. Render-crate raster backend

Status: Planned.

Acceptance criteria:

- Add a raster backend or render-crate raster path distinct from the current CLI
  PNG adapter.
- Raster output uses the same render model and theme semantics as SVG.
- CLI PNG output is either migrated to the shared backend or documented as a
  compatibility wrapper.
- Tests compare dimensions, background, and representative mark placement.

### 3. Canvas backend prototype

Status: Planned.

Acceptance criteria:

- Add a Canvas-oriented backend or a serializable draw-list that a Canvas client
  can consume.
- The backend supports a documented subset first if full guide/text parity is
  too large.
- Unsupported features produce clear diagnostics or fallback behavior.
- The implementation does not require a browser runtime for CLI builds.

### 4. Interaction metadata model

Status: Planned.

Acceptance criteria:

- Define a source and IR model for safe interactions such as tooltips,
  highlights, or selections.
- Interactions are declarative data/mark metadata, not executable source code.
- SVG output remains script-free unless an explicit opt-in is added.
- LSP preview remains read-only and script-safe by default.

### 5. Interactive preview path

Status: Planned.

Acceptance criteria:

- Extend the LSP/VS Code preview to display interactive metadata safely if the
  backend supports it.
- Preview cancellation and generation semantics remain intact.
- Clients that do not support interaction still receive static SVG.
- Remote workspace data-path watching continues to work.

### 6. WebGL feasibility note

Status: Planned.

Acceptance criteria:

- Document what WebGL would need beyond Canvas and raster backends.
- Identify which marks and scales would benefit most.
- Do not make WebGL a required dependency in this release.

### 7. Spec, plan, and example hygiene

Status: Planned.

Acceptance criteria:

- Workspace and VS Code versions are bumped to `0.24.0` when the release branch
  is ready.
- Spec §18, §21, §22, §23, §24, §29, and §30 are updated for promoted backend
  and interaction behavior.
- README documents the available output modes and their equivalence limits.
- Existing SVG examples regenerate without drift.

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
