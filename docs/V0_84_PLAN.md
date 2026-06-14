# Algraf v0.84.0 Plan

Status: Implemented
Target version: 0.84.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_83_PLAN.md`](V0_83_PLAN.md)
Roadmap theme: Fix the v0.80.0 Cartesian-clip regression by making the clip mask
**follow the pinned domain bounds** — clip an axis edge only where the author
pinned that bound (or zoom narrows that axis), per edge, and never clip a purely
data-trained panel. No new syntax.
Cross-repo coordination: none required to ship 0.84.0. The browser packages
`algraf-wasm` and `algraf-editor` are not published at 0.84.0 implementation
time, so their package versions and consumer pins remain on the latest verified
published version, 0.81.0 (`npm view algraf-wasm versions --json` and
`npm view algraf-editor versions --json`, verified during implementation).

## Purpose

v0.80.0 introduced default Cartesian plot clipping (§18.5): every Cartesian
panel opens a rectangular clip scope around its data-mark layers, set to exactly
the plot rectangle. The intent was narrow and sound — when an explicit
`Scale(axis:, domain:)` bound or `Space(zoomX:/zoomY:)` deliberately narrows the
view, marks for excluded data should be masked instead of bleeding into margins,
legends, titles, or neighboring panels.

The implementation, however, gates the clip on coordinate kind alone:

```rust
// crates/algraf-render/src/render/panels.rs
fn clips_cartesian_data_marks(space: &SpaceIr) -> bool {
    matches!(space.coords, CoordsIr::Cartesian) // true for EVERY cartesian panel
}
```

So the clip is opened on *every* Cartesian panel, on all four sides, including
plain auto-fit panels whose axis ranges are trained from the data and therefore
exclude nothing. Because continuous position scales map domain-min/max straight
to the panel edges (zero expansion), a mark at a boundary data value renders
with its *center* on the plot edge and its *body* straddling it. The hard clip
then slices that body:

- A `Point` at the maximum x (or y) value loses its outer half — visible in
  `examples/temporal_formats_auto.png`, where the `2026-05-30 12:00 / 18` marker
  is cut by the right plot edge.
- An `Area`/`Line` trained to the data fills right up to the boundary, so its
  edge and any stroke half-width get shaved even though nothing is out of domain.

This is the regression in the maintainers' words: geometry that *should* be
clipped (out-of-domain bleed under an explicit domain or zoom) still is, but
geometry that should *not* be clipped (boundary marks on an ordinary auto-fit
panel) now is too. The clip fires in cases where there is, by construction,
nothing to mask — it can only ever subtract pixels an author wanted.

The v0.84.0 goal is to make the clip mask **follow the author's pinned domain
bounds**: clip exactly the edges the author closed, leave data-trained edges
open, and stay non-destructive at any edge it does clip.

## Release Thesis

A pinned domain bound *is* the author saying "the visible view stops here" —
which is exactly, and only, the edge where overflow should be masked. A
`null`/absent bound means "fit to data," so by construction nothing overflows
there and clipping it could only slice a legitimate boundary mark (the
regression). The clip therefore needs no new surface of its own; it is a derived
consequence of the domain the author already declared.

`ScaleIr` already carries this signal per bound:

```rust
// ScaleIr — unchanged
pub domain: Option<[Option<f64>; 2]>, // [lower, upper]; each bound Some(v) or None
//  domain: [0, null]  →  lower pinned to 0, upper from data (spec §16.11)
```

Two principles:

1. **Clip per edge, driven by pinned bounds (and zoom).** A Cartesian panel
   clips a given visual edge only when the bound that produces that edge is
   *pinned* — an explicit numeric bound in `Scale(axis:, domain:)` — or when
   `Space(zoomX:/zoomY:)` narrows that axis (zoom closes both ends of its axis).
   A data-trained (`null`/absent) bound leaves its edge open. A purely
   data-trained, unzoomed panel clips nothing. The bound→edge mapping is
   resolved through the trained scale's pixel orientation (so a reversed axis
   maps lower↔edge correctly), not by assuming "lower = bottom."

2. **Each clipped edge bleeds by the mark extent.** Where an edge *is* clipped,
   the mask is offset outward from the plot edge by a small, deterministic
   **bleed** equal to the panel's maximum data-mark half-extent (point radius,
   stroke half-width), capped to the reserved chrome on that side. A mark whose
   data coordinate sits on a clipped edge then renders its whole body, while
   geometry past that edge by more than the mark's own size is still masked. Open
   edges are not offset — they are simply not masked.

Three boundaries are preserved:

- **Scale, zoom, and clip stay distinct** (unchanged from v0.80.0): pinned
  domains and zoom do not filter rows or stat rows; the clip is the final
  renderer mask, applied after scale training and coordinate-view resolution.
- **Determinism is unchanged.** The per-edge mask is a pure function of the
  resolved IR (which bounds are pinned, zoom presence, scale orientation); the
  bleed is a pure function of the panel's resolved mark extents. No time,
  locale, or hash-order dependence.
- **Every backend agrees.** SVG `clipPath`, draw-list `clipStart`/`clipEnd`,
  raster mask, and interaction metadata consume the same per-edge mask and the
  same bled rectangle; metadata `clip_rect` reports the rectangle actually used,
  and a panel with no clipped edge reports no `clip_rect`.

## Reference Intent

The motivating reports are auto-fit panels with nothing out of domain, plus the
"floor" case that pinned-bound clipping now expresses for free:

| Scenario | Pinned bounds | Result |
| --- | --- | --- |
| Boundary `Point` sliced at the right edge (`temporal_formats_auto`) | none | No edge clipped → point renders whole |
| `Area` clipped along an edge the author wanted whole | none | No edge clipped → area renders whole |
| Cut everything below zero, let peaks spike past the top | `domain: [0, null]` on y | Bottom clipped; top open |
| Mask out-of-domain bleed under a fully pinned domain | `domain: [lo, hi]` | Both edges of that axis clipped (as v0.80.0) |

## Must

### A. Clip per edge, driven by pinned domain bounds and zoom

- The renderer MUST compute a per-edge clip mask `{top, bottom, left, right}`
  for each Cartesian panel instead of a single panel-wide boolean. An edge is
  clipped when, and only when, the bound that maps to it is pinned, or zoom
  closes its axis:
  - For each position axis (x, y), a pinned lower or upper bound in the merged
    `Scale(axis:, domain:)` (`Some(v)`, not `None`) closes the corresponding
    visual edge; the lower↔edge / upper↔edge assignment MUST be taken from the
    trained scale's pixel direction so reversed/descending axes map correctly.
  - `Space(zoomX:)` closes both x edges (left, right); `Space(zoomY:)` closes
    both y edges (top, bottom).
  - A `null`/absent bound leaves its edge open.
- A purely data-trained, unzoomed Cartesian panel MUST clip no edges: it opens
  no clip scope, emits no `clipPath`/`clipStart`, reports no `clip_rect` in
  metadata, and produces no `"clipped": true` marks. This restores pre-v0.80.0
  byte output for ordinary auto-fit charts (the regression fix).
- `domain: [0, null]` (or any half-open domain) MUST clip only the pinned edge,
  leaving the data-trained edge open — the "floor"/"ceiling" behavior.
- The per-edge decision MUST be computed once per panel from the merged scales,
  the trained scale orientation, and the space's zoom state, and propagate
  unchanged to the SVG, draw-list, raster, and interaction-metadata backends.
  Acceptance: an auto-fit `Space(x * y)` with `Point`/`Line`/`Area` emits no
  clip scope and renders boundary marks whole; `Scale(axis: y, domain: [0,
  null])` masks geometry below `0` while leaving the top edge open; a fully
  pinned `domain: [lo, hi]` masks both ends as in v0.80.0.

### B. Per-edge clip bleed sized to the panel's mark extent

- For each *clipped* edge, the mask boundary MUST be offset outward from the
  plot edge by a **bleed** `b ≥ 0`, computed deterministically as the panel's
  maximum data-mark half-extent: the largest of the resolved point radii and the
  largest stroke half-width among the panel's data-mark layers (and `0` when the
  panel draws only zero-extent marks).
- The bleed MUST be capped per side so the mask never extends past the reserved
  chrome on that side (title, caption/source, axis title, legend, or a
  neighboring facet panel), so out-of-domain geometry can never reach them.
- Open (unclipped) edges MUST NOT be masked at all — they are not offset; the
  rectangle is unbounded on that side (clamped to the viewport).
- The resolved rectangle MUST drive SVG, draw-list, raster, and metadata
  identically; `clip_rect` reports the resolved rectangle, and a mark counts as
  `"clipped": true` only when its point coordinate lies outside it on a clipped
  edge.
- The bleed MUST be `0` (mask flush with the plot edge, current v0.80.0
  geometry) when no data-mark layer has positive extent, so the change is a
  no-op for those panels on the edges they clip.
  Acceptance: a `Point` whose center sits exactly on a pinned domain edge renders
  its full radius; an area whose values exceed a pinned bound by far more than
  its stroke half-width is still masked at that edge.

### C. Spec alignment

- §18.5 (Clipping) MUST be revised so the normative rule is per-edge and
  domain-derived: a Cartesian panel clips a visual edge when the bound mapping
  to it is pinned or zoom narrows its axis; data-trained edges and purely
  data-trained, unzoomed panels do not clip. Document the bound→edge mapping
  through scale orientation, the per-edge mark-extent bleed, and its margin cap.
- §16.11 (explicit/half-open domains do not filter rows) MUST cross-reference
  that a *pinned* bound additionally closes its visual edge for clipping, while a
  `null` bound does not.
- §24.3 (render metadata `clip_rect` / `"clipped"`) MUST be reconciled: a panel
  reports `clip_rect` only when at least one edge is clipped, and the rectangle
  equals the bled per-edge mask.
- The v0.80.0 milestone note ("Cartesian panels MUST open a clip scope …
  regardless of whether the axis view comes from data-trained domains, explicit
  bounds, or zoom") MUST be superseded by the v0.84.0 wording. Record the change
  as a v0.84.0 refinement of v0.80.0, not a silent edit of history.

### D. Regression coverage and examples

- Re-render `examples/temporal_formats_auto.{svg,png}` (auto-fit temporal line +
  points) via `./examples/generate.sh` and confirm the trailing boundary point
  is whole; this is the visual canary for the auto-fit (no-pin) case.
- Add a worked example demonstrating domain-derived single-edge clipping — an
  area or line with `Scale(axis: y, domain: [0, null])` whose data dips below the
  pinned floor, so the bottom is masked while the top is open (final filename and
  data fixture chosen at implementation time; reuse an existing CSV if one fits).
  Wire it into `examples/generate.sh` and add a `README.md` tutorial section.
- Add focused renderer regression tests asserting that:
  - an auto-fit Cartesian panel emits **no** clip scope and **no** `clip_rect`,
  - a half-open `domain: [0, null]` clips **only** the pinned edge (the other
    edge stays open and a boundary mark there renders whole),
  - a fully pinned `domain: [lo, hi]` still masks both ends — the v0.80.0
    `explicit_axis_domain_clips_cartesian_marks_by_default` test (or its
    successor) MUST continue to assert masking for the fully pinned case, and
  - a boundary mark sitting on a clipped edge is not sliced (bleed applied) while
    a far-out-of-domain mark is.
- Confirm the rest of the example corpus is byte-stable except panels that were
  being clipped only because of the over-broad v0.80.0 gate (i.e. auto-fit panels
  and the open edges of half-open domains); any such corpus diffs MUST be
  inspected as PNGs and explained in the promotion notes.

## Deferred

- **Explicit author clip control** (a `clip:` argument on `Space(...)`, e.g.
  per-edge force-on/off or a bleed-despite-pinning escape hatch). v0.84.0 derives
  the mask entirely from pinned bounds and zoom; an explicit override surface is
  intentionally deferred unless a concrete case needs the "pin the bound but let
  that edge bleed" (or "clip without pinning") behavior, which has no motivating
  request today.
- **Geometry-level clip opt-in/opt-out** (per-mark `clip:` on Cartesian data
  marks) remains deferred, as in v0.80.0. The mask is panel-level.
- **Configurable bleed amount.** The bleed is derived from mark extents and is
  not author-tunable in 0.84.0.
- **Scale expansion / range padding.** Adding default continuous-scale expansion
  (so boundary marks sit inset from the edge) is a separate, larger change and is
  not part of this regression fix.
- **Polar / radial clipping** stays as in v0.80.0 (deferred); this plan touches
  only the Cartesian per-edge mask and bleed.
- **Clipping of non-data chrome** (grid lines, axis ticks, guides) is unchanged.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Focused validation:

- `cargo test -p algraf-render explicit_axis_domain_clips_cartesian_marks_by_default`
  (the fully pinned domain case still masks both ends).
- `cargo test -p algraf-render` for the new auto-fit "no clip scope / no
  `clip_rect`" assertion, the half-open "clip only the pinned edge" assertion,
  and the boundary-mark bleed assertion.
- `./examples/generate.sh` then `git diff --stat examples/` to confirm
  `temporal_formats_auto.*` renders the trailing point whole, the new half-open
  domain example masks only its pinned edge, and to review any other corpus diffs
  (expected only where the over-broad v0.80.0 gate clipped an auto-fit panel or
  an open half-open edge).
- Manual: open `examples/temporal_formats_auto.png` and confirm the
  `2026-05-30 12:00` point is a full circle; render the half-open example and
  confirm the floor masks below-bound geometry while the top bleeds freely.

## Promotion Workflow

When implemented:

1. Replace the panel-wide `clips_cartesian_data_marks` boolean with a per-edge
   mask derived from the merged scales (pinned vs `null` bounds, mapped through
   the trained scale orientation) and `SpaceIr` zoom state (A).
2. Offset each clipped edge outward by the deterministic mark-extent bleed with a
   per-side margin cap, leave open edges unmasked, and route the resolved
   rectangle through SVG, draw-list, raster, and metadata (`clip_rect`) so all
   backends agree (B).
3. Update `ALGRAF_SPEC.md` §18.5, §16.11, §24.3, and the v0.80.0 milestone note
   to the v0.84.0 per-edge wording (C); this change reserves no new diagnostic
   codes.
4. Regenerate `examples/temporal_formats_auto.{svg,png}`, add the half-open
   domain example and its `README.md` section, and add the renderer regression
   tests (D); inspect any other corpus PNG diffs and record them.
5. Add the 0.84.0 row to the milestone table and mark this plan Implemented.
6. Align Rust, spec, and VS Code release version stamps to `0.84.0`; keep the
   unpublished browser packages (`algraf-wasm`, `algraf-editor`, demo) on their
   latest verified published pins, since browser package publication is
   independent of the Rust/CLI release (see `CLAUDE.md`).
7. Run the validation commands listed above.

## Implementation Notes

- Implemented in the renderer as a resolved `PanelClip`: edge decisions come
  from explicit pinned axis-domain bounds plus `zoomX`/`zoomY`, mapped through
  the trained scale range so reversed axes swap lower/upper visual edges.
- The resolved mask uses mark-extent bleed on closed edges and viewport
  expansion on open edges. SVG, draw-list/raster replay, and metadata consume
  that same rectangle.
- Added `examples/domain_floor_clip.ag` and regenerated the full example corpus
  with `./examples/generate.sh`.
- Inspected `examples/temporal_formats_auto.png`: the trailing
  `2026-05-30 12:00` point is a full circle at the right boundary.
- Inspected `examples/domain_floor_clip.png`: losses below zero are masked at
  the pinned floor while the top edge remains open.
- Other regenerated example diffs are expected where ordinary auto-fit panels
  previously emitted a `clipPath`/`clip_rect` only because of the over-broad
  v0.80.0 Cartesian gate; those panels now emit no clip scope.
