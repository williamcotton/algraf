# Algraf v0.4.0 Plan

Status: Planned (not started)
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_3_PLAN.md`](V0_3_PLAN.md)

## Purpose

This document defines the intended v0.4.0 release shape: deepening the editor and
authoring experience on top of the LSP foundation shipped in v0.2.0.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when syntax/protocol, diagnostics,
tests, and examples land together.

## Release Thesis

v0.4.0 is an **editor & authoring** release: make writing and editing `.ag` fast,
discoverable, and safe.

v0.2.0 established semantic tokens and quickfix code actions; v0.3.0 widened what
charts can be drawn. v0.4.0 makes the existing language pleasant to author —
navigation, in-editor help, and safe edits — without adding new chart surface.
All language logic stays in `algraf-lsp` (the VS Code client is a thin LSP
client; see CLAUDE.md), so every feature here is exercised by `algraf-lsp` tests.

The release adds no new geometries, stats, or data backends. It is purely about
tooling over the language that already exists.

## Scope Rules

- Editor features reuse the existing registry, semantic IR, and diagnostics; no
  feature requires rendering.
- Advertise an LSP capability only when it is implemented (spec §21).
- Prefer high-confidence behavior: navigation and edits must not surprise.
- Code actions and refactors preserve unrelated formatting where practical.
- Keep the in-editor *preview pane* deferred (it belongs to a later platform
  release, not this one).

## Current LSP Surface (baseline)

Already advertised and implemented: completion, hover (including operator hover),
document symbols, document formatting, semantic tokens, and quickfix code
actions. Not yet implemented: go-to-definition, find references / document
highlight, signature help, rename, and range/on-type formatting.

## v0.4.0 Must

### 1. Go To Definition

Status: Not started. Spec §21.8 currently says version 0.1 MAY not support it.

Implement definition navigation for the references that resolve unambiguously
within a document plus its data source.

Minimum target:

- A derived column (produced by a `Derive`) jumps to that `Derive` declaration.
- A column identifier that resolves to a CSV header opens the data file at that
  header (best effort; requires the data path to resolve).
- The `data` string value opens the resolved CSV file.

Acceptance criteria:

- `textDocument/definition` is advertised only once implemented.
- Resolution reuses semantic name resolution (spec §9.4); no rendering.
- Ambiguous or unresolved references return no definition rather than guessing.
- Spec §21.8 is rewritten from "MAY not support" to the implemented behavior.
- Tests cover derived-column-to-`Derive`, column-to-header, and data-path cases.

### 2. Find References and Document Highlight

Status: Not started.

For a column or derived-table name, report every use within the document.

Acceptance criteria:

- `textDocument/references` and `textDocument/documentHighlight` advertised only
  when implemented.
- Highlights cover the declaration and all in-scope uses, using the same name
  resolution as completion and go-to-definition.
- Spans are byte-accurate (test with non-ASCII identifiers, per CLAUDE.md).
- Tests cover a derived name used across multiple spaces and a source column.

### 3. Signature Help

Status: Not started.

While the cursor is inside a geometry or declaration call, surface the accepted
properties and indicate the active argument.

Minimum target:

```ag
Point(|)        # shows Point's properties: fill, stroke, alpha, size, shape
Scale(axis: x, |)   # shows remaining Scale keys
```

Acceptance criteria:

- `textDocument/signatureHelp` driven by the geometry/property registry (spec
  §13.8–13.9) — the same metadata completion uses.
- Active-parameter tracking follows the cursor across argument commas.
- Works for geometry calls and for `Scale`/`Guide`/`Theme`/`Layout` declarations.
- Spec §21 gains a signature-help subsection; tests cover protocol shape and
  active-parameter selection.

### 4. Expanded Code Actions

Status: Partial. v0.2.0 shipped quickfix actions (quoted enum/string, quoted
color, misspelled geometry, blend parenthesization).

Add a small set of additional high-confidence actions, including at least one
refactor-kind action.

Minimum target:

- Quick fixes for additional existing diagnostics where the fix is unambiguous.
- A `refactor` action that desugars a stat geometry into its explicit derived
  table form (e.g. `Histogram(bins: n)` → `Derive ... = Bin(...)` plus `Rect`),
  mirroring the desugaring the analyzer already performs.

Acceptance criteria:

- `code_action_kinds` advertises `refactor` only when a refactor action exists.
- Actions do not require rendering and preserve unrelated formatting.
- Tests cover at least one edit for each new action family.

### 5. Spec, Version, and Example Hygiene

Status: Not started; mirrors prior releases.

Acceptance criteria:

- `Cargo.toml` workspace version bumped to `0.4.0` when the release branch is ready.
- Spec §21 sections for each shipped capability are made normative; §21.8 no
  longer describes go-to-definition as optional.
- LSP capability advertisement matches what is implemented (no over-advertising).
- README/editor docs note the new navigation and authoring features.
- This document is updated as each item completes, is rejected, or moves scope.

## v0.4.0 Should

### Range and On-Type Formatting

Status: Not started. Whole-document formatting exists (spec §21.10).

Add `textDocument/rangeFormatting` and possibly on-type formatting, reusing the
existing formatter. Promote to Must only if it lands cleanly with the formatter.

### Rename

Status: Not started.

`textDocument/rename` for derived-table names and user-introduced identifiers,
updating all in-scope uses. Lower priority until composition features (v0.5)
introduce more renameable bindings.

### Inlay Hints

Status: Not started.

Inlay hints showing inferred column types or the output columns a stat produces
(e.g. `bin_start`, `bin_end`, `bin_center`). Useful but additive; keep optional.

## Explicitly Deferred Past v0.4.0

Carried forward and unchanged unless a later planning decision moves them:

- In-editor preview pane / custom render request (platform-level; later release).
- All v0.5 composition features (user variables, custom themes, multi-chart).
- All v0.6 data-backend features (SQL, Polars, large data).
- Everything under the standing deferred list in [`V0_3_PLAN.md`](V0_3_PLAN.md)
  not promoted here.

## Optional-Item Audit

### Promote In v0.4.0 (Must)

- Go to definition.
- Find references / document highlight.
- Signature help.
- Expanded code actions (including one refactor action).

### Consider If Capacity Allows (Should)

- Range/on-type formatting.
- Rename.
- Inlay hints.

### Keep Deferred

- In-editor preview pane.
- Everything assigned to v0.5 and v0.6.

## Promotion Workflow

1. Move the chosen behavior into the relevant normative section of
   [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) (LSP behavior lives in §21).
2. Reserve or add diagnostic codes before implementation if behavior can fail.
3. Implement in `algraf-lsp`; advertise the capability only once it works.
4. Add focused `algraf-lsp` tests for protocol shape and behavior.
5. Update editor/README docs when authoring features change.
6. Update this document when a candidate is completed, rejected, or moved scope.
