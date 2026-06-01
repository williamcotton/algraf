# Algraf v0.48.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_47_PLAN.md`](V0_47_PLAN.md)
Roadmap theme: editor hover parity for table-backed data sources.

## Purpose

v0.48.0 closes the editor gap between derived tables, named tables, and
constructor-backed data sources. Authors can inspect a table binding without
remembering whether it came from `Derive`, `Table`, a direct CSV path, or an
explicit source constructor such as `Parquet(...)`.

## Scope

### Named Table Hover

Status: Implemented.

Hover on a named table shows its sampled schema when available.

Supported sites:

- `Table name = ...` declaration names.
- `Chart(data: name)` primary table references.
- `Space(..., data: name)` table bindings.
- `Derive output from name = ...` input-table references.

The hover identifies the source spelling when available and reuses the same
bounded preview data as source-string hover.

### Source Constructor Path Hover

Status: Implemented.

Source preview hover applies to the path string inside recognized source
constructors wherever direct string-path hover already applied.

Examples:

```ag
Chart(data: Parquet("rides.parquet")) {
    Space(trip_distance * total_amount) {
        Point()
    }
}

Chart(data: "main.csv") {
    Table zones = TopoJson("zones.topojson", object: "tracts")
    Space(geom, data: zones) {
        Geo()
    }
}
```

Raw row preview remains bounded and format-dependent. CSV/TSV may show rows;
Parquet and geospatial constructors may show only schema and sample values when
the schema loader provides them.

## Non-Goals

- No new data source syntax.
- No change to schema loading, cache invalidation, or render behavior.
- No raw row preview requirement for Parquet or geospatial formats.
- No change to duplicate table-name diagnostics or scope rules.

## Validation

- Editor-service hover tests cover named table declarations, `data:` bindings,
  `Derive ... from` named-table inputs, and constructor path previews.
- Shared service tests cover constructor-backed named table source hover through
  the same analysis path used by browser and LSP clients.
