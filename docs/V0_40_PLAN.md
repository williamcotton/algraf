# Algraf v0.40.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_39_PLAN.md`](V0_39_PLAN.md)
Follow-on plan: [`V0_41_PLAN.md`](V0_41_PLAN.md)
Roadmap theme: ggplot2 feature comparability without ggplot2 API compatibility.

## Purpose

This release brings scales and guides closer to ggplot2's practical coverage:
breaks, labels, limits, expansion, binned and identity scales, additional
aesthetic targets, and guide layout controls. Algraf already has position scale
types, manual categorical color maps, continuous gradients, temporal formatting,
size/strokeWidth ranges, shape legends, legend merging, and guide suppression.
The remaining gap is control depth.

## Release Thesis

v0.40.0 is the **scale and guide control** release. Users should be able to
control what values appear in axes and legends, how they are labeled, whether
domains expand or clip, and how common non-color aesthetics are scaled.

This is not a ggplot2 scale-function clone. Algraf keeps one `Scale(...)` and
one `Guide(...)` declaration surface, extended with clear target-specific
properties.

## Scope Rules

- Prefer additive `Scale`/`Guide` properties over new declaration kinds.
- Axis selectors remain bare `x` and `y`.
- Aesthetic targets remain explicit: `fill`, `stroke`, `size`, `shape`,
  `strokeWidth`, alpha/stroke-style only if promoted.
- Manual maps must use the existing map literal shape and deterministic category
  ordering.
- Any domain/limit behavior must clearly distinguish scale training from visual
  zooming, which is coordinated with v0.41.
- Scale conveniences that can be expressed as data transforms plus existing
  scales, especially binned scales, must define that lowering and include
  byte-for-byte equivalence tests against the explicit transformed-table form.

## Current Coverage Audit

Already covered before this release:

- axis scale type: linear, log10, sqrt;
- axis domain and reverse;
- categorical palettes and manual color maps;
- continuous fill/stroke gradients with positioned stops;
- size and strokeWidth numeric ranges;
- temporal input parsing and axis time formats;
- axis title overrides and suppression;
- tick-label rotation;
- legend suppression and merging.

Gaps assigned to this release:

| ggplot2 concept | Classification | Feature target |
| --------------- | -------------- | ------------------------ |
| breaks | Scale/guide control gap | No primitive substitute for exact tick placement; use current nice ticks or explicit domains. |
| labels | Partial existing scale control | Manual labels already work for categorical fill/stroke maps; broader axis labels are a guide gap. |
| limits | Existing partial scale control | `Scale(axis: ..., domain: ...)` pins domains today; visual zoom semantics are v0.41. |
| expansion | Scale-control gap | Use wider explicit domains today. |
| binned scales | Data-prep recipe first | Pre-bin values into categorical columns, then use existing categorical color scales. |
| identity scales | Data-prep/manual-map recipe first | Map known categorical values to visual values explicitly; do not accept arbitrary visual strings yet. |
| date/datetime scales | Existing partial guide control | Use temporal parsing and `Guide(timeFormat: ...)`; exact breaks are still a gap. |
| shape/size scales | Existing partial scale control | Size ranges and shape mappings exist; manual shape ranges are a later control gap. |
| guide axis dodge | Guide-layout gap | Use `tickLabelAngle` or fewer categories today. |

## Primitive and Existing-Control Recipes

These sketches avoid new scale syntax. Where exact guide control is impossible
today, they show the closest current declarative form and name the remaining
control gap.

### Domain pinning and existing guide labels

```text
Chart(data: "revenue.csv", width: 760, height: 460,
      title: "Revenue against target") {
    Scale(axis: y, domain: [0, 1000000])
    Guide(axis: x, label: "Quarter", tickLabelAngle: -35)
    Guide(axis: y, label: "Revenue")
    Space(quarter * revenue) {
        Bar(fill: region, layout: "stack")
        HLine(y: 750000, stroke: "#333333", dash: "dashed",
              label: "target")
    }
}
```

This charts quarterly revenue with a pinned y domain and current automatic tick
planning. Exact tick breaks and custom axis tick labels remain scale/guide
control gaps, not primitive marks.

### Binned fill by preparing a categorical column

```text
Chart(data: "counties_binned.csv", width: 760, height: 520,
      title: "Population density classes") {
    Scale(fill: density_class,
          range: ["0-50" => "#eff3ff",
                  "50-100" => "#bdd7e7",
                  "100-250" => "#6baed6",
                  "250-500" => "#3182bd",
                  "500+" => "#08519c"],
          label: "People / sq mi")
    Space(geom, projection: "albers_usa") {
        Geo(fill: density_class, stroke: "#ffffff", strokeWidth: 0.2)
    }
}
```

This charts a binned choropleth by doing the continuous-to-class transform in
data. A future binned scale should move that transform into scale training
without changing the `Geo` primitive.

### Identity-like colors through explicit manual maps

```text
Chart(data: "brand_points.csv", width: 720, height: 460,
      title: "Brand colors from data") {
    Scale(fill: brand,
          range: ["alpha" => "#1f77b4",
                  "beta" => "#ff7f0e",
                  "gamma" => "#2ca02c"],
          label: "Brand")
    Space(x * y) {
        Point(fill: brand, size: 4, alpha: 0.85)
        Text(label: brand, dx: 6, dy: -4, fill: "#111827")
    }
}
```

This charts brand-colored points without identity-scale semantics. The visual
values are explicit in source, which keeps validation and SVG safety simple.

### Existing shape and size controls

```text
Chart(data: "cities.csv", width: 760, height: 500,
      title: "Cities by type and population") {
    Scale(size: population,
          domain: [0, null],
          range: [2, 14],
          label: "Population")
    Space(longitude * latitude) {
        Point(shape: type, size: population,
              fill: "#ffffff", stroke: "#111827", alpha: 0.85)
    }
}
```

This charts symbolic point shape and scaled size with the current shape mapping
and size range support. Manual shape value maps and exact size legend breaks are
control gaps, not new mark types.

### Crowded guide labels with current rotation

```text
Chart(data: "daily_sales.csv", width: 820, height: 420,
      title: "Daily sales") {
    Guide(axis: x, label: "Date", tickLabelAngle: -45)
    Guide(axis: y, label: "Sales")
    Space(day * sales) {
        Line(stroke: "#2f6fbb", strokeWidth: 2)
    }
}
```

This charts a dense temporal series with the existing guide decluttering and
tick-label angle controls. Multi-row tick labels are a guide-layout gap.

## Feature Target Sketches

These non-runnable sketches identify the new control surface. Binned scale
sugar must lower to the same trained visual mapping as an explicit binned
column plus manual scale.

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

### Binned scale as sugar over an explicit binned column

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

If promoted, this must train to the same colors as a source that first derives
`density_class` and then uses the explicit manual categorical scale.

## v0.40.0 Must

### 1. Add explicit breaks and labels

Status: Planned.

- Define break values for position axes and aesthetic legends.
- Extend label maps or arrays to all targets where labels are meaningful.
- Specify validation for mismatched break/label lengths, duplicate breaks, and
  labels for absent categories.
- Ensure temporal breaks preserve timezone and formatting rules.

### 2. Clarify domain, limits, clipping, and expansion

Status: Planned.

- Write the normative distinction between data-domain training, explicit domain
  bounds, visual coordinate zoom, and clipping.
- Add scale expansion/padding controls for continuous, temporal, and categorical
  axes.
- Keep zoom controls coordinated with v0.41 coordinate work.

### 3. Add binned aesthetic scales

Status: Planned.

- Add a scale mode that maps continuous values into deterministic bins and then
  into discrete colors or other aesthetics.
- Specify bin boundary defaults, explicit breaks, legend labels, missing values,
  and domain behavior.
- Reuse bin helper code where possible without coupling visual scales to
  `Derive Bin`.
- Add equivalence tests proving the binned scale produces byte-for-byte output
  to an explicit binned-column plus manual-scale chart for the same boundaries.

### 4. Add identity scale mode where safe

Status: Planned.

- Allow selected aesthetics to use data values as visual values when validation
  is deterministic and secure.
- Start with color-like channels only if arbitrary strings can be sanitized as
  SVG-safe colors. Defer unsafe targets.
- Define how identity scales interact with legends and guide suppression.

### 5. Add alpha and stroke-style scale targets if promoted

Status: Planned.

- Decide whether alpha and dash/stroke-style become scale targets in this
  release.
- If promoted, add semantic registry support, scale training, legends, draw-list
  serialization, and LSP metadata.
- Keep the value space enumerated for dash/stroke-style.

### 6. Improve guide layout controls

Status: Planned.

- Add axis tick-label dodging or multi-row layout for crowded categorical axes.
- Add legend position controls only if theme layout is ready; otherwise defer to
  v0.42.
- Preserve deterministic guide measurement and layout.

### 7. Spec, examples, README, and release hygiene

Status: Planned.

- Update scale, guide, diagnostic, LSP, CLI schema/IR output, sidecar, and tests.
- Add examples demonstrating breaks, labels, expansion, binned fill scales, and
  identity color if promoted.

## v0.40.0 Should

### Palette audit

Status: Planned.

- Evaluate whether additional built-in categorical and sequential palettes are
  needed for practical parity. If added, keep names stable and document exact
  color stops.

### Legend ordering

Status: Planned.

- Add ordering controls only if the guide collection code can support them
  without destabilizing existing legend merging.

## Explicitly Deferred Past v0.40.0

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

1. Specify target-by-target scale and guide behavior in the spec.
2. Add IR fields with clear defaults and preserve old chart output when absent.
3. Implement scale training and guide planning changes.
4. Add LSP, CLI IR/schema output, sidecar, and backend tests.
5. Add byte-equivalence tests for scale conveniences that lower to explicit
   data-transform forms.
6. Add examples after implementation.
