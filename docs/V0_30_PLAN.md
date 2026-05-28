# Algraf v0.30.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_29_PLAN.md`](V0_29_PLAN.md)
Follow-on plan: [`V0_31_PLAN.md`](V0_31_PLAN.md) (language-surface polish); the
thematic follow-on for the host-runtime / interactive story is
[`V0_32_PLAN.md`](V0_32_PLAN.md).

## Purpose

This document defines the intended v0.30.0 release shape: the *foundation* half
of the interactivity work that [`V0_24_PLAN.md`](V0_24_PLAN.md) carried forward.
v0.24 deferred both the interaction metadata model (item 4) and the interactive
preview path (item 5), noting that the backend contract was the foundation they
would build on. [`V0_29_PLAN.md`](V0_29_PLAN.md) completes that foundation by
giving every mark a draw-list primitive and stable identity.

This release defines a safe, declarative model for tooltips and highlights,
emits that metadata from the render scene through both backends as inert data
and accessible static SVG, and reserves diagnostics — but ships **no embedded
runtime, no interactive SVG, and no interactive preview**. Those land in
[`V0_32_PLAN.md`](V0_32_PLAN.md) as part of a host-runtime contract that lets
real applications (React, plain JS, Canvas/WebGL hosts) drive interactivity
themselves over a documented data surface.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.30.0 is a **declarative interaction metadata** release. Algraf's value is
deterministic, declarative charts; interactivity must not turn rendering into
arbitrary code execution. The decision is that interactions are *data attached
to marks*, not event-handler source. A chart declares which fields appear in a
tooltip and which key groups marks for highlight emphasis; the renderer
attaches that as inert metadata.

In v0.30 the SVG backend uses that metadata to emit accessible static
affordances — per-mark `<g>` groups with `data-*` attributes and `<title>`
children — that work in any browser with zero JavaScript via native tooltips
and CSS `:hover`. The draw-list backend records the same metadata as inert
data alongside ops. v0.32 will pick the metadata up and turn it into a
host-runtime contract (SVG + JSON sidecar, invertible scale serialization,
reference React component); v0.30 does not ship any runtime.

## Current Debt Surface

The plan/spec/code audit found:

- [`V0_24_PLAN.md`](V0_24_PLAN.md) item 4 (interaction metadata model) is
  deferred and not started. v0.24 notes interaction metadata "would ride on
  the render scene and be emitted by both backends." Item 5 (interactive
  preview path) is also deferred and now waits on v0.32's host runtime.
- Spec §3 still lists runtime interactivity among the things Algraf "does not
  initially support," and §29.1 keeps interactive output disabled "unless a
  later version defines and tests explicit opt-in surfaces."
- SVG accessibility output exists (spec §18.10) but there is no per-mark
  `<title>`/tooltip surface and no way to declare which data a tooltip shows.
- [`V0_24_PLAN.md`](V0_24_PLAN.md)'s "URL-valued property policy" Should item
  is still a design gap: there is no decision on whether images, hyperlinks,
  or tooltip URLs are ever allowed, or how they interact with SVG injection
  (spec §29.3) and previews.
- After v0.29, marks carry a stable identity in the draw list, which
  interaction metadata can reference; before that, there was nothing to
  attach to.

## Scope Rules

- Interactions are declarative data/mark metadata, never executable source.
- SVG output remains script-free in v0.30; no embedded runtime ships.
- Interaction metadata is emitted by both the SVG and draw-list backends from
  the same scene (spec §24.6).
- No URL-valued properties in v0.30 (deny-only design note — see §3 below).
  No network access from interactions.
- Output stays deterministic: metadata ordering is stable and
  locale-independent. Charts with no interaction properties produce
  byte-for-byte unchanged SVG.

## Capstone Acceptance Target

The capstone is a scatter plot whose points carry declarative tooltips and
whose fill legend groups for highlight, rendered as (a) a static script-free
SVG with accessible `<title>` tooltips and stable per-mark `data-*` identity,
and (b) a draw list carrying the same interaction metadata as inert data:

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
algraf render chart.ag --output /tmp/static.svg                 # script-free, <title> tooltips
algraf render chart.ag --format draw-list --output /tmp/scene.json
```

Hovering a point in any browser shows the native tooltip from the `<title>`
child; CSS `:hover` on `[data-group-species]` can dim siblings without script.
Anything richer (custom tooltip styling, crosshair readouts, click-to-pin)
waits for v0.32's host-runtime contract.

The release must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

Static SVG examples without interaction properties regenerate without drift.

## Design Decisions (settled)

1. **Interactions are declarative metadata.** A geometry property such as
   `tooltip:` / `highlight:` declares *what* data participates; there is no
   event-handler syntax and no scripting language.
2. **Static-only this release.** v0.30 emits accessible `<title>` tooltips and
   per-mark `data-*` identity on `<g>` groups. CSS `:hover` is the only
   browser-side affordance. No embedded `<script>`, no interactive preview.
   The interactive runtime story is deferred wholesale to
   [`V0_32_PLAN.md`](V0_32_PLAN.md) where it becomes a *host-runtime contract*
   (SVG + JSON sidecar consumed by hosts like React) rather than a
   self-contained Algraf-shipped script.
3. **Metadata rides the scene.** Interaction metadata is attached to marks
   during planning and emitted by both backends, reusing the v0.29 per-mark
   identity.
4. **URLs are denied in v0.30.** No URL-valued properties ship this release
   (no hyperlinks, no image hrefs, no tooltip links). Spec §29 records the
   rejection as a design note pointing at v0.32 as the natural place to
   revisit alongside host-runtime work.

## v0.30.0 Must

### 1. Interaction metadata model (source + IR)

Status: Done. `tooltip:` (a column or array of columns) and `highlight:` (a
grouping column) lower into `InteractionIr` on `GeometryIr` (spec §14.25). They
ride the geometry IR without touching scale training or layout, are inert data,
and are validated from schema alone. Analysis validates that referenced columns
exist (`E1101`) and that the properties appear only on supported geometries.

Acceptance criteria:

- Define source syntax for declarative interactions on geometries — at minimum
  `tooltip:` (a column, or an array of columns/labels), and a highlight/grouping
  key for hover emphasis.
- Define how interactions lower into the semantic IR and ride on the geometry IR
  without affecting scale training or layout.
- Interactions are inert data: no callbacks, expressions, or scripts.
- Semantic analysis validates that referenced columns exist and that interaction
  properties are only used where supported; reserve targeted diagnostics.
- Schema-only analysis can validate interaction metadata without materializing
  data rows (spec §24.6).

### 2. Per-mark static affordances through both backends

Status: Done. Interaction metadata rides the shared mark sink
(`begin_mark`/`end_mark`). The SVG backend emits accessible per-mark `<title>`
tooltips and a stable `data-algraf-highlight` group attribute with no script;
the draw-list backend records the same metadata as an inert `interaction` object
on the shape op. Ordering is deterministic and locale-independent, and a chart
with no interaction properties produces byte-for-byte unchanged SVG (covered by
`render.rs` and `draw_list.rs` tests). Supported on `Point`, `Bar`, `Rect`, and
`Tile`.

Acceptance criteria:

- The render scene attaches interaction metadata to per-mark primitives using
  the v0.29 mark identity.
- The SVG backend wraps each mark in a stable `<g class="algraf-mark"
  data-mark-id="…" data-group-…="…">` group and emits a `<title>` child built
  from the tooltip columns, with no script.
- The draw-list backend records the same interaction metadata in a top-level
  inert block alongside ops (e.g. `interactions: { marks, groups }`).
- Metadata ordering is deterministic and locale-independent; numeric and
  temporal formatting follow the existing locale-independent rules.
- Charts with no interaction properties produce byte-for-byte unchanged SVG.

### 3. URL-valued property policy

Status: Done (rejected). URL-valued properties (hyperlinks, image hrefs, tooltip
links) are **not supported in v0.30** and are rejected rather than embedded; the
rationale is recorded in spec §29 as a deny-only design note pointing at v0.32
for a future revisit alongside host-runtime work.

Acceptance criteria:

- Spec §29 is updated to formally reject URL-valued properties in v0.30: no
  hyperlinks, no image hrefs, no tooltip links, no `--allow-urls` flag, no
  host policy hook.
- The rejection is recorded as a design note that points at
  [`V0_32_PLAN.md`](V0_32_PLAN.md) as the natural place to revisit alongside
  the host-runtime work, where there is finally a consumer that could act on
  URLs.
- No URL-valued property surface exists in source: any attempt to use one
  produces a targeted diagnostic.

### 4. Diagnostics, LSP, and editor metadata

Status: Done. `E1206` (interaction property on an unsupported geometry) and
`E1207` (invalid interaction value) are reserved in spec §26 and implemented;
unknown columns reuse `E1101`. LSP completion, hover, and signature help surface
`tooltip`/`highlight` (with value-shape docs) on supported geometries, and the
VS Code TextMate grammar recognizes both keywords.

Acceptance criteria:

- Reserve and implement diagnostics for unknown/misused interaction properties,
  interaction columns that do not exist, and interaction properties on
  geometries that do not support them (finalized in spec §26 before
  implementation).
- LSP completion/hover/signature help surface `tooltip`, `highlight`, and any
  other interaction properties and their accepted value shapes.
- Semantic tokens and the VS Code TextMate grammar are updated if new property
  keywords become source-visible.

### 5. Examples and README

Status: Done. `examples/tooltips.ag` (declarative tooltips) and
`examples/highlight.ag` (highlight grouping via static `data-*` affordances) are
added and wired into `generate.sh`; regeneration leaves all existing examples
drift free. README gains "Declarative tooltips" and "Highlight-on-hover"
tutorial sections framing the metadata foundation for v0.32 host runtimes.

Acceptance criteria:

- Add at least one example demonstrating declarative tooltips and one
  demonstrating a highlight grouping key, both rendering to static accessible
  SVG (the static `<title>` and `data-*` surface).
- Regenerate artifacts with `./examples/generate.sh`; static examples without
  interaction must not drift.
- Add README sections in the appropriate tutorial position (after theming /
  output modes), framing the interaction metadata as the data foundation that
  v0.32 host runtimes will build on.

### 6. Spec, plan, and release hygiene

Status: Done. Spec updates cover §2/§3 (declarative metadata supported, runtime
deferred to v0.32), §14.25 (interaction properties), §18.10 (static
affordances), §24.6 (interaction metadata on both backends), §26
(`E1206`/`E1207`), and §29 (URL deny-only design note). Workspace `Cargo.toml`
and `editors/vscode/package.json` are at `0.30.0`; README, examples, and
rendered artifacts are synchronized.

Acceptance criteria:

- Spec updates cover §3 (interactivity moves from non-goal to "declarative
  metadata supported, runtime deferred to v0.32"), §14 (interaction
  properties on geometries), §18 (static `<title>` and `data-*` affordances),
  §24.6 (interaction metadata on the scene/both backends), §26 (diagnostics),
  and §29 (URL deny-only design note).
- Workspace `Cargo.toml` and `editors/vscode/package.json` are bumped to
  `0.30.0` when the release branch is ready.
- README, examples, and rendered artifacts stay synchronized.

## v0.30.0 Should

### Selection / brushing model (design only)

Status: Deferred. Not taken up in v0.30.0; remains a candidate for a later release (kept declarative and metadata-driven if implemented).

Sketch a declarative selection/brushing affordance — e.g. a `select:` key that
declares which mapping legends drive cross-mark emphasis — without
implementing browser-side selection state. Implementation lands with the host
runtime in [`V0_32_PLAN.md`](V0_32_PLAN.md), but the source/IR surface can be
designed in v0.30 if it stays declarative and safe.

### Animated SVG design

Status: Deferred. Design carried forward; no animation runtime shipped.

Carry forward the v0.24 animated-SVG design item: decide whether enter/update
transitions can be expressed declaratively, kept deterministic, script-safe,
and snapshot-testable. Do not implement animation. If a workable design lands,
it likely ships alongside the host runtime in v0.32.

## Explicitly Deferred Past v0.30.0

- Embedded interactive SVG runtime (any inline `<script>` shipped by Algraf
  inside SVG output). Replaced by v0.32's host-runtime contract.
- Interactive LSP preview. Returns in v0.32 reusing the host runtime.
- Host-runtime contract itself: SVG + JSON sidecar, invertible scale
  serialization, per-mark pixel positions, reference React component. All in
  [`V0_32_PLAN.md`](V0_32_PLAN.md).
- Arbitrary JavaScript or user-authored event handlers.
- Network-backed interactions, fetches, or live data.
- Cross-chart linked brushing beyond a single chart document.
- Animation runtime (design only here).
- Required WASM/browser product.

## Optional-Item Audit

### Promote In v0.30.0 (Must)

- Interaction metadata model.
- Per-mark static affordances through both backends.
- URL-valued property policy (deny-only).
- Diagnostics, LSP, and editor metadata.
- Examples and README.
- Spec, plan, and release hygiene.

### Consider If Capacity Allows (Should)

- Selection / brushing model (design only).
- Animated SVG design.

### Keep Deferred

- Embedded interactive SVG runtime, interactive LSP preview, host-runtime
  contract (all in v0.32).
- Arbitrary scripting, network interactions, cross-chart brushing, animation
  runtime, and browser product work.

## Promotion Workflow

1. Reserve interaction diagnostics in spec §26 and specify the metadata model
   before coding.
2. Add source syntax + IR for declarative interactions; validate columns and
   placement.
3. Attach interaction metadata to per-mark scene primitives (v0.29 identity).
4. Emit static `<g>`/`<title>`/`data-*` affordances (SVG) and inert metadata
   (draw list).
5. Reject URL-valued properties in source; record the design note in spec §29.
6. Add examples, README, LSP metadata; bump versions; confirm static SVG has no
   drift.
