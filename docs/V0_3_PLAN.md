# Algraf v0.3.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_2_PLAN.md`](V0_2_PLAN.md)

## Purpose

This document defines the intended v0.3.0 release shape and selects the
expressiveness-focused items promoted from the v0.2.0 deferred backlog.

As with v0.2.0, items here are planning guidance. A feature becomes normative
only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is updated
with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion in this plan
is a commitment to *attempt*, not a guarantee the syntax, diagnostics, tests,
and examples will all land together; an item ships only when they do.

## Release Thesis

v0.3.0 is an **expressiveness** release: more charts users can draw, expressed
with backwards-compatible, additive syntax.

v0.2.0 made existing charts easier to control and edit. v0.3.0 widens the set of
charts that can be expressed at all, while reusing the infrastructure already
built — the Gaussian KDE stat, the continuous-fill render path, the binning
stat, and the derived-table pipeline — rather than adding new platforms or data
backends.

The release deliberately stays inside the existing data model and rendering
architecture. No new data sources, no interactivity, no new runtime targets.

## Scope Rules

- Prefer backwards-compatible syntax additions; existing `.ag` files keep working.
- Prefer features that reuse infrastructure already shipped in v0.1/v0.2.
- Promote a deferred item only when its syntax, diagnostics, tests, and examples
  can be finished together.
- Keep product-scale directions (SQL, WASM, Polars, interactivity) out of v0.3.0.
- When a deferred item is not chosen for v0.3.0, leave it explicitly deferred
  rather than silently implied.
- Every new geometry/stat must be deterministic and snapshot-testable (spec
  §4.4, §22, §23.6).

## v0.3.0 Must

### 1. Violin Geometry

Status: Implemented. `Violin` is registered and rendered directly using the
Gaussian KDE path shared with `Density`.

Implement `Violin` as a real geometry, reusing the existing KDE path.

Minimum target:

```ag
Chart(data: "penguins.csv") {
    Space(species * body_mass_g) {
        Violin(fill: species)
    }
}
```

```ag
Violin(quantiles: [0.25, 0.5, 0.75])
```

Acceptance criteria:

- `Violin` is registered in the geometry registry (spec §13.8) with required and
  optional aesthetics, settings, default stat, and completion metadata.
- Supported space is categorical position by continuous value (spec §14.12).
- Per-group density is computed through the same Gaussian KDE used by `Density`
  (spec §15.11), with the same deterministic defaults (Silverman bandwidth,
  256-point grid, 3-bandwidth extension) and `bandwidth`/`n` overrides.
- The violin renders as a symmetric mirrored density area per category band.
- `quantiles` draws deterministic quantile lines; omitted means no quantile lines.
- Removing the `MAY defer` language from spec §14.12 and making it normative.
- Semantic tests, SVG render tests, and an example (`examples/violin.ag`).

### 2. Source-Level Continuous Color Gradients

Status: Implemented. Continuous `fill` and `stroke` scales accept source-level
`gradient` stops.

Add `Scale(...)` syntax to declare continuous color gradient stops.

Minimum target:

```ag
Scale(fill: body_mass_g, gradient: ["#3366cc", "#cc3333"])
Scale(fill: depth, gradient: ["#000004", "#bb3754", "#fcffa4"])
```

Acceptance criteria:

- A `gradient` key on `Scale(fill: col, ...)` / `Scale(stroke: col, ...)` accepts
  an ordered array of two or more color literals.
- Stops interpolate evenly across the trained continuous domain unless a future
  position syntax is added (positions remain deferred).
- The gradient drives the existing continuous-fill render path and the legend
  gradient swatch.
- `gradient` is only valid for continuous color mappings; using it with a
  categorical column or with a non-color array emits a targeted diagnostic
  (assign a new `E16xx` code; reserve it in the spec before implementing).
- Interacts correctly with existing `Scale(fill: col, label: "...")` (§16.13).
- Valid at chart scope and space scope, with space-local override (parallel to
  other `Scale` declarations, §16.11–16.12).
- Semantic tests, SVG render tests, and an example (`examples/gradient.ag`).

### 3. Chained Derived Tables

Status: Implemented. Derived declarations are dependency-resolved and may feed
later derived stats.

Allow a `Derive` declaration to read from an earlier `Derive` in the same scope
using the explicit `from` source introduced in v0.47.0.

Minimum target:

```ag
Chart(data: "series.csv") {
    Derive binned = Bin(value, bins: 30)
    Derive trend from binned = Smooth(bin_center, count, method: "lm")

    Space(bin_center * count, data: binned) {
        Line()
    }
}
```

(`Smooth` currently supports only `method: "lm"`; `"loess"` is reserved and
deferred per spec §14.10/§15.7.)

Acceptance criteria:

- Derived-table resolution forms a dependency graph; each `Derive` may reference
  columns from the source data or from earlier `Derive` outputs in scope.
- Resolution order is deterministic and independent of declaration interleaving
  beyond the dependency edges (spec §10.6, §15.3).
- A cycle between `Derive` declarations emits a targeted diagnostic (assign a new
  `E1xxx` code; reserve it in the spec before implementing) and does not loop.
- A `Derive` referencing a column that no upstream table produces emits the
  existing unknown-column diagnostic with a useful span.
- Existing single-level `Derive` behavior is unchanged.
- Semantic tests cover chaining, cycle detection, and missing upstream columns;
  add an example that uses a two-step derivation.

### 4. Range Geometries over Categorical and Temporal Domains

Status: Implemented. `Rect` supports categorical bounds, temporal union axes,
and zero-extent markers.

Make the `Rect` primitive draw ranges over the domains users actually have —
categorical bands and unified temporal axes — so range/Gantt-style charts are
expressible without numeric-only coordinates.

Minimum target:

```ag
Chart(data: "legal_caseload_tracking.csv") {
    Space((start_date + end_date) * (attorney / phase)) {
        Rect(xmin: start_date, xmax: end_date, ymin: phase, ymax: phase, fill: phase)
    }
}
```

Acceptance criteria:

- **Categorical `Rect` bounds.** When a positional bound (`xmin`/`xmax`/`ymin`/
  `ymax`) maps to a categorical column, `Rect` resolves it to that category's
  band — center ± bandwidth/2 — instead of returning no coordinate. Today
  `geom.rs::pos` returns `None` for categorical cells, so the row is skipped and
  the geometry emits `W2002` ("produced no marks"). This unlocks categorical
  Gantt bars and categorical ranges.
- **Temporal `Union` (`+`) domains.** A blended axis over temporal columns
  (e.g. `start_date + end_date`) trains a temporal scale spanning the combined
  min/max, not the current fallback to `[0, 1]`. Today the `FrameIr::Union` arm
  in `space.rs` uses only `numeric_domain`, so temporal members collapse the
  axis. Reuse the existing `temporal_domain`/`TemporalScale` path.
- **1D markers.** A `Rect` whose width or height resolves to zero (e.g.
  `xmin == xmax` for a deadline marker) renders as a thin marker clamped to its
  `strokeWidth` rather than being dropped by the SVG zero-extent rule.
- Numeric and temporal `Rect` bounds (already working) are unchanged.
- Spec: §8 (algebra/union frames) documents temporal union; §14.5 (`Rect`)
  documents the categorical-bound fallback and 1D-marker behavior.
- Render tests cover categorical bounds, temporal union scaling, and zero-extent
  markers; add a Gantt example (`examples/gantt.ag`) with a checked-in CSV.

### 5. Series Grouping (`group` Aesthetic)

Status: Implemented. `Line` and `Smooth` accept `group` and separate series
independently from color.

Add a `group` aesthetic so a geometry can separate series without binding color.

Minimum target:

```ag
Space(time * value) {
    Line(group: series, stroke: "#888888")  # several gray lines, one per series
    Point(fill: series)
}
```

Acceptance criteria:

- `group` is a registry-accepted aesthetic on `Line` (and `Smooth`, per §15.7);
  it accepts a column mapping.
- When `group` is present, `line()` partitions rows by the `group` category
  (preserving domain order and per-group x-sort, per spec §14.3) independent of
  `stroke`. When absent, behavior is unchanged (group by `stroke`).
- A constant `stroke` with a `group` mapping yields one path per group in a
  single color — the case that currently degenerates into one sawtooth path.
- Spec §14.3/§15.7 made normative for `group`; semantic and render tests added,
  plus an example showing constant-color multi-series lines.

### 6. Spec, Version, and Example Hygiene

Status: Implemented.

Bring documentation and package metadata into alignment with the release.

Acceptance criteria:

- `Cargo.toml` workspace version is bumped to `0.3.0` when the release branch is
  ready (currently `0.2.0`).
- Spec sections for each promoted feature (§14.8 if frequency polygon lands,
  §14.12 Violin, §16.x gradients, §10.6/§15.3 chained derives, §8 temporal
  union, §14.5 categorical/1D `Rect`, §14.3/§15.7 `group` aesthetic, §14.2/§14.16
  `shape` if it lands) are made normative, and `MAY defer` / "deferred" language
  is removed for shipped items.
- New diagnostic codes are reserved in the spec before implementation.
- The README tutorial gains a section for each new example, placed by topic
  progression (basics → layering → stats → layouts → derived tables →
  annotations → theming), not appended at the end.
- `./examples/generate.sh` is run; SVG/PNG outputs are regenerated for changed
  and new examples.
- This document is updated as each item completes, is rejected, or moves scope.

## v0.3.0 Should

### Frequency Polygon

Status: Implemented. `FreqPoly` is registered and desugars through `Bin` plus
`Line`.

Implement `FreqPoly` as a line drawn over histogram bin centers, reusing the
existing `Bin` stat and `Line` rendering. Low cost given both halves already
exist; promote to Must only if it can land with the binning work cleanly.

Acceptance criteria:

- Registered geometry with the `Bin` stat as default, rendering a line over
  `bin_center` against `count`.
- Shares the temporal/numeric binning path used by `Histogram` (spec §15.6).
- Spec §14.8 made normative; example and snapshot tests added.

### 2D Binning and Hex Bins

Status: Implemented. `Bin2D` renders rectangular 2D bins and `HexBin` renders
hexagonal bins.

Add 2D density via rectangular or hexagonal binning for two continuous
positions, rendering through the existing `Tile`/`Rect` fill path with a
continuous gradient (depends on Must item 2 for nice color control).

This is the most speculative v0.3.0 candidate: it needs new spec sections, a new
binning stat, and a new geometry. Keep it a Should — implement only if Must items
land with capacity to spare, and split rectangular 2D binning (cheaper, reuses
`Tile`) from hexagonal binning (new tessellation + render) so the former can
ship without the latter.

Acceptance criteria:

- New spec section(s) for 2D binning stat and geometry, with diagnostic codes.
- Deterministic bin assignment with `bins`/`binwidth` parallel to 1D `Bin`.
- Continuous-gradient fill via Must item 2; example and snapshot tests.

### Shape Aesthetic Rendering

Status: Implemented. `Point.shape` renders circle, square, triangle, and
diamond marks for literal settings or categorical mappings.

Render distinct point shapes (e.g. circle, square, triangle, diamond) driven by
the `shape` setting or a categorical `shape` mapping, giving a non-color channel
to distinguish series. Modest and additive; keep a Should.

Acceptance criteria:

- `point()` renders the requested shape; an unknown shape falls back to circle
  with a diagnostic or warning.
- A categorical `shape` mapping assigns shapes deterministically by domain order.
- Spec §14.2 enumerates the supported shape values; render tests added.

### Integer Axis Ticks

Status: Implemented. `Scale(axis: …, integer: true)` constrains continuous axis
ticks to whole integers while preserving expansion padding.

Rank-, week-, and count-style charts have integer-valued domains, but the 8%
expansion padding makes the bounds fractional, so the nice-tick algorithm picks
a 0.5 step. `integer: true` lets a chart request whole-number ticks without
pinning an explicit `domain` (which would also drop the padding).

Acceptance criteria:

- `integer: true` is accepted only on axis scales; a non-boolean value or an
  aesthetic target emits `E1204`.
- Ticks use the nice step rounded up to at least 1, so small domains land on
  consecutive integers and large ones keep an integer stride (§16.10).
- Spec §16.10/§16.11 document the key; semantic + render tests and the
  `reversed_axis` example exercise it.

## Explicitly Deferred Past v0.3.0

Carried forward from v0.2.0 and unchanged unless a later planning decision moves
them. These remain out of v0.3.0:

- SQL-backed data sources.
- WebAssembly runtime.
- Interactive SVG or interactive output.
- IDE preview panes through custom LSP requests.
- Polars backend.
- Streaming or million-row rendering architecture.
- Multi-chart or multi-page documents.
- Nested `Space` blocks.
- User variables, let-bindings, or user-defined shadowing.
- Plugins.
- Custom stats.
- Custom theme object syntax.
- Go to definition / find references.
- Feature gates.
- URL-valued properties.
- 3D Cartesian rendering.
- Qualified names using `.`.
- Unicode escape syntax.
- Advanced quoted-identifier escape modes.
- Property aliases such as `colour`.
- Gradient stop *positions* (evenly spaced stops only in v0.3.0).
- Calendar-aware bin intervals such as `interval: "month"`.

## Optional-Item Audit

### Promote In v0.3.0 (Must)

- `Violin` geometry (reusing the KDE stat).
- Source-level continuous color gradient declarations.
- Chained derived tables (`Derive` referencing earlier `Derive`).

### Implemented In v0.3.0 (Should)

- Frequency polygon geometry.
- 2D rectangular binning and hex bins.
- Shape aesthetic rendering.

### Keep Deferred

- Everything under "Explicitly Deferred Past v0.3.0" above.

## Promotion Workflow

1. Move the chosen behavior into the relevant normative section of
   [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md).
2. Reserve or add diagnostic codes before implementation if behavior can fail.
3. Implement parser, semantic, render, CLI, and LSP changes as needed.
4. Add focused tests in the crate closest to the behavior.
5. Add or update examples when the behavior affects user-facing charts.
6. Regenerate examples when rendered output changes.
7. Update this document when a candidate is completed, rejected, or moved out of
   scope.
