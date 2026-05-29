# Algraf v0.30.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_29_PLAN.md`](V0_29_PLAN.md)
Follow-on plan: [`V0_31_PLAN.md`](V0_31_PLAN.md)

## Purpose

This document defines the intended v0.30.0 release shape: the interactivity half
of the output-backends work that [`V0_24_PLAN.md`](V0_24_PLAN.md) carried
forward. v0.24 deferred both the interaction metadata model (item 4) and the
interactive preview path (item 5), noting that the backend contract was the
foundation they would build on. [`V0_29_PLAN.md`](V0_29_PLAN.md) completes that
foundation by giving every mark a draw-list primitive and stable identity.

This release defines a safe, declarative model for tooltips, highlights, and
selections, emits that metadata from the render scene through both backends, and
makes the LSP preview interactive while staying script-safe by default.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.30.0 is an **interactivity** release. Algraf's value is deterministic,
declarative charts; interactivity must not turn rendering into arbitrary code
execution. The decision is that interactions are *data attached to marks*, not
event-handler source. A chart declares which fields appear in a tooltip and how
hover/selection should highlight related marks; the renderer attaches that as
inert metadata, and a viewer (browser host or LSP preview) interprets it.

SVG output stays script-free by default. Interactive SVG (with a small, audited,
non-user-authored script) is an explicit opt-in. The draw list carries the same
metadata as inert data so a Canvas/WebGL/raster host can implement interaction
itself.

## Current Debt Surface

The plan/spec/code audit found:

- [`V0_24_PLAN.md`](V0_24_PLAN.md) items 4 and 5 (interaction metadata model and
  interactive preview path) are deferred and not started. v0.24 notes
  interaction metadata "would ride on the render scene and be emitted by both
  backends."
- Spec §3 still lists runtime interactivity among the things Algraf "does not
  initially support," and §29.1 keeps interactive output disabled "unless a
  later version defines and tests explicit opt-in surfaces."
- The LSP preview (spec §21.18) is read-only inline SVG and "MUST NOT execute
  scripts in the preview surface." It has no hover/tooltip affordance.
- SVG accessibility output exists (spec §18.10) but there is no per-mark
  `<title>`/tooltip surface and no way to declare which data a tooltip shows.
- [`V0_24_PLAN.md`](V0_24_PLAN.md)'s "URL-valued property policy" Should item is
  still a design gap: there is no decision on whether images, hyperlinks, or
  tooltip URLs are ever allowed, or how they interact with SVG injection
  (spec §29.3) and previews.
- After v0.29, marks carry a stable identity in the draw list, which interaction
  metadata can reference; before that, there was nothing to attach to.

## Scope Rules

- Interactions are declarative data/mark metadata, never executable source.
- SVG output remains script-free unless the chart (or invocation) explicitly
  opts into interactive output.
- Any opt-in interactive script is a fixed, audited, non-user-authored runtime
  shipped by Algraf; the chart cannot inject arbitrary JS or event handlers.
- The LSP preview stays script-safe by default; interactive preview is opt-in
  and uses a vetted runtime, never user script.
- Interaction metadata is emitted by both the SVG and draw-list backends from
  the same scene (spec §24.6).
- No network access from interactions. URL-valued properties, if allowed at all,
  are policy-gated and default off (spec §29).
- Output stays deterministic: metadata ordering is stable and locale-independent.

## Capstone Acceptance Target

The capstone is a scatter plot whose points carry declarative tooltips and whose
fill legend drives hover highlighting, rendered as (a) static script-free SVG
with accessible `<title>` tooltips, (b) opt-in interactive SVG, and (c) a draw
list carrying the same interaction metadata as inert data:

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
algraf render chart.ag --interactive --output /tmp/interactive.svg
algraf render chart.ag --format draw-list --output /tmp/scene.json
```

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
   `tooltip:` / `highlight:` / `select:` declares *what* data participates;
   there is no event-handler syntax and no scripting language.
2. **Static is the default; interactive is opt-in.** Absent interaction opt-in,
   SVG stays script-free. Declarative tooltips degrade to accessible `<title>`
   elements (spec §18.10) in static SVG.
3. **One vetted runtime, never user code.** Interactive SVG embeds a small,
   fixed, audited script that reads the inert metadata; the chart never supplies
   script text. This satisfies the §29.3 SVG-injection rules.
4. **Metadata rides the scene.** Interaction metadata is attached to marks during
   planning and emitted by both backends, reusing the v0.29 per-mark identity.
5. **URLs are policy-gated and off by default.** Any URL-valued property
   (hyperlink, image, tooltip link) requires an explicit host/CLI policy, and
   the default denies it (spec §29).

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

### 2. Per-mark interaction emission through both backends

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
- The SVG backend emits accessible static affordances by default: per-mark
  `<title>` tooltips and stable group/class identity for highlight keys, with no
  script.
- The draw-list backend records the same interaction metadata as inert data.
- Metadata ordering is deterministic and locale-independent.
- Charts with no interaction properties produce byte-for-byte unchanged SVG.

### 3. Opt-in interactive SVG runtime

Status: Done. CLI `--interactive` (and `render_interactive*` in the render
crate) embed a single fixed, audited script that reads the inert metadata and
implements tooltip- and highlight-on-hover (spec §29.3). Chart source cannot
supply or extend it; without the flag the SVG is script-free. The interactive
chart body is byte-for-byte identical to the static render up to the appended
script (covered by `render.rs` and `cli.rs`).

Acceptance criteria:

- Add an explicit opt-in (CLI `--interactive` and/or a source/`Chart` option,
  with one documented precedence) that embeds a small, fixed, audited script
  implementing tooltip-on-hover and highlight behavior from the inert metadata.
- The embedded script is shipped by Algraf and identical across charts; chart
  source can never supply or extend it (spec §29.3).
- Without the opt-in, SVG is script-free.
- The interactive script is deterministic given the same metadata and degrades
  gracefully where unsupported.

### 4. Interactive LSP preview path

Status: Done. `algraf/preview` accepts an optional `interactive` param (default
`false`, spec §21.18). When set, the server renders with the audited runtime;
otherwise the preview is script-free, preserving the read-only, superseded, and
generation semantics. The VS Code client gains an `algraf.preview.interactive`
setting that inlines the SVG and relaxes CSP only in that mode; all language
logic stays in `algraf-lsp`.

Acceptance criteria:

- Extend the `algraf/preview` surface (spec §21.18) so the preview can show
  tooltips and highlights using a vetted, non-user runtime, while remaining
  script-safe by default and preserving the existing read-only, superseded, and
  generation semantics.
- The preview never executes user-authored script; interaction comes only from
  the audited runtime over inert metadata.
- A document with no interaction metadata previews exactly as today.
- Update the VS Code client wiring only as needed to enable the interactive
  preview surface; all language logic stays in `algraf-lsp`.

### 5. URL-valued property policy

Status: Done (rejected). URL-valued properties (hyperlinks, image hrefs, tooltip
links) are **not supported in v0.30** and are rejected rather than embedded; the
rationale (SVG-injection/exfiltration surface vs. the no-network and
script-free-by-default rules) is recorded in spec §29.3, which also states the
default-denied, explicit-policy requirement for any future support.

Acceptance criteria:

- Decide and specify whether URL-valued properties (hyperlinks, image hrefs,
  tooltip links) are ever allowed.
- If allowed, they are gated by an explicit host/CLI policy and denied by
  default; specify how they interact with SVG injection (spec §29.3), previews,
  and the no-network rule (spec §29).
- If not allowed in v0.30, record the rejection as a design note in the spec and
  this plan.

### 6. Diagnostics, LSP, and editor metadata

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

### 7. Examples and README

Status: Done. `examples/tooltips.ag` (declarative tooltips) and
`examples/highlight.ag` (highlight-on-hover, static + `--interactive`) are added
and wired into `generate.sh`; regeneration leaves all existing examples drift
free. README gains "Declarative tooltips" and "Highlight-on-hover" tutorial
sections and documents `--interactive` under Output modes.

Acceptance criteria:

- Add at least one example demonstrating declarative tooltips and one
  demonstrating highlight-on-hover, with both static and interactive output.
- Regenerate artifacts with `./examples/generate.sh`; static examples without
  interaction must not drift.
- Add README sections in the appropriate tutorial position (after theming /
  output modes).

### 8. Spec, plan, and release hygiene

Status: Done. Spec updates cover §2/§3 (interactivity → opt-in supported), §14.25
(interaction properties), §18.10 (static affordances), §21.18 (interactive
preview), §24.6 (interaction metadata on both backends), §26 (`E1206`/`E1207`),
§29.3 (opt-in script + URL policy + no network), and §22.3 (`--interactive`).
Workspace `Cargo.toml` and `editors/vscode/package.json` are at `0.30.0`; README,
examples, and rendered artifacts are synchronized.

Acceptance criteria:

- Spec updates cover §3 (interactivity moves from non-goal to opt-in supported),
  §14 (interaction properties on geometries), §18 (static affordances), §21.18
  (interactive preview), §24.6 (interaction metadata on the scene/both
  backends), §26 (diagnostics), §29 (security: opt-in script, URL policy,
  no network), and §30 if a feature gate is used.
- Workspace `Cargo.toml` and `editors/vscode/package.json` are bumped to
  `0.30.0` when the release branch is ready.
- README, examples, and rendered artifacts stay synchronized.

## v0.30.0 Should

### Selection / brushing model

Status: Deferred. Not taken up in v0.30.0; remains a candidate for a later release (kept declarative and metadata-driven if implemented).

Design (and implement only if it stays declarative and safe) a selection or
brushing affordance — e.g. clicking a legend entry filters or emphasizes a
series. Keep it metadata-driven; no user scripting.

### Animated SVG design

Status: Deferred. Design carried forward; no animation runtime shipped.

Carry forward the v0.24 animated-SVG design item: decide whether enter/update
transitions can be expressed declaratively, kept deterministic, script-safe, and
snapshot-testable. Do not implement animation unless those hold.

### Browser/WASM playground groundwork

Status: Deferred. No WASM runtime added; the draw list plus inert interaction metadata remain the consumption surface a future playground would build on.

Tie the v0.19 WASM audit, the v0.24 backend work, and this release's interaction
metadata into a concrete browser-playground design that consumes the draw list
plus interaction metadata. Do not require a WASM runtime for this release.

## Explicitly Deferred Past v0.30.0

- Arbitrary JavaScript or user-authored event handlers.
- Network-backed interactions, fetches, or live data.
- Cross-chart linked brushing beyond a single chart document.
- Animation runtime (design only here).
- Required WASM/browser product.

## Optional-Item Audit

### Promote In v0.30.0 (Must)

- Interaction metadata model.
- Per-mark interaction emission through both backends.
- Opt-in interactive SVG runtime.
- Interactive LSP preview path.
- URL-valued property policy.
- Diagnostics, LSP, and editor metadata.
- Examples and README.
- Spec, plan, and release hygiene.

### Consider If Capacity Allows (Should)

- Selection / brushing model.
- Animated SVG design.
- Browser/WASM playground groundwork.

### Keep Deferred

- Arbitrary scripting, network interactions, cross-chart brushing, animation
  runtime, and browser product work.

## Promotion Workflow

1. Reserve interaction diagnostics in spec §26 and specify the metadata model
   before coding.
2. Add source syntax + IR for declarative interactions; validate columns and
   placement.
3. Attach interaction metadata to per-mark scene primitives (v0.29 identity).
4. Emit static `<title>`/group affordances (SVG) and inert metadata (draw list).
5. Add the opt-in interactive SVG runtime with a fixed audited script.
6. Extend the LSP preview to an opt-in interactive, script-safe surface.
7. Decide and document the URL-valued property policy.
8. Add examples, README, LSP metadata; bump versions; confirm static SVG has no
   drift.
