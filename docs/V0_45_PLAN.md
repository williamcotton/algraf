# Algraf v0.45.0 Plan

Status: Drafting
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_44_PLAN.md`](V0_44_PLAN.md)
Roadmap theme: finish the v0.44.0 inset planning/emission separation.

## Purpose

This release is an **internal health release**, not a language release. v0.44.0
shipped insets as a first-class block item but kept too much of the work in the
paint pass: a single `inset.rs` (606 LOC) re-trained scales, resolved anchors,
matched rows, and built child viewports *during emission*, then the metadata
collector and the legend collector each re-walked the IR and re-did the same
work in parallel. That violated the §24.6 two-half pipeline that the rest of the
renderer follows, and it left three drift surfaces between paint output, sidecar
metadata, and legend swatches.

An in-progress refactor on the working tree at v0.44.0 has already begun closing
this gap by introducing a `PlannedInset`/`PlannedInsetInstance` planned-scene
representation, an `inset_plan.rs` planning module, a generic `RowSubsetTable`,
and an `InsetMatchIndex` hash-bucketed match. With that change, `metadata.rs`
deletes its `plan_metadata_child_space` copy of the inset planner, `backend.rs`
drops three fields from `RenderScene` (`primary`, `derived`, `cli_theme_override`),
and `inset.rs` shrinks to a 150-LOC painter that walks the planned tree.

v0.45.0 lands that refactor and finishes the remaining alignment work it began
but did not complete: extracting recursive inset planning out of `panels.rs`,
teaching `legend.rs` to read from the planned scene the same way `metadata.rs`
now does, renaming `inset.rs` for symmetry with `inset_plan.rs`, and adding the
unit coverage the new `InsetMatchIndex` lacks.

## Release Thesis

v0.45.0 is the **inset planning/emission separation** release. Its success
criterion is *behavioral invisibility*: refactoring planning into its own
modules MUST NOT change any rendered SVG, draw-list JSON, raster output,
interaction sidecar, diagnostic code, or diagnostic order. The only intended
external changes are confined to the Should section: a coalesced W2002
diagnostic for sparse-match insets, and additional unit-test coverage for
`InsetMatchIndex`.

This is the same discipline as v0.35.0's architecture-hardening release: the
output corpus produced by `./examples/generate.sh` MUST be byte-for-byte
identical to v0.44.0 for every Must item.

## Scope Rules

- **No behavior change in Must items.** Each refactor MUST be covered by an
  output-equivalence check against the v0.44.0 baseline (existing snapshot/insta
  tests plus the full example corpus via `./examples/generate.sh` with an empty
  `git diff -- examples`).
- **No new language surface.** No new inset arguments, geometries, properties,
  scales, themes, diagnostics, or CLI flags. Behavior-changing items (W2002
  coalescing) are confined to the Should section.
- **Single source of truth for planned scenes.** After this release, neither
  metadata collection nor legend collection nor any backend may re-derive inset
  matches, anchors, viewports, or trained child scales — they all read the
  `PlannedInset`/`Panel` produced by `panels::build_render_plan`.
- **Module splits preserve public APIs.** Re-exports MUST keep each crate's
  existing `pub use` surface stable. All inset planning types remain
  `pub(super)`-scoped to `crate::render`.
- **Determinism is preserved.** Diagnostic emission order, parent-row iteration
  order, `union_rows` ordering, and the `i{inset}[{parent_row}]:s{space}` plot
  ID format MUST be identical to v0.44.0 byte-for-byte.

## Source of Work Items

Every Must and Should item below maps to a concrete finding from the v0.44.0
inset feature review (architectural separation-of-concerns assessment of the
v0.44.0 commit `948b4a8` and the in-progress refactor on the working tree).

## Current Coverage Audit

Shipped in v0.44.0 and preserved by this release:

- `Inset(...)` block grammar, `InsetIr`, row-context name resolution, match
  semantics, scale policy, viewport sizing, clipping, placement, anchor;
- recursive nested insets up to `MAX_INSET_DEPTH = 8` with `E2109`;
- recursive mark-budget estimation with `E2110`;
- W2002 warnings for unmatched parent rows and unresolved anchors;
- SVG, draw-list, raster, and sidecar metadata parity for inset scenes;
- the three checked-in example charts (`inset_city_pies`, `inset_sparklines`,
  `nested_insets`).

Gaps assigned to this release (all internal):

| Area | v0.44.0 shape | v0.45.0 gap |
| ---- | ------------- | ----------- |
| Planning/emission seam for insets | In-progress on working tree, not yet committed | Land the working-tree refactor as the first Must. |
| `panels.rs` scope | 1,218 LOC: facet layout + axis sharing + spatial glue + recursive inset planning | Move recursive inset planning to `inset_plan.rs`. |
| Legend collection | `legend::collect_inset_legend_candidates` walks IR, builds its own `InsetMatchIndex`, computes matches independently | Read inset legend candidates from the planned scene. |
| File naming | `inset.rs` is now purely a painter but keeps the generic name | Rename to `inset_paint.rs` for symmetry with `inset_plan.rs`. |
| Test coverage | `InsetMatchIndex` bucketing key (NaN exclusion, ±0 normalization, int↔float coercion) is only exercised end-to-end | Add focused unit tests. |
| Diagnostic flood | One W2002 per unmatched parent row | Coalesce to one summary diagnostic per inset declaration. |
| Root vs child intent | Six call sites pass `&[], 0` as `ancestors`/`depth` to `planned_panel` | Add a `build_root_panel` wrapper for clarity. |

## v0.45.0 Must

### 1. Land the in-progress planning/emission separation

Status: Drafting (currently uncommitted on working tree).

- Commit the working-tree refactor that introduces `PlannedLayer<'t>`,
  `PlannedInset<'t>`, `PlannedInsetInstance<'t>`, `inset_plan.rs`, and
  `row_table.rs`, and shrinks `inset.rs` to a painter that consumes the planned
  tree.
- Drop `primary`, `derived`, and `cli_theme_override` from `RenderScene`; the
  scene MUST be format-agnostic.
- Delete `metadata.rs::plan_metadata_child_space` and the duplicated
  `LegendRowsTable`; the generic `RowSubsetTable` is the single row-subset
  adapter.
- Replace `inset.rs::matched_child_rows` with `InsetMatchIndex::matched_rows`
  everywhere it was used (panels, metadata, legend).
- Output corpus MUST be byte-for-byte identical to v0.44.0. `./examples/generate.sh`
  followed by `git diff -- examples` MUST produce no diff.

### 2. Move recursive inset planning out of `panels.rs`

Status: Pending.

- Move `plan_inset` and `plan_child_panel` (currently ~230 LOC in `panels.rs`)
  into `inset_plan.rs` alongside `InsetMatchIndex`, `inset_anchor`, `inset_size`,
  `inset_plot`, `mapped_size_domain`, and `inset_budget_diagnostic`.
- `panels.rs::plan_layers` calls into `inset_plan::plan_inset`; the mutual
  recursion through `planned_panel` stays via a `pub(super)` function in
  `panels.rs`.
- After the move, `inset_plan.rs` owns inset planning end-to-end and `panels.rs`
  is back to "root panel layout + facet + axis sharing + spatial glue."
- Target: `panels.rs` under 1,000 LOC.
- Output corpus MUST be byte-for-byte identical.

### 3. Read inset legend candidates from the planned scene

Status: Pending.

- Replace `legend::collect_inset_legend_candidates`'s IR walk with a walk over
  `PlannedInset` and `PlannedInsetInstance`. The collector MUST NOT build its
  own `InsetMatchIndex`, call `active_table`, or re-compute matches.
- If the planned scene does not yet carry enough information for legend
  collection (e.g. shared child rows), add the minimal fields to
  `PlannedInset`/`Panel` rather than re-deriving them in the legend module.
- Reorder the planning pipeline if needed so legend collection runs after
  `build_render_plan` produces panels.
- Verify deduplicated legend output is unchanged for every example that uses
  inset scales (`inset_city_pies`, `inset_sparklines`, `nested_insets`).
- Output corpus MUST be byte-for-byte identical.

### 4. Rename `inset.rs` to `inset_paint.rs`

Status: Pending.

- After Must 1, `inset.rs` is purely the painter. Rename to `inset_paint.rs` so
  the file pair `inset_plan.rs` / `inset_paint.rs` reads symmetrically and
  matches the §24.6 planning/emission boundary.
- Update `mod` declarations in `render.rs`.
- This is a pure rename — no logic changes, no test changes, no diagnostic
  changes, no output changes.

## v0.45.0 Should

### Focused unit tests for `InsetMatchIndex`

Status: Pending.

- Add unit tests in `crates/algraf-render/tests/` covering the bucketing key
  normalization:
  - NaN child or parent values produce no match (and no panic);
  - `+0.0` and `-0.0` bucket together;
  - large `i64` values that lose precision in `f64` still produce correct
    matches (the post-bucket `data_values_match` filter must catch false
    bucket positives);
  - cross-type Int↔Float matches behave the same as v0.44.0's element-wise
    comparison;
  - Temporal and String keys round-trip;
  - composite match keys (e.g. `[city => city, category => category]`)
    correctly require all components to match;
  - empty child tables and empty parent row lists produce empty results
    without panicking.
- These tests document the contract the post-refactor inset planner depends on.

### Coalesce W2002 sparse-match diagnostic

Status: Pending.

- For a parent row set with `M` parent rows and `N` unmatched parent rows
  (`N > 1`), emit one summary diagnostic such as
  `"Inset matched no child rows for N of M parent rows"` rather than `N`
  individual W2002 warnings.
- Keep per-row W2002 emission when `N == 1` so single-row failures still
  surface a precise span.
- This is a small, intentional behavior change — reserve the wording in the
  spec under W2002 before implementation.
- Update fixtures that exercise sparse-match insets to expect the coalesced
  form.

### Make root-vs-child panel construction explicit

Status: Pending.

- Add a thin `build_root_panel` wrapper around `planned_panel` that fills in
  `ancestors: &[], depth: 0` for the six call sites in `panels.rs` that
  currently spell those defaults inline.
- Inset child construction continues to call `planned_panel` directly via
  `plan_child_panel`, threading the real ancestor stack and depth.
- This is purely a readability change.

## Explicitly Deferred Past v0.45.0

- Any change to the `Inset(...)` user-facing surface (new arguments, new clip
  shapes, new placement modes, new scale policies, per-inset legends).
- A general-purpose `SceneNode` enum spanning Space/Geometry/Inset that
  replaces `Panel`/`PlannedLayer`. The current planned-scene shape is
  sufficient; a unified node tree is a larger architectural change that should
  be motivated by a feature, not by symmetry.
- Sharing the recursive IR walk between `panels::build_render_plan` and any
  future planning pass. Each planning pass remains free to walk the IR
  independently as long as it does not duplicate match/anchor/scale-training
  work that the planner has already done.
- Promoting `RenderBackend` or `RenderScene` to a public extension point. The
  trait stays `pub(super)`-scoped and the set of backends stays closed
  (spec §24.6).
- Performance optimization of recursive inset planning beyond the hash-bucketed
  match index that v0.44.0's refactor already introduces. Mark-budget
  estimation already protects against pathological output.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

`git diff -- examples` MUST be empty for all Must items. The Should item that
coalesces W2002 may add or change a diagnostic fixture; that diagnostic change
MUST be the only `examples`-adjacent diff and MUST be reviewed against the
spec's reserved W2002 wording.

Inset-touching changes should also re-run focused parity tests for:

- one-level pie insets on a projected map (`inset_city_pies`);
- one-level sparkline insets on a Cartesian scatterplot (`inset_sparklines`);
- nested insets with a composite parent/child match (`nested_insets`);
- recursive mark-budget failure (E2110);
- recursion-depth failure (E2109);
- sparse-match W2002 emission (after the Should coalescing lands).

## Promotion Workflow

1. Land Must 1 (the in-progress refactor) as a self-contained commit. Verify
   `./examples/generate.sh` produces an empty diff before merging.
2. Land Must 2 (move recursive inset planning out of `panels.rs`) as a
   separate commit. Verify empty example diff.
3. Land Must 3 (legend reads from planned scene) as a separate commit, possibly
   extending `PlannedInset` with the fields legend collection needs. Verify
   empty example diff.
4. Land Must 4 (file rename) last so it does not interfere with diff review of
   the substantive refactors.
5. Reserve the coalesced W2002 wording in the spec, then land the Should item
   together with its fixture update.
6. Add `InsetMatchIndex` unit tests at any point; they are additive.
7. Add the `build_root_panel` wrapper at any point; it is additive.
8. Do not close v0.45.0 until `panels.rs` is under 1,000 LOC, `legend.rs` no
   longer builds its own `InsetMatchIndex`, and every backend reads inset state
   exclusively from the planned scene.
