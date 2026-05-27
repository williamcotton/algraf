# Algraf v0.23.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_22_PLAN.md`](V0_22_PLAN.md)
Follow-on plan: [`V0_24_PLAN.md`](V0_24_PLAN.md)

## Purpose

This document defines the intended v0.23.0 release shape: promoting the
statistical and geometry polish items that have remained optional after the
core grammar-of-graphics surface stabilized.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.23.0 is a **stat and geometry polish** release: add the missing charting
details users expect once the larger data, language, and geospatial foundations
are in place.

This release keeps the existing SVG renderer and eager execution model. It
promotes chart-surface features, not backend or plugin architecture.

## Current Debt Surface

The plan/spec audit found:

- v0.6 deferred tapered-polygon ribbons for variable-width paths.
- The spec reserves `Smooth(method: "loess")` and `se` output for later.
- Histogram grouping by fill mapping is mentioned as a later feature.
- `Segment` column mappings are mentioned as future support.
- Boxplot outlier behavior is optional and not a clear Must in current plans.
- `sqrt` scales remain listed as later in the scale section.
- 3D Cartesian rendering remains explicitly unsupported.

## Scope Rules

- No new data source backends, network access, output backends, plugins, or
  custom stats.
- Every new geometry/stat option must be deterministic and snapshot-testable.
- High-level geometry additions should document primitive desugarings where
  practical.
- Existing examples should render unchanged unless a new example is added.
- Keep 3D Cartesian rendering out of Must scope unless the design becomes clear
  enough to avoid a partial implementation.

## Capstone Acceptance Target

The capstone is a chart suite showing grouped histograms, loess smooths,
tapered paths, and mapped segments, with deterministic SVG snapshots.

The release must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

## Design Decisions (settled)

1. **Prefer narrow options over broad new abstractions.** Each chart polish item
   should be independently useful and testable.
2. **Keep stats pure.** Loess, standard-error bands, and grouped histogram stats
   do not read external resources.
3. **Tapered paths are rendering, not scale training.** The existing
   strokeWidth scale remains the data-to-width mapping.
4. **3D rendering needs a separate design.** This release may document it, but
   it should not rush a partial renderer.

## v0.23.0 Must

### 1. Loess smoothing

Status: Planned.

Acceptance criteria:

- `Smooth(method: "loess")` is accepted and documented.
- The algorithm is deterministic, dependency choices are documented, and output
  does not depend on platform locale or randomization.
- Invalid inputs, too few rows, and unsupported spaces produce targeted
  diagnostics.
- LSP completion/hover stops treating `loess` as merely reserved.
- Examples and tests cover grouped and ungrouped loess.

### 2. Smooth standard-error output

Status: Planned.

Acceptance criteria:

- Smooth stat can emit standard-error or confidence-band columns where the
  chosen method supports them.
- The source surface for drawing bands is specified, either as options on
  `Smooth` or as explicit derived data plus `Ribbon`.
- Diagnostics explain unsupported method/option combinations.
- Tests cover schema planning and render output.

### 3. Grouped histograms

Status: Planned.

Acceptance criteria:

- `Histogram` supports grouping by `fill` or an explicit `group` mapping.
- Bin assignment and stacking/dodging/faceting behavior are specified before
  implementation.
- The primitive desugaring remains understandable and hygienic.
- Examples cover overlaid, stacked, or faceted grouped histograms according to
  the chosen design.

### 4. Tapered ribbons for variable-width paths

Status: Planned.

Acceptance criteria:

- `Line`/`Path` with mapped `strokeWidth` can render a filled tapered polygon
  mode in addition to the existing per-segment stroke-width baseline.
- The source option name and fallback behavior are specified.
- Joins, caps, missing values, and group boundaries are deterministic.
- Existing per-segment rendering remains available and unchanged by default.

### 5. Segment column mappings

Status: Planned.

Acceptance criteria:

- `Segment(x:, y:, xend:, yend:)` accepts column mappings in addition to literal
  endpoints.
- Mapped segment endpoints train or use appropriate x/y scales.
- Missing endpoint values are skipped with aggregated warnings.
- Examples cover slope/dumbbell-style charts where `Line` is not a natural fit.

### 6. Boxplot outlier rendering

Status: Planned.

Acceptance criteria:

- Boxplot outliers are explicitly specified as rendered, suppressed, or exposed
  through a derived-table option.
- Rendering is deterministic and compatible with nested categorical spaces.
- Tests cover outlier detection and SVG output.
- Existing boxplot output changes only if the behavior is deliberately promoted.

### 7. Additional scale transforms

Status: Planned.

Acceptance criteria:

- Add `Scale(axis: ..., type: "sqrt")` for continuous position axes.
- Validate domain requirements and tick generation.
- Existing `linear` and `log10` behavior remains unchanged.
- LSP completion/hover and diagnostics document supported transform values.

### 8. Manual tick label rotation

Status: Implemented.

Acceptance criteria:

- `Guide(axis: ..., tickLabelAngle: number)` accepts finite numeric angles in
  degrees for x and y axes.
- Angles outside `[-90, 90]`, non-numeric values, and missing axis selectors
  produce targeted diagnostics.
- Rotated tick labels render deterministically in SVG and guide planning
  reserves sufficient bottom/left margin.
- LSP completion/hover, CLI IR JSON, tests, and an example document the option.

### 9. Spec, plan, and example hygiene

Status: Planned.

Acceptance criteria:

- Workspace and VS Code versions are bumped to `0.23.0` when the release branch
  is ready.
- Spec §13, §14, §15, §16, §21, §26, and §30 are updated for promoted behavior.
- README and examples demonstrate the new stat/geometry surface.
- Examples are regenerated with `./examples/generate.sh`.

## v0.23.0 Should

### 3D Cartesian rendering design

Status: Planned.

Write a concrete design for whether Algraf should ever render `x * y * z`
directly, and if so whether the target is facets, depth projection, or a
separate 3D backend. Do not implement direct 3D rendering without that design.

### Text label connectors and horizontal declutter

Status: Planned.

Extend the v0.5 text decluttering model to optional connector lines and
horizontal overlap handling if it can stay deterministic.

## Explicitly Deferred Past v0.23.0

- New data backends.
- Plugin/custom stat execution.
- New output backends or interactivity.
- Direct 3D Cartesian rendering unless promoted from the design note.

## Optional-Item Audit

### Promote In v0.23.0 (Must)

- Loess smoothing.
- Smooth standard-error output.
- Grouped histograms.
- Tapered ribbons for variable-width paths.
- Segment column mappings.
- Boxplot outlier rendering.
- Additional scale transforms.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- 3D Cartesian rendering design.
- Text label connectors and horizontal declutter.

### Keep Deferred

- Backend, plugin, and direct 3D implementation work.

## Promotion Workflow

1. Add focused stat/geometry guard tests.
2. Implement loess and smooth uncertainty behavior.
3. Add grouped histogram semantics and rendering.
4. Add tapered path rendering mode.
5. Add mapped segments and boxplot outlier behavior.
6. Add sqrt scale transform.
7. Update specs, examples, README, and LSP metadata; regenerate examples.
