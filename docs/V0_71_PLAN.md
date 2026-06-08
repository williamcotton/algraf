# Algraf v0.71.0 Plan

Status: Implemented
Target version: 0.71.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_70_PLAN.md`](V0_70_PLAN.md)
Roadmap theme: Replace the `Inset` block with a chart-valued mark ("glyph").
Cross-repo coordination: none. Algraf-only language change.

> This plan landed in release 0.71.0. The `Inset` block has been removed and
> replaced with the `Glyph` declaration plus the chart-valued glyph mark; the
> `train:` Scale property, the `outer.` row-context qualifier, and the
> `E2201`–`E2210` diagnostic range have all shipped. Treat this document as a
> historical record of the design.

## Purpose

The `Inset(...)` block (spec §7.3 inset rules, diagnostics `E2101`–`E2110`) is
the largest single construct in the language: ~12 arguments, a bespoke
`match: [a => b]` join mini-DSL, a private viewport-sizing scheme
(`size`/`minSize`/`maxSize`) unrelated to the normal `Scale(size:)` machinery,
and a `parent.`-qualified row-context system used nowhere else. It solves a real
problem — subordinate charts anchored at arbitrary parent rows (map glyphs,
sparklines on points, mini-pies) — but at a cost that scales poorly with the
rest of the language.

v0.71 replaces `Inset` with a **glyph**: a chart-valued mark. A glyph is a
reusable sub-chart that is invoked exactly where an ordinary geometry is, draws
at the host row's anchor like `Point`, and participates in the existing mark,
scale, and legend systems instead of reinventing them.

Faceting is **not** changed. The pressure-test in design discussion showed that
grid faceting and anchored insets are *not* the same operation — grid faceting
consumes and subdivides the plot rectangle (a layout pass), while an anchored
subordinate floats over an already-allocated plot (an overlay). Forcing them
into one construct leaks. This plan keeps faceting algebraic (`/` + `Layout`)
and reframes only the inset half as a mark. This is the "Model C" of the design
exploration.

## Release Thesis

A chart used as a mark is more powerful and smaller than a block that embeds a
chart. By making the subordinate chart a first-class mark we get, for free:

- **size** is the ordinary size aesthetic (`Scale(size:, range: […])`),
- **legends** merge through the existing per-mark legend machinery,
- **scale training scope** becomes a per-`Scale` property (`train:`) that also
  subsumes the facet `free-x`/`free-y` vocabulary,
- **reuse**: a glyph is defined once and placed in any chart,
- **nesting**: glyphs inside glyphs need no `parent.` ceremony because a
  one-sided `key:` resolves up the lexical row-context chain.

The net is one new declaration (`Glyph`) and one new mark-call form, in exchange
for removing an entire block grammar plus ten diagnostic codes.

## Proposed Spec Changes

### §7.x Glyph Declaration (new)

A `Glyph` declaration is a chart-scoped, reusable, chart-valued mark template.

```
GlyphDecl   ::= "Glyph" Ident "(" GlyphArgs ")" BlockStart SpaceBlock+ BlockEnd
GlyphArgs   ::= GlyphArg ("," GlyphArg)*
GlyphArg    ::= "data" ":" TableRef
              | "key" ":" KeySpec
              | "scales" ":" ("\"shared\"" | "\"local\"")
KeySpec     ::= Ident                                  // single correlation column
              | "[" KeyEntry ("," KeyEntry)* "]"
KeyEntry    ::= Ident                                  // child col == host col (same name)
              | Ident ":" RowRef                        // child col == named host expr
RowRef      ::= Ident | Ancestor "." Ident
Ancestor    ::= "outer"                                 // force a specific ancestor row
```

Rules:

- A `Glyph` MUST be declared in chart scope (alongside `Table`, `Derive`,
  `Scale`, `Theme`, `Guide`).
- `data` is REQUIRED and MUST name a chart-scoped `Table` or `Derive`.
- `key` is REQUIRED and lists the correlation columns. Each child column is
  equi-matched against the host row context (see §14.x key resolution).
- `scales` sets the default training scope for the glyph's internal scales and
  defaults to `"shared"`. Per-`Scale` `train:` overrides it.
- A glyph body MUST contain one or more `Space` blocks, identical in form to any
  other space (exactly one algebra expression each, §4.2). A glyph body MUST NOT
  contain user-authored JavaScript, CSS, HTML, external images, or raw SVG —
  the same prohibition the `Inset` block carried.
- A glyph name MUST NOT shadow a built-in geometry name (see §13.x precedence).

### §13.x Glyph / Geometry Name Precedence (new)

The analyzer resolves a call head `Name(...)` inside a `Space` body as:

1. a built-in geometry from the geometry registry (§13.8), else
2. a chart-scoped `Glyph` declaration, else
3. `E1102` unknown geometry/glyph.

A `Glyph` whose name collides with a registry geometry is `E2201` at the
declaration site (geometry names are reserved). This keeps geometry calls
unambiguous and prevents a glyph from silently redefining `Point`.

### §14.x Glyph Mark (new; supersedes §7.3 Inset)

A glyph is invoked like a geometry, inside any `Space` body:

```
pie(size: footfall, clip: "circle", padding: 1, at: "position")
```

- The glyph renders once per host row of the enclosing space (after that space's
  own per-row resolution), anchored at the row.
- **Aesthetics reused from the mark system**: `size` (viewport footprint, via
  `Scale(size:)`), `alpha`. A mapped `size` uses the size scale's `range:` as
  min/max — there is no `minSize`/`maxSize`.
- **Glyph-only viewport props**: `clip` (`"rect"` | `"circle"` | `false`,
  default `"rect"`), `padding`, `dx`, `dy`, `width`/`height` (fixed rectangular
  footprint; MUST NOT combine with `size`), and `at` (`"position"` |
  `"mark-center"` | `"centroid"`, the placement strategy, replacing the old
  `placement`/`anchor` pair). Ordinary geometries ignore these props.
- **Key resolution**: for each declared `key` column, the analyzer resolves the
  host value by searching the row-context chain outward — the immediate host row
  first, then each enclosing glyph's host row — until a column of that name is
  found. `outer.col` forces the nearest enclosing glyph host when a name is
  shadowed. `null` never matches (including null-to-null). Matched child rows
  preserve child-table order; duplicates render deterministically.
- A glyph mark in a space whose row context cannot supply a `key` column is
  `E2204`. Incompatible match column types are `E2205`.

### §16.x Scale Training Scope (`train:`) (new; generalizes facet + inset policy)

`Scale(...)` gains an optional `train:` property:

```
Scale(y: gdp, train: "local")     // each instance auto-scales y
Scale(x: year, train: "shared")   // all instances share the x domain
```

- `train: "shared"` (default) trains the scale across the union of all instances
  of the enclosing glyph within its host space.
- `train: "local"` trains the scale per glyph instance.
- This subsumes two older vocabularies:
  - facet `Layout(facetScales: "free-x")` is equivalent to `train: "local"` on
    the x position scale (and stays available as facet sugar),
  - inset `scales: "shared" | "local"` becomes the glyph-level default that
    `train:` overrides per scale.
- A position or data-trained scale under `train: "local"` produces no
  chart-level legend (no shared domain). Aesthetic scales with a fixed domain
  (e.g. categorical color `range:` maps) always merge regardless of `train:`.

### §17.x Legend Merging for Glyphs (new)

Glyph internal scales flow into the chart legend collection exactly like any
mark's scales, deduplicated by `(aesthetic, domain)`. N glyph instances sharing
one `fill: category` scale yield one legend. Per-call suppression reuses the
ordinary mark legend control (`legend: false`).

### Diagnostics

The `Inset` codes `E2101`–`E2110` are **removed** (the block no longer exists).
Reserve a parallel glyph range `E2201`–`E2210`:

| Old (Inset) | New (Glyph) | Meaning |
| ----------- | ----------- | ------- |
| `E2101` | `E2201` | invalid/unsupported glyph argument, or glyph name shadows a geometry |
| `E2102` | `E2202` | unknown or invalid glyph `data` table |
| `E2103` | `E2203` | invalid or missing glyph `key` |
| `E2104` | `E2204` | unresolved glyph key in the host row-context chain |
| `E2105` | `E2205` | unsupported `at:` placement or incompatible key column types |
| `E2106` | `E2206` | invalid glyph viewport sizing (`size`+`width`/`height` conflict) |
| `E2107` | `E2207` | reserved for glyph guide/legend policy errors |
| `E2108` | `E2208` | reserved for glyph placement policy errors |
| `E2109` | `E2209` | nested glyph depth exceeded |
| `E2110` | `E2210` | recursive glyph mark budget exceeded |

`ALGRAF_SPEC.md` MUST mark `E2101`–`E2110` as removed and add `E2201`–`E2210`
before implementation lands.

## Worked Examples

### Faceted scatter with per-store category-mix glyph

```ag
Chart(data: "stores.csv", width: 960, height: 620,
      title: "Store performance by region") {
    Theme(name: "minimal")
    Table mix = "store_category_mix.csv"        # store, category, share

    Glyph pie(data: mix, key: store, scales: "shared") {
        Space(share, coords: polar, theta: y) {
            Bar(fill: category, layout: "fill")
        }
    }

    Scale(size: footfall, range: [16, 44])
    Scale(fill: category,
          range: ["grocery" => "#4E79A7", "apparel" => "#F28E2B", "home" => "#59A14F"],
          label: "Category")
    Layout(facetColumns: 2, facetScales: "fixed", facetLabel: "value",
           panelSpacing: [16, 12])

    Space((revenue * satisfaction) / region) {
        Guide(axis: x, label: "Revenue ($M)")
        Guide(axis: y, label: "Satisfaction")
        Point(alpha: 0.15, size: 2)
        pie(size: footfall, clip: "circle")
        Text(label: store, dy: -20, size: 7, anchor: "middle", fill: "#374151")
    }
}
```

### Nested glyphs (port of `examples/nested_insets.ag`)

```ag
Chart(data: "inset_nodes.csv", width: 560, height: 380, title: "Nested glyphs") {
    Table mix = "inset_node_mix.csv"        # id, category, value
    Table trends = "inset_node_trends.csv"  # id, category, t, value

    Scale(size: size, range: [38, 62])
    Scale(fill: category,
          range: ["hardware" => "#4E79A7", "software" => "#F28E2B", "services" => "#59A14F"])

    Glyph trend(data: trends, key: [id, category], scales: "local") {
        Space(t * value) { Line(stroke: "#111827", strokeWidth: 0.7) }
    }

    Glyph nodepie(data: mix, key: id, scales: "shared") {
        Space(value, coords: polar, theta: y) {
            Bar(fill: category, layout: "fill")
            trend(width: 18, height: 8, at: "mark-center")
        }
    }

    Space(x * y) {
        Point(alpha: 0.1, size: 2)
        nodepie(size: size, clip: "circle", padding: 1)
        Text(label: id, dy: -38, size: 10, anchor: "middle")
    }
}
```

`trend`'s `key: [id, category]` resolves both columns from the `mix` row
context directly — no `parent.` qualifier, because the one-sided key searches
the row-context chain by name.

## Scope

### Glyph Declaration And Grammar

Status: Implemented.

Add `Glyph` declaration parsing (§7.x), AST/CST nodes, resilient recovery, and
the geometry-name precedence rule (§13.x). Reuse the existing `Space` block
parser for glyph bodies.

### Glyph Mark Resolution And Key Chain

Status: Implemented.

Resolve glyph call heads after the geometry registry, implement the outward
row-context key search, `outer.` ancestor qualifier, and the `at:` placement
strategies (`position`, `mark-center`, `centroid`). Emit `E2201`–`E2206`.

### Scale Training Scope

Status: Implemented.

Add `train:` to `Scale`, wire shared/local training through render scale
training, and map facet `facetScales` + glyph `scales:` defaults onto it.

### Legend Merging

Status: Implemented.

Route glyph internal scales through the chart legend collection with
`(aesthetic, domain)` dedup; suppress chart-level legends for `train: "local"`
position scales.

### Inset Removal And Migration

Status: Implemented.

Remove the `Inset` block grammar, semantics, IR, and `E2101`–`E2110`. Provide a
mechanical codemod (see Promotion Workflow) and port every `examples/inset_*`
and `examples/nested_insets.ag` to glyphs, re-rendering and visually verifying
output parity.

### Spec, Version, And Release Metadata

Status: Implemented.

Update `ALGRAF_SPEC.md` (new §7.x/§13.x/§14.x/§16.x/§17.x, removed §7.3 inset
rules, diagnostics table), bump workspace + package version stamps to 0.71.0,
and update the VS Code TextMate grammar with the `Glyph` keyword.

## Migration (Breaking)

`Inset` is removed. The mapping is mechanical and codemod-able:

| Old `Inset` | New glyph |
| ----------- | --------- |
| `Inset(data: T, match: [c => h]) { Space(...) {...} }` | `Glyph g(data: T, key: [c => h]) { Space(...) {...} }` + `g(...)` at the call site |
| `match: [c => c]` | `key: c` |
| `match: [c => parent.c]` | `key: [c]` (resolves up the chain) or `key: [c => outer.c]` if shadowed |
| `size:` + `minSize`/`maxSize` | `g(size:)` + `Scale(size:, range: [min, max])` |
| `placement`/`anchor` | `at:` |
| `scales: "shared"|"local"` | glyph `scales:` default + per-`Scale` `train:` |
| `clip`/`padding`/`dx`/`dy`/`width`/`height` | unchanged call-site props |

## Non-Goals

- Changing faceting. `/` nest + `Layout(facet*)` stay exactly as specified in
  §17.4. `facetScales` is retained as facet sugar over `train:`.
- Unifying facet and inset into one construct. The layout-participation fork
  (grid subdivides vs. glyph overlays) makes that a leak; this plan keeps them
  distinct on purpose.
- A panel-as-aesthetic model ("Model B"). Recorded in design discussion as the
  intellectually pure alternative, deferred in favor of the mark model's reuse
  of existing machinery.
- Glyphs as top-level charts, glyph parameters/arguments beyond `data`/`key`,
  or recursion. A glyph MUST NOT (transitively) invoke itself.

## Open Questions

1. **`at: "mark-center"` / `"centroid"` remain data-dependent placement** — the
   one place layout depends on rendered mark geometry. Isolated to a single
   call-site arg, but not dissolved. Acceptable, but confirm determinism rules
   match the current inset centroid spec (§7.3) verbatim.
2. **Glyph viewport vs. mark clip interaction** — confirm `clip` semantics when
   a glyph is nested inside another glyph's clipped viewport.
3. **Whether `width`/`height` should also become scales** (a "viewport size"
   aesthetic pair) for consistency with `size`, or stay literal-only.

## Promotion Workflow

1. Reserve `E2201`–`E2210` and mark `E2101`–`E2110` removed in `ALGRAF_SPEC.md`.
2. Add normative §7.x/§13.x/§14.x/§16.x/§17.x text (`MUST`/`SHOULD`) before code.
3. Add guard tests: glyph parse/recovery, key-chain resolution, `train:`
   training scope, legend dedup, geometry-name precedence, removal diagnostics.
4. Implement glyph grammar + analyzer resolution + render path.
5. Add `train:` to scale training; map facet/inset defaults onto it.
6. Remove `Inset`; run the codemod over `examples/`.
7. Port and re-render `examples/inset_*` + `nested_insets.ag`; **visually verify**
   the rendered PNGs match intent (positions, clips, legends), not just SVG byte
   diffs.
8. Add new glyph examples and corresponding `README.md` tutorial sections.
9. Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace`, `./examples/generate.sh`, and require an empty
   `git diff -- examples` except intended ports.
10. Bump version stamps (`Cargo.toml`, `Cargo.lock`, `ALGRAF_SPEC.md`,
    `editors/vscode/package.json` + lockfile, demo/package manifests) to 0.71.0.
