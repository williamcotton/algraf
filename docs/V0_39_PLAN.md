# Algraf v0.39.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_38_PLAN.md`](V0_38_PLAN.md)
Follow-on plan: [`V0_40_PLAN.md`](V0_40_PLAN.md)
Roadmap theme: ggplot2 feature comparability without ggplot2 API compatibility.

## Purpose

This release extends Algraf's statistical layer beyond the currently supported
binning, count, smoothing, density, boxplot, and spatial stats. The ggplot2
cheat sheet exposes a broad "stat as alternative layer" model. Algraf's
equivalent is not `after_stat(...)`; it is explicit derived data feeding
primitive marks.

## Release Thesis

v0.39.0 is the **model and summary stats** release. It should make common
one-dimensional, bivariate, and summary-stat graphics expressible through
stable `Derive` output schemas that render with existing primitives.

The release should also settle a policy for computed stat variables: Algraf
should prefer named derived columns over implicit magic variables. Any later
convenience syntax must first have a documented visual equivalence to an
explicit `Derive` plus primitive mark form.

## Scope Rules

- New stats must be pure table transforms with deterministic output schemas.
- High-level stat geometries are deferred by default. They are allowed only when
  they remove meaningful ceremony and can be specified as lowerings.
- No user-authored runtime code is introduced. Reducer and model choices are
  enumerated strings, not arbitrary functions.
- Diagnostics must point back to the user-authored stat or geometry call.
- Any promoted high-level stat geometry must lower to an explicit `Derive` plus
  primitive marks. The sugar source and the hand-written primitive source must
  produce byte-for-byte identical SVG, draw-list JSON, raster output, and
  interaction sidecar bytes.

## Current Coverage Audit

Already covered before this release:

- `Bin`, `Count`, `Smooth`, `Density`, `Bin2D`, `HexBin`, boxplot summaries,
  centroid/simplify/spatial join, and high-level histogram/frequency polygon.

Gaps assigned to this release:

| ggplot2 concept | Classification | Feature target |
| --------------- | -------------- | ------------------------ |
| ecdf | Derived stat gap; primitive rendering exists | Precompute ECDF vertices and render `Path`; a future stat should only produce the table. |
| qq / qq_line | Derived stat gap; primitive rendering exists | Precompute theoretical/sample quantiles; render `Point` and a `Segment` reference line. |
| quantile | Derived/model stat gap; primitive rendering exists | Precompute modeled rows and render `Line`. |
| summary | Derived stat gap; primitive rendering exists | Precompute grouped summaries; render `Point`, `Segment`, `Rect`, or bars. |
| summary_bin | Derived stat gap; primitive rendering exists | Precompute binned summaries; render `Line`, `Point`, `Rect`, or `Bar`. |
| stat_function | Data-source/expression gap | Prefer precomputed tables until a safe expression model exists. |
| stat_identity / stat_unique | Pipeline convenience gap | Use original or pre-deduplicated tables unless a real in-DSL table transform is needed. |

## Primitive Recipe Sketches

These sketches use current primitives over precomputed or externally prepared
tables. Future `Derive` stats should aim to produce exactly these tables rather
than own new rendering behavior.

### ECDF as path vertices

```text
Chart(data: "latency_ecdf.csv", width: 720, height: 460,
      title: "Latency empirical CDF") {
    Guide(axis: x, label: "Latency (ms)")
    Guide(axis: y, label: "Share of requests")
    Space(x * y) {
        Path(stroke: "#2f6fbb", strokeWidth: 2)
    }
}
```

This charts an empirical cumulative distribution from pre-expanded step
vertices. A future `Ecdf` stat would only create `latency_ecdf.csv`-shaped rows.

### QQ plot with a segment reference line

```text
Chart(data: "residuals_qq.csv", width: 620, height: 620,
      title: "Normal QQ check") {
    Table reference = "qq_reference_line.csv"

    Guide(axis: x, label: "Theoretical quantile")
    Guide(axis: y, label: "Sample quantile")
    Space(theoretical * sample) {
        Point(fill: "#4c78a8", alpha: 0.72, size: 2.4)
    }
    Space(x * y, data: reference) {
        Segment(x: x, y: y, xend: xend, yend: yend,
                stroke: "#111111", strokeWidth: 1)
    }
}
```

This charts sample residual quantiles against theoretical quantiles. The
reference line is just a segment table.

### Grouped summary feeding existing primitives

```text
Chart(data: "trial_summary.csv", width: 760, height: 460,
      title: "Mean outcome with standard error") {
    Scale(fill: cohort, palette: "accent")
    Space((treatment / cohort) * mean) {
        Segment(x: cohort, y: mean_minus_se,
                xend: cohort, yend: mean_plus_se,
                stroke: cohort)
        Point(fill: cohort, size: 3)
    }
}
```

This charts a precomputed grouped summary. A future `Summary` stat should
produce this table shape; the marks stay `Segment` and `Point`.

### Binned summary as line and points

```text
Chart(data: "traffic_summary_bins.csv", width: 760, height: 460,
      title: "Average conversion by traffic bin") {
    Guide(axis: x, label: "Traffic")
    Guide(axis: y, label: "Mean conversion rate")
    Space(bin_center * value) {
        Line(stroke: "#111827", strokeWidth: 2)
        Point(fill: "#111827", size: 2)
    }
}
```

This charts a binned average as ordinary line/point marks over a prepared table.
The stat does not imply a visual mark; the user chooses primitives.

### Quantile curves as ordinary grouped lines

```text
Chart(data: "housing_quantile_curves.csv", width: 760, height: 500,
      title: "Sale price quantile curves") {
    Scale(stroke: quantile, palette: "accent",
          labels: ["0.1" => "10th", "0.5" => "Median", "0.9" => "90th"])
    Space(square_feet * predicted_price) {
        Line(group: quantile, stroke: quantile, strokeWidth: 2)
    }
}
```

This charts modeled quantile lines from precomputed rows. If Algraf later adds a
`Quantile` stat, it should be a table producer feeding the same `Line` recipe.

## Feature Target Sketches

These non-runnable sketches show the intended new stat layer. The stats create
tables; optional sugar must lower to the same table plus primitive marks.

### ECDF stat feeding a path

```text
Chart(data: "latency_samples.csv", width: 720, height: 460,
      title: "Latency empirical CDF") {
    Derive ecdf_rows = Ecdf(latency_ms)

    Space(x * y, data: ecdf_rows) {
        Path(stroke: "#2f6fbb", strokeWidth: 2)
    }
}
```

This feature computes ECDF vertices. Rendering remains `Path`.

### Summary stat feeding interval primitives

```text
Chart(data: "trial_observations.csv", width: 760, height: 460,
      title: "Mean outcome with standard error") {
    Derive summary = Summary(outcome,
                             by: [treatment, cohort],
                             reducer: "mean_se")
    Derive intervals = IntervalSegments(treatment, lower, upper,
                                        by: cohort,
                                        orientation: "vertical")

    Space((treatment / cohort) * value, data: summary) {
        Point(fill: cohort, size: 3)
    }
    Space(x * y, data: intervals) {
        Segment(x: x, y: y, xend: xend, yend: yend,
                stroke: cohort)
    }
}
```

This combines the v0.39 summary stat with the v0.37 interval-parts transform.

### Optional ECDF sugar lowers exactly

```text
Chart(data: "latency_samples.csv", width: 720, height: 460,
      title: "Latency empirical CDF") {
    Space(latency_ms) {
        Ecdf(stroke: "#2f6fbb", strokeWidth: 2)
    }
}
```

If this sugar is promoted, it must lower to `Ecdf(...)` as a derived table plus
`Path(...)`, with byte-for-byte output equivalence.

## v0.39.0 Must

### 1. Settle computed-stat variable policy

Status: Planned.

- Document that Algraf computed variables are named output columns on derived
  tables, not an `after_stat(...)` expression language.
- For each high-level stat geometry, specify the equivalent `Derive` output
  columns and primitive rendering path.
- For every promoted high-level stat geometry, add a primitive equivalence
  fixture that proves byte-for-byte identical output.
- Add LSP output-schema hints and completions for every promoted stat.

### 2. Add identity and unique stats

Status: Planned.

- Add an identity stat only if it serves a real derived-table workflow; otherwise
  document that binding the original table is already identity.
- Add a unique/distinct stat with deterministic first-row retention or fully
  specified ordering.
- Ensure names and output schemas preserve input columns predictably.

### 3. Add ECDF support

Status: Planned.

- Add a one-dimensional empirical CDF stat with output x/y columns.
- Render ECDF through step vertices plus `Path`; if `Ecdf` or `Step` sugar is
  promoted, it must lower to that primitive form.
- Define missing-value handling, duplicate-value behavior, and endpoints.

### 4. Add QQ support

Status: Planned.

- Add a QQ stat that produces sample and theoretical quantile columns.
- Start with the normal distribution unless a broader distribution enum is
  promoted.
- Add optional reference-line support through v0.36 annotation primitives.

### 5. Add summary and summary-bin stats

Status: Planned.

- Add grouped summary reducers over one or more grouping columns.
- Add binned summary over a continuous axis, reusing bin-boundary policy from
  `Bin`.
- Start with enumerated reducers that are deterministic and easy to test:
  count, mean, min, max, sum, median, and standard error if supported by a
  shared numeric helper.
- Make output columns explicit: group keys, value, optional lower/upper bounds,
  bin boundaries where relevant.

### 6. Evaluate quantile regression

Status: Planned.

- Decide whether quantile regression is implementable with acceptable
  dependency, determinism, and WASM cost.
- If promoted, start with a constrained model and enumerated quantile list.
- If deferred, document how boxplot/violin quantiles and summaries cover the
  distribution-summary use case without regression lines.

### 7. Spec, examples, README, and release hygiene

Status: Planned.

- Update spec sections for stats, high-level geometry lowerings, output schemas,
  diagnostics, and LSP.
- Add examples for ECDF, QQ, summary intervals, and binned summaries after
  implementation.

## v0.39.0 Should

### Shared numeric summary helpers

Status: Planned.

- Centralize mean, median, quantile, variance, standard error, and confidence
  interval helpers so boxplot, summary stats, and future tests agree.

### Stat execution performance audit

Status: Planned.

- Add focused performance fixtures for stats that sort or group heavily.

## Explicitly Deferred Past v0.39.0

- Arbitrary user-defined functions or formulas.
- Full distribution families for QQ plots.
- Robust or nonlinear modeling beyond already-supported smoothers unless a
  deterministic dependency is chosen.
- Direct `after_stat(...)` expression syntax.
- ggplot2-style stat function names as aliases.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

## Promotion Workflow

1. Specify output schemas and equivalent primitive lowerings before coding.
2. Add stat schema planning and semantic validation.
3. Implement derived-table execution.
4. Add high-level convenience syntax only where it meaningfully improves
   ergonomics and has a specified primitive lowering.
5. Add paired sugar-vs-primitive byte-equivalence tests for every promoted
   convenience form.
6. Add examples, README sections, LSP metadata, and tests.
