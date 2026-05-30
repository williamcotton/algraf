# Algraf v0.37.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_36_PLAN.md`](V0_36_PLAN.md)
Follow-on plan: [`V0_38_PLAN.md`](V0_38_PLAN.md)
Roadmap theme: ggplot2 feature comparability without ggplot2 API compatibility.

## Purpose

This release adds primitive-backed uncertainty and interval construction.
Algraf users can already draw many interval charts with `Rect`, `Ribbon`,
`Segment`, `Point`, and `Text`; this plan turns the repetitive endpoint and
rectangle-bound calculations into deterministic derived-table helpers, then
allows optional sugar only when it lowers to those primitive rows exactly.

The goal is not to add another statistical model. These transforms consume
explicit columns or literals for bounds and centers. Summary statistics that
compute the bounds are planned for v0.39.

## Release Thesis

v0.37.0 is the **uncertainty construction** release: error bars, lineranges,
pointranges, and crossbars should be implemented as derived interval parts that
feed `Segment`, `Point`, and `Rect`. Dedicated geometries are allowed only as
sugar over the same lowering, with byte-for-byte output to the explicit
primitive version.

## Scope Rules

- This release consumes data; it does not compute confidence intervals.
- Orientation must be explicit or inferable from the inherited frame without a
  renderer-wide coordinate transform.
- Interval recipes must compose with ordinary frame algebra and
  `transpose(...)` when the underlying primitive marks are valid.
- Whisker width, crossbar width, point size, fill, stroke, strokeWidth, alpha,
  and dash should reuse existing property semantics where possible.
- Sugar must lower before render. A promoted `ErrorBar`, `LineRange`,
  `PointRange`, or `CrossBar` form MUST produce the same primitive IR, SVG,
  draw-list JSON, raster output, and interaction sidecar bytes as the explicit
  derived-table plus primitive-mark source.

## Current Coverage Audit

Already possible but verbose:

- vertical and horizontal intervals with `Segment`;
- bands with `Ribbon` or `Rect`;
- central points with `Point`;
- Gantt/candlestick-style rectangles with `Rect`.

Gaps assigned to this release:

| ggplot2 concept | Classification | Feature target |
| --------------- | -------------- | -------------- |
| errorbar | Derived interval-parts feature plus possible sugar | Generate vertical stem/cap segment rows; optional `ErrorBar` lowers to those rows plus `Segment`. |
| errorbarh | Derived interval-parts feature plus possible sugar | Generate horizontal stem/cap segment rows; optional horizontal sugar lowers to the same `Segment` rows. |
| linerange | Derived interval-parts feature plus possible sugar | Generate one segment row per interval; optional `LineRange` lowers to `Segment`. |
| pointrange | Derived interval-parts feature plus possible sugar | Generate interval segment rows and render the center with `Point`; optional sugar lowers to the same two primitive layers. |
| crossbar | Derived interval-parts feature plus possible sugar | Generate body rectangle bounds and middle segment rows; optional sugar lowers to `Rect` plus `Segment`. |

## Primitive Recipe Sketches

These sketches use existing primitives. They assume bound and cap columns are
already present in the data or produced by an earlier data-prep step.

### Categorical estimate with explicit uncertainty columns

```text
Chart(data: "treatment_effects.csv", width: 720, height: 460,
      title: "Estimated treatment effect by group") {
    Guide(axis: x, label: "Group")
    Guide(axis: y, label: "Effect")
    Space(group * estimate) {
        HLine(y: 0, stroke: "#555555", dash: "dotted")
        Segment(x: group, y: lower95, xend: group, yend: upper95,
                stroke: "#333333", strokeWidth: 1.2)
        Segment(x: cap_left, y: lower95, xend: cap_right, yend: lower95,
                stroke: "#333333", strokeWidth: 1.2)
        Segment(x: cap_left, y: upper95, xend: cap_right, yend: upper95,
                stroke: "#333333", strokeWidth: 1.2)
        Point(fill: treatment, size: 4)
    }
}
```

This charts estimates with confidence intervals using only `Segment`, `HLine`,
and `Point`. The cap positions are explicit data columns, so no interval mark is
needed to start.

### Algebraic dodge through nested frames

```text
Chart(data: "experiment_summary.csv", width: 760, height: 460,
      title: "Dose response by cohort") {
    Scale(fill: cohort, palette: "accent")
    Space((dose / cohort) * mean_response) {
        Segment(x: cohort, y: low, xend: cohort, yend: high,
                stroke: cohort, strokeWidth: 1)
        Point(fill: cohort, size: 3)
    }
}
```

This charts grouped intervals side by side. The dodge comes from frame algebra
`dose / cohort`; no `position` setting or error-bar geometry is required for the
basic case.

### Horizontal pointranges from existing segments

```text
Chart(data: "ranked_metrics.csv", width: 760, height: 520,
      title: "Metric estimate and plausible range") {
    Guide(axis: x, label: "Metric value")
    Guide(axis: y, label: "Metric")
    Space(estimate * metric) {
        Segment(x: lower, y: metric, xend: upper, yend: metric,
                stroke: "#2f6fbb", strokeWidth: 1.4)
        Point(fill: "#ffffff", stroke: "#2f6fbb", size: 3)
    }
}
```

This charts horizontal intervals directly by authoring the continuous value on
x and the category on y. No transposed high-level interval mark is needed.

### Crossbars over precomputed summaries

```text
Chart(data: "model_summaries.csv", width: 720, height: 460,
      title: "Median fit with interquartile band") {
    Scale(fill: model, palette: "accent")
    Space(model * median_fit) {
        Rect(xmin: x0, xmax: x1, ymin: q25, ymax: q75,
             fill: model, alpha: 0.35,
             stroke: "#222222", strokeWidth: 1)
        Segment(x: x0, y: median_fit, xend: x1, yend: median_fit,
                stroke: "#222222", strokeWidth: 1)
        Point(fill: "#222222", size: 2)
    }
}
```

This charts a filled interval body with a central line. The only data-prep
burden is computing `x0`/`x1` band bounds; once present, existing primitives
cover the chart.

## Feature Target Sketches

These are non-runnable sketches of the new implementation work. The new
features create primitive-ready rows; the visible marks remain ordinary
primitives.

### Interval segments as a derived table

```text
Chart(data: "treatment_effects.csv", width: 720, height: 460,
      title: "Estimated treatment effect by group") {
    Derive whiskers = IntervalSegments(group, lower95, upper95,
                                       orientation: "vertical",
                                       capWidth: 0.55)

    Space(x * y, data: whiskers) {
        Segment(x: x, y: y, xend: xend, yend: yend,
                stroke: "#333333", strokeWidth: 1.2)
    }
    Space(group * estimate) {
        Point(fill: treatment, size: 4)
    }
}
```

This charts error bars by asking a table transform to emit segment endpoint
rows. The renderer still only sees `Segment` and `Point`.

### Crossbar parts as primitive rectangles and segments

```text
Chart(data: "model_summaries.csv", width: 720, height: 460,
      title: "Median fit with interquartile band") {
    Derive boxes = IntervalRects(model, q25, q75, width: 0.65)
    Derive middles = IntervalMiddles(model, median_fit, width: 0.65)

    Space(x * y, data: boxes) {
        Rect(xmin: xmin, xmax: xmax, ymin: ymin, ymax: ymax,
             fill: model, alpha: 0.35,
             stroke: "#222222", strokeWidth: 1)
    }
    Space(x * y, data: middles) {
        Segment(x: x, y: y, xend: xend, yend: yend,
                stroke: "#222222", strokeWidth: 1)
    }
}
```

This charts a crossbar by deriving the primitive rectangle and center-line
tables separately.

### Optional sugar lowers to the same primitive parts

```text
Chart(data: "treatment_effects.csv", width: 720, height: 460,
      title: "Estimated treatment effect by group") {
    Space(group * estimate) {
        ErrorBar(ymin: lower95, ymax: upper95,
                 capWidth: 0.55,
                 stroke: "#333333", strokeWidth: 1.2)
        Point(fill: treatment, size: 4)
    }
}
```

If this sugar is promoted, it must lower to the `IntervalSegments(...)` plus
`Segment(...)` form before rendering. The sugar fixture and explicit primitive
fixture must be byte-for-byte identical.

## v0.37.0 Must

### 1. Add derived interval-parts transforms

Status: Implemented.

Implemented as `IntervalSegments`, `IntervalRects`, and `IntervalMiddles`.
The transforms emit primitive-ready endpoint/bound columns, stable
`interval_role`/`interval_id` columns, and non-conflicting source-column
passthrough for downstream aesthetics.

- Add a transform that emits segment endpoint rows for vertical and horizontal
  intervals, with optional caps.
- Add transforms that emit rectangle bounds and middle-line endpoints for
  crossbars.
- Establish output columns: `x`, `y`, `xend`, `yend`, optional `xmin`, `xmax`,
  `ymin`, `ymax`, role columns, and stable group ids where needed.
- Document missing-value behavior through the underlying `Segment`, `Rect`, and
  `Point` marks.

### 2. Promote sugar only as exact lowerings

Status: Implemented.

Promoted `ErrorBar`, `LineRange`, `PointRange`, and `CrossBar`. Each lowers
before render to the interval transforms plus `Segment`, `Point`, and `Rect`.
Paired render tests assert byte-for-byte equality for SVG, draw-list JSON,
render-model raster pixels, and interaction metadata against the explicit
primitive source.

- Decide whether `ErrorBar`, `LineRange`, `PointRange`, and `CrossBar` earn
  source-level names after the derived transforms exist.
- Implement any promoted names by lowering to the derived interval tables and
  primitive marks before render planning.
- Add paired fixtures that assert byte-for-byte identical SVG, draw-list,
  raster, and interaction sidecar output between sugar and explicit primitives.
- Diagnostics should still point to the user-authored sugar call where useful.

### 3. Document line, point, and crossbar compositions

Status: Completed.

- Show lineranges and error bars as `Segment`.
- Show pointranges as `Segment` plus `Point`.
- Show crossbars as `Rect` plus a center `Segment`.
- Keep layer ordering explicit in source order.
- Use existing interaction support on the component primitives rather than
  inventing composite interaction behavior.

### 4. Add documentation examples that use existing data-prep patterns

Status: Completed.

- Add examples using explicit columns for fit, lower, upper, cap endpoints, and
  rectangle bounds.
- Include vertical and horizontal forms.
- Add README sections near the annotation and statistical-summary sequence.
- Use existing syntax for runnable recipe examples; add runnable promoted-syntax
  examples only after the analyzer accepts them.

### 5. LSP, registry, and backend support

Status: Implemented.

- Update registry/LSP/backend metadata only for promoted names or properties.
- Add render examples proving the primitive recipes cover the common charts.

## v0.37.0 Should

### Band-relative defaults

Status: Deferred.

Numeric `capWidth`/`width` is implemented in data units. `IntervalRects` can
emit full-band categorical rectangle bounds through `Rect`, but band-relative
partial widths and categorical cap offsets remain deferred because `Segment`
endpoints resolve categorical values to band centers rather than band edges.

- For categorical-position frames, default widths should be relative to the band
  width, matching `Bar`, `Boxplot`, and `Violin` behavior.
- For continuous-position frames, choose a deterministic pixel default or require
  explicit width.

### Interaction metadata

Status: Completed.

Composite interaction metadata was not added. Promoted sugar lowers to
component primitives, and those primitives keep the existing sidecar behavior.

- Prefer interaction on the component primitives. Add composite metadata only if
  dedicated syntax is promoted and the primitive sidecar data is insufficient.

## Explicitly Deferred Past v0.37.0

- Computing intervals from raw data. Summary stats are v0.39.
- Position adjustments for dodged error bars. General positioning is v0.41.
- Arbitrary composite geometry authoring by users.
- Exact ggplot2 parameter aliases such as `fatten`.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

## Promotion Workflow

1. Specify the interval property model and orientation rules in the spec.
2. Add registry entries and semantic validation.
3. Implement render, draw-list, raster, and interaction behavior.
4. Add unit tests for dropped rows, orientation, transposition, and backend
   parity.
5. Add paired sugar-vs-primitive equivalence tests for every promoted
   convenience form.
6. Add examples and README coverage after implementation.
