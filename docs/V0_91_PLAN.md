# Algraf v0.91.0 Plan

Status: Implemented
Target version: 0.91.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_90_PLAN.md`](V0_90_PLAN.md)
Roadmap theme: Document-scoped custom themes with the existing value and
declaration model.

## Purpose

Algraf v0.91 should let authors define a reusable house theme once per source
file and apply it to one or more charts or spaces with local overrides. The
release should preserve the tiny-grammar principle: no theme block, no import
system, no user functions, and no separate theme registry declaration.

The intended surface is document-scope `let` plus `Theme(base: ...)`:

```text
let house = Theme(
    name: "minimal",
    background: "#f3f0eb",
    plotBackground: "#f3f0eb",
    gridMajor: Line(stroke: "#d8d4cc", strokeWidth: 1),
    axisText: Text(size: 11, fill: "#7a7a7a"),
)

Chart(data: "strategic_reserves.csv") {
    Theme(base: house, axisYPosition: "right", gridX: false)
}
```

This sketch is the target syntax for v0.91, not a runnable example until the
release lands.

## Release Thesis

v0.91.0 is a **custom theme reuse** release. It should make theme reuse feel like
the rest of Algraf: a constant value bound by `let`, referenced by a bare
identifier, and refined by ordinary call arguments. It should not introduce
global mutable state, style sheets, theme packages, macros, or a second naming
system.

## Current Coverage Audit

Already available:

- `Theme(name: "minimal", ...)` selects a built-in base and layers local
  overrides on top.
- `Theme(...)` accepts grouped `Text(...)`, `Line(...)`, and `Rect(...)`
  override values.
- `let` declarations already exist in chart and space scopes, and
  `let muted = Style(...)` proves that a call value can be stored as a binding.
- Document-scope `Table` declarations already prove the root can host shared
  declarations outside `Chart`.
- Rendering already resolves a `ThemeIr` by selecting a base theme and applying
  overrides.

Gaps this release should close:

- The root parser and AST expose document-scope `Table` declarations but not
  document-scope `let` declarations.
- The semantic analyzer has chart and space variable maps, but no document
  variable scope visible to all charts.
- `let` does not yet accept `Theme(...)` as a constant theme value.
- `Theme(...)` has `name:` for built-in bases but no `base:` key for a
  user-bound theme value.
- Editor completion, hover, formatting, symbols, language templates, and the
  normative spec do not describe document-scope custom themes.

## v0.91.0 Must

### Document-Scope `let`

Status: Implemented.

Allow `let` declarations at document scope, before or between top-level charts
and tables.

Acceptance criteria:

- The parser accepts document-scope `LetDecl` after an optional `Algraf(...)`
  source header and before, between, or after document-scope `Table` and
  `Chart` items, while still requiring at least one chart in the source file.
- `Root` exposes document-scope lets in source order, matching the existing
  `Root::tables()` pattern.
- Document-scope `let` values support the existing constant forms and
  `Style(...)` fragments.
- Document-scope variables are visible inside every chart and space in the file.
- Shadowing order for ordinary property values is
  space scope > chart scope > document scope.
- Duplicate document-scope bindings emit the existing duplicate-let diagnostic
  for that scope; duplicates in narrower scopes remain legal shadowing.
- Formatter output places top-level lets consistently with document-scope
  tables, preserving readable blank lines between root declarations and charts.
- Document symbols include top-level let bindings without nesting them under the
  first chart.

Implementation touch points:

- `crates/algraf-syntax/src/parser/block.rs` root item dispatch and recovery.
- `crates/algraf-syntax/src/ast.rs` root accessors.
- `crates/algraf-syntax/src/format.rs` and formatter tests.
- `crates/algraf-semantics/src/analyzer/context.rs` variable scope lookup.
- `crates/algraf-editor-services/src/symbols.rs`, completions, hover, and
  semantic-token assumptions if any are root-context-specific.

### Theme Values In `let`

Status: Implemented.

Permit document-scope `let` declarations to bind `Theme(...)` values. A bound
theme is a data-independent style value, not a chart and not executable code.

Acceptance criteria:

- `let house = Theme(...)` is valid at document scope.
- Theme bindings reuse the same argument validation as chart/space `Theme(...)`
  declarations: unknown keys are `E1704`, invalid value shapes are `E1705`, and
  unknown built-in base names keep the current theme-name diagnostic path.
- A document-bound `Theme(...)` with `name:` selects that built-in base and
  stores its local overrides.
- A document-bound `Theme(...)` with no base selector defaults to the built-in
  default theme, not to a later chart's local state.
- `Theme(...)` values remain data-independent. Column mappings, algebra
  expressions, `input`/`stdin`, and non-theme call heads remain invalid in a
  theme binding.
- Chart-scope and space-scope `let Theme(...)` remains deferred unless the
  implementation can support it without changing the scope model. v0.91 only
  requires document-scope theme bindings.

Implementation touch points:

- Extend the analyzer's let-value representation with a theme value form.
- Reuse `theme_decl`/theme override validation rather than adding a second
  parser for theme values.
- Add semantic tests for valid document theme bindings and invalid theme-binding
  values.

### `Theme(base: ...)`

Status: Implemented.

Add a `base:` argument to `Theme(...)` so a chart-level or space-local theme can
inherit from a user-bound theme value and then layer local overrides.

Acceptance criteria:

- `Theme(base: house, axisYPosition: "right")` resolves `house` as a bare
  identifier in the document theme-binding scope.
- `Theme(base: "minimal", ...)` selects a built-in base theme. Existing
  `Theme(name: "minimal", ...)` remains valid and remains a first-class
  built-in-base spelling.
- `name:` and `base:` in the same `Theme(...)` call are rejected as conflicting
  base selectors.
- A `base:` bare identifier that is not a document-bound theme emits an invalid
  theme property value diagnostic with a message that names the missing base.
- User theme bases may chain through other document-bound themes if cycle
  detection is implemented in the same change. Cycles must emit a diagnostic and
  must not panic.
- Override layering is deterministic:
  built-in base < document-bound theme overrides < local `Theme(...)` overrides.
- A space-local `Theme(base: house, ...)` replaces the inherited chart base with
  `house` before applying its own overrides, matching current `Theme(name: ...)`
  behavior for built-in bases.
- `Theme(...)` without `base:` or `name:` keeps current behavior: chart-level
  declarations layer over the default base; space-local declarations inherit the
  chart theme and layer overrides.

Implementation touch points:

- Add `base` to the theme argument registry and declaration docs.
- Add a theme-base resolver that flattens user theme bindings into a built-in
  base plus merged overrides during semantic analysis, or otherwise passes a
  resolved theme registry to rendering without letting rendering depend on
  syntax names.
- Add a small override-merge helper so document-bound and local overrides layer
  in one tested order.
- Keep CLI `--theme <name>` semantics unchanged unless the spec intentionally
  changes them in this release.

### Spec, Templates, And Editor Surface

Status: Implemented.

Document the implemented custom-theme surface in the normative and generated
language references.

Acceptance criteria:

- `docs/ALGRAF_SPEC.md` updates sections 7.1, 7.10, 9.6, 20.7, and 20.8,
  diagnostics, tests, and the milestone table when the implementation lands.
- `crates/algraf-cli/templates/ALGRAF_LANGUAGE.md` and composed
  `ALGRAF_LANG.md` document document-scope lets, theme bindings, `base:`, and
  the shadowing rules.
- Completion suggests `let` where root declarations are valid and suggests
  `base` inside `Theme(...)`.
- Hover/signature help for `Theme` describes `base` and keeps `name` as a
  first-class spelling for built-in bases.
- VS Code TextMate grammar changes only if static highlighting is currently
  context-sensitive enough to miss document-scope `let`.

### Custom Theme Example

Status: Implemented.

Add a runnable example that demonstrates a document-bound house theme applied
with a local override.

Acceptance criteria:

- Add an example chart using `let house = Theme(...)` at document scope and
  `Theme(base: house, ...)` inside a chart or space.
- The example should be visually distinct enough to prove that background,
  grid, axis text, and at least one layout-related theme field are inherited.
- Regenerate example SVG/PNG output and visually inspect the PNG.
- Add the example to the top-level `README.md` in the theming tutorial
  position, and update any examples index that exists at implementation time.

## v0.91.0 Should

### Theme Base Chaining

Status: Implemented.

If the Must implementation can do it cleanly, allow one document-bound custom
theme to inherit from another:

```text
let house = Theme(name: "minimal", gridMajor: Line(stroke: "#d8d4cc"))
let compact_house = Theme(base: house, fontSize: 10, legendSpacing: 10)
```

The implementation must detect cycles before rendering. If cycle handling makes
the release larger than intended, v0.91 can restrict document theme bindings to
built-in bases while still allowing chart/space `Theme(base: house, ...)`.

### Keep `name:` And `base:` Co-Equal

Status: Implemented.

Do not add editor guidance that rewrites `Theme(name: "minimal")` to
`Theme(base: "minimal")`. `name:` remains a supported built-in theme selector,
and `base:` is added for built-in bases and document-bound custom themes.

## Explicitly Deferred Past v0.91.0

- External theme files, theme packages, imports, includes, or search paths.
- Theme names addressed by quoted user strings such as `Theme(base: "house")`;
  user themes are referenced by bare identifiers.
- User-defined functions, computed color expressions, or data-dependent theme
  values.
- Runtime theme mutation or chart-to-chart inheritance.
- CLI selection of document-bound custom themes by name.
- Publishing a browser-package release unless a separate package publication
  plan verifies available npm versions.

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- Focused parser tests for document-scope lets around headers, tables, multiple
  charts, and recovery after invalid root items.
- Focused semantic tests for document variable shadowing, `let Theme(...)`,
  `Theme(base: house)`, invalid bases, conflicting `name`/`base`, and cycle
  handling if base chaining is promoted.
- Formatter tests for top-level lets and nested theme call values.
- Editor-service tests for completions, hover/signature help, and document
  symbols.
- `./examples/generate.sh`
- Manual PNG review of the custom-theme example.

## Promotion Workflow

1. Finish or explicitly defer the v0.90 release scope before making v0.91 the
   active implementation target.
2. Promote the document-scope `let`, theme binding, and `Theme(base:)` semantics
   into `docs/ALGRAF_SPEC.md` before implementation lands.
3. Decide whether invalid user theme bases and cycles reuse `E1705` or reserve a
   dedicated diagnostic code, then document that choice in the spec before code
   emits it.
4. Implement syntax, semantics, rendering integration, editor services,
   templates, examples, and tests in the same release branch.
5. Update this plan's `Status:` lines as each item lands, defers, or changes
   scope.
6. When complete, align every required version stamp and add the v0.91 row to
   the spec milestone table.
