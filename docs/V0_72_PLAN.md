# Algraf v0.72.0 Plan

Status: Implemented
Target version: 0.72.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_71_PLAN.md`](V0_71_PLAN.md)
Roadmap theme: Make the `Glyph` body the canonical home for a glyph mark's
own scales — the size scale drives glyph viewport sizing and the legend
pipeline picks it up.
Cross-repo coordination: none required to ship 0.72.0. Downstream Studio
([`../studio/docs/V0_19_PLAN.md`](../../studio/docs/V0_19_PLAN.md)) is
waiting on a published `algraf-wasm@0.72` / `algraf-editor@0.72` before it
can mark its glyph migration shipped; the wait ends when this plan does.

## Purpose

Two related v0.71 gaps surfaced during the Studio Solar Step 04 migration
to the `Glyph` mark (`studio/src/datafarm-solar/04/seasonal-pie-map.ag`):

1. **There is no good home for a glyph mark's size scale.** A glyph call
   such as `pie(size: generation_gwh, ...)` needs an associated
   `Scale(size: column, range: [a, b], label: "...")` so the viewport
   pixel range and legend label are defined. Today, only a *chart-scope*
   `Scale(size:, range:, label:)` actually drives the renderer's pixel
   range (`crates/algraf-render/src/render/glyph_plan.rs::
   mapped_size_pixel_range`, line 529, called at line 228 with only
   `&ir.scales`). But chart-scope Scales resolve column names against the
   chart's primary data alone (`crates/algraf-semantics/src/analyzer/
   chart.rs:119` → `analyzer/context.rs:167`). When the chart primary is
   a `GeoJson` basemap and the size column lives in a side `Table` (or a
   host `Space`'s `data:` override), the analyzer correctly emits
   `E1101 unknown column`. That is the right diagnostic; reaching into
   arbitrary tables from chart scope would muddy what "chart scope"
   means. The result is that the user has *no* scope where the size
   scale is both analyzer-clean and renderer-effective.

   Authors can express the scale inside the *glyph declaration body*
   (between `Glyph foo(...) {` and the inner `Space(...)`), which is the
   only scope where column resolution behaves intuitively — the glyph's
   `data:` table legitimately owns the column. The analyzer accepts it
   today; the renderer ignores it.

2. **The size legend never renders.** Even when the chart-scope variant
   happens to resolve and apply, no size swatch reaches the legend.
   `collect_glyph_legend_candidates` in
   `crates/algraf-render/src/render/legend.rs` (line 70–87) only recurses
   into glyph child panels for fill/stroke; it never walks any scale
   list to register a size legend whose only downstream consumer is a
   `GlyphCallIr.size = Mapped { column, ... }`. The non-glyph
   `collect_geometry_legend_candidates` walks scales explicitly (line
   127–157), but glyph calls have no `geo.mappings` to drive the same
   path.

v0.72 makes the glyph declaration body the canonical, working home for
glyph-call aesthetic scales (`size:`, `strokeWidth:`), and routes those
scales through the existing legend pipeline. Chart-scope `Scale(size:)`
keeps working unchanged for the case where the column genuinely lives in
the chart primary, and now also produces a legend.

## Release Thesis

The glyph body owns the glyph's own scales. The renderer reads them with
higher precedence than chart scales; the legend collector treats a
glyph-call aesthetic the same way it treats a geometry mapping. No new
diagnostic codes; the strict `E1101` at chart scope is preserved on
purpose.

## Proposed Spec Changes

### §14.27 Glyph Mark (extend)

Append a paragraph stating that a `Glyph` body MAY contain
`Scale(size: col, range: [...], label: "...")` and `Scale(strokeWidth:
col, range: [...], label: "...")` declarations. Column resolution for
such scales uses the glyph's `data:` table (the same row context the
glyph body's inner `Space` sees). Glyph-body scales drive the
corresponding call-site aesthetic's pixel range when the call's
`size:` (resp. `strokeWidth:`) column name matches the scale's column,
taking precedence over a same-aesthetic chart-scope scale. They produce
a legend through the normal legend pipeline (§16.13). Chart-scope
variants remain valid; they fire only when the column resolves against
the chart primary (§13.17 phase 6) and act as a fallback when no
glyph-body scale matches.

### §16.13 Scale-Driven Legend Labels (extend)

Append one sentence: a `size` / `strokeWidth` scale whose only
downstream consumer is a glyph call's `size:` (resp. `strokeWidth:`)
argument MUST produce a legend candidate, with the scale's `label:` (or
default column name when `label:` is omitted) as the swatch title. The
candidate dedupes against same-aesthetic chart-scope scales using the
existing legend-merge rules.

No new diagnostic codes. No changes to `E1101`. No grammar changes.

## Must

- Carry glyph-body Scales on the IR.

  Status: Implemented.

  Add `body_scales: Vec<ScaleIr>` to `GlyphCallIr` in
  `crates/algraf-semantics/src/ir.rs` and re-export through
  `crates/algraf-semantics/src/lib.rs`. The analyzer
  (`crates/algraf-semantics/src/analyzer/frames.rs::glyph_call`,
  ~line 877) already parses glyph-body `Scale` items via
  `scale_decl(&decl, &glyph_data_table)`; today their result is
  consumed only by the child Space planning and otherwise discarded.
  Stash the parsed `ScaleIr`s on the call-site IR instead, then keep
  feeding them into the existing inner-Space merge so fill/stroke
  inside the glyph body does not regress.

- Resolve glyph-call `size:` against glyph-body scales first.

  Status: Implemented.

  Change `mapped_size_pixel_range` in
  `crates/algraf-render/src/render/glyph_plan.rs` to walk
  glyph-body scales before falling back to the chart-scope scales it
  reads today. Cleanest shape: take both lists (or a single chained
  iterator) and return the first column-name match. The call site at
  line 228 (`mapped_size_pixel_range(glyph, &ir.scales)`) becomes
  `mapped_size_pixel_range(glyph, &glyph.body_scales, &ir.scales)`.
  Glyph-body wins because it is more specific.

- Emit a size legend for glyph-call size scales.

  Status: Implemented.

  Extend `collect_glyph_legend_candidates` in
  `crates/algraf-render/src/render/legend.rs` (line 70–87). Before the
  existing child-panel recursion, walk `glyph.body_scales` and the
  chart `ir.scales` for any `Scale(target: size | strokeWidth)` whose
  column name matches the glyph call's `size:` / `strokeWidth:`
  column; build a `Legend` via the same
  `number_spec(...).legend(&title, LegendKind::Radius)` path
  `collect_geometry_legend_candidates` uses at line 127–157. Extract a
  small helper if it removes duplication. Honor `glyph.legend` (the
  call-site `legend:` flag) for suppression and continue to honor
  `glyph.scale_policy != GlyphScalePolicyIr::Shared` so per-instance
  scales do not duplicate the swatch. This change also fixes the
  chart-scope legend gap as a side effect — intentional, and aligned
  with the second symptom users reported.

- Update `ALGRAF_SPEC.md`.

  Status: Implemented.

  Apply the §14.27 and §16.13 additions described above in the same
  change that lands the implementation. No new diagnostic codes;
  remove no existing prose.

- Convert `examples/inset_city_pies.ag` to the glyph-body Scale form.

  Status: Not adopted — example retained at chart scope.

  During implementation the migration revealed that the example's
  `population` column lives in the host `Space`'s `data: cities`
  override (and incidentally in the chart-primary GeoJson), but *not*
  in the glyph's `data: city_mix` (`city_population_mix.csv` has
  `city,age_group,count` only). Per spec §14.27 glyph-body Scale
  column resolution uses the glyph's `data:` table, so the migrated
  body-scope `Scale(size: population)` correctly emits E1101. The
  example's structural shape is "per-host-row size" — chart-scope
  Scale is the right home for it. The chart-scope legend-gap fix
  (intentional side effect of the glyph-call legend pipeline) now
  produces the missing size swatch on the unchanged example. The
  body-scope code path is exercised by analyzer + render tests
  instead (see Promotion Workflow §3–§5 below).

- Bump release version stamps to 0.72.0.

  Status: Implemented.

  Updates: workspace `Cargo.toml`, `Cargo.lock` workspace member
  entries, `docs/ALGRAF_SPEC.md` (`Status:` line and the inline
  "current implementation is version" prose, plus a new v0.72 history
  line), `editors/vscode/package.json`,
  `editors/vscode/package-lock.json`, `demo/package.json`, and
  `demo/package-lock.json`.

## Should

- Honor the same precedence for `strokeWidth:`.

  Status: Deferred to v0.73 — noted in §14.27.

  The glyph mark call surface (`GlyphCallIr`) only exposes `size:` as
  a per-instance numeric aesthetic; there is no call-site
  `strokeWidth:` argument to take precedence for. Glyph-body
  `Scale(strokeWidth: …)` declarations remain folded into child Space
  scales (existing behavior) and are not promoted to call-site
  precedence. §14.27 carries a one-line note marking the
  call-site `strokeWidth:` aesthetic deferred.

- Leave host-`Space`-body Scales out of scope.

  Status: Proposed (deferred).

  A host-`Space`-body Scale would also resolve the column cleanly
  (against the host space's active table) and would drive sizing for
  any glyph call inside that space. It is a valid third home, but the
  user explicitly picked glyph-body as the canonical location; adding
  the host-Space path doubles the precedence rules and risks legend
  ambiguity. Hold for v0.73 unless trivial during implementation; if
  added, the precedence becomes glyph-body → host-`Space`-body →
  chart-scope.

- Publish browser packages alongside the workspace release.

  Status: Manifests bumped — publish step pending.

  `packages/wasm/package.json` and `editors/monaco/package.json` are
  stamped at 0.72.0 in this change. The actual `npm publish` of
  `algraf-wasm@0.72.0` and `algraf-editor@0.72.0` is a separate user
  action; `demo/package.json` still pins the published 0.71.0
  versions and should be bumped once the new packages land on the
  registry so downstream Studio (`../studio/docs/V0_19_PLAN.md`
  "Should" item) can bump its consumed pins and unblock its release.

## Validation

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./examples/generate.sh
git diff -- examples   # only the inset_city_pies.ag move + its re-rendered SVG/PNG
```

Targeted regression for the originating Studio chart (run from the
`algraf/` workspace with the local sibling `../studio` checked out):

```bash
cp ../studio/src/datafarm-solar/data/us_counties.geojson \
   ../studio/src/datafarm-solar/04/us_counties.geojson    # transient
./target/debug/algraf check  ../studio/src/datafarm-solar/04/seasonal-pie-map.ag
./target/debug/algraf render ../studio/src/datafarm-solar/04/seasonal-pie-map.ag \
   --output /tmp/spm.svg
rm ../studio/src/datafarm-solar/04/us_counties.geojson
```

Acceptance, with the Studio chart's `Scale(size: generation_gwh, range:
[22, 54], label: "Annual generation (GWh)")` moved inside its `Glyph
pie(...) { ... }` body:

- `check` reports zero diagnostics (no `E1101`).
- The rendered SVG sizes one pie per state at the `albers_usa`-projected
  centroid, with radii spanning the declared `[22, 54]` range.
- A size legend swatch is drawn at the chart level with the title
  "Annual generation (GWh)".
- Chart-scope `Scale(size: col, range:, label:)` on the unchanged
  `inset_city_pies.ag` (if restored to its v0.71 form for the
  experiment) continues to size correctly and now *also* gets a legend.
- `E1101` still fires for chart-scope `Scale(target: col)` when `col`
  is absent from the chart primary — strict analyzer behavior
  preserved.
- No regression in fill/stroke legends inside any glyph child Space.

## Open Questions

1. Whether a single precedence walk function should serve both
   `mapped_size_pixel_range` and the new legend collector path, or
   whether two narrow helpers stay clearer. Decide during
   implementation; prefer the narrow form if the size and legend
   call sites both stay short.
2. Whether the chart-scope size-legend fix should be split out as a
   separate, isolated commit (it has independent value and a
   tighter blast radius) or land bundled. Default to bundled —
   single release, single user-visible behavior story.
3. Whether the spec text should call the glyph-body `Scale` placement
   "RECOMMENDED" or merely "permitted" alongside chart-scope. Lean
   "RECOMMENDED" because chart-scope only works when the column
   accidentally exists in the chart primary, which is a fragile
   correspondence.

## Promotion Workflow

1. Confirm the `body_scales` field name and location with a quick
   reading of `GlyphCallIr` and its construction sites; rename if a
   neighbor field clashes.
2. Add the §14.27 / §16.13 spec additions (no new codes) before the
   code change, per the repository's spec-before-implementation
   convention.
3. Add semantic tests: glyph-body `Scale(size:)` parsed and stored on
   `GlyphCallIr.body_scales`; analyzer accepts a `size:` column that
   exists in the glyph's `data:` table; analyzer still rejects an
   unknown column with `E1101`.
4. Add render tests: glyph instance pixel range comes from
   `body_scales` when present, falls back to `ir.scales` otherwise;
   precedence is glyph-body over chart-scope on a same-column
   collision.
5. Add legend tests: glyph-body size scale produces a legend; chart-
   scope size scale produces a legend; both producing the same
   aesthetic dedupe through the existing legend-merge path.
6. Implement: IR field → analyzer wiring → `mapped_size_pixel_range`
   precedence → `collect_glyph_legend_candidates` walk → spec.
7. Convert `examples/inset_city_pies.ag` to the glyph-body Scale form
   and re-render. Visually verify the new size swatch and unchanged
   pies. Inspect any other `examples/*` that use a glyph mark and a
   `Scale(size:)` to confirm no regressions.
8. Run `cargo fmt --all --check`, `cargo clippy --workspace
   --all-targets -- -D warnings`, `cargo test --workspace`,
   `./examples/generate.sh`. Require an empty `git diff -- examples`
   except the intended `inset_city_pies` port and its regenerated
   outputs.
9. Bump version stamps (`Cargo.toml`, `Cargo.lock`,
   `docs/ALGRAF_SPEC.md`, `editors/vscode/package.json` + lockfile,
   `demo/package.json` + lockfile) to 0.72.0.
10. Publish `algraf-wasm@0.72.0` / `algraf-editor@0.72.0` so Studio's
    pending `V0_19_PLAN.md` Should item can land.
