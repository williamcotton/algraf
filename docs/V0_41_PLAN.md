# Algraf v0.41.0 Plan

Status: Implemented.
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_40_PLAN.md`](V0_40_PLAN.md)
Follow-on plan: [`V0_42_PLAN.md`](V0_42_PLAN.md)
Roadmap theme: ggplot2 feature comparability without ggplot2 API compatibility.

## Purpose

This release addresses the remaining coordinate, facet, and position-adjustment
gaps. Algraf already has algebraic faceting through `/`, automatic facet-wrap
layout, `Layout(facetColumns: ...)`, polar coordinates, axis reversal, and
Cartesian transposition. The missing ggplot2-parity pieces are fixed-aspect and
zoom semantics, facet grids and free scales, and reusable position adjustments
such as jitter and nudge.

## Release Thesis

v0.41.0 is the **layout and position control** release. It should make Algraf
capable of common ggplot2 facet and position workflows while preserving the
language's algebraic model: grouping and dodging should still be expressed by
frame structure where that is clearer than a string-valued position setting.

## Scope Rules

- Do not replace frame algebra with ggplot2's `position = ...` string model.
  Add named controls only where algebra cannot express the behavior.
- Coordinate zoom must not silently change statistical computations unless the
  spec explicitly says data is filtered before stats. Prefer visual zoom first.
- Facet-grid support should extend the existing `/` semantics rather than add a
  second faceting language.
- Random-looking position adjustments must be deterministic and seed-free unless
  a stable seed property is explicitly specified.
- Position conveniences such as jitter, nudge, or generalized dodge must lower
  to deterministic adjusted coordinates or existing frame algebra before
  rendering. Where an explicit primitive source can express the same chart, the
  sugar and primitive versions must be byte-for-byte identical.

## Current Coverage Audit

Already covered before this release:

- facet wrap through `(x * y) / group`;
- automatic facet columns and `Layout(facetColumns: n)`;
- stacked/fill bars and algebraic dodging through nested frames;
- polar coordinates with start angle and direction;
- transpose as the `coord_flip` equivalent for orientation-locked geoms;
- axis reverse and log/sqrt scale transforms.

Gaps assigned to this release:

| ggplot2 concept | Classification | Feature target |
| --------------- | -------------- | ------------------------ |
| coord_cartesian | Coordinate/layout control gap | Use `Scale(axis: ..., domain: ...)` today, but it is not the same as visual-only zoom. |
| coord_fixed | Coordinate/layout control gap | Approximate with square chart dimensions only when domains already match. |
| coord_trans | Existing partial scale control | Use `Scale(type: "log10" | "sqrt")`; arbitrary transforms stay deferred. |
| facet_grid | Layout control gap | Use current facet wrap through `/`; row/column assignment needs layout work. |
| free scales | Layout/scale control gap | No primitive substitute; current facets share trained scales. |
| labeller | Layout/guide control gap | Pre-label category values in data where possible. |
| jitter | Data-prep recipe first | Precompute jittered x/y columns and plot those. |
| nudge | Existing primitive support | Use `Text(dx:, dy:)` or precomputed label coordinates. |
| dodge | Algebraic recipe first | Use nested frame algebra such as `(category / group) * value`. |

## Primitive and Algebraic Recipes

These sketches show the current primitive/algebraic path first. Remaining items
are layout or coordinate controls, not mark syntax.

### Domain-pinned view with current scales

```text
Chart(data: "penguins.csv", width: 720, height: 500,
      title: "Pinned body-mass view") {
    Scale(axis: x, domain: [170, 220])
    Scale(axis: y, domain: [3000, 6000])
    Guide(axis: x, label: "Flipper length")
    Guide(axis: y, label: "Body mass")
    Space(flipper_length * body_mass) {
        Point(fill: species, alpha: 0.55)
        Smooth(method: "lm", stroke: "#111827", se: true)
    }
}
```

This charts a constrained domain using current scale declarations. The remaining
gap is a true visual zoom that does not alter scale training semantics or stat
behavior.

### Fixed-aspect approximation with current chart dimensions

```text
Chart(data: "calibration_grid.csv", width: 620, height: 620,
      title: "Measured distortion") {
    Scale(axis: x, domain: [0, 100])
    Scale(axis: y, domain: [0, 100])
    Space(x * y) {
        Segment(x: x0, y: y0, xend: x1, yend: y1,
                stroke: error, strokeWidth: 1)
        Point(fill: "#ffffff", stroke: "#111827", size: 2)
    }
}
```

This approximates fixed aspect by matching chart dimensions and domains. A real
fixed-aspect coordinate control is still needed when margins, legends, and
facets change the plot rectangle.

### Current facet wrap through frame algebra

```text
Chart(data: "mpg_like.csv", width: 900, height: 620,
      title: "City vs highway mileage by drivetrain") {
    Layout(facetColumns: 3)
    Space((city_mpg * highway_mpg) / drivetrain) {
        Point(fill: fuel, alpha: 0.65)
        Smooth(method: "lm", stroke: "#111827", se: false)
    }
}
```

This charts faceted scatter plots with the existing `/` algebra and wrap layout.
Facet grid row/column assignment and free scales remain layout work.

### Jitter by plotting precomputed coordinates

```text
Chart(data: "survey_jittered.csv", width: 720, height: 460,
      title: "Responses by group") {
    Space(group_jittered * score) {
        Point(fill: group, alpha: 0.45)
    }
    Space(group * score) {
        Boxplot(fill: group, alpha: 0.18, outliers: false)
    }
}
```

This charts overplotted observations with jittered x coordinates supplied by the
data. A future jitter feature should be a deterministic coordinate adjustment,
not a new point geometry.

### Nudge labels with existing text offsets

```text
Chart(data: "top_products.csv", width: 760, height: 460,
      title: "End-labeled product trends") {
    Space(month * revenue) {
        Line(stroke: product, strokeWidth: 2)
        Text(label: product,
             x: label_month, y: label_revenue,
             dx: 10, dy: 0,
             anchor: "start", declutter: true)
    }
}
```

This charts direct labels using the current `Text` offset properties. Most
nudge use cases should stay at this primitive level.

### Dodge through nested frame algebra

```text
Chart(data: "quarterly_sales.csv", width: 760, height: 460,
      title: "Quarterly sales by channel") {
    Scale(fill: channel, palette: "accent")
    Space((quarter / channel) * sales) {
        Bar(fill: channel)
    }
}
```

This charts side-by-side bars without a position setting. The nested frame is
the declarative dodge model.

## Feature Target Sketches

These non-runnable sketches separate true layout features from position sugar
that should lower to primitive-ready coordinates.

### Visual zoom as a coordinate feature

```text
Chart(data: "penguins.csv", width: 720, height: 500,
      title: "Zoomed body-mass view") {
    Space(flipper_length * body_mass,
          zoomX: [170, 220],
          zoomY: [3000, 6000]) {
        Point(fill: species, alpha: 0.55)
        Smooth(method: "lm", stroke: "#111827", se: true)
    }
}
```

This feature is not syntactic sugar over primitives. It changes coordinate
visibility while preserving the data used by stats.

### Deterministic jitter lowering to adjusted columns

```text
Chart(data: "survey.csv", width: 720, height: 460,
      title: "Responses by group") {
    Derive jittered = JitterPoints(group, score,
                                   width: 0.32,
                                   height: 0)

    Space(x * y, data: jittered) {
        Point(fill: group, alpha: 0.45)
    }
}
```

This feature computes stable adjusted coordinates. If point-level jitter sugar
is promoted, it must lower to this derived-table shape before render.

### Optional jitter sugar lowers exactly

```text
Chart(data: "survey.csv", width: 720, height: 460,
      title: "Responses by group") {
    Space(group * score) {
        Point(fill: group, alpha: 0.45, jitter: [0.32, 0])
    }
}
```

If promoted, this sugar and the explicit `JitterPoints(...)` plus `Point(...)`
source must render byte-for-byte identically.

## v0.41.0 Must

### 1. Add visual coordinate zoom

Status: Implemented.

- Add a coordinate-level zoom concept that limits the visible range without
  filtering data before stats.
- Specify clipping behavior and how out-of-range marks are represented in SVG,
  draw-list, raster, and interaction sidecar output.
- Keep explicit scale-domain filtering/training distinct from coordinate zoom.

### 2. Add fixed-aspect Cartesian layout

Status: Implemented.

- Add fixed x/y unit ratio support for Cartesian spaces.
- Define interactions with margins, legends, facets, polar/spatial spaces, and
  explicit chart width/height.
- Add tests proving circles/squares stay visually proportional under the fixed
  ratio.

### 3. Add facet-grid semantics

Status: Implemented.

- Extend facet layout to support row facets, column facets, and row-by-column
  facet grids.
- Decide whether the source surface is an extension of algebra, `Layout(...)`,
  or a combination. Keep existing facet-wrap syntax valid and unchanged.
- Specify panel ordering, empty panels, strip label placement, and guide sharing.

### 4. Add free scales per facet

Status: Implemented.

- Support fixed, free-x, free-y, and fully free scale modes for facets.
- Decide how free scales interact with overlaid spaces, legends, polar spaces,
  and coordinate zoom.
- Ensure sidecar axes identify the correct panel-local scales.

### 5. Add deterministic jitter and nudge

Status: Implemented.

- Add deterministic jitter for point-like marks. The algorithm must be stable
  across platforms and independent of wall-clock randomness.
- Add nudge behavior for text/label and point-like marks where existing `dx`/`dy`
  is too geometry-specific.
- Specify scale-space vs pixel-space offsets. Pixel-space is usually safer for
  labels; scale-space may be useful for data nudges.
- Implement position sugar as lowering to adjusted coordinate tables where
  possible, with byte-equivalence fixtures against explicit primitive sources.

### 6. Audit generalized dodge

Status: Implemented.

- Document where Algraf's existing nested algebra already covers dodge better
  than a position setting.
- Add a general dodge mechanism only for marks such as intervals where nested
  algebra is too verbose or impossible.
- Keep `Bar(layout: "dodge")` rejected unless the project intentionally changes
  the earlier algebra-first decision.

### 7. Spec, examples, README, and release hygiene

Status: Implemented.

- Update spec sections for layout, scales, coordinates, guides, diagnostics,
  sidecar, and examples.
- Add examples for coordinate zoom, fixed aspect, facet grid, free scales,
  jitter, nudge, and any promoted dodge behavior.

## v0.41.0 Should

### Facet label formatting

Status: Implemented.

- Add a small deterministic labeller model for value-only, name-and-value, and
  custom map labels. Avoid arbitrary formatting code.

### Panel spacing controls

Status: Implemented.

- Add layout controls for facet panel spacing if the new grid layout exposes
  spacing needs that current theme fields cannot cover.

## Explicitly Deferred Past v0.41.0

- Arbitrary coordinate transforms beyond enumerated scale transforms.
- Non-Cartesian transposition under polar coordinates.
- Random jitter without deterministic seeding.
- Nested facets beyond a 2D grid unless frame algebra already expresses them.
- ggplot2 formula syntax for facets.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

## Promotion Workflow

1. Specify coordinate zoom and fixed-aspect semantics first; they affect scale
   training, guides, sidecar axes, and clipping.
2. Specify facet-grid IR and layout behavior before changing renderer layout.
3. Add position adjustments only after the mark support and tests identify the
   exact geometry hooks needed.
4. Add paired sugar-vs-primitive byte-equivalence tests for position
   conveniences that lower to adjusted tables or algebra.
5. Add examples and README coverage after implementation.
