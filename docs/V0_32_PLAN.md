# Algraf v0.32.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_31_PLAN.md`](V0_31_PLAN.md)

## Purpose

This document defines the intended v0.32.0 release shape: resolving the
long-reserved language items that the grammar has held in "reserved for later
versions" limbo since v0.1 — **nested `Space` blocks** and **space-local
annotation declarations** (spec §4.2).

These were originally bundled into the v0.31 language-surface-polish release but
were pulled out (see [`V0_31_PLAN.md`](V0_31_PLAN.md)) because, unlike the
independent polish items there, they are a single coherent grammar-and-scope
subsystem: they change how a `Space` block is parsed, what it may contain, and
how scope/inheritance flows through the chart tree. That deserves a release of
its own.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.32.0 is a **scope-and-composition** release. Its single job is to take the two
reserved §4.2 items out of indefinite limbo: each ships with a tested, specified
design **or** is formally rejected with a recorded rationale that replaces the
open-ended "reserved for later versions" language with a concrete decision. No
item stays "reserved."

The guiding constraint is that composition must not fork the model. Algraf
already has one well-defined way to subdivide a plane — algebraic faceting via
the nesting operator (`a / b`, spec §8, §4.2) — and already supports space-local
`Scale`, `Guide`, and `Theme` declarations on a `SpaceIr`. This release decides
what *block* nesting and *annotation* declarations add on top of those, in a way
that reuses the existing frame, scale-training, and layout machinery rather than
introducing a parallel one.

## Current Debt Surface

The plan/spec/code audit found:

- Spec §4.2 says "Nested spaces are reserved for later versions" and "The first
  implementation SHOULD reject nested `Space` blocks with a diagnostic." The
  parser/analyzer reject a `Space` declared inside another `Space`; there is no
  defined meaning for it.
- Spec §4.2 also says "A space MAY own local scale, guide, or annotation
  declarations in later versions." Of these, **space-local scales and guides are
  already implemented** (`SpaceIr.scales`, `SpaceIr.guides`), as is a space-local
  `Theme`. The genuinely-undefined remainder is the **annotation** declaration:
  there is no scoped, reusable annotation construct distinct from placing
  `HLine`/`VLine`/`Text`/`Rect` geometries directly in a space (which already
  works).
- Faceting (spec §8.3, "nested spaces represent facets when applied to a whole
  Cartesian plane") already provides one nesting semantics via algebra. Any
  block-level nesting must be clearly distinguished from faceting to avoid two
  overlapping mechanisms.
- The §4.2 reservation language is the last "reserved for later versions" clause
  of its kind in the core grammar; leaving it open indefinitely is the debt this
  release clears.

## Scope Rules

- The decision is binary per item: ship a tested/specified design, or formally
  reject and document. No "reserved" status survives this release.
- Block nesting, if shipped, MUST reuse the existing algebraic frame, scale
  training, and layout machinery; it MUST NOT introduce a second faceting model
  or a parallel scale engine.
- Faceting via algebra (`a / b`) stays the canonical way to subdivide a plane;
  any block nesting is additive and must be unambiguously distinguished from it.
- Scope/inheritance MUST be explicit and deterministic: a nested construct
  inherits a single well-defined parent and overrides are last-wins, matching the
  existing space-local `Theme`/`Scale`/`Guide` resolution (spec §20.1, §16.11).
- Charts that use neither feature MUST render byte-for-byte unchanged; existing
  examples MUST NOT drift.
- Output stays deterministic and locale-independent.

## Capstone Acceptance Target

The capstone is whichever of the two items ships. If nested blocks ship, a chart
that nests a `Space` to compose layers with a shared parent frame and a
space-local override; if rejected, the rejection is recorded and the capstone is
the diagnostic plus the spec design note.

Illustrative (subject to the item 1 design decision — *not* yet accepted syntax):

```ag
Chart(data: "metrics.csv", width: 720, height: 460) {
    Space(time * value) {
        Line()
        Space(time * forecast) {
            Scale(axis: y, domain: [0, null])
            Line(stroke: "#888", dash: "dashed")
        }
    }
}
```

The release must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

Existing examples without nested blocks or annotation declarations regenerate
without drift.

## Design Decisions (settled)

1. **No indefinite limbo.** Both §4.2 reserved items reach a concrete decision in
   this release — shipped-and-tested or formally-rejected — and the spec's
   "reserved for later versions" wording is replaced accordingly.
2. **One subdivision model.** Algebraic faceting (`a / b`) remains the canonical
   way to split a plane. Block nesting, if shipped, is for *composition/layering*
   with shared or overridden frames, not a second faceting path.
3. **Reuse, don't fork.** Any nesting reuses the existing `FrameIr`, scale
   training, and layout planning; space-local overrides reuse the resolution
   already used for `Theme`/`Scale`/`Guide` on a space.
4. **Annotations are evaluated against the existing geometry model.** A
   space-local annotation construct, if shipped, lowers to the reference-mark and
   text geometries that already exist (spec §14.17–14.20); it does not add a new
   drawing primitive.
5. **Backwards compatibility is non-negotiable.** Absent the new constructs,
   parsing, analysis, and SVG output are byte-for-byte unchanged.

## v0.32.0 Must

### 1. Nested `Space` block semantics: decide and specify

Status: Planned.

Acceptance criteria:

- Decide whether nested `Space` blocks ship in v0.32, and specify their exact
  meaning if so. Candidate semantics to evaluate (pick one or reject all):
  - **Layer composition with frame inheritance:** a child `Space` shares the
    parent's plot area and inherits its frame, optionally overriding one axis or
    the coordinate system, to overlay a related series (e.g. a secondary line or
    a forecast band) without a new panel.
  - **Inset / sub-panel:** a child `Space` occupies a sub-rectangle of the
    parent's plot area with an independent frame.
  - **Formal rejection:** block nesting adds nothing that algebraic faceting and
    overlaid geometries do not already provide, so it is rejected and the
    diagnostic/spec note is finalized.
- Whichever is chosen, explicitly distinguish it from algebraic faceting
  (spec §8.3) so the two mechanisms do not overlap or compete.
- Record the decision and rationale in the spec (§4.2) and this plan, replacing
  the "reserved for later versions" language.

### 2. Nested `Space`: grammar, scope, and rendering (only if item 1 ships)

Status: Planned.

Acceptance criteria:

- Update the parser/CST to accept a `Space` inside a `Space` (removing the
  current rejection diagnostic only where the new design permits it) and the AST
  to expose the child space.
- Define scope/inheritance in §9 terms: which frame, scales, guides, theme, and
  coordinate system a child inherits, and how local declarations override them
  (last-wins, matching existing space-local resolution).
- Train scales and plan layout for nested blocks by reusing the existing
  pipeline; no parallel scale engine or layout path.
- Reserve and implement diagnostics for invalid nesting (e.g. a frame/coordinate
  combination the chosen semantics forbids).
- If item 1 rejects nesting, this item is dropped and the rejection diagnostic is
  finalized and tested instead.

### 3. Space-local annotation declarations: decide and (if shipped) implement

Status: Planned.

Acceptance criteria:

- Decide whether a dedicated space-local *annotation* declaration ships, distinct
  from placing `HLine`/`VLine`/`Text`/`Rect` geometries directly in a space
  (which already works). Candidate: a named or reusable annotation construct, or a
  grouping that scopes reference marks to a space.
- If it ships: specify its grammar and lower it to the existing reference-mark and
  text geometries (no new drawing primitive); validate placement and reserve
  diagnostics.
- If it is rejected: record that direct geometry placement is the supported
  mechanism, and update §4.2 to drop the "annotation declarations" reservation.

### 4. Diagnostics, LSP, and editor metadata

Status: Planned.

Acceptance criteria:

- Reserve and implement any new diagnostics in spec §26 before coding (invalid
  nesting, invalid space-local annotation placement), or finalize the rejection
  diagnostic(s).
- LSP completion, hover, signature help, document symbols, and folding handle a
  nested `Space` (and any annotation construct) where they ship.
- The VS Code TextMate grammar and `language-configuration.json` track any new
  source-visible keywords or block structure.

### 5. Examples, README, spec, and release hygiene

Status: Planned.

Acceptance criteria:

- Add at least one example for each shipped feature (or none, with a recorded
  rejection); regenerate with `./examples/generate.sh` and confirm no drift on
  existing examples.
- README gains a composition/scope tutorial section if a feature ships.
- Spec updates cover §4.2 (the nested-space and annotation decisions, replacing
  the reserved language), §8/§9 (scope/inheritance semantics if nesting ships),
  §14 (annotation lowering if it ships), and §26 (diagnostics).
- Workspace `Cargo.toml` and `editors/vscode/package.json` are bumped to
  `0.32.0`; LSP completion/hover and the VS Code grammar gain any new keywords.

## v0.32.0 Should

### Dual-axis legibility

Status: Planned.

If layer-composition nesting ships with a per-child axis override, design how a
secondary axis is drawn and labeled without implying a shared scale, including
guidance against misleading dual-axis charts.

### Named/reusable annotation fragments

Status: Planned.

If a space-local annotation construct ships, consider a reusable fragment
mechanism (analogous to `Style(...)`, spec §7) so a set of reference marks can be
declared once and applied to multiple spaces. Keep it declarative and inert.

## Explicitly Deferred Past v0.32.0

- A second faceting model distinct from algebraic nesting (`a / b`).
- Independent, free-floating sub-charts or picture-in-picture composition beyond
  the chosen nesting semantics.
- Dual-axis charts with independent *competing* scales presented as one axis pair
  (kept out unless item 1 explicitly designs the secondary-axis case safely).
- Extensibility — plugins, custom stats/geometries, user-defined functions, and
  macros remain the scope of [`V0_25_PLAN.md`](V0_25_PLAN.md), still pending and
  not reopened here.

## Optional-Item Audit

### Promote In v0.32.0 (Must)

- Nested `Space` block semantics decision.
- Nested `Space` grammar/scope/rendering (if shipped).
- Space-local annotation declaration decision (and implementation if shipped).
- Diagnostics, LSP, and editor metadata.
- Examples, README, spec, and release hygiene.

### Consider If Capacity Allows (Should)

- Dual-axis legibility.
- Named/reusable annotation fragments.

### Keep Deferred

- A second faceting model, free-floating sub-charts, competing dual-axis scales,
  and extensibility (v0.25).

## Promotion Workflow

1. Decide nested-`Space` semantics (item 1); record the decision in the spec
   before any code.
2. Reserve new diagnostics in spec §26.
3. If nesting ships: extend the parser/AST, then define scope/inheritance in §9
   and reuse the existing scale-training and layout pipeline.
4. Decide and (if shipped) implement space-local annotation declarations over the
   existing reference-mark/text geometries.
5. Add diagnostics, LSP, and grammar updates for whatever ships.
6. Add examples and README sections; bump versions; confirm no unintended example
   drift.

## A note on sequencing

After this release the last large unimplemented subsystem is **extensibility** —
plugins, custom stats, custom geometries, user-defined functions, and macros —
which already has a written but unshipped plan in
[`V0_25_PLAN.md`](V0_25_PLAN.md). It remains the plan-of-record for extensibility
and should be slotted in (and renumbered if desired) once the reserved grammar
items are resolved here.
