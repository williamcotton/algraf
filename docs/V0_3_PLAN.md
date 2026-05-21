# Algraf v0.3.0 Plan

Status: Planned (not started)
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

Status: Not started. `GeometryKind::Violin` exists as an enum variant and is
named by the CLI, but it is not in the geometry registry, has no render
implementation, and does not desugar. The Gaussian KDE stat (§15.11) it depends
on is already implemented for `Density`.

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

Status: Not started. Gradient fills already render for continuous color
mappings, but there is no source syntax to configure the gradient. This is the
explicitly-still-deferred item from the v0.2.0 "Scale Completeness" section.

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

Status: Not started. Listed as "derived stats depending on earlier derived
tables" under v0.2.0 "Keep Deferred."

Allow a `Derive` declaration to reference a column produced by an earlier
`Derive` in the same scope.

Minimum target:

```ag
Chart(data: "series.csv") {
    Derive binned = Bin(value, bins: 30)
    Derive trend = Smooth(bin_center, count, method: "lm")

    Space(bin_center * count) {
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

### 4. Spec, Version, and Example Hygiene

Status: Not started; mirrors v0.2.0 item 5.

Bring documentation and package metadata into alignment with the release.

Acceptance criteria:

- `Cargo.toml` workspace version is bumped to `0.3.0` when the release branch is
  ready (currently `0.2.0`).
- Spec sections for each promoted feature (§14.8 if frequency polygon lands,
  §14.12 Violin, §16.x gradients, §10.6/§15.3 chained derives) are made
  normative, and `MAY defer` / "deferred" language is removed for shipped items.
- New diagnostic codes are reserved in the spec before implementation.
- The README tutorial gains a section for each new example, placed by topic
  progression (basics → layering → stats → layouts → derived tables →
  annotations → theming), not appended at the end.
- `./examples/generate.sh` is run; SVG/PNG outputs are regenerated for changed
  and new examples.
- This document is updated as each item completes, is rejected, or moves scope.

## v0.3.0 Should

### Frequency Polygon

Status: Not started. Spec §14.8 is drafted but unimplemented (no enum variant,
not registered, not rendered).

Implement `FreqPoly` as a line drawn over histogram bin centers, reusing the
existing `Bin` stat and `Line` rendering. Low cost given both halves already
exist; promote to Must only if it can land with the binning work cleanly.

Acceptance criteria (if implemented):

- Registered geometry with the `Bin` stat as default, rendering a line over
  `bin_center` against `count`.
- Shares the temporal/numeric binning path used by `Histogram` (spec §15.6).
- Spec §14.8 made normative; example and snapshot tests added.

### 2D Binning and Hex Bins

Status: Not started; not currently in the spec.

Add 2D density via rectangular or hexagonal binning for two continuous
positions, rendering through the existing `Tile`/`Rect` fill path with a
continuous gradient (depends on Must item 2 for nice color control).

This is the most speculative v0.3.0 candidate: it needs new spec sections, a new
binning stat, and a new geometry. Keep it a Should — implement only if Must items
land with capacity to spare, and split rectangular 2D binning (cheaper, reuses
`Tile`) from hexagonal binning (new tessellation + render) so the former can
ship without the latter.

Acceptance criteria (if implemented):

- New spec section(s) for 2D binning stat and geometry, with diagnostic codes.
- Deterministic bin assignment with `bins`/`binwidth` parallel to 1D `Bin`.
- Continuous-gradient fill via Must item 2; example and snapshot tests.

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

### Consider If Capacity Allows (Should)

- Frequency polygon geometry.
- 2D rectangular binning (and, secondarily, hex bins).

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
