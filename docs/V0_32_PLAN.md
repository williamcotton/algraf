# Algraf v0.32.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_31_PLAN.md`](V0_31_PLAN.md)
Foundation plan: [`V0_30_PLAN.md`](V0_30_PLAN.md) (declarative interaction
metadata)

## Purpose

This document defines the intended v0.32.0 release shape: the **host-runtime
contract** that turns the declarative interaction metadata shipped in v0.30
into a real integration story for applications that embed Algraf charts.

[`V0_30_PLAN.md`](V0_30_PLAN.md) shipped the metadata foundation — `tooltip:`
and `highlight:` source syntax, per-mark `<g data-mark-id="…">` SVG groups
with `<title>` children, and an inert `interactions: { marks, groups }` block
on the draw list — but deliberately shipped *no runtime*. Static SVG with
CSS `:hover` and native `<title>` tooltips covered the zero-JS path; anything
richer was left for a release that could think the integration boundary
through.

The integration model in v0.32 is: **Algraf renders an SVG plus a JSON
sidecar; the host runtime (React, Vue, plain JS, Canvas/WebGL) consumes the
sidecar and drives interactivity itself.** Algraf ships a documented data
contract and a reference host implementation. The opt-in SVG runtime from v0.30
is retained for compatibility, but the v0.32 integration boundary is the
sidecar.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
is updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion
is a commitment to *attempt*; an item ships only when code, tests, docs, and
examples remain synchronized.

## Release Thesis

v0.32.0 is the **host-runtime** release. It picks up the v0.30 interaction
metadata and exposes it as a stable JSON contract alongside the rendered SVG,
adds the extra pieces a host needs (plot rect, invertible scale serialization,
per-mark pixel positions), and ships a reference React component plus an
interactive LSP preview that both consume the same contract.

The decision is that Algraf does not ship UX behavior. Algraf provides data;
the host decides how interactivity looks and feels. The same sidecar serves a
React tooltip overlay, a Canvas/WebGL custom renderer, a vanilla-JS crosshair,
and the LSP preview — one contract, many consumers.

## Current Debt Surface

The plan/spec/code audit found:

- The interaction metadata model from [`V0_30_PLAN.md`](V0_30_PLAN.md) gives
  hosts per-mark identity and tooltip data but no way to invert mouse
  coordinates back to data values (needed for crosshair / cursor readouts).
  Scales in `crates/algraf-render` are training-time structures, not exported.
- The v0.30 release landed static interaction affordances and an opt-in
  interactive SVG runtime, but no stable host sidecar contract. v0.32 fills
  that integration gap.
- The "Browser/WASM playground groundwork" Should item from v0.24/v0.30 has
  no concrete consumer in tree; without a reference host runtime the metadata
  contract is theory.
- The URL-valued property policy was settled deny-only in v0.30 (spec §29).
  A host runtime that supports tooltip hyperlinks or image hrefs is the
  natural place to revisit, with a real consumer to gate against.

## Scope Rules

- Algraf ships *data*, not UX. The host runtime owns hover visuals, tooltip
  styling, crosshair rendering, selection state, and animation.
- The sidecar contract is the carrier for new host metadata. Static SVG remains
  script-free by default; the existing `--interactive` SVG runtime stays
  explicit opt-in.
- The draw-list and the sidecar carry *the same JSON shape* for interaction
  metadata. One contract, two carriers.
- The reference React component is reference material — it exercises the
  contract end-to-end and is the worked example for other hosts. It is not a
  required runtime for SVG consumers.
- Algraf emits no inline `<script>` unless `--interactive` is requested. SVG
  with no sidecar remains exactly the static artifact.
- Output stays deterministic: sidecar key ordering is stable;
  locale-independent number/time formatting (spec §18.12, §19.4) carries
  into sidecar formatting.
- URL-valued properties remain denied in source unless this release ships a
  host-gated policy (see Must §6).

## Capstone Acceptance Target

The capstone is the v0.30 scatter chart rendered with the new sidecar, plus a
reference React component that consumes it and implements three behaviors:
mark-hover tooltip, crosshair with axis value readout, and legend hover
highlight.

```ag
Chart(data: "penguins.csv", width: 760, height: 520) {
    Space(flipper_length * body_mass) {
        Point(
            fill: species,
            tooltip: [species, flipper_length, body_mass],
            highlight: "species"
        )
    }
}
```

```bash
algraf render chart.ag --output /tmp/chart.svg --metadata /tmp/chart.meta.json
# or equivalently
algraf render chart.ag --format svg+json --output /tmp/chart      # writes both
algraf render chart.ag --format draw-list --output /tmp/scene.json
```

The sidecar shape (illustrative; finalized in spec §24.6):

```jsonc
{
  "version": 1,
  "plot_rect": { "x": 60, "y": 24, "width": 680, "height": 440 },
  "axes": {
    "x": { "scale": "linear", "domain": [170, 232], "range": [60, 740],
           "format": "%.0f mm", "label": "flipper_length" },
    "y": { "scale": "linear", "domain": [2700, 6300], "range": [464, 24],
           "format": "%.0f g",  "label": "body_mass" }
  },
  "marks": [
    { "id": "g0:42", "x_px": 412, "y_px": 318,
      "groups": { "species": "Adelie" },
      "tooltip": [{ "label": "flipper_length", "value": "195" }] }
  ],
  "groups": { "species": ["Adelie","Chinstrap","Gentoo"] }
}
```

A reference React component (`@algraf/react` or in-tree equivalent) renders
the SVG inline, overlays an absolute event layer sized to `plot_rect`, picks
nearest mark by `x_px`/`y_px`, draws a vertical guideline that reads its
value from the inverted `axes.x` scale, and toggles group highlight CSS on
mark/legend hover.

The release must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

Static SVG examples (no sidecar requested) regenerate without drift.

## Design Decisions (settled in advance of implementation)

1. **Hosts own UX.** Algraf does not ship a runtime script, a CSS theme for
   tooltips, or selection state. The sidecar is the boundary.
2. **One contract, two carriers.** The sidecar JSON shape and the draw-list
   `interactions` block carry the same data, versioned together.
3. **Scales serialize as data, not closures.** The sidecar describes scale
   *parameters* (kind, domain, range, format string); the host implements
   inversion using a documented algorithm per scale kind. No host-side WASM
   dependency.
4. **The reference component is reference material.** It exists to exercise
   the contract end-to-end and document the integration pattern. It is not
   required for SVG consumption.

## v0.32.0 Must

### 1. SVG + JSON sidecar emission

Status: Implemented.

Acceptance criteria:

- Add a `--metadata <path>` flag (or equivalent `--format svg+json`) to
  `algraf render` that emits the sidecar JSON alongside the SVG.
- Sidecar contents and key ordering are deterministic; numeric/time
  formatting is locale-independent.
- Without the flag, SVG output is byte-identical to v0.30 for non-interaction
  charts; charts with `tooltip:`/`highlight:` still render their v0.30 static
  affordances (`<title>`, `data-*` groups).

### 2. Plot rect and invertible scale serialization

Status: Implemented.

Acceptance criteria:

- The sidecar carries `plot_rect` (the inner plot area in SVG pixel space)
  and per-axis scale metadata sufficient for host-side inversion: scale
  kind, domain, range, and format string at minimum.
- Inversion algorithms per scale kind (linear, log, time, ordinal/band) are
  documented in spec §24.6 so hosts can implement them without consulting
  Algraf internals.
- The reference React component invertibly maps a mouse-x within `plot_rect`
  to a formatted axis-x value for every supported scale kind.

### 3. Per-mark pixel positions in sidecar

Status: Implemented.

Acceptance criteria:

- The sidecar's `marks[]` entries carry `x_px`/`y_px` (or a richer
  shape-specific descriptor for non-point marks) so hosts can pick nearest
  marks without re-running layout.
- Mark IDs are deterministic sidecar IDs (`p{panel}:g{geometry}:r{row}`) and
  are stable for the same source/data/layout.
- Group keys and tooltip rows are formatted using the same locale-independent
  rules as elsewhere in the renderer.

### 4. Draw-list parity with sidecar

Status: Implemented.

Acceptance criteria:

- The draw-list backend's `interactions` block carries the same JSON shape as
  the sidecar (modulo any draw-list-only fields), versioned together with one
  shared schema.
- A snapshot test confirms the two carriers produce equivalent interaction
  metadata for the capstone chart.

### 5. Reference host runtime

Status: Implemented.

Acceptance criteria:

- Ship a reference component (React in `editors/` or a sibling package; a
  vanilla-JS reference is acceptable if React tooling adds too much scope)
  that consumes the sidecar and implements: mark-hover tooltip, crosshair
  with axis value readout, and legend hover highlight.
- The component is documented in the README as the worked integration
  pattern; other host integrations (Vue, Canvas) reference its source.
- Determinism: focused renderer tests cover the sidecar and draw-list parity;
  the reference React package type-checks against the published sidecar shape.

### 6. URL-valued property policy (revisit)

Status: Implemented.

Acceptance criteria:

- Revisit the v0.30 deny-only design note now that a host runtime exists.
  Decide whether URL-valued properties (hyperlinks, image hrefs in tooltip
  rows) ship in v0.32 gated behind an explicit host/CLI policy with deny
  default, or remain rejected.
- If shipping: specify the surface, the policy hook, and how SVG injection
  rules (spec §29.3) apply to URL values in the sidecar.
- The policy remains deny-only in v0.32; spec §29 points any future support at
  an explicit host/CLI policy.

### 7. Interactive LSP preview

Status: Implemented.

Acceptance criteria:

- Extend the `algraf/preview` surface (spec §21.18) so VS Code returns the same
  sidecar JSON alongside the SVG. Static preview is unchanged and remains the
  default.
- The interactive preview path remains the existing explicit opt-in SVG runtime;
  the sidecar is returned for hosts that choose to drive their own overlay.
- A document with no interaction metadata previews exactly as today (static
  `<img src="data:…" />` path).

### 8. Examples, README, spec, and release hygiene

Status: Implemented.

Acceptance criteria:

- Add at least one example whose generation produces both an SVG and a
  sidecar; the reference component's integration test exercises the sidecar
  end-to-end.
- README gains a "Embedding in a host runtime" section after the
  output-modes tutorial.
- Spec updates cover §3 (interactivity is supported via the host-runtime
  contract), §18 (no change to static SVG; sidecar is a sibling artifact),
  §21.18 (opt-in interactive preview), §24.6 (sidecar shape, scale
  serialization, draw-list parity), §29 (URL policy revisit), and §30 if a
  feature gate is used.
- Workspace `Cargo.toml` and `editors/vscode/package.json` are bumped to
  `0.32.0`; the reference component package (if separately published) is
  versioned in lockstep.

## v0.32.0 Should

### Selection / brushing in the reference runtime

Status: Implemented.

The reference React component implements this over the sidecar without adding
source syntax: legend labels can be clicked to persist a group selection, and
dragging a rectangle over the plot produces a brush selection from `marks[]`
`x_px`/`y_px` coordinates. Selection state is host-owned through component props
and `onSelectionChange`; Algraf only provides the data shape. The vanilla host
demo mirrors the same behavior for dependency-free consumers.

### Animated SVG / transitions

Status: Deferred by design.

Carry forward the v0.30 animated-SVG design item. If declarative,
deterministic, snapshot-testable transitions can be expressed without
turning rendering into code execution, ship them; otherwise keep design-only.
The v0.32 audit keeps this design-only: the sidecar gives hosts enough data to
animate overlays, but Algraf still lacks a retained update identity model and a
snapshot strategy for renderer-authored transitions. No source syntax or runtime
animation ships in v0.32.

### WASM rendering path

Status: Implemented as design sketch.

If the host story is mature, sketch (do not require) a WASM-compiled Algraf
renderer that produces sidecar + draw list in-browser, completing the
v0.19/v0.24 WASM line. The sketch is recorded in
[`WEBGL_FEASIBILITY.md`](WEBGL_FEASIBILITY.md#v032-hostwasm-rendering-sketch)
and the required packaging/runtime work is sequenced into
[`V0_34_PLAN.md`](V0_34_PLAN.md).

## Explicitly Deferred Past v0.32.0

- Algraf-shipped UX policy (theme tokens for tooltip styling, cursor
  affordances). Hosts own this.
- Arbitrary JavaScript or user-authored event handlers in source.
- Network-backed interactions, fetches, or live data.
- Cross-chart linked brushing beyond a single chart document (unless the
  reference runtime makes it trivial; design-only otherwise).
- Required WASM/browser product.

## Optional-Item Audit

### Promote In v0.32.0 (Must)

- SVG + JSON sidecar emission.
- Plot rect and invertible scale serialization.
- Per-mark pixel positions in sidecar.
- Draw-list parity with sidecar.
- Reference host runtime.
- URL-valued property policy revisit.
- Interactive LSP preview.
- Examples, README, spec, and release hygiene.

### Consider If Capacity Allows (Should)

- Selection / brushing in the reference runtime.
- Animated SVG / transitions.
- WASM rendering path design sketch.

### Keep Deferred

- Algraf-shipped UX policy, arbitrary scripting, network interactions,
  multi-chart linked brushing, required WASM/browser product.

## Promotion Workflow

1. Finalize the sidecar schema and scale-serialization algorithms in spec
   §24.6 before coding.
2. Add the `--metadata` / `--format svg+json` CLI surface and wire the
   sidecar emitter from the render scene.
3. Add per-mark pixel positions and plot rect to the scene's interaction
   metadata; reuse for both sidecar and draw list.
4. Bring the draw-list `interactions` block to parity with the sidecar
   (shared shape, shared snapshot tests).
5. Implement the reference host runtime against the sidecar; add the
   integration test on the capstone chart.
6. Revisit the URL-valued property policy and either ship a gated surface
   or extend the v0.30 deny-only design note.
7. Extend the LSP preview to an opt-in interactive surface reusing the
   reference runtime; update the VS Code client wiring and CSP.
8. Add examples, README, LSP metadata; bump versions; confirm static SVG
   has no drift and the sidecar snapshot is stable.
