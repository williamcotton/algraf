# Algraf v0.90.0 Plan

Status: Implemented
Target version: 0.90.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_89_PLAN.md`](V0_89_PLAN.md)
Roadmap theme: Annotated categorical heatmaps with first-class layer polish.

## Purpose

Algraf v0.90 should make the annotated heatmap workflow expressible without
dropping into a producer language for chart-specific drawing decisions. The
acceptance target is the Plotnine annotated heatmap pattern:

- a complete categorical x by categorical y tile grid;
- a continuous fill scale with a colorbar-style legend;
- numeric labels centered in every tile;
- label color chosen from a derived low/high class;
- January shown at the top through an explicit y-axis order or reverse;
- axis titles and ticks suppressed while tick labels remain;
- small gutters between adjacent tiles.

The release should reuse existing language pieces where they already fit:
`Derive ... = Cut(...)`, `Tile`, `Text`, categorical axis coercion, categorical
axis ordering, reversed position axes, continuous gradients, manual categorical
color maps, and theme overrides.

## Release Thesis

v0.90.0 is a **layer polish and heatmap parity** release. It should not add a
new chart family, a dataframe expression language, or ggplot compatibility
aliases. Instead it should close the small but user-visible gaps that prevent a
clean, direct annotated heatmap recipe from being a tutorial-quality Algraf
example.

## Current Coverage Audit

Already available:

- `Tile(fill: value)` over categorical x/y spaces.
- `Text(label: value, fill: group, anchor: "middle")` centered on inherited
  space positions.
- `Cut(value, breaks:, labels:, output:)` for the low/high label class.
- `Scale(axis: x, type: "categorical")` for integer years as discrete bands.
- `Scale(axis: y, reverse: true)` and string-array `domain:` for top-to-bottom
  categorical ordering.
- `Scale(fill: value, gradient:, breaks:, labels:, label:)` for numeric fill.
- `Scale(fill: class, range: ["low" => "...", "high" => "..."])` for manual
  label colors.
- `Theme(axisTitle: Text(hidden: true), axisTicks: Line(strokeWidth: 0),
  panelBackground: Rect(...))` for minimal chart chrome.

Gaps this release should close:

- `Tile` has no author-facing `width`/`height` fraction, so tile gutters require
  indirect axis padding and affect every layer in the space.
- A mapped annotation layer currently contributes legends like any other mapped
  aesthetic. Plotnine's `show_legend=False` equivalent is missing.
- Continuous color legends are rendered as stepped swatches. The heatmap target
  needs a compact colorbar-style guide with deterministic ticks and labels.
- Multiple scales for the same aesthetic but different columns expose that
  legend title lookup should be column-scoped, not just aesthetic-scoped.

## Target Recipe

The completed release should allow a chart shaped like this:

```ag
Chart(data: "air_passengers.csv", width: 900, height: 650) {
    Derive annotated = Cut(passengers,
                           breaks: [0, 300],
                           labels: ["low", "high"],
                           output: "p_group")

    Theme(
        axisTitle: Text(hidden: true),
        axisTicks: Line(strokeWidth: 0),
        panelBackground: Rect(fill: "#ffffff", stroke: "#ffffff", strokeWidth: 0),
        plotBackground: "#ffffff",
        legendPosition: "right"
    )

    Space(year * month, data: annotated) {
        Scale(axis: x, type: "categorical")
        Scale(axis: y, reverse: true)
        Scale(fill: passengers,
              gradient: ["#440154", "#31688e", "#35b779", "#fde725"],
              breaks: [200, 300, 400, 500, 600],
              label: "passengers")
        Scale(fill: p_group,
              range: ["low" => "#ffffff", "high" => "#000000"])

        Tile(fill: passengers, width: 0.95, height: 0.95)
        Text(label: passengers,
             fill: p_group,
             size: 18,
             anchor: "middle",
             legend: false)
    }
}
```

The exact example dimensions may change during visual review, but the language
features above should be sufficient. The `p_group` scale is space-local because
`p_group` is created by the derived table.

## v0.90.0 Must

### Fractional Tile Sizing

Status: Implemented.

Add optional `width` and `height` properties to `Tile`.

Acceptance criteria:

- `Tile(width: n, height: n)` accepts finite numeric literals in `(0, 1]`.
- The values are fractions of the resolved x and y band widths. Defaults remain
  `1.0`.
- Cartesian tiles stay centered in their band cells and use `bandwidth * width`
  and `band height * height`.
- Polar annular tiles apply the same fractions to angular and radial bands.
- Invalid values emit the same property validation class used for invalid
  numeric geometry settings and do not panic.
- Existing `Tile(...)` output remains visually unchanged when width/height are
  omitted.

Implementation touch points:

- `crates/algraf-semantics/src/ir.rs` if a new `PropertyKey` is needed, or reuse
  existing `Width` plus add `Height` if absent.
- `crates/algraf-semantics/src/registry.rs` for `Tile` property metadata,
  completion docs, and hover text.
- `crates/algraf-render/src/geom/rect_tile.rs` for Cartesian and polar emission.
- `docs/ALGRAF_SPEC.md`, language templates, and examples.

### Layer-Level Legend Suppression

Status: Implemented.

Add a geometry-level `legend: false` property, equivalent to suppressing legend
candidates produced by that one layer while leaving all marks and other layers
unchanged.

Acceptance criteria:

- `legend` is accepted as a boolean property on built-in geometries that can
  produce mapped aesthetic legends. Default is `true`.
- When `legend: false`, the geometry contributes no fill, stroke, size,
  strokeWidth, shape, or image legend candidates.
- The same space's other layers still contribute legends normally.
- `Guide(legend: false)`, `Guide(fill: null)`, and `Guide(stroke: null)` keep
  their existing broader behavior.
- Mark rendering, scale training, interaction metadata, and clipping are
  unaffected.
- LSP completion, hover, signature help, formatting, and the language templates
  document the new property.

Implementation touch points:

- Add `PropertyKey::Legend` if no reusable key exists.
- Add a common property spec path so the property is not copied inconsistently
  across geometry registries.
- Short-circuit `collect_geometry_legend_candidates` in
  `crates/algraf-render/src/render/legend.rs` when the geometry setting is
  explicitly false.

### Column-Scoped Scale Labels For Legends

Status: Implemented.

Make legend title lookup match both the aesthetic and the mapped column.

Acceptance criteria:

- `Scale(fill: passengers, label: "passengers")` titles only the `passengers`
  fill legend.
- A second `Scale(fill: p_group, range: ...)` does not accidentally inherit the
  passenger legend title if its own legend is enabled.
- Existing single-scale charts keep their current titles.
- Tests cover at least two `fill` scales in one space.

### Colorbar-Style Continuous Legends

Status: Implemented.

Render continuous color legends as compact colorbars rather than discrete
swatch rows.

Acceptance criteria:

- Continuous fill/stroke legends render a vertical colorbar for right/left
  legend positions and a horizontal colorbar for top/bottom positions.
- Tick labels come from `breaks:`/`labels:` when supplied, otherwise from the
  existing deterministic gradient tick generation.
- The emitted guide is deterministic and backend-neutral: SVG, raster, and
  draw-list paths use the same planned guide model.
- The default output remains compact enough for tutorial examples.
- Existing continuous legends remain semantically equivalent, with visual diffs
  accepted and snapshot tests updated deliberately.

Implementation touch points:

- Extend the `Legend` model in `crates/algraf-render/src/aes.rs` to carry the
  continuous domain, gradient stops, and tick labels, or add a dedicated
  colorbar legend payload.
- Update `crates/algraf-render/src/guide/plan.rs` measurement for colorbars.
- Update `crates/algraf-render/src/guide/emit.rs` to emit deterministic
  segmented colorbar primitives through `MarkSink`, not SVG-only defs.
- Add render snapshots for explicit breaks and default ticks.

### Annotated Heatmap Example

Status: Implemented.

Add a tutorial example that recreates the air-passenger annotated heatmap.

Acceptance criteria:

- Add `examples/annotated_heatmap.ag` and a matching CSV source with
  `year`, `month`, and `passengers` columns.
- The example uses `Cut` for the text color class, `Tile(width: 0.95,
  height: 0.95)`, `Text(..., legend: false)`, categorical x coercion, reversed
  y order, and a continuous passenger colorbar.
- Regenerate SVG/PNG example outputs and visually inspect the PNG.
- Add the example to the top-level `README.md` in the heatmap or annotation part
  of the tutorial progression.

## v0.90.0 Should

### Band-Grid Aspect Control

Status: Implemented.

Evaluate whether categorical `Space(..., aspect: 1)` should make x/y band steps
equal for heatmap-like grids. If implementation is small and does not destabilize
layout, promote it in v0.90; otherwise keep explicit chart dimensions as the
documented workaround.

Acceptance criteria if promoted:

- `aspect` on categorical x/y spaces preserves equal band-step ratios inside
  the final plot rectangle.
- Facets, legends, and axis margin reservation remain deterministic.
- Existing categorical charts without `aspect` are unchanged.

### Named Viridis Gradient

Status: Implemented.

Consider a named continuous gradient such as `gradient: "viridis"` only if it can
reuse the existing gradient validation and interpolation model cleanly. The
Must-scope example can already use explicit color stops, so named gradients are
not required for v0.90.

## Explicitly Deferred Past v0.90.0

- Automatic label contrast such as `Text(fill: contrast(passengers))`.
- A general expression or mutate language for arbitrary per-row conditionals.
- A `geom_label`-style auto-sized label box. Existing `Rect` plus `Text` remains
  the explicit form.
- Full ggplot/Plotnine naming aliases such as `show_legend`, `scale_*_viridis`,
  or `coord_equal`.
- Automatic text fitting or clipping inside tiles.
- Legend tick marks and colorbar minor ticks beyond the deterministic major
  labels needed by this release.

## Spec, Templates, And Editor Artifacts

When implementation starts, update the implemented surface in the same change:

- `docs/ALGRAF_SPEC.md` sections for `Tile`, geometry properties, guides,
  legends, diagnostics, and the milestone table.
- `crates/algraf-cli/templates/ALGRAF_LANGUAGE.md` and the composed
  `ALGRAF_LANG.md`.
- Editor service metadata for completion, hover, signature help, semantic
  validation, and formatting.
- VS Code TextMate grammar only if new keywords or syntax forms require it.

Workspace/spec/package version stamps are bumped to `0.90.0` once v0.89 is
closed and this plan is implemented as the active release.

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- Focused semantic tests for `Tile(width:, height:)` and `legend: false`.
- Focused render tests for tile gutters, text legend suppression, column-scoped
  legend labels, and colorbar output.
- `./examples/generate.sh`
- Manual PNG review of `examples/annotated_heatmap.png`.

## Promotion Workflow

1. Close or explicitly supersede v0.89 release-artifact work before making
   v0.90 the active implementation target.
2. Promote each Must item into `docs/ALGRAF_SPEC.md` with stable behavior and
   diagnostics before implementation lands.
3. Implement semantics, rendering, editor services, templates, examples, and
   tests in the same release branch.
4. Update this plan's `Status:` lines as each item lands, defers, or changes
   scope.
5. When complete, align every required version stamp and add the v0.90 row to
   the spec milestone table.
