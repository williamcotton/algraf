# Algraf v0.36.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_35_PLAN.md`](V0_35_PLAN.md)
Follow-on plan: [`V0_37_PLAN.md`](V0_37_PLAN.md)
Roadmap theme: ggplot2 feature comparability without ggplot2 API compatibility.

## Purpose

This release starts a deliberate ggplot2-comparability roadmap by turning
common low-level drawing recipes into primitive-ready table transforms and
stroke controls. Algraf already has the core grammar-of-graphics shape: data
sources, frame algebra, scales, guides, themes, facets, and a broad set of
marks and stats. Many cheat-sheet "missing geoms" are better understood as data
shaping into `Path`, `Segment`, `Rect`, `Text`, and `Geo`.

The target is feature comparability, not source compatibility. Algraf keeps its
block-scoped DSL, PascalCase geometry names, explicit frame algebra, `fill` vs.
`stroke` distinction, and `Derive` table model. This plan uses ggplot2 as a
coverage checklist, not as an API template.

## Release Thesis

v0.36.0 is the **primitive construction** release. It should add functional
helpers that generate the rows primitive marks need, plus broaden stroke
styling on line-like marks. It should not add ggplot-style `Step`, `Curve`,
`AbLine`, `Spoke`, or `Label` marks unless a primitive lowering is specified
first and the source form is clearly worth the extra surface area.

The release closes these ggplot2 cheat-sheet groups:

- graphical recipes that are not yet documented clearly: step lines, polygons,
  curved segments, label boxes, and limit-anchor/blank behavior;
- line-segment annotations beyond `HLine`, `VLine`, and `Segment`: ablines and
  spokes;
- stroke-style consistency for line-like marks.

As with prior plans, this file is guidance. A feature becomes normative only
when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is updated with
`MUST` / `SHOULD` / `MUST NOT` language and diagnostics before implementation.

## Design Constraints

- **Keep the Algraf surface explicit.** Do not add ggplot2 aliases such as
  `color` or `linetype` as parallel aesthetics. If a stroke style is promoted,
  it should extend Algraf's existing `stroke`, `strokeWidth`, and `dash` model.
- **Sugar must lower exactly.** A promoted convenience mark such as `Step` or
  `Curve` MUST lower to the same primitive IR a user could write with
  `Derive` plus `Path`/`Segment`. The rendered SVG, draw-list JSON, raster
  output, and interaction sidecar MUST be byte-for-byte identical to the
  primitive fixture, aside from diagnostics pointing back to the sugar source.
- **Prefer primitive marks over magic positions.** New primitives should draw
  exactly what their properties and inherited frame specify. Jittering, dodging,
  and nudging belong to the later position release.
- **Respect existing geometry columns.** A Cartesian `Polygon` mark must not
  conflict with the spatial `Geo` mark, which already renders geometry-typed
  columns.
- **Keep plans non-normative.** Keep unimplemented syntax in clearly labeled
  non-runnable sketches; add runnable examples only after the feature is
  promoted and implemented.

## Current Coverage Audit

Already covered before this release:

- points, lines, source-order paths, bars, rectangles, tiles, ribbons, areas,
  text, rug marks, horizontal/vertical reference lines, explicit segments, and
  spatial geometry rendering;
- high-level histogram/frequency polygon, density, smooth, boxplot, violin,
  2D rectangular bins, and hex bins;
- Cartesian transpose and axis reversal.

Gaps assigned to this release:

| ggplot2 concept | Classification | Feature target |
| --------------- | -------------- | -------------- |
| step line | Functional table-transform feature plus possible sugar | Add a `StepVertices`-style derived helper that expands x/y rows into source-ordered path vertices; allow `Step` only as byte-for-byte sugar over that lowering. |
| polygon | Existing primitive via geometry data; possible Cartesian primitive gap | Prefer geometry-typed data plus `Geo`; defer x/y-row polygon syntax until geometry columns are demonstrably wrong. |
| curve | Functional table-transform feature plus possible sugar | Add a curve-sampling derived helper that outputs grouped path vertices for `Path`; allow `Curve` only as byte-for-byte sugar over that lowering. |
| label | Primitive recipe with limitations | Keep `Text` plus optional precomputed `Rect`; auto-padded labels require renderer measurement and remain deferred. |
| blank / expand limits | Scale-control gap | Keep explicit `Scale(axis: ..., domain: ...)` today; exact expansion is assigned to v0.40. |
| abline | Functional endpoint feature | Add a deterministic endpoint helper only if panel-domain-derived endpoints are needed; otherwise use ordinary segment tables. |
| spoke | Functional endpoint feature | Add a vector-endpoint helper that turns angle/radius into `Segment` endpoints. |
| line type | Primitive property feature | Generalize `dash`, and possibly cap/join, across line-like primitive marks. |

## Primitive Recipe Sketches

These sketches use the existing language surface where possible. They assume
data has already been shaped into the columns the primitive marks need.

### Step line from expanded path vertices

```text
Chart(data: "inventory_step_vertices.csv", width: 760, height: 420,
      title: "Inventory after each restock") {
    Guide(axis: x, label: "Day")
    Guide(axis: y, label: "Units on hand")
    Space(day * units) {
        Path(stroke: "#2f6fbb", strokeWidth: 2)
        Point(fill: "#2f6fbb", size: 3)
    }
}
```

This charts inventory as a state that changes after each event. The CSV contains
the duplicated intermediate vertices needed for horizontal-then-vertical steps;
`Path` just draws them in source order.

### Polygon-like fills from geometry data

```text
Chart(data: GeoJson("district_outlines.geojson"), width: 720, height: 520,
      title: "Territory outlines") {
    Theme(name: "minimal")
    Scale(fill: district, palette: "accent")
    Space(geom) {
        Geo(fill: district, stroke: "#ffffff",
            strokeWidth: 1, alpha: 0.45)
    }
}
```

This charts polygons using the existing geometry column primitive. If the input
is only x/y vertex rows, the current primitive answer is to prebuild GeoJSON or
another geometry source before rendering.

### Curves, ablines, and spokes as paths or segments

```text
Chart(data: "model_check.csv", width: 640, height: 520,
      title: "Observed vs predicted") {
    Table one_to_one = "one_to_one_endpoints.csv"
    Table curve_points = "paired_curve_vertices.csv"
    Table vectors = "vector_endpoints.csv"

    Guide(axis: x, label: "Predicted")
    Guide(axis: y, label: "Observed")
    Space(predicted * observed) {
        Point(fill: segment, alpha: 0.65)
    }
    Space(x * y, data: one_to_one) {
        Segment(x: x, y: y, xend: xend, yend: yend,
                stroke: "#222222", strokeWidth: 1)
    }
    Space(x * y, data: curve_points) {
        Path(group: pair_id, stroke: "#8a3ffc", alpha: 0.5)
    }
    Space(x * y, data: vectors) {
        Segment(x: x, y: y, xend: xend, yend: yend,
                strokeWidth: speed, alpha: 0.7)
    }
}
```

This charts calibration points, a one-to-one reference line, sampled curved
links, and vector spokes. The recipe keeps all geometry explicit: endpoints and
sampled curve vertices are data, and the marks are existing primitives.

### Label boxes only when bounds are data

```text
Chart(data: "outliers_labeled.csv", width: 720, height: 460,
      title: "Labeled outliers") {
    Space(score * residual) {
        Point(fill: flagged, alpha: 0.55)
        Rect(xmin: label_xmin, xmax: label_xmax,
             ymin: label_ymin, ymax: label_ymax,
             fill: "#ffffff", stroke: "#333333")
        Text(label: name, x: label_x, y: label_y,
             anchor: "start", declutter: true)
    }
}
```

This charts points and boxed labels with precomputed label rectangles. If the
box should size itself from rendered text and padding, that is a real renderer
feature; it should not hide behind a new mark until the primitive limitation is
understood.

## Feature Target Sketches

These are non-runnable sketches of the new implementation work. The important
point is that each feature returns ordinary rows for primitive marks.

### Step vertices as a table transform

```text
Chart(data: "inventory.csv", width: 760, height: 420,
      title: "Inventory after each restock") {
    Derive step_rows = StepVertices(day, units, direction: "hv")

    Space(day * units, data: step_rows) {
        Path(stroke: "#2f6fbb", strokeWidth: 2)
    }
}
```

This would chart the same step line as the hand-expanded `Path` recipe, but the
new feature is the functional row transform, not a new mark.

### Vector endpoints for spoke-style segments

```text
Chart(data: "wind_vectors.csv", width: 720, height: 520,
      title: "Wind direction and speed") {
    Derive vectors = VectorEndpoints(x, y, angle, speed,
                                     lengthScale: 0.12)

    Space(x * y, data: vectors) {
        Segment(x: x, y: y, xend: xend, yend: yend,
                stroke: speed, strokeWidth: 1.2)
    }
}
```

This charts spokes by generating endpoint columns. The mark remains `Segment`,
so legends, clipping, and sidecar output stay primitive.

### Sampled curves as grouped path rows

```text
Chart(data: "paired_observations.csv", width: 720, height: 520,
      title: "Linked paired observations") {
    Derive links = CurveSample(x0, y0, x1, y1,
                               curvature: 0.35,
                               points: 16)

    Space(x * y, data: links) {
        Path(group: link_id, stroke: cohort, alpha: 0.5)
    }
}
```

This charts curved links by turning each row into sampled path vertices. If a
future visual shortcut exists, it should lower to this table shape.

### Optional sugar must match the primitive source exactly

```text
Chart(data: "inventory.csv", width: 760, height: 420,
      title: "Inventory after each restock") {
    Space(day * units) {
        Step(direction: "hv", stroke: "#2f6fbb", strokeWidth: 2)
    }
}
```

If this convenience form is promoted, it must lower to `StepVertices(...)` plus
`Path(...)` before rendering. A fixture using the sugar and a fixture using the
explicit derived table must produce identical SVG, draw-list, raster, and
sidecar bytes.

## v0.36.0 Must

### 1. Add primitive-ready path and segment table transforms

Status: Planned.

- Add a step-vertex transform that emits source-ordered x/y rows suitable for
  `Path`.
- Add a vector-endpoint transform that emits `x`, `y`, `xend`, and `yend`
  columns suitable for `Segment`.
- Evaluate a curve-sampling transform that emits grouped x/y rows suitable for
  `Path`.
- Specify deterministic output ordering, grouping, missing-value breaks, and
  output schemas before implementation.
- If `Step`, `Curve`, or similar convenience marks are promoted, implement them
  as lowering-only syntax with byte-for-byte primitive equivalence tests.

### 2. Generalize stroke style for line-like primitives

Status: Planned.

- Promote a coherent `dash` or stroke-style property for existing line-like
  geometries first: `Line`, `Path`, `Segment`, `HLine`, `VLine`, and `Smooth`.
  Any later convenience forms inherit this only if they are promoted.
- Audit whether `lineCap`, `lineJoin`, and mitre limits are worth exposing for
  SVG/draw-list/raster parity. If promoted, keep values tightly enumerated.
- Keep the initial values small and deterministic. Do not add arbitrary SVG
  dash arrays until there is a clear parser/validation policy.
- Ensure legends only appear for mapped stroke-style aesthetics if a later scale
  release promotes them.

### 3. Audit Cartesian polygon needs against `Geo`

Status: Planned.

- Document the current geometry-column recipe: prebuild GeoJSON/TopoJSON or
  another geometry source, then render with `Space(geom) { Geo(...) }`.
- Promote an x/y-row `Polygon` mark only if there is a strong use case where
  geometry columns are the wrong abstraction.
- If promoted, specify missing-value behavior, subgroup/hole policy, stable row
  ordering, and draw-list/raster parity. Until then, spatial polygons remain
  `Geo`.

### 4. Keep curve, abline, and spoke rendering data-driven

Status: Planned.

- Document `Segment` endpoint recipes for ablines and spokes.
- Document sampled-curve `Path` recipes for Bezier-like curves.
- Implement derived helpers before considering dedicated mark names.
- Reuse existing scale mapping, clipping, stroke, alpha, and dash support when
  any convenience syntax is promoted.

### 5. Keep label boxes as `Rect` plus `Text` until auto-measurement is needed

Status: Planned.

- Document the current `Rect` plus `Text` recipe for labels with precomputed
  bounds.
- Treat auto-sized padded labels as a renderer text-measurement feature, not as
  inevitable new mark syntax.
- If promoted, specify whether label boxes participate in decluttering as
  rectangles rather than text baselines.

### 6. Spec, examples, README, LSP, and release hygiene

Status: Planned.

- Update spec geometry sections, property registry, diagnostics, LSP completion,
  hover text, semantic tokens, and TextMate grammar only where names/properties
  are promoted.
- Add recipe examples first; add runnable syntax examples for promoted features
  only after implementation.
- Keep examples runnable: add the files only after the implementation accepts
  the new syntax.

## v0.36.0 Should

### Limit anchors

Status: Planned.

- Decide whether `Blank` is useful in Algraf or whether explicit
  `Scale(axis: ..., domain: ...)` and future scale expansion make it redundant.
- If a visible no-op mark is rejected, add a documentation note mapping the
  ggplot2 use case to Algraf's scale-domain mechanism.

### Stroke caps and joins

Status: Planned.

- Audit whether `lineend`, `linejoin`, and mitre limits are worth exposing for
  SVG/draw-list/raster parity. If promoted, keep values tightly enumerated.

## Explicitly Deferred Past v0.36.0

- Error bars, lineranges, pointranges, and crossbars: v0.37.
- Jitter, nudge, and general position adjustments: v0.41.
- Contours, rasters, and z-field stats: v0.38.
- Computed-stat variable mapping and model stats: v0.39.
- Full linetype/stroke-style scales and legends: v0.40.
- Source-level aliases for ggplot2 names such as `geom_*`, `color`, or
  `linetype`.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

## Promotion Workflow

1. Update the normative spec sections for each promoted recipe, primitive, or
   property, including supported spaces, required properties, default values,
   diagnostics, and backend requirements.
2. Add semantic registry entries first, then renderer/draw-list/raster support.
3. Add focused semantic and render tests for promoted behavior, including
   missing values and grouped data where relevant.
4. For any convenience syntax, add paired sugar-vs-primitive fixtures and assert
   byte-for-byte identical SVG, draw-list, raster, and sidecar output.
5. Add examples and README sections only after the syntax is implemented.
6. Run the required checks and verify unrelated examples do not drift.
