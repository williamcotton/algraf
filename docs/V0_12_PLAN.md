# Algraf v0.12.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_11_PLAN.md`](V0_11_PLAN.md)

## Purpose

This document defines the intended v0.12.0 release shape: cleaning up tooling,
diagnostics, and parser/editor glue after the driver, semantics, and renderer
have been modularized.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when syntax, diagnostics, tests, and
examples land together.

## Release Thesis

v0.12.0 is a **tooling and diagnostics architecture** release: split the LSP
monolith, centralize diagnostic-code ownership, and clean up parser/editor
registry drift.

This is intentionally last in the refactor sequence. Once v0.9 removes source
pipeline duplication, v0.10 splits semantics, and v0.11 splits rendering, the
LSP can be decomposed without copying old boundaries. Diagnostic-code migration
also becomes more mechanical after the largest code moves are complete.

## Current Debt Surface

The refactor survey found:

- `crates/algraf-lsp/src/lib.rs` is about 3,211 lines and owns server bootstrap,
  document state, schema cache, preview, diagnostics conversion, semantic
  tokens, completions, hover, document symbols, code actions, navigation,
  rename, signature help, inlay hints, and tests.
- Diagnostic code strings are scattered across syntax, semantics, render, and
  LSP, with especially high repetition for `E1204` and `E1404`.
- LSP code actions match raw diagnostic string codes.
- Parser misspelled-keyword recovery repeats nearly identical branches for chart
  body items.
- Keyword/property/theme docs and completions are manually synchronized across
  registry, semantics, render, CLI, LSP, and VS Code assets.

## Scope Rules

- No new editor features are required. This release is about maintainability of
  existing diagnostics, completions, hover, symbols, navigation, and preview.
- LSP protocol, diagnostic JSON, and parser behavior may change when the cleanup
  improves the pre-release design, but tests and spec updates must document the
  intentional new behavior.
- Diagnostic JSON/LSP wire output should continue to expose string codes unless
  the registry migration deliberately changes that shape.
- Parser behavior must remain resilient.
- Do not make contextual keywords hard lexer tokens. Contextual keyword behavior
  is intentional.

## Capstone Acceptance Target

The capstone is tooling parity:

```bash
cargo test -p algraf-lsp
cargo test -p algraf-syntax
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

Manual smoke testing of the VS Code extension is desirable when this release
lands. The current checked-in examples are still the visual regression baseline:
`git diff -- examples` must be empty after regeneration.

## Design Decisions (settled)

1. **Split LSP by feature.** Keep protocol glue shallow and move feature logic
   into focused modules.
2. **Centralize codes without changing output.** Rust call sites use typed
   diagnostic codes; serialized diagnostics still show strings such as `E1101`.
3. **Preserve contextual keywords.** Consolidate keyword registries where useful,
   but do not break quoted identifier or contextual parsing behavior.
4. **Treat VS Code as a thin client.** The extension should remain mostly
   server lifecycle and preview webview glue.

## v0.12.0 Must

### 1. Diagnostic code registry

Status: Not started.

Acceptance criteria:

- `algraf-core` exposes a diagnostic-code type or typed constants with one
  canonical entry for every syntax, semantic, render, warning, hint, and data/LSP
  code.
- `Diagnostic` constructors accept typed codes while serialization, LSP
  conversion, and CLI JSON continue to emit string values unless a deliberate
  wire-shape cleanup is included.
- Production call sites stop introducing raw `"E1234"`/`"W1234"`/`"H1234"`/
  `"R1234"` literals.
- A unit test proves registered codes are unique, match the expected prefix and
  numeric shape, and cover the codes listed in spec §26 plus every production
  code emitted across syntax, semantics, render, and data/LSP adapters. Any
  production code missing from spec §26 is either added to the spec or documented
  as intentionally internal.
- LSP code actions match typed diagnostic codes rather than raw strings.

### 2. LSP module split

Status: Not started.

Acceptance criteria:

- `crates/algraf-lsp/src/lib.rs` is reduced to module declarations, public server
  construction, and protocol glue.
- Feature modules are split along these lines:
  - `backend.rs`;
  - `document.rs`;
  - `preview.rs`;
  - `diagnostics.rs`;
  - `positions.rs`;
  - `semantic_tokens.rs`;
  - `completion.rs`;
  - `hover.rs`;
  - `symbols.rs`;
  - `navigation.rs`;
  - `code_actions.rs`;
  - `signature.rs`;
  - `inlay.rs`.
- The LSP uses the v0.9 driver for source/schema/preview behavior.
- Existing LSP tests pass after each major move.

### 3. LSP docs and registry drift cleanup

Status: Not started.

Acceptance criteria:

- Geometry/property completions and docs reuse semantic registry data where that
  reduces duplication.
- Theme names, theme override keys, source constructors, chart args, and scale
  target names are centralized or explicitly documented as intentionally local.
- CLI/LSP string conversions use enum display helpers where available.
- Histogram refactor code action is tested against semantic lowering behavior or
  uses a shared rewrite helper if one exists after v0.10.

### 4. Parser recovery and contextual keyword cleanup

Status: Not started.

Acceptance criteria:

- This item is completed here only if the v0.10 parser-recovery Should item did
  not already land.
- `recover_misspelled_chart_item` uses a small table for repeated declaration
  cases such as `Scale`, `Guide`, `Theme`, and `Layout`.
- Parser recovery behavior and diagnostics remain covered by syntax tests.
- Parser, formatter, semantic tokens, and VS Code grammar draw from shared
  keyword lists where practical.
- Contextual keyword compatibility is preserved.

### 5. VS Code extension hygiene

Status: Not started.

Acceptance criteria:

- The extension remains a thin client around server lifecycle, preview webview,
  and data-file watching.
- Any preview protocol changes are reflected in the extension package in the
  same release.
- `editors/vscode/package.json` version is bumped to `0.12.0` with the workspace
  release.

### 6. Spec, version, and example hygiene

Status: Not started.

Acceptance criteria:

- Workspace version is bumped to `0.12.0` when the release branch is ready.
- Spec §26 and §30.4 are updated for any diagnostic-registry clarifications.
- Examples are regenerated with `./examples/generate.sh`; `git diff -- examples`
  must be empty for current checked-in examples.
- This document is updated as each item completes, is rejected, or moves scope.

## v0.12.0 Should

### LSP feature tests by module

Status: Not started.

As modules split, add focused tests near the feature they cover rather than
keeping all coverage in one integration-style test file.

### VS Code generated grammar audit

Status: Not started.

Review TextMate grammar and language configuration after keyword registry
cleanup to ensure editor highlighting still matches parser behavior.

## Explicitly Deferred Past v0.12.0

- New editor features beyond maintaining existing behavior.
- Language version declarations.
- Plugins/custom stats.
- Interactive previews or live browser UI.
- Full typed geometry-property IR.

## Optional-Item Audit

### Promote In v0.12.0 (Must)

- Diagnostic code registry.
- LSP module split.
- LSP docs and registry drift cleanup.
- Parser recovery and contextual keyword cleanup.
- VS Code extension hygiene.
- Spec/version/example hygiene.

### Consider If Capacity Allows (Should)

- LSP feature tests by module.
- VS Code generated grammar audit.

### Keep Deferred

- New editor features.
- Language/runtime features.
- Full typed geometry-property IR.

## Promotion Workflow

1. Add diagnostic-code registry and migrate constructors/call sites.
2. Update LSP code actions to use typed codes.
3. Split LSP modules one feature at a time.
4. Consolidate docs/registry duplication after modules are separated.
5. Clean parser recovery and contextual keyword registries.
6. Run LSP, syntax, and workspace tests.
7. Regenerate examples and require an empty `git diff -- examples`.
