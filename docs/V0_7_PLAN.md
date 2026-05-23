# Algraf v0.7.0 Plan

Status: Planned (not started)
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_6_PLAN.md`](V0_6_PLAN.md)

## Purpose

This document defines the intended v0.7.0 release shape: overlaying an annotated
second dataset on a shared coordinate space and taking manual control of color,
size, and label scales — enough to reproduce a class of rich, hand-styled
infographics.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when syntax, diagnostics, tests, and
examples land together.

## Release Thesis

v0.7.0 is an **external data sources & manual scales** release: overlay a second
dataset on a shared coordinate space, draw ordered paths with data-driven width,
and take manual control of color, size, and label scales.

The release is scoped by working backward from one capstone chart — **Minard's
map of Napoleon's 1812 Russian campaign** (as recreated by ggsql). That single
infographic needs every primitive in this release: a troop-path layer drawn in
data order, line width encoding survivor counts, two hand-picked colors for
advance/retreat with renamed legend entries, an open-ended size scale, a *second*
CSV supplying city labels on the same long/lat space, and suppressed axis titles.
Each primitive is independently useful beyond Minard (paths, variable-width
lines, manual color scales, multi-source overlays).

This release builds on the data-source thinking of v0.6 (embedded SQL, extra
formats). Multiple-CSV-in-one-chart is the item *explicitly deferred past v0.6*
(see [`V0_6_PLAN.md`](V0_6_PLAN.md) "Explicitly Deferred"), so v0.7 is its home.
The `Table name = <source>` declaration (Must item 1) is deliberately designed to
also host v0.6's source constructors later (`Table sales = Sqlite("sales.db",
"SELECT ...")`), so the two releases share one source-declaration grammar rather
than inventing two.

## Scope Rules

- New syntax MUST be backwards compatible: existing `.ag` files keep working.
- New data sources sit behind the dataframe boundary (spec §10.5); parser/LSP/
  semantics/render gain no source-specific knowledge beyond naming a source.
- Output MUST stay deterministic regardless of source count or ordering (spec
  §18.12, §23.6).
- Position scales MUST be shared/unioned across compatible overlaid spaces even
  when those spaces are backed by different tables (spec §17.5).
- Reserve new diagnostic codes in the spec before implementing behavior that can
  fail.
- Prefer per-primitive examples plus the one capstone over a sprawl of variants.

## Capstone Example (acceptance target)

`examples/minard.ag` must parse, analyze, and render:

```ag
Chart(
    data: "minard_troops.csv",
    title: "Napoleon's Russian Campaign",
    subtitle: "Inspired by the graphic of C.J. Minard",
    marginRight: 40
) {
    Table cities = "minard_cities.csv"

    Scale(stroke: direction,
          range:  ["A" => "burlywood", "R" => "black"],
          labels: ["A" => "Advance",   "R" => "Retreat"],
          label: "Direction")
    Scale(strokeWidth: survivors, domain: [0, null], range: [0, 30], label: "Troops")

    Guide(axis: x, label: null)
    Guide(axis: y, label: null)

    Space(long * lat) {
        Path(stroke: direction, strokeWidth: survivors, group: group)
    }

    Space(long * lat, data: cities) {
        Text(label: city, size: 6)
    }
}
```

## Design Decisions (settled)

1. **Secondary data → chart-scoped `Table` declaration**, bound to a `Space` by
   bare identifier. Parallels `Derive`; keeps the existing "`data:` is an
   identifier" rule (spec §7.3); reuses derived-table resolution and LSP column
   completion.
2. **Manual color/rename → `=>` map literals.** A new value form
   `[ key => value, ... ]` with a `=>` (fat-arrow) token. Map keys define
   category order, so categorical color scales need no separate `domain`.
3. **Open-ended bounds → `null` inside `domain`/`range` arrays** ("infer this
   bound from the data").
4. **Variable line width → per-segment stroke width** (each polyline segment
   gets a width derived from its endpoints' scaled values). Tapered-polygon
   ribbons are deferred.
5. **Path drawing → a distinct `Path` geometry** that never sorts, rather than
   `Line(sort: false)`. `Line`'s automatic x-sort stays a deliberate feature.
6. **Hide axis title → `Guide(axis: x, label: null)`**, reusing the existing
   `null` = "suppress" convention (cf. `Guide(fill: null)`).

## v0.7.0 Must

### 1. Named `Table` declarations + multi-source overlay

Status: Not started. Multiple independent CSVs in one chart is listed as deferred
past v0.6.0.

Add a chart-scoped declaration that loads an independent CSV and binds to a
`Space` by identifier.

Minimum target:

```ag
Chart(data: "minard_troops.csv") {
    Table cities = "minard_cities.csv"
    Space(long * lat, data: cities) { Text(label: city, size: 6) }
}
```

Acceptance criteria:

- Grammar adds `TableDecl ::= "Table" Ident "=" String` at chart scope; `Table`
  becomes a reserved keyword (spec §6.5). Stable AST node, byte-accurate spans.
  The value position is specified as a *source expression* — currently just a
  string-literal CSV path — explicitly leaving room for v0.6 source constructors.
- Resolution: the `SpaceDataRef` enum gains a `Table(name)` variant alongside
  `Primary`/`Derived`; `space_data()` resolves a `Table` name the same way it
  resolves a derived table.
- Loading: the CLI loads every declared `Table` CSV (path resolution and
  `--base-dir` identical to `Chart(data:)`), keyed by name, and passes a map of
  named tables to `render()` alongside the primary. Source-security rules (spec
  §10.8) apply unchanged.
- **Shared position scales across sources.** When compatible spaces overlay (spec
  §17.5) but back onto different tables, position-scale domains MUST be unioned
  across all contributing tables so the secondary layer aligns with the primary.
  Tested with a secondary table whose x/y extent differs from the primary's.
- LSP: column completion and hover inside `Space(..., data: tableName)` resolve
  against that table's schema (reuse derived-table machinery).
- Diagnostics (reserve in spec first): `E1105` duplicate `Table` name; `E1106`
  table file not found; `E1107` table file unreadable; `E1108` `Table` name
  conflicts with the primary or a derived table. (`E1103` still fires for an
  unknown identifier passed to `data:`.)
- Semantic, render, and LSP tests, plus a focused example.

### 2. `Path` geometry

Status: Not started.

A registered geometry identical to `Line` minus the x-sort: connects rows in
source order, honoring `group:` (separate sub-paths), `stroke:` (categorical
mapping), and `strokeWidth:` (Must item 3).

Acceptance criteria:

- Registry entry mirroring `Line` (spec §13.8); reuse Line's group-splitting and
  stroke logic rather than duplicating it.
- Spec gains a normative `Path` geometry section (§14.x) noting its relationship
  to `Line` (`Path` preserves row order; `Line` sorts by x).
- Reuses `E1201`/`E1302`; no new codes.
- Snapshot test contrasting `Path` row order against `Line` x-sort.

### 3. Mappable, scaled `strokeWidth` (and `size`) with per-segment width

Status: Not started. `strokeWidth` currently accepts only a literal number.

Acceptance criteria:

- Registry: `strokeWidth` accepts a column mapping in addition to a number, for
  `Line`/`Path` (spec §13.8).
- Training: a continuous scale trains `strokeWidth`/`size` from the mapped
  column's domain into an output range (Must item 4). The documented size
  default range ([2,8]) is preserved unless overridden; `strokeWidth`'s default
  range (line-width px) is specified.
- Rendering: `Path`/`Line` emit **per-segment** stroke widths from the scaled
  values at each segment's endpoints. Tapered-polygon ribbons are deferred.
- Spec §16.8 (aesthetic scales) gains `strokeWidth`; size/width range units are
  clarified.
- Diagnostic: `E1607` strokeWidth/size scale requires a numeric column.

### 4. Output `range:` and open-ended `domain`/`range` on `Scale`

Status: Not started. `domain` currently requires two finite numbers; there is no
`range:` argument; scale targets are limited to `axis`/`fill`/`stroke`.

Minimum target:

```ag
Scale(strokeWidth: survivors, domain: [0, null], range: [0, 30])
```

Acceptance criteria:

- `Scale` accepts a `range:` argument and accepts `size`/`strokeWidth` as scale
  targets.
- Numeric `domain`/`range` arrays accept `null` per element, meaning "infer this
  bound from the data" (e.g. `domain: [0, null]`).
- Validation covers `range`/`domain` arity and element types per scale kind, and
  restricts `null` to bounds where data inference is meaningful.
- Spec §16.11 is rewritten to document `range`, `null` bounds, and the expanded
  target set; §16.8 is cross-referenced.
- Diagnostics: `E1603` invalid `range` declaration; `E1605` `null` bound not
  permitted here. (`E1204` still covers gross type errors.)

### 5. `=>` map literals → manual categorical colors and legend renaming

Status: Not started. Algraf has no map/dict literal today.

Minimum target:

```ag
Scale(stroke: direction,
      range:  ["A" => "burlywood", "R" => "black"],
      labels: ["A" => "Advance",   "R" => "Retreat"],
      label: "Direction")
```

Acceptance criteria:

- Lexer adds a `=>` (fat-arrow) token (spec §6.11).
- Grammar adds a map value form `Map ::= "[" Entry ("," Entry)* ","? "]"`,
  `Entry ::= Value "=>" Value`, distinguished from `Array` by the presence of
  `=>` (spec §7.8). The formatter round-trips it; semantic tokens and completion
  are aware of it.
- Semantics: on a categorical color `Scale`, `range:` MAY be a map
  (category → color) and `labels:` a map (category → display name). Map keys
  define category and legend-entry order; renamed labels flow into scale-driven
  legend labels (spec §16.13).
- Diagnostics: `E0021` malformed map literal entry (parse); `E1604` map keys do
  not match the column's categories, or `range`/`labels` key sets disagree;
  `E1606` map used where an array is required for that scale kind (or vice-versa).
- Parser/formatter round-trip tests, semantic tests, and a render test for
  renamed legend entries.

### 6. Suppress axis titles with `label: null`

Status: Not started. `Guide(axis:x, label:)` currently requires a string.

Acceptance criteria:

- `Guide(axis: x, label: null)` suppresses that axis's title; ticks and grid are
  unaffected. Reuses the existing `null` = "suppress" convention.
- Spec §19.x documents `label: null` for axes.
- No new diagnostic codes; accepting `null` is additive.

### 7. Spec, version, and example hygiene

Status: Not started; mirrors prior releases.

Acceptance criteria:

- `Cargo.toml` workspace version bumped to `0.7.0` (and `editors/vscode/package.json`)
  when the release branch is ready.
- Made normative: §6.5 (`Table` keyword), §6.11 (`=>`), §7.x (`TableDecl`, map
  literal), §10.x (named CSV sources / source expression), §13.8 (registry:
  `Path`, mappable `strokeWidth`), §14.x (`Path`), §16.8 / §16.11 (`range`, null
  bounds, size/strokeWidth targets), §16.13 (renamed legend labels), §19.x
  (`label: null`), §26 (all reserved diagnostic codes below).
- `editors/vscode/` TextMate grammar and `language-configuration.json` updated
  for the `Table` keyword and `=>`.
- README gains a Minard tutorial section (placed under the annotations /
  multi-source progression) plus smaller examples for each primitive in
  isolation; `minard_troops.csv` and `minard_cities.csv` fixtures checked into
  `examples/`.
- Examples regenerated via `./examples/generate.sh`.
- This document updated as each item completes, is rejected, or moves scope.

## v0.7.0 Should

### Tapered-Ribbon Line Width

Status: Not started.

A higher-fidelity alternative to per-segment stroke widths: render the variable
width as a filled polygon that tapers smoothly along the path. Only pursue if the
Must items land with capacity; per-segment width (Must item 3) is the baseline.

### `Table` Source-Expression Generalization

Status: Not started.

If v0.6 source constructors exist by the time this lands, allow
`Table x = Sqlite(...)` and other source forms in the `Table` value position.
Otherwise just reserve the source-expression shape in the grammar.

## Explicitly Deferred Past v0.7.0

Carried forward and unchanged unless a later planning decision moves them:

- Tapered/anti-aliased variable-width ribbons (unless promoted from Should).
- Joins or relational operations between `Table`s — sources stay independent
  overlays.
- True geographic projection or basemap tiles — Minard uses raw long/lat as plain
  Cartesian coordinates, which is sufficient.
- Everything still deferred from v0.5/v0.6 (nested spaces, user-defined functions
  and macros, SQL/formats not yet shipped, Polars, network/command sources) and
  the standing deferred list in [`V0_3_PLAN.md`](V0_3_PLAN.md).

## Diagnostic Codes to Reserve (before implementation)

- `E0021` malformed map literal entry
- `E1105` duplicate `Table` declaration
- `E1106` `Table` data file not found
- `E1107` `Table` data file could not be read
- `E1108` `Table` name conflicts with primary or derived table
- `E1603` invalid `range` declaration
- `E1604` map key / category mismatch (or disagreeing key sets)
- `E1605` `null` bound not permitted here
- `E1606` map/array form wrong for this scale kind
- `E1607` strokeWidth/size scale requires a numeric column

## Optional-Item Audit

### Promote In v0.7.0 (Must)

- Named `Table` declarations + multi-source overlay.
- `Path` geometry.
- Mappable/scaled `strokeWidth` with per-segment width.
- `range:` and open-ended `domain`/`range` on `Scale` (with `size`/`strokeWidth`
  targets).
- `=>` map literals for manual categorical colors and legend renaming.
- `Guide(axis, label: null)` axis-title suppression.

### Consider If Capacity Allows (Should)

- Tapered-ribbon line width.
- `Table` source-expression generalization toward v0.6 constructors.

### Keep Deferred

- Joins between tables, geographic projection, tapered ribbons (unless promoted).
- All standing-deferred items from v0.5/v0.6 not promoted here.

## Promotion Workflow

1. Move the chosen behavior into the relevant normative section of
   [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) (lexical §6, grammar §7, data §10,
   registry §13, geometry §14, scales §16, layout §17, guides §19, diagnostics
   §26).
2. Reserve or add diagnostic codes before implementation if behavior can fail.
3. Implement parser, semantic, render, CLI, and LSP changes as needed; keep new
   sources behind the dataframe boundary (spec §10.5).
4. Add focused tests in the crate closest to the behavior, plus a snapshot for
   the Minard capstone.
5. Add or update examples (and checked-in fixtures) when behavior affects
   user-facing charts.
6. Regenerate examples via `./examples/generate.sh` when rendered output changes.
7. Update this document when a candidate is completed, rejected, or moved scope.
