# Algraf v0.22.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_21_PLAN.md`](V0_21_PLAN.md)
Follow-on plan: [`V0_23_PLAN.md`](V0_23_PLAN.md)

## Purpose

This document defines the intended v0.22.0 release shape: completing the
geospatial features that v0.8 deliberately deferred after establishing geometry
columns, projections, and the `Geo` mark.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.22.0 is a **geospatial completion** release: make maps more correct and more
useful without changing the rest of the grammar-of-graphics model.

The release promotes the geospatial items called out in v0.8's deferred list:
the full `albers_usa` composite, TopoJSON ingestion, graticules, antimeridian
and great-circle handling, and basic geometry-producing spatial stats.

## Current Debt Surface

The plan/spec audit found:

- v0.8 implemented `albers_usa` as lower-48 Albers and deferred Alaska/Hawaii
  insets.
- v0.8 deferred TopoJSON, graticules, antimeridian handling, great-circle
  resampling, spatial joins, geometry-producing stats, raster/slippy basemaps,
  and grid-shift files.
- Spatial spaces currently draw no latitude/longitude axes or grid lines.
- PostGIS geometry columns are planned as part of the broader SQL source work,
  but map rendering already has the geometry column and `Geo` foundation.

## Scope Rules

- New geospatial sources and transforms must use the existing geometry column
  model.
- No network tile fetching by default.
- No general basemap service integration in this release.
- Projection and geometry operations must remain deterministic enough for
  snapshot tests.
- Keep non-spatial chart output unchanged.
- Avoid high-accuracy grid-shift dependencies unless they are pure-Rust,
  optional, and deterministic.

## Capstone Acceptance Target

The capstone is a full US county map using the conventional Alaska/Hawaii insets
plus a graticule example:

```ag
Chart(data: GeoJson("us_counties.geojson"), width: 900, height: 600) {
    Theme(name: "void")
    Space(geom, projection: "albers_usa") {
        Geo(fill: population, stroke: "#ffffff", strokeWidth: 0.25)
        Graticule(stroke: "#cccccc", strokeWidth: 0.5)
    }
}
```

The release must pass workspace tests and example regeneration.

## Design Decisions (settled)

1. **Keep Simple Features as the exchange model.** New sources and stats produce
   geometry columns, not renderer-specific path fragments.
2. **Projection correctness precedes basemaps.** Composite projections and
   resampling should land before tile layers.
3. **Graticules are guides, not data marks.** They are tied to spatial scales and
   projection settings.
4. **Spatial operations are explicit stats.** Centroids, simplification, buffers,
   and joins should be visible in the source or derived-table model.

## v0.22.0 Must

### 1. Full `albers_usa` composite projection

Status: Planned.

Acceptance criteria:

- `projection: "albers_usa"` routes lower-48, Alaska, and Hawaii coordinates
  through the conventional composite projection with deterministic scale and
  translation.
- Existing lower-48 examples keep their current appearance unless a deliberate
  fixture update is documented.
- Tests cover points and polygons in lower-48, Alaska, and Hawaii regions.
- Projection conflict diagnostics remain unchanged.

### 2. TopoJSON source constructor

Status: Planned.

Acceptance criteria:

- Add `TopoJson("path.topojson", object: "name")` or an explicitly specified
  equivalent source constructor.
- Topology arcs decode to the same geometry column model as GeoJSON/Shapefile.
- Properties become scalar columns through the existing inference pipeline.
- Missing object names, malformed topology, and unsupported geometry types emit
  targeted diagnostics.
- LSP completions and preview support the constructor through the driver.

### 3. Graticule guide/mark

Status: Planned.

Acceptance criteria:

- Add a spatial-only graticule surface, either `Guide(graticule: true)` or a
  dedicated `Graticule(...)` mark, with the syntax specified before
  implementation.
- Graticules project longitude/latitude lines through the active spatial scale.
- Styling is deterministic and theme-compatible.
- Non-spatial use emits a targeted diagnostic.
- Render tests cover at least equirectangular, mercator, and albers-style
  projections.

### 4. Antimeridian and great-circle resampling

Status: Planned.

Acceptance criteria:

- Spatial rendering handles geometries crossing the antimeridian without drawing
  long erroneous chords across the map.
- Long line segments can be resampled before projection when the projection
  requires it.
- Resampling thresholds are deterministic and documented.
- Existing spatial examples remain stable unless intentionally updated.

### 5. Geometry-producing spatial stats

Status: Planned.

Acceptance criteria:

- Add explicit derived stats for at least centroids and simplification, e.g.
  `Derive centroids = Centroid(geom)` and `Derive simple = Simplify(geom, ...)`.
- Output schemas include geometry columns and scalar passthrough columns where
  specified.
- Stats are pure, deterministic, and do not read external resources.
- Tests cover schema planning, execution, and `Geo` rendering of outputs.

### 6. Spatial joins

Status: Planned.

Acceptance criteria:

- Specify a join surface for joining point/geometry tables to polygon/geometry
  tables by spatial predicate.
- The join result is a named derived table behind the normal dataframe boundary.
- Deterministic behavior is specified for multiple matching polygons and missing
  geometries.
- The first implementation supports a narrow predicate such as `within`.

### 7. Spec, plan, and example hygiene

Status: Planned.

Acceptance criteria:

- Workspace and VS Code versions are bumped to `0.22.0` when the release branch
  is ready.
- Spec §10, §14, §15, §16, §17, §19, §21, §26, and §30 are updated for promoted
  geospatial behavior.
- README and examples add focused geospatial examples with checked-in fixtures.
- Examples are regenerated with `./examples/generate.sh`.

## v0.22.0 Should

### Grid-shift file audit

Status: Planned.

Document whether high-accuracy grid-shift files can be supported as optional,
offline resources without violating determinism or single-binary expectations.

### Raster/tile basemap design note

Status: Planned.

Write a design note for raster or slippy-tile basemaps, including network
policy, caching, attribution, and offline fixture requirements. Do not implement
network tile fetching in this release.

## Explicitly Deferred Past v0.22.0

- Default network basemaps or tile fetching.
- High-accuracy grid-shift implementation unless promoted from the audit.
- WebGL map rendering.
- Large-scale spatial indexing beyond the first spatial join implementation.

## Optional-Item Audit

### Promote In v0.22.0 (Must)

- Full `albers_usa` composite projection.
- TopoJSON source constructor.
- Graticule guide/mark.
- Antimeridian and great-circle resampling.
- Geometry-producing spatial stats.
- Spatial joins.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- Grid-shift file audit.
- Raster/tile basemap design note.

### Keep Deferred

- Network basemaps, WebGL maps, and broad GIS engine scope.

## Promotion Workflow

1. Add spatial projection and geometry guard tests.
2. Implement full `albers_usa` routing.
3. Add TopoJSON loading behind the dataframe boundary.
4. Add graticule rendering and resampling.
5. Add geometry-producing stats.
6. Add the narrow spatial join surface.
7. Update examples, specs, and docs; run the full no-drift test suite.
