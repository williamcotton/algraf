# Algraf v0.40.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_39_5_PLAN.md`](V0_39_5_PLAN.md)
Merged plan: [`V0_39_PLAN.md`](V0_39_PLAN.md) (superseded into this release)
Follow-on plan: [`V0_41_PLAN.md`](V0_41_PLAN.md)
Roadmap theme: ggplot2 feature comparability without ggplot2 API compatibility.

## Purpose

This release combines the model/summary-stat work originally planned for v0.39
with the scale and guide controls planned for v0.40. The product target is one
pipeline: users start from ordinary observation data, Algraf prepares derived
tables with stat functions where needed, primitive marks render those derived
rows, and scales/guides control the resulting visual mapping.

Algraf already has `Bin`, `Count`, `Smooth`, `Density`, 2D binning, hex binning,
spatial stats, interval-part transforms, position scale types, categorical
palettes, manual color maps, continuous gradients, temporal formatting,
size/strokeWidth ranges, shape legends, legend merging, and guide suppression.
The remaining gap is practical usefulness: common statistical and scale-control
tasks should not require users to hand-author precomputed helper CSV files.

## Release Thesis

v0.40.0 is the **derived data, scale, and guide control** release. Users should
be able to compute common one-dimensional, bivariate, and grouped summaries with
`Derive`, then control the axes and legends that explain those derived values.

This is not a ggplot2 API clone. Algraf keeps explicit `Derive` declarations for
data preparation, primitive marks for rendering, and one `Scale(...)` /
`Guide(...)` declaration surface for visual controls.

For v0.40, a promoted feature should remove real authoring work. If a proposed
feature still depends on users supplying precomputed helper columns, it should
be treated as a workaround, an equivalence fixture, or a candidate for a
separate `Derive` transform, not as the feature itself.

## Scope Rules

- New stats must be pure table transforms with deterministic output schemas.
- `Derive` stats are the preferred way to prepare data inside Algraf; prepared
  external tables are test fixtures, not the primary user experience.
- High-level stat geometries remain optional sugar. They are allowed only when
  they remove meaningful ceremony and lower to explicit `Derive` plus primitive
  marks before rendering.
- No user-authored runtime code is introduced. Reducer, model, distribution, and
  binning choices are enumerated strings or typed options, not arbitrary
  functions.
- Diagnostics must point back to the user-authored stat, scale, guide, or sugar
  call.
- Scale conveniences that replace an external data-prep recipe, especially
  binned scales, must compute that mapping inside the scale/guide pipeline. They
  must still define an equivalent explicit transformed-table form for tests, but
  the user-facing feature must not require prepared input columns.
- Any domain/limit behavior must clearly distinguish scale training from visual
  zooming, which is coordinated with v0.41.

## Current Coverage Audit

Already covered before this release:

- `Bin`, `Count`, `Smooth`, `Density`, `Bin2D`, `HexBin`, `Summary2D`,
  `SummaryHex`, contour/density contour stats, interval-part transforms,
  centroid/simplify/spatial join, high-level histogram/frequency polygon, and
  boxplot summaries.
- Axis scale type: linear, log10, sqrt.
- Axis domain, integer ticks, and reverse.
- Categorical palettes and manual color maps.
- Continuous fill/stroke gradients with positioned stops.
- Size and strokeWidth numeric ranges.
- Temporal input parsing and axis time formats.
- Axis title overrides and suppression.
- Tick-label rotation.
- Legend suppression and merging.

Gaps assigned to this release:

| ggplot2 concept | Classification | Feature target |
| --------------- | -------------- | -------------- |
| computed stat variables | Derived-data policy gap | Use named `Derive` output columns, not `after_stat(...)` magic variables. |
| identity / unique stats | Pipeline convenience gap | Add a real derived-table transform only where binding the original table is insufficient. |
| ecdf | Derived stat gap; primitive rendering exists | Add `Ecdf` to compute step/path vertices from raw samples. |
| qq / qq_line | Derived stat gap; primitive rendering exists | Add `Qq` to compute theoretical/sample quantiles, with optional reference-line support. |
| summary | Derived stat gap; primitive rendering exists | Add grouped summaries that feed `Point`, `Segment`, `Rect`, bars, or interval transforms. |
| summary_bin | Derived stat gap; primitive rendering exists | Add binned summaries over a continuous axis, sharing bin policy with `Bin`. |
| binned classes | Derived stat and scale gap | Provide an in-Algraf way to classify continuous values for reusable categorical mappings. |
| quantile | Derived/model stat gap | Evaluate constrained quantile regression; defer if dependency/determinism cost is too high. |
| breaks | Scale/guide control gap | Add exact tick/legend value placement. |
| labels | Partial existing scale control | Extend labels beyond categorical fill/stroke maps to axes and legends where meaningful. |
| limits | Existing partial scale control | Clarify `domain` versus future visual zoom semantics. |
| expansion | Scale-control gap | Add scale expansion/padding controls. |
| binned scales | Scale-training gap | Bin continuous values during scale training for visual mapping. |
| identity scales | Scale-validation gap | Allow safe visual values directly from data only when deterministic and SVG-safe. |
| date/datetime scales | Existing partial guide control | Use temporal parsing and `Guide(timeFormat: ...)`; add exact breaks/labels. |
| shape/size scales | Existing partial scale control | Size ranges and shape mappings exist; manual shape value maps remain a later control gap. |
| guide axis dodge | Guide-layout gap | Add deterministic multi-row or dodged tick-label layout. |

## Pipeline Target Sketches

These sketches show the intended new work. The stats create primitive-ready
tables from ordinary input data; optional sugar must lower to the same table
plus primitive marks.

### Grouped summary without a prepared summary file

```text
Chart(data: "trial_observations.csv", width: 760, height: 460,
      title: "Mean outcome with standard error") {
    Derive summary = Summary(outcome,
                             by: [treatment, cohort],
                             reducer: "mean_se")

    Scale(fill: cohort, palette: "accent")
    Space((treatment / cohort) * mean, data: summary) {
        Segment(x: cohort, y: mean_minus_se,
                xend: cohort, yend: mean_plus_se,
                stroke: cohort)
        Point(fill: cohort, size: 3)
    }
}
```

This is the core shift from the old v0.39 sketch: the source data is raw trial
observations. Algraf computes the grouped summary table.

### Binned summary from raw observations

```text
Chart(data: "traffic_events.csv", width: 760, height: 460,
      title: "Average conversion by traffic bin") {
    Derive bins = SummaryBin(traffic, conversion,
                             bins: 12,
                             reducer: "mean")

    Guide(axis: x, label: "Traffic")
    Guide(axis: y, label: "Mean conversion rate")
    Space(bin_center * value, data: bins) {
        Line(stroke: "#111827", strokeWidth: 2)
        Point(fill: "#111827", size: 2)
    }
}
```

`SummaryBin` prepares the binned table that users previously had to provide as
`traffic_summary_bins.csv`.

### ECDF stat feeding a path

```text
Chart(data: "latency_samples.csv", width: 720, height: 460,
      title: "Latency empirical CDF") {
    Derive ecdf_rows = Ecdf(latency_ms)

    Guide(axis: x, label: "Latency (ms)")
    Guide(axis: y, label: "Share of requests")
    Space(x * y, data: ecdf_rows) {
        Path(stroke: "#2f6fbb", strokeWidth: 2)
    }
}
```

This feature computes ECDF vertices. Rendering remains `Path`.

### QQ stat feeding points

```text
Chart(data: "model_residuals.csv", width: 620, height: 620,
      title: "Normal QQ check") {
    Derive qq = Qq(residual, distribution: "normal")

    Guide(axis: x, label: "Theoretical quantile")
    Guide(axis: y, label: "Sample quantile")
    Space(theoretical * sample, data: qq) {
        Point(fill: "#4c78a8", alpha: 0.72, size: 2.4)
    }
}
```

The stat computes quantile rows from raw residuals. Optional reference-line
support should also be expressed as derived endpoint rows plus `Segment`.

### Reusable binned classes from a stat

```text
Chart(data: "counties.csv", width: 760, height: 520,
      title: "Population density classes") {
    Derive classes = Cut(density,
                         breaks: [0, 50, 100, 250, 500],
                         labels: ["0-50", "50-100", "100-250",
                                  "250-500", "500+"])

    Scale(fill: density_class,
          range: ["0-50" => "#eff3ff",
                  "50-100" => "#bdd7e7",
                  "100-250" => "#6baed6",
                  "250-500" => "#3182bd",
                  "500+" => "#08519c"],
          label: "People / sq mi")
    Space(geom, data: classes, projection: "albers_usa") {
        Geo(fill: density_class, stroke: "#ffffff", strokeWidth: 0.2)
    }
}
```

The exact stat name and output column naming must be settled during spec
promotion. The point is not the name; the point is that Algraf can create the
class column when the class is useful across layers, labels, or tests.

### Binned scale computed during scale training

```text
Chart(data: "counties.csv", width: 760, height: 520,
      title: "Population density classes") {
    Scale(fill: density,
          mode: "binned",
          breaks: [0, 50, 100, 250, 500],
          range: ["#eff3ff", "#bdd7e7", "#6baed6",
                  "#3182bd", "#08519c"])
    Space(geom, projection: "albers_usa") {
        Geo(fill: density, stroke: "#ffffff", strokeWidth: 0.2)
    }
}
```

This is the visual-only version of the same idea. If users only need a binned
visual mapping, scale training can classify values without exposing a derived
column. If users need the classes as data, they should use the derived stat.

### Explicit breaks and labels

```text
Chart(data: "revenue.csv", width: 760, height: 460,
      title: "Revenue against target") {
    Scale(axis: y,
          domain: [0, 1000000],
          breaks: [0, 250000, 500000, 750000, 1000000],
          labels: ["0", "250k", "500k", "750k", "1M"])
    Space(quarter * revenue) {
        Bar(fill: region, layout: "stack")
    }
}
```

This feature controls tick placement and tick text directly. There is no mark
or primitive substitute for exact guide planning.

### Identity color scale from validated data values

```text
Chart(data: "brand_points.csv", width: 720, height: 460,
      title: "Brand colors from data") {
    Scale(fill: brand_color, mode: "identity")
    Guide(fill: null)
    Space(x * y) {
        Point(fill: brand_color, size: 4, alpha: 0.85)
    }
}
```

If promoted, this must validate each `brand_color` value as a safe color and use
it directly for the mark fill. It is useful only when it removes the source-level
manual map; it is not a license to accept arbitrary SVG/CSS strings.

## Equivalence Baselines

Prepared tables are still useful for tests. For each promoted stat or scale
convenience, add a fixture that compares the new in-Algraf form against the
explicit table shape it is supposed to replace:

- `Ecdf(...)` versus a hand-authored ECDF vertex table.
- `Qq(...)` versus a hand-authored QQ quantile table.
- `Summary(...)` versus a grouped summary table.
- `SummaryBin(...)` versus a binned summary table.
- `Cut(...)` or `Scale(mode: "binned")` versus a classified categorical column
  plus manual categorical scale.
- High-level stat geometry sugar, if promoted, versus explicit `Derive` plus
  primitive marks.

These baselines belong in tests. User-facing examples and README tutorial
sections should use ordinary input data and Algraf stats.

## v0.40.0 Must

### 1. Settle computed-stat variable policy

Status: Implemented.

- Document that Algraf computed variables are named output columns on derived
  tables, not an `after_stat(...)` expression language.
- For each high-level stat geometry, specify the equivalent `Derive` output
  columns and primitive rendering path.
- For every promoted high-level stat geometry, add a primitive equivalence
  fixture that proves byte-for-byte identical output.
- Add LSP output-schema hints and completions for every promoted stat.

### 2. Add identity, distinct, ECDF, and QQ stats

Status: Implemented.

- Add an identity stat only if it serves a real derived-table workflow; otherwise
  document that binding the original table is already identity.
- Add a distinct/unique stat with deterministic first-row retention or fully
  specified ordering.
- Add a one-dimensional empirical CDF stat with output `x`/`y` columns and clear
  endpoint behavior.
- Add a QQ stat that produces sample and theoretical quantile columns. Start
  with the normal distribution unless a broader distribution enum is promoted.
- Define missing-value handling, duplicate-value behavior, sorting, and stable
  output order for every stat.

### 3. Add summary, summary-bin, and classing stats

Status: Implemented.

- Add grouped summary reducers over one or more grouping columns.
- Add binned summary over a continuous axis, reusing bin-boundary policy from
  `Bin`.
- Start with enumerated reducers that are deterministic and easy to test: count,
  mean, min, max, sum, median, and standard error if supported by a shared
  numeric helper.
- Make output columns explicit: group keys, value, optional lower/upper bounds,
  bin boundaries where relevant.
- Add a reusable binned-class derived stat if binned categories need to be shared
  across layers, labels, interactions, or equivalence tests. Settle its name and
  output-column policy in the spec before implementation.

### 4. Evaluate quantile regression

Status: Evaluated and deferred.

- Decide whether quantile regression is implementable with acceptable dependency,
  determinism, and WASM cost.
- If promoted, start with a constrained model and enumerated quantile list.
- If deferred, document how boxplot/violin quantiles and summaries cover the
  distribution-summary use case without regression lines.

### 5. Add explicit breaks and labels

Status: Implemented.

- Define break values for position axes and aesthetic legends.
- Extend label maps or arrays to all targets where labels are meaningful.
- Specify validation for mismatched break/label lengths, duplicate breaks, and
  labels for absent categories.
- Ensure temporal breaks preserve timezone and formatting rules.

### 6. Clarify domain, limits, clipping, and expansion

Status: Implemented.

- Write the normative distinction between data-domain training, explicit domain
  bounds, visual coordinate zoom, and clipping.
- Add scale expansion/padding controls for continuous, temporal, and categorical
  axes.
- Keep zoom controls coordinated with v0.41 coordinate work.

### 7. Add binned aesthetic scales

Status: Implemented.

- Add a scale mode that maps continuous values into deterministic bins and then
  into discrete colors or other aesthetics.
- Specify bin boundary defaults, explicit breaks, legend labels, missing values,
  and domain behavior.
- Generate default bin labels and legend entries from the computed boundaries,
  with explicit label overrides using the v0.40 breaks/labels model.
- Reuse bin helper code where possible without coupling visual scales to
  `Derive Bin`.
- Ensure authors can use the original continuous column in mark aesthetics; a
  separately prepared categorical input column is only the test oracle.
- Add equivalence tests proving the binned scale produces byte-for-byte output
  to an explicit binned-column plus manual-scale chart for the same boundaries.

### 8. Add identity scale mode where safe

Status: Implemented.

- Allow selected aesthetics to use data values as visual values when validation
  is deterministic and secure.
- Start with color-like channels only if arbitrary strings can be sanitized as
  SVG-safe colors. Defer unsafe targets.
- Define how identity scales interact with legends and guide suppression.
- Treat manual maps over prepared categorical values as the current workaround,
  not the v0.40 implementation target.
- If safe identity colors cannot be specified without weakening SVG safety,
  defer the feature rather than shipping another prepared-data recipe.

### 9. Add alpha and stroke-style scale targets if promoted

Status: Evaluated and deferred.

- Decide whether alpha and dash/stroke-style become scale targets in this
  release.
- If promoted, add semantic registry support, scale training, legends, draw-list
  serialization, and LSP metadata.
- Keep the value space enumerated for dash/stroke-style.

### 10. Improve guide layout controls

Status: Implemented.

- Add axis tick-label dodging or multi-row layout for crowded categorical axes.
- Add legend position controls only if theme layout is ready; otherwise defer to
  v0.42.
- Preserve deterministic guide measurement and layout.

### 11. Spec, examples, README, and release hygiene

Status: Implemented.

- Update stat, scale, guide, diagnostic, LSP, CLI schema/IR output, sidecar, and
  tests.
- Add examples demonstrating ECDF, QQ, summary intervals, binned summaries,
  breaks, labels, expansion, binned fill scales, and identity color if promoted.
- New examples for derived stats, binned scales, or identity scales must use
  ordinary input columns; prepared-table equivalents belong in tests, not the
  user-facing tutorial path.
- Update README sections for every new example in the tutorial progression.

## v0.40.0 Should

### Shared numeric summary helpers

Status: Implemented.

- Centralize mean, median, quantile, variance, standard error, and confidence
  interval helpers so boxplot, summary stats, and future tests agree.

### Stat execution performance audit

Status: Implemented.

- Add focused performance fixtures for stats that sort or group heavily.

### Palette audit

Status: Evaluated; no new palettes added.

- Evaluate whether additional built-in categorical and sequential palettes are
  needed for practical parity. If added, keep names stable and document exact
  color stops.

### Legend ordering

Status: Evaluated and deferred.

- Add ordering controls only if the guide collection code can support them
  without destabilizing existing legend merging.

## Explicitly Deferred Past v0.40.0

- Arbitrary user-defined functions or formulas.
- Direct `after_stat(...)` expression syntax.
- Full distribution families for QQ plots.
- Robust or nonlinear modeling beyond already-supported smoothers unless a
  deterministic dependency is chosen.
- Quantile regression lines.
- Alpha and dash/stroke-style scale targets.
- Legend position and legend ordering controls.
- ggplot2-style stat function names as aliases.
- Arbitrary scale transformation functions beyond enumerated transforms.
- Locale-aware date labels.
- CSS/SVG style strings as arbitrary identity values.
- Full guide object scripting.
- Coordinate zoom implementation, unless promoted jointly with v0.41.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

## Promotion Workflow

1. Specify stat output schemas, equivalent primitive lowerings, and target-by-
   target scale/guide behavior in the spec.
2. Add stat schema planning and semantic validation before render execution.
3. Implement derived-table execution for promoted stats.
4. Add IR fields for scale/guide controls with clear defaults and preserve old
   chart output when absent.
5. Implement scale training and guide planning changes.
6. Add paired equivalence tests for every promoted convenience form against the
   explicit derived-table or transformed-table form.
7. Add examples, README sections, LSP metadata, CLI output coverage, sidecar
   coverage, and backend tests after implementation.
