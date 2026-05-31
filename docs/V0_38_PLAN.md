# Algraf v0.38.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_37_PLAN.md`](V0_37_PLAN.md)
Follow-on plan: [`V0_39_PLAN.md`](V0_39_PLAN.md)
Roadmap theme: ggplot2 feature comparability without ggplot2 API compatibility.

## Purpose

This release covers z-field and gridded-data graphics: rasters, contours,
filled contours, 2D density contours, and 2D summary grids. Algraf already has
`Tile`, `Bin2D`, and `HexBin`, so the foundation exists. The missing pieces are
continuous field extraction, contour generation, regular-grid raster behavior,
and summary aggregation over x/y bins with a third variable.

## Release Thesis

v0.38.0 is the **z-field statistics** release. It should make "three-variable"
plots feature-comparable with ggplot2's contour, raster, tile, and summary-bin
family while preserving Algraf's explicit `Space` plus `Derive` model.

The main architectural rule: field-like graphics should produce ordinary
derived tables or primitive marks wherever practical. Contours may require path
generation, but their output still needs deterministic ordering and backend
parity.

## Scope Rules

- `Tile` stays the basic rectangular-cell mark. This release adds behavior that
  `Tile` cannot express cleanly: regular raster cells, interpolation policy,
  contour isolines, filled contour bands, and z-summary stats.
- All z-field stats must define output schemas for LSP inlay hints and
  `Space(..., data: derived)` completions.
- All algorithms must be deterministic: stable bin boundaries, stable contour
  path ordering, stable tie handling, and no locale/time dependence.
- Heavy interpolation and smoothing choices must be explicit or deferred.
- Any promoted visual sugar such as contour, filled-contour, raster, or summary
  marks must lower to derived tables plus `Path`, `Geo`, `Rect`, or `Tile`.
  Sugar and explicit primitive sources must produce byte-for-byte identical
  SVG, draw-list JSON, raster output, and interaction sidecar bytes.

## Current Coverage Audit

Already covered before this release:

- `Tile(fill: value)` over two axes;
- `Bin2D` and `HexBin` count surfaces;
- `Scale(fill: ..., gradient: ...)` for continuous colors;
- spatial `Geo` and projections, which are separate from Cartesian z-fields.

Gaps assigned to this release:

| ggplot2 concept | Classification | Feature target |
| --------------- | -------------- | ------------------------ |
| raster | Primitive recipe for non-interpolated grids; possible backend optimization later | Use `Tile` or `Rect` with explicit cell bounds. |
| contour | Derived stat feature plus possible sugar | Generate contour vertices into a table and render `Path`; any `Contour` mark lowers to that form. |
| contour_filled | Derived stat feature plus possible sugar | Generate band geometry and render `Geo`, or explicit cells with `Rect`; any sugar lowers to those primitives. |
| density_2d | Derived stat feature | Generate density grids or contour tables and render `Path`, `Tile`, or `Rect`. |
| summary_2d | Derived stat feature | Generate x/y bin bounds and summary value, then render `Rect`. |
| summary_hex | Derived stat feature; partial existing support | `HexBin` handles counts; z summaries add a table transform that feeds hex geometry or cells. |

## Primitive Recipe Sketches

These sketches show how to chart z-field outputs today when the field, contour,
or summary data has already been materialized.

### Regular raster-like field with tiles

```text
Chart(data: "temperature_grid.csv", width: 720, height: 520,
      title: "Surface temperature grid") {
    Scale(fill: temp_c, gradient: ["#2b6cb0", "#f7fafc", "#c53030"])
    Guide(axis: x, label: "Longitude")
    Guide(axis: y, label: "Latitude")
    Space(lon * lat) {
        Tile(fill: temp_c)
    }
}
```

This charts a regular x/y grid as heatmap cells. If the cell centers do not
imply the desired extents, use `Rect` with explicit `xmin`/`xmax`/`ymin`/`ymax`
columns instead.

### Precomputed contours as paths

```text
Chart(data: "elevation_points.csv", width: 720, height: 520,
      title: "Elevation contours") {
    Table contours = "elevation_contours.csv"

    Space(x * y, data: contours) {
        Scale(stroke: level, gradient: ["#6b7280", "#111827"])
        Path(group: contour_id, stroke: level, strokeWidth: 1)
        Text(label: level_label, x: label_x, y: label_y,
             size: 9, fill: "#111827")
    }
}
```

This charts contour isolines after an external or future `Derive` step has
materialized contour vertices. The rendering side is already primitive: grouped
paths plus optional labels.

### Filled contour bands as geometry data

```text
Chart(data: GeoJson("pressure_bands.geojson"), width: 720, height: 520,
      title: "Pressure bands") {
    Scale(fill: band, palette: "accent")
    Space(geom) {
        Geo(fill: band, stroke: "#ffffff", strokeWidth: 0.5)
    }
}
```

This charts filled isoline bands as ordinary geometry data. The missing piece is
the stat that creates those bands, not a special filled-contour rendering model.

### 2D summary grid rendered with primitive rectangles

```text
Chart(data: "sensor_summary_grid.csv", width: 760, height: 520,
      title: "Mean signal by spatial bin") {
    Scale(fill: value, gradient: ["#f7fbff", "#08306b"])
    Space(x_center * y_center) {
        Rect(xmin: x_start, xmax: x_end,
             ymin: y_start, ymax: y_end,
             fill: value, stroke: "#ffffff", strokeWidth: 0.25)
    }
}
```

This charts a binned x/y/z summary once the summary table already exists. The
future language work is a `Derive` stat that produces these columns
deterministically.

### 2D density contours as precomputed tables

```text
Chart(data: "samples.csv", width: 720, height: 520,
      title: "Bivariate density levels") {
    Table levels = "density_contours.csv"

    Space(x * y) {
        Point(alpha: 0.18, fill: "#111827", size: 1.4)
    }
    Space(grid_x * grid_y, data: levels) {
        Path(group: contour_id, stroke: level, strokeWidth: 1.2)
    }
}
```

This charts raw samples plus precomputed density isolines. The deeper direction
remains functional composition: density tables feed contour tables, and paths
render rows.

## Feature Target Sketches

These sketches show the new derived stats feeding primitive marks, plus the
equivalence rule for any future source-level sugar.

### Contour stat feeding paths

```text
Chart(data: "elevation_grid.csv", width: 720, height: 520,
      title: "Elevation contours") {
    Derive contours = ContourLines(x, y, z: elevation, levels: 12)

    Space(x * y, data: contours) {
        Path(group: contour_id, stroke: level, strokeWidth: 1)
    }
}
```

This feature computes contour vertices. Rendering remains grouped `Path`.

### Summary grid feeding rectangles

```text
Chart(data: "sensor_samples.csv", width: 760, height: 520,
      title: "Mean signal by spatial bin") {
    Derive grid = Summary2D(x, y, z: signal,
                            bins: [48, 32],
                            reducer: "mean")

    Space(x_center * y_center, data: grid) {
        Rect(xmin: x_start, xmax: x_end,
             ymin: y_start, ymax: y_end,
             fill: value)
    }
}
```

This feature computes the bin table. The visual mark stays `Rect`.

### Optional contour sugar lowers exactly

If future source-level contour sugar is promoted, it must lower to
`ContourLines(...)` plus `Path(...)` before rendering, with byte-for-byte output
equivalence. v0.38.0 intentionally ships only the explicit derived-stat form.

## v0.38.0 Must

### 1. Add a z-channel validation model

Status: Implemented.

- Introduce a consistent way for geometries/stats to require a third numeric
  column while x and y come from the inherited frame.
- Ensure z mappings are resolved against the active table, including derived
  tables and named table spaces.
- Add targeted diagnostics for missing z, non-numeric z, and unsupported frame
  shapes.

### 2. Add regular raster semantics

Status: Implemented.

- Document the `Tile`/`Rect` recipe for regular grids, then define any
  raster-specific primitive only if deterministic cell extents or backend
  optimization require it.
- Decide whether interpolation is in scope for v0.38. If included, it must be
  backend-consistent; if not, document nearest-cell rendering only.
- Ensure raster cells participate in fill scales and legends exactly like `Tile`
  where possible.

Decision: no interpolation keyword or raster-specific primitive is included in
v0.38. Regular numeric grids use explicit `Rect` cell bounds; banded grids may
continue to use `Tile`. Both paths train normal fill scales and legends.

### 3. Add contour and filled-contour generation

Status: Implemented.

- Implement deterministic contour isolines over gridded x/y/z data.
- Define contour level selection, explicit levels, and output columns before
  implementation.
- Add filled contour bands only after the polygon fill model from v0.36 is
  available, or explicitly lower to backend path primitives.
- Specify behavior for missing grid cells and non-rectangular grids.
- Add any `Contour` or `ContourFilled` source-level sugar only as exact
  lowerings over the derived-table forms.

Decision: source-level `Contour`/`ContourFilled` sugar remains deferred.
`ContourLines` emits grouped path vertices; `ContourBands` emits geometry
polygons that render through `Geo`.

### 4. Add 2D density contours

Status: Implemented.

- Add a 2D KDE stat over continuous x/y data with deterministic bandwidth and
  grid defaults.
- Expose contour-level output for line contours and filled bands.
- Keep expensive defaults bounded and document performance costs.

### 5. Add x/y/z summary stats

Status: Implemented.

- Add 2D rectangular summary and hex summary stats that aggregate z by x/y bin.
- Start with a small deterministic set of reducers: count, mean, min, max, sum,
  and median if the implementation has a stable median helper.
- Define empty-bin handling, output schemas, fill-domain training, and examples.

### 6. Spec, examples, README, LSP, and backend hygiene

Status: Implemented.

- Update stats, geometry, scale, render, diagnostic, LSP, and backend sections.
- Add examples for raster, contour lines, filled contours, 2D density contours,
  rectangular z summaries, and hex z summaries.
- Add tests for deterministic output order and backend parity.

## v0.38.0 Should

### Contour label spike

Status: Deferred past v0.38.0.

- Evaluate whether contour labels belong in this release. If not, document a
  `Text` overlay path through derived contour metadata where possible.

Decision: automatic contour-label placement is deferred. The derived contour
tables expose `level` metadata; authors can overlay `Text` with authored or
separately derived label positions.

### Shared grid helpers

Status: Implemented.

- Factor common grid/bin boundary planning so `Bin2D`, `HexBin`, summaries, and
  raster/contour inputs do not drift.

## Explicitly Deferred Past v0.38.0

- Irregular triangulated contouring.
- Spatial raster basemaps and web tile fetching.
- Large lazy raster datasets.
- GPU/WebGL acceleration.
- Arbitrary user-defined reducer functions.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

## Promotion Workflow

1. Specify z-channel property resolution and diagnostics.
2. Add stat output schemas before render execution.
3. Implement summary/raster paths first, then contour line generation, then
   filled contours.
4. Add deterministic algorithm tests with stable fixtures.
5. Add paired sugar-vs-primitive byte-equivalence tests for every promoted
   convenience form.
6. Add examples and README sections only after the source syntax is accepted.
