# Algraf v0.42.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_41_PLAN.md`](V0_41_PLAN.md)
Roadmap theme: ggplot2 feature comparability without ggplot2 API compatibility.

## Purpose

This release closes the ggplot2-comparability roadmap at the presentation layer:
theme presets, theme element controls, legend placement, accessibility labels,
and final audit documentation. Algraf already supports named themes, source and
space-local `Theme(...)` overrides, title/subtitle/caption, CLI output formats,
SVG accessibility metadata, raster output, and interaction sidecars. The
remaining gap is breadth and polish, not a new grammar model.

## Release Thesis

v0.42.0 is the **presentation parity and closure** release. It should make
Algraf charts configurable enough for the common theme, labels, legends, and
export workflows shown in ggplot2 teaching material while preserving
deterministic output and the existing block-scoped theme declaration.

## Scope Rules

- Theme presets are named base themes plus explicit overrides, not arbitrary
  scripts.
- Legend position and layout must be deterministic and included in viewport
  measurement.
- Accessibility text should be first-class chart metadata, not a hidden SVG
  side effect.
- Export behavior remains CLI/runtime responsibility; source files should not
  embed output paths.
- Presentation sugar that is only a shortcut for ordinary geometry, such as an
  optional annotation helper, must lower to literal `Text`, `Segment`, `Rect`,
  `HLine`, or `VLine` marks and pass byte-for-byte equivalence tests against
  the explicit primitive source.

## Current Coverage Audit

Already covered before this release:

- `Theme(name: "minimal" | "classic" | "dark" | "void" | "light")`;
- custom theme overrides for selected text/grid/background fields;
- chart title, subtitle, and caption;
- SVG title/description behavior;
- SVG, draw-list, raster/PNG, and interaction sidecar outputs;
- guide suppression and some legend merging.

Gaps assigned to this release:

| ggplot2 concept | Classification | Feature target |
| --------------- | -------------- | ------------------------ |
| theme_gray / bw / linedraw | Theme preset gap | Use existing `minimal`, `classic`, `dark`, `void`, and `light` themes today. |
| legend.position | Layout/theme control gap | No primitive substitute; suppress or simplify legends when placement is the issue. |
| theme element controls | Partial existing theme control | Use current structured overrides; broaden only where examples prove need. |
| labs | Mostly existing metadata/control | Use chart `title`, `subtitle`, `caption`, `Guide(label:)`, and `Scale(label:)`; alt text remains a metadata gap. |
| annotate | Existing primitive recipe | Use ordinary literal-valued geometries (`VLine`, `HLine`, `Segment`, `Text`, `Rect`). |
| ggsave | CLI/export documentation | Use CLI output flags; source remains output-path free. |

## Existing Presentation Recipes

These sketches use the current presentation surface. New theme or legend syntax
should only be added after these recipes prove insufficient.

### Current theme plus structured overrides

```text
Chart(data: "penguins.csv", width: 760, height: 500,
      title: "Penguin measurements",
      subtitle: "Bill and flipper measurements by species",
      caption: "Source: palmerpenguins") {
    Theme(name: "minimal",
          axisText: Text(size: 11, fill: "#374151"),
          gridMajor: Line(stroke: "#e5e7eb", strokeWidth: 1),
          plotBackground: "#ffffff")
    Scale(fill: species, label: "Species")
    Space(flipper_length * bill_length) {
        Point(fill: species, alpha: 0.72, size: 3)
        Smooth(method: "lm", stroke: species, se: false)
    }
}
```

This charts a scatter plot with the current structured theme override model.
Broader theme elements should extend this style, not introduce CSS strings.

### Current legend simplification

```text
Chart(data: "sales.csv", width: 820, height: 460,
      title: "Quarterly revenue mix") {
    Theme(name: "minimal")
    Guide(stroke: null)
    Scale(fill: segment,
          range: ["enterprise" => "#254e70",
                  "midmarket" => "#37718e",
                  "self_serve" => "#8ee3ef"],
          labels: ["enterprise" => "Enterprise",
                   "midmarket" => "Mid-market",
                   "self_serve" => "Self-serve"],
          label: "Segment")
    Space(quarter * revenue) {
        Bar(fill: segment, layout: "stack", alpha: 0.9)
    }
}
```

This charts a stacked bar chart with current legend labeling and suppression.
Moving the legend to bottom/top/left/right is a layout control gap, not a mark
or primitive gap.

### Annotation as ordinary geometry

```text
Chart(data: "deployment_latency.csv", width: 760, height: 420,
      title: "Latency around deployment") {
    Theme(name: "minimal")
    Guide(axis: x, label: "Time", timeFormat: "%b %-d %H:%M")
    Guide(axis: y, label: "Latency (ms)")
    Space(timestamp * latency_ms) {
        Line(stroke: service, strokeWidth: 1.8)
        VLine(x: datetime("2026-05-28T01:00:00Z"),
              stroke: "#c0392b", dash: "dashed",
              label: "deploy")
        Text(label: "deploy",
             x: datetime("2026-05-28T01:00:00Z"),
             y: 420,
             dx: 6, dy: -4,
             fill: "#c0392b",
             anchor: "start")
    }
}
```

This charts a deployment annotation without adding a special `annotate(...)`
surface. Literal-valued marks are already the annotation model.

### Multi-theme comparison as ordinary multi-chart source

```text
Chart(data: "distribution.csv", width: 520, height: 360,
      title: "Minimal theme") {
    Theme(name: "minimal")
    Space(value) {
        Density(fill: "#4c78a8", alpha: 0.45)
    }
}

Chart(data: "distribution.csv", width: 520, height: 360,
      title: "Void theme for embedding") {
    Theme(name: "void")
    Space(value) {
        Density(fill: "#4c78a8", alpha: 0.45)
    }
}
```

This charts the same density under two existing themes. Multi-chart documents
already exist, so theme comparison does not need a new faceting or export
surface.

## Feature Target Sketches

These non-runnable sketches distinguish real presentation controls from sugar
that should lower to ordinary geometry.

### Legend placement as real layout control

```text
Chart(data: "sales.csv", width: 820, height: 460,
      title: "Quarterly revenue mix") {
    Theme(name: "minimal", legendPosition: "bottom")
    Scale(fill: segment, label: "Segment")
    Space(quarter * revenue) {
        Bar(fill: segment, layout: "stack", alpha: 0.9)
    }
}
```

This feature changes guide layout and cannot be represented by a primitive mark.

### Optional annotation sugar lowers to literal marks

```text
Chart(data: "deployment_latency.csv", width: 760, height: 420,
      title: "Latency around deployment") {
    Space(timestamp * latency_ms) {
        Line(stroke: service, strokeWidth: 1.8)
        Annotate("vline",
                 x: datetime("2026-05-28T01:00:00Z"),
                 label: "deploy",
                 stroke: "#c0392b",
                 dash: "dashed")
    }
}
```

If this sugar is ever promoted, it must lower to the explicit `VLine(...)` plus
`Text(...)` source shown in the primitive recipe, with byte-for-byte identical
outputs.

## v0.42.0 Must

### 1. Add missing theme presets

Status: Planned.

- Add documented base themes corresponding to common neutral presets not yet
  covered, such as gray, bw, and linedraw if they are valuable in Algraf.
- Each preset must define concrete colors, font sizes, grid behavior, panel
  background, plot background, and axis/legend defaults.
- Add render snapshots so theme values do not drift accidentally.

### 2. Broaden theme element overrides

Status: Planned.

- Extend `Theme(...)` overrides to cover plot title, subtitle, caption, axis
  title, axis text, strip text, legend text/title, panel background, plot
  background, grid major/minor, and spacing where the layout engine supports it.
- Keep override values structured (`Text(...)`, `Line(...)`, `Rect(...)`-like
  objects) rather than untyped CSS strings.
- Validate every override with targeted diagnostics and LSP completion.

### 3. Add legend placement and layout controls

Status: Planned.

- Support at least right and bottom legend positions if layout work permits;
  top/left are desirable but may be deferred if they complicate margins.
- Define legend ordering and wrapping behavior or explicitly defer it.
- Ensure legend placement is represented in draw-list and sidecar outputs.

### 4. Complete chart labels and accessibility metadata

Status: Planned.

- Audit chart title, subtitle, caption, alt text, SVG title/desc, and runtime
  sidecar metadata.
- Add an explicit chart-level alt/description field if the current subtitle-as-
  description behavior is not sufficient.
- Ensure CLI JSON/IR output surfaces metadata consistently.

### 5. Document annotation and export parity

Status: Planned.

- Add documentation showing that Algraf annotation is ordinary geometry with
  literal or mapped properties, not a special `annotate(...)` function.
- If an annotation shortcut is promoted anyway, implement it only as lowering
  to ordinary literal marks and require byte-for-byte primitive equivalence
  tests.
- Update CLI and README guidance for SVG, PNG/raster, draw-list, and metadata
  outputs as the equivalent of common save/export workflows.

### 6. Final ggplot2-comparability audit

Status: Planned.

- Add a non-normative coverage matrix that maps ggplot2 cheat-sheet concepts to
  Algraf equivalents, implemented features, and explicitly deferred features.
- Use the matrix to decide whether any remaining item needs a follow-on plan.
- Keep the matrix descriptive; the spec remains the normative source of truth.

## v0.42.0 Should

### Theme migration examples

Status: Planned.

- Add examples that show the same chart under several themes and legend
  placements, if this does not bloat the README tutorial.

### Guide theming consistency

Status: Planned.

- Audit axes, polar guides, legends, and facet strips for consistent theme-field
  usage.

## Explicitly Deferred Past v0.42.0

- Arbitrary CSS injection or user-authored SVG fragments.
- Full ggplot2 theme element API compatibility.
- Source-level output filenames or a `ggsave` equivalent in the DSL.
- Interactive theme switching in static source.
- Exact replication of ggplot2's default visual theme.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

## Promotion Workflow

1. Specify new theme fields and presets with exact defaults before coding.
2. Update theme IR, analyzer validation, LSP metadata, and renderer layout.
3. Add snapshot tests for every preset and key legend placement.
4. Add paired sugar-vs-primitive byte-equivalence tests for any annotation or
   presentation shortcut that lowers to ordinary marks.
5. Update export/accessibility docs and examples.
6. Add the final non-normative comparability matrix after the roadmap features
   have landed.
