# Algraf v0.85.0 Plan

Status: Implemented
Target version: 0.85.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_84_3_PLAN.md`](V0_84_3_PLAN.md)
Roadmap theme: Make `Bar` work naturally on temporal axes by treating
`Scale(axis:, tickInterval:)` as the temporal slot width while preserving
continuous temporal spacing.
Cross-repo coordination: none required to ship 0.85.0.

## Purpose

Algraf v0.84.3 made the existing workaround for temporal bucket bars explicit:
force the date column onto a categorical band axis, then use `Guide(timeFormat:)`
to format the visible labels. That path works, but it asks authors to choose a
categorical scale when the data and the visual question are genuinely temporal.

The intended authoring path for daily, weekly, monthly, or otherwise calendar
bucketed bars should be direct:

```ag
Chart(data: "chart.csv", width: 1120, height: 460,
      title: "Nick Wilkens' Daily Lines Added",
      subtitle: "Daily additions from git.db") {
    Theme(name: "minimal")
    Scale(axis: x, type: "temporal", tickInterval: "1 day")
    Guide(axis: x, label: "Date", timeFormat: "%b %d")
    Guide(axis: y, label: "Lines Added")

    Space(bucket_day * total_lines_added) {
        Bar(alpha: 0.86)
    }
}
```

This should render as a bar chart without requiring the author to drop to
`Rect(...)` or reinterpret dates as categories.

## Release Thesis

Temporal bars are interval marks on a continuous time scale. A bar centered on
`2026-03-07` with a `tickInterval: "1 day"` represents a one-day temporal slot
centered on that date, with the painted bar inset inside the slot like an
ordinary categorical bar, not an ordinal category named `"2026-03-07"`.

The renderer should therefore keep the temporal axis continuous and use the
scale's declared calendar cadence as the bar's temporal slot width. Missing days
remain visible gaps. `Guide(timeFormat:)` continues to format real temporal
ticks. `Scale(tickInterval:)` still owns tick placement, but for `Bar` it also
supplies the deterministic slot interval when the position axis is temporal.

This is intentionally not implemented by turning temporal ticks into categories.
Treating ticks as categories would collapse elapsed time, hide missing periods,
and make the position scale disagree with `Line`, `Area`, and `Point` in the
same space. The visual result should feel like categorical bars; the coordinate
system should remain temporal.

## Scope

### Temporal Position Bars

Status: Implemented.

Acceptance criteria:

- A Cartesian `Bar` MUST render when exactly one physical position axis is
  temporal and the other physical axis is continuous numeric:
  - temporal x by continuous y renders vertical bars;
  - continuous x by temporal y renders horizontal bars.
- A temporal-position `Bar` with `Scale(axis:, tickInterval: "...")` MUST use
  that interval as its temporal slot width. For a vertical bar, the internal
  bounds are equivalent to:

  ```ag
  slot = [bucket_day - tickInterval / 2, bucket_day + tickInterval / 2]
  Rect(xmin: slot.start + padding, xmax: slot.end - padding,
       ymin: baseline, ymax: value)
  ```

  The horizontal form is analogous on y.
- The bucket interval MUST be applied with the same deterministic UTC-equivalent
  calendar arithmetic used by temporal tick generation. Calendar units with
  variable duration, such as month, quarter, and year, MUST use the midpoints
  between the previous calendar anchor, the row anchor, and the next calendar anchor
  rather than a fixed microsecond constant.
- The renderer MUST normalize pixel bounds so reversed temporal axes and
  descending output ranges still produce positive rectangle dimensions.
- The painted bar MUST be inset inside the temporal slot with regular bar
  spacing, matching ordinary categorical `Bar` defaults.
- Missing temporal position values or missing numeric value-axis values MUST
  skip the affected row, matching existing `Bar` behavior.
- Temporal `Bar` MUST preserve continuous temporal spacing. Missing buckets in
  the data produce visual gaps; they are not inserted as zero-height bars unless
  the input table contains rows for them.
- Nested temporal positions such as `Space(day / group * value)` MUST keep the
  outer temporal axis continuous and subdivide each temporal bucket into inner
  categorical group slots.
- Temporal `Bar` MUST preserve `Guide(axis:, timeFormat:)`, `tickLabelAngle`,
  and `tickLabelRows` behavior from ordinary temporal axes.

### Layouts, Baselines, And Domain Training

Status: Implemented.

Acceptance criteria:

- `layout: "identity"`, `layout: "stack"`, and `layout: "fill"` MUST work on
  temporal-position bars.
- Stack/fill grouping MUST key bars by the temporal bucket anchor and the active
  grouping/color semantics, so rows sharing the same temporal value stack in one
  bucket.
- The value baseline MUST default to `0`. If `baseline:` is promoted for `Bar`
  in this release, it MUST map through the continuous value axis exactly as
  `Area(baseline:)` does.
- Scale-domain training MUST include both temporal bucket bounds so the first
  and last bars are not clipped or squeezed to zero-width marks on an ordinary
  auto-fit chart.
- Scale-domain training MUST include the value-axis baseline so ordinary bars
  share a visible zero baseline unless the author pins or zooms the value axis
  otherwise.
- Explicit domains and zoom still control the visible view. Temporal bars whose
  interval extends outside an author-pinned domain are subject to the existing
  clipping rules for that domain.

### Diagnostics And Fallbacks

Status: Implemented.

Acceptance criteria:

- A temporal-position `Bar` without an available bucket interval MUST emit a
  targeted diagnostic that asks the author to declare
  `Scale(axis: ..., tickInterval: "...")` or use `Scale(axis: ..., type:
  "categorical")` for ordinal bucket bars.
- This release MUST NOT silently infer temporal bar width from adjacent rows.
  Adjacent-row inference is attractive, but it becomes ambiguous under sparse
  data, filtered tables, facets, and grouped/stacked layers.
- Exact `breaks:` are tick positions, not bucket widths. If `breaks:` are used
  without an active `tickInterval`, they MUST NOT by themselves make temporal
  bars render.
- Existing categorical-position `Bar` behavior MUST remain unchanged, including
  the v0.84.3 path where a temporal column forced to
  `Scale(axis:, type: "categorical")` draws discrete bands and can format labels
  with `Guide(timeFormat:)`.
- The incompatible-space `R0002` help MUST distinguish the two authoring paths:
  use a temporal axis plus `tickInterval` for elapsed-time bars, or use a
  categorical axis for ordinal bucket bars.

### Documentation, Examples, And Editor Context

Status: Implemented.

Acceptance criteria:

- `docs/ALGRAF_SPEC.md` MUST update §14.6 (`Bar`) to include temporal-position
  bars, the `tickInterval` slot-width rule, the no-adjacent-inference rule, and
  the unchanged categorical override path.
- `docs/ALGRAF_SPEC.md` MUST cross-reference §16.11 so `tickInterval` is clearly
  both a temporal tick cadence and the default temporal `Bar` bucket interval.
- `crates/algraf-cli/templates/ALGRAF_LANG.md` MUST document the temporal `Bar`
  pattern and explain that missing dates remain gaps.
- Editor registry hover/completion documentation MUST mention that `Bar` can
  draw categorical bands or temporal buckets.
- Add an example chart showing daily temporal bars with `Scale(axis: x, type:
  "temporal", tickInterval: "1 day")` and `Guide(axis: x, timeFormat: "%b %d")`.
  The example should include at least one missing day so the elapsed-time gap is
  visible.
- Add an example chart showing grouped temporal bars with `Space(day / group *
  value)` so the inner categorical grouping behavior is visible.

### Tests

Status: Implemented.

Acceptance criteria:

- Renderer tests MUST cover a vertical temporal x bar chart using
  `tickInterval: "1 day"` and formatted date labels.
- Renderer tests MUST cover a horizontal temporal y bar chart using
  `tickInterval: "1 day"` or another calendar interval.
- Renderer tests MUST assert that the x/y axis metadata remains temporal rather
  than categorical.
- Renderer tests MUST cover a missing-bucket gap so continuous temporal spacing
  is preserved.
- Renderer tests MUST cover grouped temporal bars so nested temporal positions
  subdivide each elapsed-time bucket rather than becoming categorical dates.
- Renderer tests MUST cover stacked temporal bars sharing the same bucket anchor.
- Renderer tests MUST cover the diagnostic for temporal bars without
  `tickInterval`.
- Existing v0.84.3 tests for temporal categorical bar labels MUST continue to
  pass unchanged.

## Deferred

- **Explicit temporal bar width syntax**, such as `Bar(width: "1 day")` or
  `Bar(interval: "1 day")`. v0.85.0 should first make the common
  `Scale(tickInterval:)` path work without adding a second interval spelling.
- **Adjacent-row width inference** when no `tickInterval` is declared. This may
  be useful later, but it needs careful semantics for sparse, filtered,
  faceted, and grouped data.
- **Centered temporal bars** where the datum is the bucket midpoint rather than
  the bucket start. v0.85.0 anchors bars at the row's temporal value and extends
  forward by the interval.
- **Automatic zero-row insertion for missing buckets.** Missing periods should
  remain visual gaps unless the data source emits explicit zero rows.
- **Non-temporal continuous bars.** This release focuses on calendar buckets.
  Numeric continuous-position bars can be promoted separately if a concrete
  use case needs numeric interval bars without `Rect`.

## Validation

- `cargo fmt --all --check`
- `cargo test -p algraf-render temporal_bar -- --nocapture`
- `./examples/generate.sh`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`

## Promotion Workflow

Implementation promoted the normative parts of this plan into
`docs/ALGRAF_SPEC.md`, updated the language-reference template, added renderer
coverage, and added the worked example. Before release, run the full required
checks from `AGENTS.md`.
