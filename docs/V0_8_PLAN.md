# Algraf v0.8.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_7_PLAN.md`](V0_7_PLAN.md)

## Purpose

This document defines the intended v0.8.0 release shape: making maps a
first-class citizen via the **Simple Features** model — geometry as a column
type, a projected spatial frame, and a polymorphic mark that fills regions by a
data value (a choropleth).

Algraf can already draw a *naive* map: `examples/minard.ag` maps `long * lat`
into a plain Cartesian `Space`, which suffices for a small regional route but
distorts anything country- or globe-scale because it ignores the Earth's
curvature and has no notion of a region polygon. There is no geometry data type,
no projection, and no way to fill an area by a data value.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when syntax, diagnostics, tests, and
examples land together.

## Release Thesis

v0.8.0 is a **geospatial** release: ingest geometry as a first-class column type,
project geographic coordinates into the plot, and render choropleths and overlaid
spatial layers from one declarative description — without changing how the rest of
the pipeline (parser, scales-as-values, deterministic SVG) behaves.

It is scoped by working backward from one capstone: a **US county population
choropleth**, rendered from a checked-in GeoJSON (or shapefile) fixture — fully
deterministic and offline. The release also delivers the grammar-of-graphics
payoff: a CSV point layer projected onto the same basemap, both sharing one
spatial scale.

This release builds on v0.7's source-constructor seam. v0.6 reserved
`Table name = <source-expr>` and `Chart(data: <source-expr>)`; v0.7 fills it with
`Sqlite(…)`; v0.8 adds `GeoJson(…)` and `Shapefile(…)` to the same seam rather
than inventing new source grammar.

## Design Decisions (settled)

Four forks were settled before writing this plan:

1. **Data model → Simple Features.** Add `DataType::Geometry`; store geometry
   objects columnar behind the dataframe boundary (one row = one feature). Not
   flattened-polygon CSVs.
2. **Projection engine → `proj4rs`** (pure-Rust PROJ port): friendly aliases plus
   a raw `+proj=…` escape hatch, preserving the single-binary MUST (spec §2).
3. **Mark → one polymorphic `Geo`** that dispatches on the per-row geometry object
   (Point → circle, LineString → polyline, Polygon/MultiPolygon → path).
4. **Sources → GeoJSON and Shapefile (files only).** Both are deterministic and
   need no network, so the single-binary, no-network-by-default story is fully
   preserved. They decode to the same `geo-types` objects, so they share the
   spatial scale, projection, and `Geo` render path — only ingestion differs.
   Networked spatial databases (PostGIS, and GIS columns in other SQL backends)
   are **deferred** to a future general-SQL release that handles ordinary SQL
   tables and spatial-DB geometry together; see "Explicitly Deferred" below.

Consequent settled choices:

- **Geometry is a column type.** `Column::Geometry(Vec<Option<Geometry>>)` wraps
  `geo_types::Geometry<f64>`, with a `DataValueRef::Geometry(&…)` borrow. The
  `Table` trait (spec §10.5) is unchanged — already type-agnostic — so
  parser/semantics/LSP/render see geometry only through the existing boundary.
- **GeoJSON ingestion:** FeatureCollection → one row per feature; each
  `properties` key becomes a scalar column via the existing CSV type-inference
  pipeline; `geometry` becomes the `geom` column. File order = row order.
- **Shapefile ingestion:** the `.shp` binary supplies the `geom` column; its
  sidecar `.dbf` supplies the attribute columns (run through the same inference
  pipeline). The constructor names the `.shp`; `.dbf`/`.shx` sidecars are resolved
  next to it. Record order = row order. Decodes to the same `geo-types` objects as
  GeoJSON.
- **`Space(geom)` is a spatial frame:** a 1‑D algebra over a geometry-typed column
  yields a new spatial algebra kind, not an ordinary 1‑D position axis.
- **Projection is a `Space` argument** — `projection: "<alias-or-PROJ-string>"` —
  resolved through a small alias registry over `proj4rs` (e.g. `albers_usa`,
  `mercator`, `robinson`, `equirectangular`) with raw `+proj=…` passthrough.
  Default when omitted is equirectangular (raw long/lat) so existing
  Cartesian-style maps degrade gracefully. Overlaid spaces sharing a plot MUST
  declare the same projection (diagnostic on conflict), mirroring shared position
  scales (spec §17.5).

## Capstone Example (acceptance target)

`examples/choropleth.ag` must parse, analyze, and render:

```ag
Chart(data: GeoJson("us_counties.geojson"), width: 900, height: 600,
      title: "US Population by County") {
    Theme(name: "void")
    Scale(fill: population, gradient: ["#f7fbff", "#08306b"], label: "Population")

    Space(geom, projection: "albers_usa") {
        Geo(fill: population, stroke: "#ffffff", strokeWidth: 0.25)
    }
}
```

Only the data source changes to read the same map from a shapefile — the
`Space`/`Geo`/`Scale` body is identical, because both formats decode to the same
geometry column:

```ag
Chart(data: Shapefile("cb_2018_us_county_20m.shp"), …) { … }
```

Plus a layering example proving the grammar composes — a projected CSV point
layer over a projected GeoJSON basemap, both sharing one spatial scale:

```ag
Chart(data: GeoJson("us_counties.geojson"), width: 900, height: 600) {
    Theme(name: "void")
    Table cities = "us_cities.csv"

    Space(geom, projection: "albers_usa") {                       // basemap (GeoJson)
        Geo(fill: "#eeeeee", stroke: "#ffffff", strokeWidth: 0.25)
    }
    Space(long * lat, projection: "albers_usa", data: cities) {   // points
        Point(size: 5, fill: "#cc3333", alpha: 0.85)
    }
}
```

## Scope Rules

- New sources sit behind the dataframe boundary (spec §10.5); parser/LSP/
  semantics gain no backend-specific knowledge beyond naming a source and a
  projection.
- Output MUST stay deterministic (spec §18.12, §23.6). `proj4rs` is floating-point
  trig; rely on the existing `num()` 3-decimal SVG formatter to absorb ULP noise,
  pin the `proj4rs` version, and round coordinates in snapshot tests.
- Single-binary MUST (spec §2) and no-network-by-default (spec §10.8) hold
  unchanged: every new dep — `geo-types`, the GeoJSON parser, the shapefile
  reader, and `proj4rs` — is pure-Rust and offline. No async runtime, no network
  code path enters the binary in this release.
- Reserve new diagnostic codes in the spec before implementing fallible behavior.
- Prefer the one choropleth capstone + one overlay example over a sprawl of variants.

## v0.8.0 Must

### 1. Geometry data type + columnar storage

Status: Implemented.

Acceptance criteria:

- `crates/algraf-data/src/schema.rs`: add `DataType::Geometry` with appropriate
  `is_continuous()`/`is_categorical()` (both false; spatial is its own kind).
- `crates/algraf-data/src/frame.rs`: add `Column::Geometry(Vec<Option<Geometry>>)`
  and `DataValueRef::Geometry(&Geometry)`; the `Table` trait signature is
  unchanged. New dep: `geo-types`.
- Spec: new §10.x ("Geometry values / Simple Features"); §10.7 `DataValue` gains a
  `Geometry` variant; document that geometry columns are not orderable as
  continuous/categorical domains.
- Tests: round-trip a geometry column through the dataframe; schema reports
  `Geometry`.

### 2. GeoJSON source constructor

Status: Implemented.

`GeoJson("path.geojson")` in `Chart(data:)` and `Table name = …`.

Acceptance criteria:

- Parser: no change (already parses a `CallValue`). Semantics: extend the
  source-expression dispatch (`analyzer.rs::resolve_tables` and the `Chart(data:)`
  resolution) to recognize the `GeoJson` constructor; reuse v0.7's generalized
  seam rather than special-casing.
- Loader: new `crates/algraf-data/src/geojson.rs` — parse FeatureCollection,
  properties → typed columns via the existing inference pipeline, geometry →
  `geom` column. Path resolves relative to the source dir / `--base-dir`,
  source-security §10.8 applies; file order = row order.
- CLI: `crates/algraf-cli/src/input.rs` loads GeoJson sources alongside CSV.
- Diagnostics (reserve first): GeoJSON parse / unsupported-geometry error;
  file-not-found / unreadable reuse `E1106`/`E1107`.
- Tests + a small checked-in `.geojson` fixture; LSP column completion inside a
  GeoJson-backed space resolves against the feature-property schema.

### 3. Shapefile source constructor

Status: Implemented.

`Shapefile("path.shp")` in `Chart(data:)` and `Table name = …`.

Acceptance criteria:

- Semantics: same source-expression dispatch as `GeoJson` — recognize the
  `Shapefile` constructor name; no parser change.
- Loader: new `crates/algraf-data/src/shapefile.rs` (via the `shapefile`/`dbase`
  crates or `geozero`) — `.shp` → `geom` column, `.dbf` → attribute columns
  through the existing inference pipeline. The constructor names the `.shp`;
  `.dbf`/`.shx` sidecars resolve next to it. Record order = row order. Path
  resolution and source-security §10.8 identical to GeoJSON.
- Produces the same `DataFrame` shape (a `geom` column + scalar attributes) as
  GeoJSON, so the spatial scale, projection, and `Geo` render path are unchanged.
- Diagnostics: reuse `E1106`/`E1107` (missing/unreadable); `E1805` covers a
  malformed/unsupported shapefile geometry alongside GeoJSON.
- Tests + a small checked-in shapefile fixture (`.shp` + `.dbf` + `.shx`); LSP
  column completion resolves against the `.dbf` attribute schema.

### 4. `Space(geom)` spatial frame + spatial scale

Status: Implemented.

Acceptance criteria:

- Semantics: `analyzer.rs::build_frame` recognizes a `Vector` over a
  `Geometry`-typed column and produces a spatial frame (`FrameIr` gains a spatial
  variant; algebra kind in spec §8.8 gains `Spatial`).
- Render: new `SpatialScale` in `crates/algraf-render/src/scale.rs` / `space.rs`
  — iterate the geom column, compute the geographic bounding box, project it
  (sampling vertices for non-affine projections), fit the projected bbox into the
  plot rect **preserving aspect ratio** (letterbox; equal-area maps must not
  stretch). Maps geographic (lon,lat) → projected → pixel, replacing independent
  x/y `ContinuousScale` for spatial spaces.
- Spec: §16.x spatial scale; §8.x spatial algebra kind; map spaces default to no
  lat/lon axes/grid (graticule deferred).
- Diagnostic: `E1801` spatial space requires a geometry column.

### 5. Projection via proj4rs

Status: Implemented, with a scoped deviation. The `albers_usa` **composite** with
Alaska/Hawaii insets is deferred: the checked-in county fixture is lower-48 + DC
(by decision during fixture prep), so `albers_usa` resolves to the continental
Albers equal-area projection. The alias and the conflict/diagnostic machinery are
in place; AK/HI compositing is a follow-up that swaps in the multi-region routing
without touching callers. `equirectangular`, `mercator`, `robinson`, `albers`,
and raw `+proj=…` strings all resolve.

Acceptance criteria:

- `projection:` `Space` argument (the parser already accepts arbitrary
  `key:value`; handle in `analyzer.rs::space_data` and store on `SpaceIr`).
- Alias registry mapping friendly names → PROJ strings, with raw `+proj=…`
  passthrough; source CRS defaults to WGS84 (`EPSG:4326`). New dep: `proj4rs`.
- **`albers_usa` is a composite projection, not a single PROJ string.** The
  capstone (a full 50-state county map with the conventional Alaska/Hawaii
  insets) requires d3-geo-style `albersUsa` compositing: route each coordinate by
  location to a continental / Alaska / Hawaii Albers sub-projection (proj4rs
  supplies each region's `aea` math), scale the AK/HI insets, and translate them
  into place. Implement it as a dedicated alias alongside the generic
  single-PROJ-string path, not via proj4rs alone. A plain `albers`
  (lower-48-only, single `aea` string) is the trivial sub-case and a useful
  fallback when data excludes AK/HI.
- Conflict rule: overlaid spaces must agree on projection.
- Spec: §16.x / §17.5 (shared spatial scale across overlaid spaces — union
  projected bboxes so basemap + points align).
- Diagnostics: `E1802` invalid/unknown projection; `E1803` conflicting projections
  across overlaid spaces.
- Tests: alias resolves, raw PROJ string resolves, invalid string errors,
  two-projection conflict errors.

### 6. Polymorphic `Geo` mark + choropleth fill

Status: Implemented.

Acceptance criteria:

- Registry: add `Geo` to `crates/algraf-semantics/src/registry.rs` (props: `fill`
  column|color, `stroke` color, `strokeWidth` number, `alpha` number);
  `GeometryKind::Geo` in `ir.rs`.
- Render: `crates/algraf-render/src/geom.rs` — a `geo()` routine walks the per-row
  `geo_types` value: Point → `<circle>`, LineString → `<polyline>`,
  Polygon → `<path>` with exterior + interior rings (`M…Z` per ring, even-odd
  fill), MultiPolygon → multiple subpaths. Project every coordinate through the
  spatial scale; resolve `fill` per feature via the existing `ColorSpec`
  (gradient/categorical) in `aes.rs`; fill legend reuses existing legend code.
- Determinism: features in row order, rings in source order.
- Scale: the capstone is ~3,143 county MultiPolygons; rendering and projecting
  that many features must stay within the synchronous render budget (spec §28.3).
  No streaming needed, but avoid per-coordinate allocation in the hot path.
- Spec: §13.8 registry (`Geo`), new §14.x `Geo` geometry section.
- Diagnostic: `E1804` `Geo` mark requires a spatial space; `E1805` GeoJSON
  parse / unsupported geometry type.
- Tests: snapshot of a small MultiPolygon-with-hole choropleth.

### 7. Spec, version, and example hygiene

Status: Implemented; mirrors prior releases.

Acceptance criteria:

- `Cargo.toml` workspace version → `0.8.0` (and `editors/vscode/package.json`)
  when the release branch is ready.
- Made normative: §10.1 (`GeoJson`/`Shapefile` constructors), new §10.x (geometry
  values), §10.7 (`DataValue::Geometry`), §10.8 (file path resolution unchanged),
  §8.x (spatial algebra kind), §13.8 + §14.x (`Geo`), §16.x (spatial scale +
  projection), §17.5 (shared spatial scale), §26 (new diagnostic codes), §30.4
  release table row.
- `editors/vscode/`: TextMate grammar + `language-configuration.json` learn
  `Geo`, `GeoJson`, `Shapefile`.
- README gains a geospatial tutorial section (choropleth + projected overlay),
  placed after the annotations / multi-source progression; `.geojson`, shapefile,
  and CSV fixtures checked into `examples/`.
- Examples regenerated via `./examples/generate.sh`.
- This document updated as each item completes, is rejected, or moves scope.

## v0.8.0 Should

### Projected CSV Overlays

Status: Implemented.

`Space(long * lat, projection: …)` point/line layers projected into the same
pixel space as a GeoJSON basemap (the overlay capstone above). Lands with the
shared-spatial-scale work but listed as a Should in case basemap-only is all that
fits.

## Explicitly Deferred Past v0.8.0

Carried forward and unchanged unless a later planning decision moves them:

- **Networked SQL + spatial-database sources.** A `Postgres(url:, query:)`-style
  constructor reading from a live database — including PostGIS geometry columns
  (WKB → `geo-types`) and GIS columns in other SQL backends — belongs with a
  future **general networked-SQL release** that handles ordinary SQL tables and
  spatial-DB geometry through one source mechanism. That release owns the work
  this defers: a network opt-in (spec §10.8), an `env("VAR")` function for
  credentials, an async DB driver, an SQL feature gate, and deterministic row
  ordering (`ORDER BY`). v0.8's geometry column, projection, and `Geo` mark are
  exactly what such a backend would feed into, so no v0.8 work is wasted. (Plan
  file for that release is not written yet.)
- Raster / slippy-tile basemaps (network, heavy, violates the pure-render goal).
- TopoJSON, graticules / grid lines, antimeridian & great-circle resampling.
- Spatial joins and geometry-producing stats (centroids, simplification, buffers).
- High-accuracy projection grid-shift files (`.gsb`/`.tif`).
- Everything still deferred from v0.7 and the standing deferred list in
  [`V0_3_PLAN.md`](V0_3_PLAN.md).

## Diagnostic Codes to Reserve (before implementation)

Next free semantic block is `E18xx`:

- `E1801` spatial space requires a geometry column (`Space(geom)` used with a
  non-geometry column)
- `E1802` invalid or unknown projection (alias not found / bad PROJ string)
- `E1803` overlaid spaces declare conflicting projections
- `E1804` `Geo` mark requires a spatial space
- `E1805` GeoJSON / shapefile parse error or unsupported geometry type

(File-not-found / unreadable reuse `E1106`/`E1107`.)

## Optional-Item Audit

### Promote In v0.8.0 (Must)

- Geometry data type + columnar storage.
- GeoJSON source constructor.
- Shapefile source constructor.
- `Space(geom)` spatial frame + spatial scale (aspect-preserving bbox fit).
- Projection via proj4rs (`projection:` arg, alias registry + PROJ passthrough).
- Polymorphic `Geo` mark + choropleth fill.

### Consider If Capacity Allows (Should)

- Projected CSV overlays on a basemap.

### Keep Deferred

- Networked SQL + spatial-database sources (PostGIS et al.) → future general-SQL release.
- Raster / tile basemaps, TopoJSON, graticules, resampling.
- Spatial joins / geometry-producing stats, grid-shift files.

## Promotion Workflow

1. Move the chosen behavior into the relevant normative section of
   [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) (algebra §8, data §10, registry §13,
   geometry §14, scales §16, layout §17.5, diagnostics §26).
2. Reserve or add diagnostic codes before implementation if behavior can fail.
3. Implement behind the dataframe boundary; keep parser/LSP/semantics/render
   backend-agnostic (spec §10.5). Every new dependency stays pure-Rust and
   offline so the single-binary, no-network-by-default story is preserved.
4. Add focused tests in the crate closest to the behavior, plus a snapshot for the
   choropleth capstone.
5. Add or update examples with checked-in fixtures when sources change.
6. Regenerate examples via `./examples/generate.sh` when rendered output changes.
7. Update this document when a candidate is completed, rejected, or moves scope.
