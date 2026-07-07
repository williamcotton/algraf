# Algraf v0.95.0 Plan

Status: Implemented
Target version: 0.95.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_94_PLAN.md`](V0_94_PLAN.md)
Follow-on plan: [`V0_96_PLAN.md`](V0_96_PLAN.md)
Roadmap theme: Make pure language-surface lists registry-driven.

## Purpose

Algraf v0.95 should reduce drift between the registry, analyzer, completions,
and valid-argument helpers for pure language-surface name lists. The worked
example is the theme override key set, but the pattern also applies to geometry
property lists and stat setting names where no per-key analyzer logic is needed.

This release should not add new language features. It should make current names
come from one source of truth and add tests that fail when future names are
added to only one surface.

## Release Thesis

v0.95.0 is a **language surface drift prevention** release. Algraf already has a
careful spec/template process; the code should provide the same guardrails for
machine-readable name lists.

The goal is not to remove every match arm. Analyzer matches often encode real
per-key value validation and should stay explicit where that is clearer. The
goal is to remove duplicate pure lists and make completion/valid-argument data
flow from registry metadata whenever possible.

## Current Debt Surface

- `crates/algraf-semantics/src/registry.rs` has a
  `THEME_OVERRIDE_KEYS` constant, an `ArgDoc` table for `Theme`, and a separate
  `valid_args` answer for `Theme`.
- `crates/algraf-semantics/src/analyzer/themes.rs` has a separate key match for
  the actual validation behavior.
- `crates/algraf-editor-services/src/completion.rs` repeats name knowledge for
  completions.
- Adding or renaming a theme token currently requires edits across the registry,
  analyzer, completion, spec, and language-template artifacts, and pure code
  list drift is easy to miss.

## v0.95.0 Must

### Theme Argument Source Of Truth

Status: Implemented.

Collapse duplicate pure theme key lists into one registry-owned source.

Acceptance criteria:

- `Theme` argument docs and `THEME_OVERRIDE_KEYS` are derived from one declared
  table, or one is mechanically generated from the other in the same module.
- The valid-argument list for `Theme` is built from the same source and includes
  `name` and `base` without retyping the override-key array.
- Existing diagnostic suggestions for unknown theme keys keep using the unified
  key list.
- The implementation keeps analyzer validation readable. Per-key theme value
  parsing may remain as explicit match arms if that is clearer than a table of
  parser functions.
- The public language surface stays unchanged.

### Drift Tests For Theme Keys

Status: Implemented.

Add tests that make drift visible when theme keys change.

Acceptance criteria:

- A semantics test proves every registry theme override key is accepted by the
  analyzer when paired with an appropriate representative value.
- A completion/editor-services test proves theme-key completion draws from the
  registry-owned list, not a stale private list.
- A registry unit test proves `valid_args("Theme")` contains exactly
  `name`, `base`, and the override keys in the intended order.
- Unknown-key suggestion tests still pass, including closest-key behavior.

### Registry-Driven Completion For Pure Lists

Status: Implemented.

Replace editor-service literal lists with registry queries where the registry
already owns the names and documentation.

Acceptance criteria:

- Completion inside `Theme(...)` uses registry argument docs for labels and
  details.
- Completion for callable arguments that already have registry `ArgDoc` entries
  avoids private duplicate lists where no context-specific filtering is needed.
- Context-sensitive completions may keep local filtering, but the base candidate
  names should come from registry data.
- Hover/signature help behavior remains unchanged except for becoming harder to
  drift.

## v0.95.0 Should

### Geometry And Stat List Audit

Status: Implemented.

Apply the same single-source pattern to the next safest pure lists.

Acceptance criteria:

- Identify geometry property lists and stat setting lists where registry docs,
  valid-argument answers, analyzer acceptance, and completion candidates repeat
  the same names.
- Convert only the lists whose values can be derived mechanically without making
  analyzer code less clear.
- Add at least one drift test for each converted family.
- Leave any list with nontrivial context-sensitive semantics documented as
  intentionally local.

Implementation note: geometry property completions already flow from
`GeometryDef::props`, with only the `style` pseudo-property and interaction
metadata appended locally because they are not ordinary geometry properties.
Stat setting completions use `registry::declaration_arg_names`; analyzer
matches remain explicit because stat options carry per-key value parsing and IR
construction. v0.95 adds drift tests for both completion paths.

### Lightweight Documentation Cross-Check

Status: Implemented.

Consider a small test or `xtask`-style helper that compares registry names with
the generated language-reference templates.

Acceptance criteria:

- The check is cheap enough to run in ordinary test workflows or clearly
  documented as an opt-in maintenance check.
- It reports missing registry names in templates and extra template names that
  no longer exist.
- It does not require network access or generated files outside the workspace.
- If this turns out larger than expected, defer it with a note rather than
  blocking the core registry cleanup.

Implementation note: v0.95 adds a cheap registry unit test that compares
registry-owned theme override keys with the split and composed language
reference templates and reports missing/extra names.

## Explicitly Deferred Past v0.95.0

- New theme tokens, guide keys, stat arguments, or geometry properties. This is
  a refactor release, not a language-surface release.
- A full analyzer table-driven rewrite.
- Broad restructuring of `registry.rs`; only move enough code to make the
  source-of-truth boundary clear.
- Splitting the giant semantics test file; see [`V0_99_PLAN.md`](V0_99_PLAN.md).

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- Focused semantics tests for theme key acceptance and unknown-key diagnostics.
- Focused editor-service completion tests for theme keys and any other converted
  registry-driven lists.
- Manual review of `docs/ALGRAF_SPEC.md` and language templates only to confirm
  no implemented language surface changed.

## Promotion Workflow

1. Align version stamps for v0.95.0 when implementation begins.
2. Convert the theme key list first and add drift tests before broader cleanup.
3. Update completions to consume registry data.
4. Audit geometry and stat lists, converting only low-risk pure duplicates.
5. Run the full required checks.
6. Mark implemented items accurately and document any intentionally deferred
   list families.
