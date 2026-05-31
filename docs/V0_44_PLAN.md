# Algraf v0.44.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_43_PLAN.md`](V0_43_PLAN.md)
Roadmap theme: Compositional glyph charts and recursive render scenes.

## Purpose

This release adds **inset plots**: small, data-bound child charts anchored at
positions in a parent chart. The motivating example is a projected city map
where each city marker is a pie or donut chart instead of a plain point, but the
same model should support scatterplot points that contain sparklines, network
nodes that contain radial summaries, heatmap cells that contain distributions,
and nested glyph charts where one inset contains another.

Algraf already has the pieces that make this natural:

- block-scoped `Space(...)` declarations;
- polar coordinates for pies, donuts, coxcombs, wind roses, and radial bars;
- projected geospatial overlays;
- draw-list, raster, SVG, and interaction-metadata backends;
- deterministic mark budgets from v0.43.0.

The missing piece is a language and render-planning construct that says: place a
child chart at this parent row's resolved position, bind it to matching child
data, train its local or shared scales, and render it inside a bounded viewport.

## Release Thesis

v0.44.0 is the **inset plot and recursive scene** release. It should promote a
first-class `Inset(...) { ... }` container that behaves like a mark-anchored
child chart rather than like a point shape, arbitrary SVG fragment, or custom
geometry plugin.

The renderer should be recursive from the first implementation. Even if the
first examples are only one level deep, the internal render model should support
nested insets so the feature does not hard-code a one-level special case that
would become immediate technical debt.

## Scope Rules

- `Inset` is an explicit block item in a `Space`, not a `Point(shape: ...)`
  shortcut and not an arbitrary SVG injection surface.
- An inset owns one or more child `Space` blocks. Child spaces render into the
  inset viewport, not into the parent plot rectangle.
- Nested insets are in scope from the first design. The implementation may set a
  conservative maximum depth, but the IR and renderer must be recursive rather
  than one-level-only.
- Parent and child row context must be explicit and deterministic. Inset data
  matching must not silently cross-join large tables.
- Child scale policy must be declared and testable. At minimum v0.44.0 should
  support shared scales across all instances of one inset declaration and local
  scales per inset instance.
- Position guides inside insets default off. Shared aesthetic legends may be
  promoted to the outer chart, but repeated per-inset axes and legends must not
  be the default.
- SVG, draw-list, raster, and interaction metadata must represent the same
  inset scene. Do not add an SVG-only feature.
- Mark budgets apply recursively. Inset charts multiply mark counts and must be
  estimated before pathological output is emitted.
- Inset contents are ordinary Algraf marks and spaces. No user-authored
  JavaScript, CSS, HTML, external image embedding, or raw SVG fragments ship as
  part of this feature.
- The release does not attempt general-purpose layout composition. `Inset`
  handles mark-anchored child viewports; dashboards, page grids, and arbitrary
  nested chart documents remain separate concerns.

## Current Coverage Audit

Already covered before this release:

- projected map overlays such as `Space(long * lat, projection: "albers_usa")`
  with `Point` and `Text` layers;
- polar pies and donuts via `Space(value, coords: "polar", theta: "y")` plus
  `Bar(layout: "fill")`;
- multiple tables and derived tables;
- space-local scales, guides, and themes;
- draw-list/raster/SVG parity through the shared mark sink;
- interaction sidecar metadata for ordinary per-row marks;
- deterministic raw-mark budgets.

Gaps assigned to this release:

| Area | Current shape | Inset gap |
| ---- | ------------- | --------- |
| Grammar | `Space` body holds geometry calls and local declarations | No block item can own child spaces. |
| Semantic IR | `ChartIr` owns a flat `spaces: Vec<SpaceIr>` | No recursive layer tree or child scene node. |
| Data context | A space has one active table | No parent-row stack or child-table match policy. |
| Scale training | Scales train per chart/space/panel | No per-inset shared-vs-local scale policy. |
| Render planning | Panels are planned from flat spaces | No recursive viewport allocation or child panel planning. |
| Mark sink | Per-mark interaction metadata is not nestable | Inset groups need hierarchical identity and scoped metadata. |
| Backends | Primitive ops use final coordinates | Inset rendering needs either a transform stack or absolute child coordinates. |
| Budgets | Budgets apply per raw geometry layer | Inset mark count multiplies by parent rows and child rows. |

## Current Recipes

These sketches use current Algraf surfaces and remain the preferred workaround
until v0.44.0 lands.

### Map with proportional bubbles

```text
Chart(data: GeoJson("us_counties.geojson"), width: 800, height: 500,
      title: "Major US Cities by Population") {
    Theme(name: "void")
    Table cities = "us_cities.csv"

    Scale(size: population, range: [5, 25], label: "Population")
    Scale(fill: population, gradient: ["#feb24c", "#f03b20"])

    Space(geom, projection: "albers_usa") {
        Geo(fill: "#f7f7f7", stroke: "#e0e0e0", strokeWidth: 0.25)
    }

    Space(long * lat, projection: "albers_usa", data: cities) {
        Point(size: population, fill: population, alpha: 0.85)
        Text(label: city, dy: -14, size: 7, anchor: "middle")
    }
}
```

This charts magnitude but cannot show composition at each city.

### Separate pie chart

```text
Chart(data: "pie_sales.csv", width: 360, height: 360,
      title: "Revenue share") {
    Space(sales, coords: "polar", theta: "y") {
        Bar(fill: product, layout: "fill")
    }
}
```

This charts composition, but it is a standalone chart rather than a glyph
anchored in another chart.

### Pre-rendered external glyphs

A user can preprocess a map outside Algraf by generating SVG pie glyphs and
placing them with another tool. That loses Algraf's scale training, metadata,
determinism, diagnostics, and backend parity. v0.44.0 should make this workflow
native.

## Feature Target Sketches

The sketches in this section are now implemented by the normative spec, runtime,
and compact examples in `examples/inset_city_pies.ag`,
`examples/inset_sparklines.ag`, and `examples/nested_insets.ag`.

### City map with demographic pies

```text
Chart(data: GeoJson("us_counties.geojson"), width: 900, height: 500,
      title: "US county population and city composition") {
    Theme(name: "void")
    Table cities = "us_cities.csv"
    Table city_mix = "city_population_mix.csv"

    Space(geom, projection: "albers_usa") {
        Geo(fill: population, stroke: "#ffffff", strokeWidth: 0.25)
    }

    Space(long * lat, projection: "albers_usa", data: cities) {
        Inset(data: city_mix,
              match: [city => city],
              size: population,
              minSize: 16,
              maxSize: 46,
              scales: "shared",
              guides: false,
              clip: "circle") {
            Space(count, coords: "polar", theta: "y") {
                Bar(fill: age_group, layout: "fill")
            }
        }

        Text(label: city, dy: -18, size: 7.5, anchor: "middle")
    }
}
```

`match: [city => city]` is a target shorthand whose left side names a column in
the inset table and whose right side resolves in the current parent row context.
The exact source syntax should be settled before implementation; the important
semantic is an explicit equi-match rather than an implicit cross product.

### Scatterplot with sparklines as point glyphs

```text
Chart(data: "countries.csv", width: 760, height: 520,
      title: "Life expectancy and income with local GDP trend") {
    Table yearly = "country_yearly.csv"

    Space(income * life_expectancy) {
        Inset(data: yearly,
              match: [country => country],
              width: 46,
              height: 18,
              scales: "shared",
              guides: false,
              clip: "rect") {
            Space(year * gdp_per_capita) {
                Line(stroke: "#1f77b4", strokeWidth: 1)
            }
        }
    }
}
```

Here `shared` y domains make sparkline magnitudes comparable across countries.
If a chart needs local shape detail instead, it can request `scales: "local"`.

### Nested city pie slices with tiny trends

```text
Chart(data: "cities.csv", width: 900, height: 500,
      title: "City composition and slice trends") {
    Table city_mix = "city_mix.csv"
    Table mix_trends = "city_mix_trends.csv"

    Space(long * lat, projection: "albers_usa") {
        Inset(data: city_mix,
              match: [city => city],
              size: population,
              minSize: 24,
              maxSize: 56,
              clip: "circle",
              guides: false) {
            Space(count, coords: "polar", theta: "y") {
                Bar(fill: category, layout: "fill")

                Inset(data: mix_trends,
                      match: [city => parent.city, category => category],
                      width: 18,
                      height: 8,
                      placement: "mark-center",
                      scales: "shared",
                      guides: false) {
                    Space(year * share) {
                        Line(stroke: "#222222", strokeWidth: 0.7)
                    }
                }
            }
        }
    }
}
```

This example shows why nested insets should be recursive from the start. The
outer row is a city; the first inset rows are composition categories; the second
inset matches both the ancestor city and current category. The `parent.city`
syntax is a target sketch for qualified parent-row references and must be
specified before implementation.

### State map with inset bars

```text
Chart(data: GeoJson("us_states.geojson"), width: 900, height: 520,
      title: "Energy mix by state") {
    Table energy = "state_energy_mix.csv"

    Space(geom, projection: "albers_usa") {
        Geo(fill: "#f8fafc", stroke: "#cbd5e1")

        Inset(data: energy,
              match: [state => state],
              anchor: "centroid",
              width: 34,
              height: 24,
              scales: "shared",
              guides: false,
              clip: "rect") {
            Space(source * share) {
                Bar(fill: source, layout: "stack")
            }
        }
    }
}
```

Spatial geometry anchors are useful but more complex than point anchors because
centroids can fall outside polygons. v0.44.0 should decide whether spatial
centroid anchors are Must scope or Should scope after the point-anchored path is
specified.

## Proposed Language Model

### Inset block

`Inset` should be parsed as a block item inside `Space`, not as a geometry call:

```text
SpaceItem ::= GeometryCall
            | InsetBlock
            | LetDecl
            | ScaleDecl
            | GuideDecl
            | ThemeDecl
            | ErrorItem

InsetBlock ::= "Inset" "(" ArgList? ")" BlockStart InsetBody BlockEnd
InsetBody  ::= SpaceBlock
             | LetDecl
             | ScaleDecl
             | GuideDecl
             | ThemeDecl
             | ErrorItem
```

Nested insets then arise naturally because an `Inset` body contains a child
`Space`, and that child `Space` body can contain another `Inset`.

This is narrower than arbitrary nested `Space` blocks: a `Space` nested directly
inside another `Space` remains out of scope unless it is inside an `Inset` body.

### Required and optional arguments

Target `Inset` arguments:

- `data`: child table or derived table to render inside each inset. Required in
  the first implementation unless the spec deliberately defines current-table
  inheritance.
- `match`: explicit equi-match list from child columns to current or parent row
  expressions.
- `size`: square viewport diameter in pixels or a numeric mapping.
- `width` / `height`: rectangular viewport dimensions; mutually exclusive with
  `size`.
- `minSize` / `maxSize`: output range for a mapped `size`.
- `scales`: `"shared"` or `"local"`.
- `guides`: boolean, default `false` for position guides inside insets.
- `clip`: `"rect"`, `"circle"`, or `false`.
- `padding`: inner viewport padding in pixels.
- `placement`: `"center"` by default; additional values such as
  `"mark-center"` or `"outside"` are optional and must be deterministic.
- `dx` / `dy`: pixel offsets after anchor resolution.
- `anchor`: optional anchor selector. For ordinary 2D spaces the default is the
  current row's resolved position. Spatial centroid anchors are a separate
  decision.

The exact property names are part of the release design work. Once chosen, they
must be added to the spec, semantic registry, LSP metadata, formatter, TextMate
grammar, and diagnostics before implementation is considered complete.

### Row context and matching

Inset rendering needs a row-context stack:

```text
root chart row
  parent inset row
    current child-space row
      nested inset row
```

Name resolution should distinguish:

- unqualified columns from the current active table;
- `parent.<name>` references to the immediate parent row context;
- repeated or otherwise explicit ancestor references for deeper nesting, if
  needed;
- child-table columns named on the left side of `match`.

The implementation must reject ambiguous references. A nested inset that needs
both city and category should spell both match keys explicitly rather than rely
on inherited filters.

### Scale policy

`scales: "shared"` should train each child `Space` once across all matched rows
for one inset declaration. This makes insets comparable and keeps legends
stable.

`scales: "local"` should train each child `Space` independently for one inset
instance. This preserves local shape detail but must be visibly documented
because magnitudes are not comparable across instances.

Additional policies such as `"parent-shared"` may be useful later, but v0.44.0
should keep the initial set small unless examples prove a real need.

### Guide and legend policy

Position axes and grid lines inside insets default off. The first release should
avoid unreadable repeated axes unless a chart explicitly opts in.

Aesthetic legends should be shared at the outer chart level when they come from
shared child scales. Per-inset legends are deferred unless the layout engine can
place them predictably without exploding the scene size.

### Viewport and clipping policy

Each inset instance owns a child viewport in parent pixel coordinates. Child
spaces are laid out within that viewport using the ordinary plot-layout
machinery, with chart titles, subtitles, captions, and outer margins suppressed
unless explicitly allowed later.

Clipping should be deterministic:

- `"rect"` clips to the inset viewport;
- `"circle"` clips to a circle inside the square viewport;
- `false` allows overflow but still counts marks for budget purposes.

Inset clipping must be represented in SVG and draw-list/raster output. A
feature that only clips in SVG is incomplete.

## Renderer Architecture Target

The render model should become a recursive scene tree, conceptually:

```rust
enum SceneNode {
    Space {
        viewport: Rect,
        panels: Vec<Panel>,
        layers: Vec<SceneNode>,
    },
    Geometry {
        layer: GeometryLayer,
    },
    Inset {
        declaration_id: InsetId,
        instance_path: Vec<RowKey>,
        anchor: Point,
        viewport: Rect,
        clip: Clip,
        children: Vec<SceneNode>,
    },
}
```

The exact Rust types may differ, but v0.44.0 should create an explicit planning
boundary between:

1. resolving parent anchors;
2. matching child data;
3. training child scales according to the inset scale policy;
4. allocating child viewports;
5. recursively planning child spaces;
6. emitting all backends from the same planned scene.

Backends may either maintain a transform/clip stack or receive absolute
coordinates from planning. The chosen approach must preserve SVG, draw-list,
raster, and metadata parity.

## v0.44.0 Must

### 1. Specify inset syntax and diagnostics

Status: Implemented.

- Add normative spec sections for `Inset` as a space item and for the allowed
  `Inset(...)` arguments.
- Reserve diagnostic codes before implementation. A likely range is
  `E2101`-`E2110` for invalid inset arguments, unknown child data, match
  failures, parent-reference errors, unsupported anchors, invalid viewport
  sizing, recursion-depth failures, and inset budget failures.
- Update the parser, formatter, LSP completions, hover docs, semantic tokens,
  TextMate grammar, and syntax fixtures.
- Keep nested `Space` blocks legal only inside `Inset` unless the release also
  deliberately promotes general nested spaces.
- Add parser recovery tests for malformed nested inset bodies, missing braces,
  and deeply nested but bounded inset structures.

### 2. Add inset IR and row-context semantics

Status: Implemented.

- Replace or supplement flat `SpaceIr.geometries` with a layer/item model that
  can hold both geometries and inset blocks in source order.
- Add an `InsetIr` carrying child data reference, match rules, viewport
  settings, scale policy, guide policy, clipping policy, child spaces, and
  source spans.
- Define a row-context stack for semantic validation and renderer planning.
- Validate child table names, match columns, parent references, and type
  compatibility before render.
- Reject ambiguous or unbounded data matching with targeted diagnostics.
- Preserve source order: geometries and insets in the same parent `Space` render
  in source order unless a future z-order control is explicitly specified.

### 3. Implement recursive render planning

Status: Implemented.

- Refactor render planning from a flat panel list into a scene representation
  that can contain child inset scenes.
- Resolve anchors from the active parent `ScaledSpace` for 2D Cartesian,
  projected longitude/latitude, polar, and 1D baseline spaces where applicable.
- Allocate child viewports from `size` or `width`/`height`, including mapped
  size ranges and deterministic min/max behavior.
- Plan child spaces inside each viewport using ordinary scale training,
  geometry planning, and theme resolution.
- Support nested insets with a deterministic default maximum depth. The default
  may be conservative, but the architecture must not be one-level-only.
- Add W2002/no-mark behavior for insets whose child match produces no rows,
  unless a more specific inset diagnostic is reserved.

### 4. Define and implement scale sharing

Status: Implemented.

- Implement `scales: "shared"` and `scales: "local"` for child spaces.
- For shared scales, train domains across all matched child rows for one inset
  declaration before rendering any instance.
- For local scales, train domains from the matched child rows for that specific
  inset instance.
- Ensure shared categorical domains remain source-order deterministic.
- Ensure legends and guide labels come from the same trained scale domains used
  to render child marks.
- Add tests proving that shared and local policies produce intentionally
  different domains where expected.

### 5. Add recursive mark budgets and diagnostics

Status: Implemented.

- Extend v0.43.0 mark-budget estimation to recursive inset scenes.
- Count parent rows, matched child rows, derived child rows, and emitted child
  primitives separately in diagnostics or debug reports where practical.
- Fail or skip predictably before generating pathological nested SVG or
  draw-list output.
- Add at least one intentional nested budget failure fixture.
- Ensure budget behavior is deterministic and independent of table iteration
  hash order or host locale.

### 6. Support all render backends and metadata

Status: Implemented.

- Emit nested inset scenes through SVG, draw-list, and render-model raster using
  the same planned scene.
- Represent clip scopes and child viewport transforms in draw-list output or
  resolve them to absolute primitive coordinates before serialization.
- Extend interaction metadata with hierarchical mark IDs and plot/inset paths,
  for example `p0:i3[Chicago]:g1:r2`.
- Preserve tooltip and highlight behavior for ordinary marks inside insets.
- Ensure an inset chart with no interaction metadata still renders identically
  in static SVG except for the actual inset marks.
- Add parity tests for SVG, draw-list JSON, raster pixels, and sidecar metadata.

### 7. Add inset examples and README tutorial coverage

Status: Implemented.

- Add a checked-in compact example that replaces city points with inset pies on
  a projected map.
- Add a compact scatterplot-with-sparklines example.
- Add one nested-inset example if it remains readable at tutorial size.
- Update the top-level README with every new example in the appropriate
  progression section.
- Regenerate example SVG/PNG outputs and visually inspect the changed PNGs.
- Keep large or dense inset demos under the large-demo policy from v0.43.0
  rather than committing heavyweight fixtures.

### 8. Document design limits

Status: Implemented.

- Document when to use insets and when to prefer faceting, aggregation, labels,
  or separate charts.
- Explain `shared` versus `local` scale policy with examples.
- Explain why axes default off inside insets.
- Document performance costs and the recursive mark-budget model.
- Document which nested-space forms remain unsupported outside `Inset`.

## v0.44.0 Should

### Spatial geometry anchors

Status: Implemented.

- Support `anchor: "centroid"` for spatial `Space(geom)` rows if a deterministic
  projected centroid is straightforward.
- Consider a later `anchor: "pointOnSurface"` only if geometry support can keep
  labels inside polygons without unstable algorithms.
- If spatial anchors are deferred, document the supported workaround: derive or
  provide explicit longitude/latitude anchor rows and use a projected 2D space.

### Composite match keys

Status: Implemented.

- Support composite matches such as
  `match: [city => parent.city, category => category]`.
- Add tests for missing keys, null key values, duplicate child rows, and stable
  child row ordering.
- Decide whether null keys match null keys or always produce no match, then
  document the rule in the spec.

### Inset placement controls

Status: Implemented.

- Add `dx`/`dy` offsets and deterministic placement values where examples need
  them.
- Consider simple edge-aware clipping or offset adjustment only if it can be
  deterministic and tested.
- Defer force-directed or iterative collision avoidance.

### Accessibility for nested charts

Status: Implemented.

- Add concise generated labels for inset groups in SVG and sidecar metadata.
- Ensure screen-reader metadata does not explode for charts with many repeated
  child plots.
- Consider a summary-only mode for large numbers of similar insets.

## Explicitly Deferred Past v0.44.0

- General nested `Space` blocks outside `Inset`.
- Arbitrary chart dashboards or page-grid layout containers.
- Automatic force-directed inset collision avoidance.
- Per-inset legends by default.
- User-authored SVG, HTML, CSS, JavaScript, iframe, or external image content
  inside insets.
- Interactive drilldown, zoom, or expansion of an inset into a full chart.
- Animation or transitions between parent and inset states.
- Custom geometry/plugin APIs for user-defined inset renderers.
- GPU/WebGL-specific inset rendering.
- Unbounded recursion or source-level recursion/macros.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

Inset implementation changes should also run focused SVG, draw-list, raster, and
interaction-metadata parity tests for at least:

- one-level pie insets on a projected map;
- one-level sparkline insets on a Cartesian scatterplot;
- nested insets with a composite parent/child match;
- recursive mark-budget failure;
- malformed source recovery and LSP completion/hover smoke tests.

## Promotion Workflow

1. Specify `Inset` grammar, row-context naming, match semantics, scale policy,
   viewport arguments, clipping, and diagnostics in the spec before coding.
2. Update parser/CST/AST, formatter, LSP syntax features, TextMate grammar, and
   semantic registry metadata for `Inset`.
3. Add `InsetIr` and recursive layer items without changing render output for
   charts that do not use insets.
4. Implement recursive render planning and child scale training behind tests
   before adding broad examples.
5. Extend mark budgets and interaction metadata for hierarchical inset paths.
6. Bring SVG, draw-list, raster, and sidecar output to parity.
7. Add compact examples, regenerate outputs, visually inspect PNGs, and update
   the README tutorial.
8. Do not close v0.44.0 until nested insets are supported by architecture and
   tests, even if the public examples emphasize one-level insets.
