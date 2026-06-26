# Algraf v0.92.0 Plan

Status: Implemented
Target version: 0.92.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_91_PLAN.md`](V0_91_PLAN.md)
Roadmap theme: Explicit variable references with a `$` sigil.

## Purpose

Algraf v0.92 should make every `let` binding reference visually explicit. The
v0.91 document-scope theme work exposed a real readability problem:

```ag
Theme(plotTitle: Text(fill: accent))

Space(flipper_length * body_mass) {
    Point(fill: species)
}
```

Both `accent` and `species` are bare identifiers, but one is a constant binding
and the other is a data column. The language can resolve that distinction from
context, but readers should not have to. v0.92 should introduce `$name` for
references to `let` bindings and reserve bare identifiers for columns,
selectors, sentinels, and other non-`let` language symbols.

Target spelling:

```ag
let accent = "#126c73"
let muted = Style(fill: "#6b7280", alpha: 0.55)
let house = Theme(
    name: "minimal",
    plotTitle: Text(fill: $accent)
)

Chart(data: "penguins.csv") {
    Theme(base: $house)

    Space(flipper_length * body_mass) {
        Point(fill: species, style: $muted)
    }
}
```

## Release Thesis

v0.92.0 is a **binding clarity** release. It should keep `let` as the single
constant-binding declaration form, but require a sigil at every use site. This
keeps the tiny grammar while removing the most confusing ambiguity introduced
by broad document-scope bindings.

The release should not add computed expressions, interpolation, imports,
macros, user functions, or a second theme declaration system.

## Current Coverage Audit

Already available after v0.91:

- `let` declarations exist at document, chart, and space scope.
- Ordinary constants and `Style(...)` fragments can be bound and referenced in
  property positions.
- Document-scope `let house = Theme(...)` can be applied with `Theme(base:
  house)`.
- Let bindings shadow by scope: space > chart > document.
- Rename/navigation machinery understands let declarations and bare references.

Problems this release should close:

- Bare property identifiers can currently mean either a column or a `let`
  binding. The answer depends on the property and in-scope bindings.
- `Theme(base: house)` is another bare-reference special case even though
  `house` is a `let` binding.
- `style: muted` and `fill: species` look alike, but one references a style
  binding and the other maps a column.
- Column names can be accidentally shadowed by `let` names in property-value
  positions.
- Editor completion and rename currently have to reason about bare identifiers
  that may or may not be binding references.

## v0.92.0 Must

### `$name` Let References

Status: Implemented.

Add a variable-reference value form spelled `$` followed immediately by an
identifier. This is the only supported spelling for referencing a `let` binding.

Acceptance criteria:

- The lexer/parser accepts `$name` anywhere a property value can appear.
- `$name` resolves through the existing scope chain: space > chart > document.
- `$name` can reference all existing let value forms: scalar constants, arrays,
  `Style(...)` fragments, and document-bound `Theme(...)` values.
- `$name` MUST NOT be accepted in algebra frames, stat input positions, table
  names, or declaration names unless a later plan explicitly adds that surface.
- `$name` uses byte-accurate spans that include the `$` for diagnostics,
  hover, definition, references, rename, and formatting.
- Backtick-quoted column identifiers remain the way to address column names
  that contain `$`, e.g. `` `"$revenue"` `` if such a source column exists.

Implementation touch points:

- Lexer tokenization for `$` or a dedicated variable-reference token.
- `ValueExpr`/AST/CST accessors for a sigiled reference.
- Formatter preservation and canonical spacing: `$name`, never `$ name`.
- Semantic substitution in `crates/algraf-semantics/src/analyzer/context.rs`.
- Editor services for hover, definition, references, rename, completion, and
  semantic tokens.

### Remove Bare `let` Resolution

Status: Implemented.

Bare identifiers in property value positions should no longer resolve to `let`
bindings. This restores a simple reading rule: bare identifiers are columns,
selectors, sentinels, or language symbols; `$name` is a binding.

Acceptance criteria:

- `Point(fill: species)` continues to map the `species` column even when a
  `let species = ...` binding exists in scope.
- `Point(fill: primary)` no longer resolves to `let primary = "#3366cc"`;
  source must use `Point(fill: $primary)`.
- `Point(style: muted)` no longer resolves to `let muted = Style(...)`; source
  must use `Point(style: $muted)`.
- `Theme(base: house)` no longer resolves to `let house = Theme(...)`; source
  must use `Theme(base: $house)`.
- `Theme(base: "minimal")` and `Theme(name: "minimal")` remain first-class
  built-in theme selectors and are not affected by the sigil change.
- If a bare identifier exactly matches an in-scope `let` binding in a property
  position where the old behavior would have substituted the binding, emit a
  targeted diagnostic with a fix-it suggesting `$name`.

Diagnostic notes:

- Reserve a new diagnostic code before implementation, likely `E1707`, for
  "let binding reference requires `$`".
- Reserve a second code only if parser-level malformed sigil cases need their
  own diagnostic instead of existing parse errors.

### Theme Base Sigil

Status: Implemented.

Make custom theme reuse use the same binding-reference syntax as every other
let use site.

Acceptance criteria:

- `let house = Theme(name: "minimal", ...)` remains the binding declaration.
- `Theme(base: $house, ...)` resolves the document-bound theme.
- Chained document themes use the sigil: `let compact = Theme(base: $house,
  fontSize: 10)`.
- Cycle detection remains required and MUST NOT panic.
- `Theme(base: house)` produces the same targeted `$house` diagnostic when
  `house` is an in-scope theme binding.
- A bare `base: house` with no matching binding remains an invalid theme base
  value diagnostic.

### Style Fragment Sigil

Status: Implemented.

Require sigiled references for style fragments.

Acceptance criteria:

- `let muted = Style(fill: "#6b7280", alpha: 0.55)` remains valid.
- Geometry calls use `style: $muted`.
- Inline `style: Style(...)` remains valid.
- Style-fragment expansion order remains unchanged: earlier fragments expand
  first, later fragments and explicit properties override earlier values.
- `Style(...)` contents use `$name` for any nested binding reference accepted by
  that style key.

### Editor Migration Surface

Status: Implemented.

Make the transition easy and explicit in editor tooling.

Acceptance criteria:

- Completion after `$` suggests in-scope let bindings, with type-aware details
  where available.
- Ordinary property-value completion should not suggest let bindings as bare
  identifiers.
- Hover on `$name` identifies the binding scope and value kind.
- Definition/references/rename operate on the `$name` reference token and the
  original `let name = ...` declaration.
- Code actions rewrite old bare references to sigiled references only when the
  analyzer can prove the old source matched an in-scope `let` binding.
- Existing code actions must not rewrite column mappings to `$` by guesswork.

### Spec, Templates, And Examples

Status: Implemented.

Document the sigil rule everywhere the language surface is described.

Acceptance criteria:

- `docs/ALGRAF_SPEC.md` updates §7.8 value forms, §7.10 let bindings, §9.6
  variable resolution, §20.8 custom themes, diagnostics, tests, and milestone
  table.
- `crates/algraf-cli/templates/ALGRAF_LANGUAGE.md` and composed
  `ALGRAF_LANG.md` document `$name`, `Theme(base: $house)`, `style: $muted`,
  and the no-bare-let rule.
- Existing examples introduced or updated for v0.91 use `$` for every binding
  reference.
- At least one example should deliberately contrast `$accent` with `species` so
  the reason for the feature is visible.

## v0.92.0 Should

### Compatibility Diagnostics Before Hard Rejection

Status: Implemented.

If the implementation can do it cleanly, old bare references should produce
targeted diagnostics and fixes rather than generic type or unknown-column
errors.

Acceptance criteria:

- `fill: primary` where `let primary = "#3366cc"` exists emits the new
  sigil-required diagnostic and suggests `fill: $primary`.
- `base: house` where `let house = Theme(...)` exists emits the same diagnostic
  and suggests `base: $house`.
- Diagnostics should be errors for v0.92 unless maintainers choose a one-release
  warning period before implementation begins.

### Mechanical Migration Command

Status: Implemented via semantic diagnostics and editor quick fixes.

Consider a CLI or editor-only migration helper that rewrites provable v0.91
bare binding references to `$name`.

Implementation note: v0.92 ships the editor-only helper path through `E1707`
code actions. The fix is semantic-diagnostic driven, leaves column mappings
unchanged, and is idempotent because migrated references no longer produce the
diagnostic. No standalone CLI migration command was added.

Acceptance criteria:

- It must be semantic, not regex-based.
- It must leave column mappings unchanged.
- It must be idempotent on already migrated files.
- It must cover examples and plan snippets as part of the implementation PR.

## Explicitly Deferred Past v0.92.0

- Expression interpolation inside strings.
- `$` references in algebra frames or stat input lists.
- User functions, computed color expressions, or arithmetic over `let`
  bindings.
- Environment variables or shell-style expansion; `$name` is an Algraf binding
  reference only.
- `$$`, `${name}`, or dotted variable paths as Algraf binding-reference syntax.
  Host invocation-variable expansion remains `${name}` only, before parsing, so
  it does not consume Algraf `$name` references.
- Theme packages, imports, includes, or external theme files.

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- Focused parser tests for `$name`, malformed `$`, non-ASCII spans around
  sigiled references, and recovery.
- Focused semantic tests for scalar, style, and theme references via `$name`;
  bare-reference diagnostics; shadowing; type mismatch at use site; and theme
  base cycles through `$` references.
- Formatter tests for `$name` in nested calls and style/theme values.
- Editor-service tests for completion after `$`, hover, definition, references,
  rename, semantic tokens, and code actions.
- Update and render affected examples with `./examples/generate.sh`.
- Manual PNG review of changed theme examples.

## Promotion Workflow

1. Promote the `$name` value form and no-bare-let rule into
   `docs/ALGRAF_SPEC.md` before implementation lands.
2. Reserve and document any new diagnostic code, likely `E1707`, before code
   emits it.
3. Implement parser/AST/formatter support before semantic migration so editor
   spans are stable.
4. Update semantic resolution so `$name` is the only `let` reference path.
5. Update editor services, language templates, examples, and tests in the same
   release branch.
6. When implementation starts, align version stamps for v0.92.0 according to
   `AGENTS.md`.
7. When complete, update this plan's `Status:` lines, align every required
   release stamp, and add the v0.92 row to the spec milestone table.
