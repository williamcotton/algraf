# Algraf v0.5.0 Plan

Status: Planned (not started)
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_4_PLAN.md`](V0_4_PLAN.md)

## Purpose

This document defines the intended v0.5.0 release shape: composition and reuse —
making charts DRY, parameterizable, and shareable.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when syntax, diagnostics, tests, and
examples land together.

## Release Thesis

v0.5.0 is a **composition & reuse** release: stop repeating values and styling
across a chart, and let one document express more.

This is the most language-design-heavy release in the roadmap. v0.3.0 widened the
chart vocabulary and v0.4.0 made authoring pleasant; v0.5.0 introduces the first
user-introduced bindings and reusable styling. Because it adds real source
syntax and new scoping rules, it leans on the v0.4.0 editor work (go-to-def,
references, rename, signature help) to keep the larger language navigable.

It deliberately stops short of data backends (v0.6) and keeps the rendering model
unchanged.

## Scope Rules

- New syntax MUST be backwards compatible: existing `.ag` files keep working.
- Name resolution and scoping changes MUST have explicit, tested rules (spec §9).
- Every new binding form needs LSP support reused from v0.4.0 (completion,
  go-to-def, references, rename) before the item is considered done.
- Prefer one well-specified composition primitive over several half-specified ones.
- Keep multi-document and nested-space ambitions scoped tightly or deferred.

## v0.5.0 Must

### 1. User Variables (Let Bindings)

Status: Not started. Listed as "user variables / user-defined shadowing" under
the standing deferred list.

Introduce chart-scoped named values that can be referenced in property positions.

Minimum target:

```ag
Chart(data: "penguins.csv") {
    let primary = "#3366cc"
    let dim_alpha = 0.4

    Space(flipper_length_mm * body_mass_g) {
        Point(fill: primary, alpha: dim_alpha)
    }
}
```

Acceptance criteria:

- Grammar adds a `let name = <value>` declaration valid at chart scope (and,
  if specified, space scope), with a stable AST node and byte-accurate spans.
- A reserved keyword (`let`) is added to the lexer/keyword set (spec §6.5) and
  the formatter, semantic tokens, and completion all handle it.
- Variables resolve in property value positions; type checking matches the
  property's accepted value forms (spec §13.9).
- Scoping and shadowing rules are specified in spec §9.6 (currently about
  inheritance), including whether space-scope `let` shadows chart-scope `let`.
- Diagnostics: unknown variable, duplicate binding, type mismatch at use site,
  and (if disallowed) cycles. Reserve new diagnostic codes in the spec first.
- LSP: go-to-definition, references, rename, and completion all work for `let`
  bindings (reusing v0.4.0 machinery).
- Variables do NOT introduce user-defined functions or shadowing of column names
  unless explicitly specified; keep the first cut to constant values.
- Semantic and formatter tests, plus an example (`examples/variables.ag`).

### 2. Custom Theme Objects

Status: Not started. Spec §20.8 currently marks custom theme syntax as deferred.

Promote source-level theme customization beyond named presets.

Minimum target:

```ag
Theme(
    name: "minimal",
    axisText: Text(size: 12, fill: "#333333"),
    gridMajor: Line(stroke: "#dddddd", strokeWidth: 1)
)
```

Acceptance criteria:

- `Theme(...)` accepts override properties layered on top of a named base theme.
- Override values reuse existing geometry/property value forms where possible.
- Unknown theme keys and type mismatches emit targeted diagnostics (reserve codes
  in the spec first).
- Spec §20.8 is rewritten from "deferred" to the implemented override model;
  §20.1 (theme object) is updated to match the render `Theme` struct fields.
- Composes with `let` bindings (Must item 1) for shared colors.
- Render tests and an example (`examples/custom_theme.ag`).

### 3. Spec, Version, and Example Hygiene

Status: Not started; mirrors prior releases.

Acceptance criteria:

- `Cargo.toml` workspace version bumped to `0.5.0` when the release branch is ready.
- Spec §6.5, §9.6, §20.1, §20.8 (and any new diagnostic codes) made normative.
- Because v0.5.0 adds real new syntax, evaluate promoting the optional language
  version declaration `Algraf(version: "0.5")` (spec §30.1); decide explicitly
  and record the decision here rather than leaving it implied.
- README gains examples for variables and custom themes, placed by tutorial
  progression (… → theming).
- Examples regenerated via `./examples/generate.sh`.
- This document updated as each item completes, is rejected, or moves scope.

## v0.5.0 Should

### Multi-Chart Documents

Status: Not started. Listed under the standing deferred list.

Allow more than one top-level `Chart` block in a single document, each rendered
independently (no shared layout). This is a large layout/CLI-output change (one
input, multiple outputs), so keep it a Should: implement only if the Must items
land with capacity, and specify output-naming before starting.

Acceptance criteria (if implemented):

- Grammar permits multiple `Chart` blocks (spec §7.1).
- CLI render specifies how multiple charts map to output files.
- Spec §17.5 (multiple spaces) is distinguished clearly from multiple charts.

### Reusable Style Fragments

Status: Not started.

Consider letting a `let` binding hold a reusable property set applied to multiple
geometries. Strictly additive on top of Must item 1; only pursue if the variable
model generalizes cleanly.

### Chart Margin Overrides

Status: Done. Surfaced by real example use: `examples/satisfaction_slope.ag` is a
slope chart whose direct end-labels were clipped by the canvas edge once the
redundant legend was removed, because nothing reserved right-margin space for
annotations.

A small layout-authoring primitive: `Chart` accepts `marginTop`, `marginRight`,
`marginBottom`, and `marginLeft` (non-negative integers, pixels). Each is a
per-side minimum margin (floor) composed over the computed margin (spec §17.3),
so it reserves room for annotations sitting outside the plot area without
shrinking content-driven margins. (Placement in v0.5 is editorial — it is a
self-contained layout knob rather than part of the composition/reuse thesis;
the spec change is what makes it normative.)

Acceptance criteria:

- `Chart(marginRight: N)` (and the other three sides) widen the corresponding
  margin to at least `N`; absent arguments leave layout unchanged.
- Spec §17.3 and the Chart-properties section document the floor semantics.
- An example exercises it (the slope chart) and the README stays in sync.

## Explicitly Deferred Past v0.5.0

Carried forward and unchanged unless a later planning decision moves them:

- Nested `Space` blocks (still deferred; large semantic change).
- User-defined functions, macros, or column shadowing via `let`.
- Plugins and custom stats.
- All v0.6 data-backend features (SQL, Polars, large data).
- Everything under the standing deferred list in [`V0_3_PLAN.md`](V0_3_PLAN.md)
  not promoted here.

## Optional-Item Audit

### Promote In v0.5.0 (Must)

- User variables (`let` bindings, constant values).
- Custom theme objects.

### Consider If Capacity Allows (Should)

- Multi-chart documents.
- Reusable style fragments built on `let`.

### Keep Deferred

- Nested spaces.
- User-defined functions / macros.
- Everything assigned to v0.6.

## Promotion Workflow

1. Move the chosen behavior into the relevant normative section of
   [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) (syntax §6–7, scope §9, themes §20).
2. Reserve or add diagnostic codes before implementation if behavior can fail.
3. Implement parser, semantic, render, CLI, and LSP changes as needed; new
   bindings need the v0.4.0 navigation features wired up.
4. Add focused tests in the crate closest to the behavior.
5. Add or update examples when behavior affects user-facing charts.
6. Regenerate examples when rendered output changes.
7. Update this document when a candidate is completed, rejected, or moved scope.
