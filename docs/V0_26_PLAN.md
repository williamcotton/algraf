# Algraf v0.26.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_25_PLAN.md`](V0_25_PLAN.md)

## Purpose

This document defines the intended v0.26.0 release shape: introducing a **polar
coordinate transform at the `Space` level** so that a whole family of circular
charts — pie, donut, coxcomb (Nightingale rose), radial bar, wind rose, circular
histogram, polar scatter/line, annular heatmap, and radar/spider — emerge from
the *existing* Cartesian geometries (`Bar`, `Rect`, `Tile`, `Point`, `Line`,
`Area`, `Ribbon`, `Histogram`) rather than from new bespoke geometries.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.26.0 is a **coordinate-systems** release. Algraf is rooted in Wilkinson's
Grammar of Graphics, where circular charts are not distinct geometries but
ordinary Cartesian geometries mapped into a polar space. Today every space is
implicitly Cartesian — the spec only ever describes "Cartesian space" (§1, §5.3,
§8.3) and there is no coordinate-system concept.

This release adds `coords`, `theta`, and `innerRadius` arguments to `Space(...)`,
remaps scale *ranges* into a polar frame, teaches the area-filling geometries to
emit SVG arc paths, and renders circular/polygonal guides — while keeping all
existing Cartesian output byte-for-byte unchanged. The grammar, not a pile of
`Pie()`/`Donut()` geometries, produces circular charts.

## Settled Design Decisions

1. **Pie/donut use a 1D space + `theta`, not a numeric-literal dummy axis.**
   `Space(amount, coords: "polar", theta: "y") { Bar(fill: product, layout: "fill") }`.
   The single 1D frame's value wraps around the angle; the radius is the full
   plotting radius. The algebra grammar is **not** changed (we do not add numeric
   literals as algebra primaries). 1D spaces already exist (the histogram example
   uses `Space(hour)`).
2. **Default orientation is 12 o'clock, clockwise.** The theta domain maps to
   the angular range `[-π/2, 3π/2]` traversed clockwise — the conventional
   pie/coxcomb origin. This is fixed in v0.26; configurable
   `startAngle`/`direction` is deferred.
3. **No new geometries.** Polar is achieved entirely through the coordinate
   transform applied to existing geometries.
4. **Cartesian stays the default and stays unchanged.** Absent `coords: "polar"`,
   every space produces identical SVG to today.

## Scope Rules

- Polar is opt-in via `coords: "polar"`.
- The polar math lives in the scaled-space layer; geometries ask the space for
  pixel coordinates (and arc parameters) rather than knowing about circles
  (spec §10.5).
- Output stays deterministic: fixed angle origin/direction, stable ordering, no
  locale/time dependence (spec §18.12, §23.6).
- Stacking/fill logic (`BarLayout`) is reused, not reimplemented — only emission
  differs in polar.
- The algebra grammar is untouched.

## Capstone Acceptance Target

A `radar.ag` example — the hardest case: a closed `Area` + `Line` + `Point` over
a categorical-theta polar space with a polygon grid — renders deterministically,
alongside `pie.ag`, `donut.ag`, and `coxcomb.ag`.

The release must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples   # only intentional new/changed SVG+PNG
```

## v0.26.0 Must

### 1. `Space` coordinate arguments (syntax + semantics + IR)

Status: Planned.

Acceptance criteria:

- No parser change for the args themselves: `Space(...)` already parses arbitrary
  `key: value` named arguments via `arg()`, surfaced as `Arg` AST nodes.
- Validate three new args in `algraf-semantics` (modeled on the existing
  `space_projection()` helper):
  - `coords`: string literal, one of `"cartesian"` (default) | `"polar"`.
    Invalid value → `E1901`.
  - `theta`: string literal `"x"` (default) | `"y"`, selecting which frame axis
    maps to the angle (the other maps to radius). Invalid → `E1902`. Only
    meaningful when `coords: "polar"`.
  - `innerRadius`: numeric literal in `[0, 1)` (fraction of max radius; `0` =
    pie, `>0` = donut). Out of range / non-numeric → `E1903`.
- Add a coordinate-system representation to `SpaceIr` (e.g. `coords: CoordsIr`
  with `Cartesian | Polar { theta, inner_radius }`), threaded like the existing
  `projection: Option<String>` field.
- Frame-shape validation: `coords: "polar"` requires a frame the transform can
  use (1D, or 2D `a * b`). An unsupported frame shape (e.g. `theta: "y"` on a 1D
  frame) → `E1904`. A 3D+ polar frame is rejected (reuse the existing 3D
  rejection, `E1306`, or a polar-specific `E1905`).

### 2. Polar scale range mapping

Status: Planned.

Acceptance criteria:

- Domain training is unchanged (continuous/temporal/band domains as today); only
  the **range** mapping changes under polar.
- The `theta` axis maps its domain to the angular range `[-π/2, 3π/2]` clockwise;
  the radius axis maps its domain to `[innerRadius · R, R]`, where
  `R = min(plot.width, plot.height) / 2`.
- Polar center and `R` derive from the plot rectangle (`layout.plot`); polar
  plots use a square-ish area.
- `resolve_x`/`resolve_y` return final Cartesian pixel coordinates after applying
  `x = cx + r·cos(θ)`, `y = cy + r·sin(θ)`, so point-like geometries need no
  polar awareness. Accessors expose raw `(θ, r)` and band/wedge extents for the
  area-filling geometries that draw arcs.

### 3. Wedge / arc emission for area-filling geometries

Status: Planned.

Acceptance criteria:

- Add an SVG arc/annular-segment path helper (using the SVG `A` arc command)
  near the existing ad-hoc `d="…"` path building.
- `Bar`, `Rect`, `Tile`, `Ribbon` branch on the space's coords: when polar, draw
  a wedge (theta extent × radius extent) or annular segment instead of a rect.
  Stacking/fill logic (`BarLayout::{Identity, Stack, Fill}`) is reused unchanged.
  - Pie/donut: `theta: "y"`, `layout: "fill"`, 1D frame → stacked angular wedges
    spanning the full radius (donut clipped by `innerRadius`).
  - Coxcomb / wind rose: `theta: "x"` → angular bands, value drives radius.
  - Radial bars: `theta: "y"` with a categorical radius → concentric ring
    segments.
  - Annular heatmap: `Tile` → curved (annular) tiles via `innerRadius` + band
    extents on both axes.
- `Histogram` (desugars to `Derive` + `Rect`) yields circular histograms for free
  once `Rect` honors polar.

### 4. Point / Line / Area in polar (incl. radar)

Status: Planned.

Acceptance criteria:

- `Point` needs no special math — it places a marker at the now-polar-projected
  `(resolve_x, resolve_y)`.
- `Line` / `Area` collect projected `(x, y)` per group as today; when polar they
  **append `Z` (closepath)** so the polygon closes from the last category back to
  the first (radar / closed cycles). Area fill closure wraps correctly in polar.
- Polar scatter/line (seasonal/periodic data) and radar share this path.

### 5. Polar guides

Status: Planned.

Acceptance criteria:

- When polar, the Cartesian grid/axes are replaced with:
  - **Radius axis**: concentric grid rings (or polygon, below) at each tick, with
    radius labels along a single spoke.
  - **Theta axis**: spokes from center to perimeter at each tick, with category
    labels placed around the perimeter.
- A guide arg `gridShape: "circle"` (default) | `"polygon"` (invalid → `E1906`).
  For `"polygon"`, draw straight line segments between adjacent spokes at each
  radius tick (radar pentagon/hexagon grid) instead of `<circle>`/arc rings.
  Wired through the existing `Guide(...)` override mechanism.

### 6. Spec, plan, and example hygiene

Status: Planned.

Acceptance criteria:

- Spec is updated to make polar normative in the same change as the code:
  - §4.2 Space — coordinate-system concept and `coords`/`theta`/`innerRadius`
    args; Cartesian is the default.
  - §7.3 Space Block — grammar note for the new args.
  - §16 Scale Training — new subsection (e.g. §16.16 "Polar coordinate
    transform") describing the `[-π/2, 3π/2]` clockwise angular range and
    `[innerRadius·R, R]` radial range.
  - §15 Geometries — wedge/arc emission for Bar/Rect/Tile/Ribbon and closed
    Line/Area under polar.
  - §18 SVG Rendering — arc (`A`) / annular-path emission.
  - §19 Guides — circular vs polygon grids, spokes, `gridShape`.
  - §26 Diagnostics Catalog — reserve `E1901`–`E1906` (and any polar frame-shape
    code) *before* implementing.
- Workspace `Cargo.toml` and `editors/vscode/package.json` are bumped to
  `0.26.0` when the release branch is ready.
- VS Code TextMate grammar / language-configuration updated only if new keywords
  are surfaced (the arg keys are plain identifiers, so likely no change); verify
  the LSP surfaces the new args.

### 7. Examples and README

Status: Planned.

Acceptance criteria:

- Add runnable examples (registered in `examples/generate.sh` and a README
  tutorial section in the theming/coords progression):
  - `pie.ag`, `donut.ag` (1D space, `theta: "y"`, fill, `innerRadius`)
  - `coxcomb.ag` (Nightingale rose)
  - `radial_bar.ag` (concentric rings)
  - `wind_rose.ag` (stacked polar bars)
  - `circular_histogram.ag` (Histogram in polar)
  - `polar_scatter.ag` (seasonal Point + Line)
  - `annular_heatmap.ag` (Tile, `innerRadius`)
  - `radar.ag` (Area + Line + Point, `gridShape: "polygon"`) — capstone
- Examples are regenerated with `./examples/generate.sh`.

## v0.26.0 Should

### LSP support for the new args

Status: Planned.

Completion/hover for `coords`, `theta`, `innerRadius`, and `gridShape` values via
`algraf-lsp`, so the editor and TextMate grammar stay in sync.

### Polar axis label legibility

Status: Planned.

Rotate/anchor perimeter labels sensibly and avoid overlap on dense theta axes.

## Explicitly Deferred Past v0.26.0

- Configurable `startAngle` / `direction` (clockwise/counterclockwise) Space
  args — fixed at 12-o'clock clockwise for now.
- Numeric-literal / constant algebra primaries (the ggplot2 `x=""` style dummy
  axis) — superseded by the 1D-space approach.
- 3D+ polar frames.
- Combining polar with geographic projections (§16.15 spatial scale).
- Faceted polar small-multiples beyond what existing faceting yields for free.
- Animation / transitions.

## Optional-Item Audit

### Promote In v0.26.0 (Must)

- `Space` coordinate args (`coords`, `theta`, `innerRadius`).
- Polar scale range mapping.
- Wedge/arc emission for Bar/Rect/Tile/Ribbon.
- Point/Line/Area in polar (closed shapes for radar).
- Polar guides (circular + polygon grids, spokes).
- Spec, plan, and example hygiene + version bump.
- Examples and README.

### Consider If Capacity Allows (Should)

- LSP completion/hover for new args.
- Polar axis label legibility polish.

### Keep Deferred

- Start-angle/direction config, numeric-literal axes, 3D polar, polar+geo,
  animation.

## Promotion Workflow

1. Reserve `E1901`–`E1906` in spec §26; add the coordinate-system concept to
   §4.2 / §7.3 before coding.
2. Add `CoordsIr` to `SpaceIr` and validate the args in semantics (model on
   `space_projection`).
3. Add the polar range mapping + center/`R` computation; keep domain training and
   Cartesian output unchanged.
4. Add the SVG arc/annular-segment path helper.
5. Branch the area-filling geometries (`Bar`, `Rect`, `Tile`, `Ribbon`) on polar;
   verify stacking/fill is reused, not reimplemented.
6. Append `Z` for polar Line/Area; confirm radar closes correctly.
7. Add polar guides + `gridShape`.
8. Add examples, README sections, LSP arg metadata; bump versions.
9. Run formatter, clippy, workspace tests; regenerate examples; review the
   `examples` diff to confirm no Cartesian output changed.
