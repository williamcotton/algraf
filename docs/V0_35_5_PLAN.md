# Algraf v0.35.5 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) §21, §24.7, §27.7
Predecessor plan: [`V0_35_PLAN.md`](V0_35_PLAN.md)
Feature-roadmap follow-on: [`V0_36_PLAN.md`](V0_36_PLAN.md)
Roadmap theme: browser editor parity before the ggplot2-comparability feature
roadmap resumes.

## Purpose

This patch release expands the root-level Monaco demo editor so it supports as
much of the VS Code LSP experience as Monaco and the browser runtime can
reasonably host. The current
[`demo/src/AlgrafEditor.tsx`](../demo/src/AlgrafEditor.tsx) integration already
uses Monaco for editing, TextMate grammar highlighting, and diagnostic markers
from the WASM render/check path. That gives authors squiggles, but it does not
surface the richer editor features already implemented in `algraf-lsp`.

The goal is to make the browser playground reuse the same language intelligence
that the native `algraf lsp` server exposes through
[`crates/algraf-lsp/src/lib.rs`](../crates/algraf-lsp/src/lib.rs) and
`backend.rs`: hover, completion, signature help, formatting, semantic tokens,
code actions, navigation, rename, references/highlights, symbols, and inlay
hints. The implementation should wire every feature with a direct Monaco
equivalent and explicitly document any feature left out because Monaco lacks a
matching UX surface or the browser cannot provide the needed backing data.

The release does not change Algraf source semantics. It promotes a browser
client surface for language intelligence that already exists in the native LSP.

## Release Thesis

v0.35.5 is a **browser editor parity** patch. It should make the Monaco demo a
browser-hosted client of the existing Algraf language intelligence, not a
separate TypeScript implementation of Algraf syntax, semantics, or metadata. The
target is maximum practical parity with the VS Code LSP client, not just one or
two isolated Monaco features.

The LSP crate already owns the feature logic. The demo work should therefore be
an adapter problem:

- expose the existing Rust feature helpers to the browser through a stable WASM
  editor-service boundary, or through a tiny shared crate if the current
  `algraf-lsp` crate shape is too native-server-specific;
- map those LSP-shaped results into Monaco provider APIs;
- keep diagnostics, hover text, completion metadata, formatting, and navigation
  behavior aligned with the native VS Code client.

Hover is the proof case. Adding or changing Algraf hover behavior should require
one Rust change in the shared hover implementation. The native VS Code LSP path
and the Monaco browser path must both call that same implementation, so they get
the same Markdown, ranges, column type details, registry docs, and operator docs
automatically.

Success means an Algraf author using the playground gets the same practical IDE
help they would expect from VS Code, subject to browser filesystem limits.

## Scope Rules

- **No new language behavior.** Do not add geometries, properties, stats,
  diagnostics, source syntax, or CLI flags for this release.
- **No TypeScript language fork.** Monaco glue may translate positions and data
  shapes, but parsing, semantic analysis, registry metadata, hover text,
  completion docs, formatting, and code-action logic must come from Rust.
- **Shared hover behavior is mandatory.** TypeScript may convert an LSP-shaped
  hover into Monaco's `IHover` shape, but it MUST NOT construct Algraf hover
  content, inspect Algraf syntax to decide hover meanings, or maintain a second
  hover table.
- **Browser limitations must be explicit.** The browser demo has in-memory data
  sources, not a workspace filesystem. Features that need external files should
  work for host-supplied files and fail gracefully otherwise.
- **Native LSP behavior remains authoritative.** The native `algraf lsp`
  protocol surface must keep working. Browser support must reuse or wrap it,
  not weaken it.
- **Unported LSP features need reasons.** If a native LSP capability is not
  wired into Monaco, the implementation must record whether the blocker is a
  missing Monaco provider, a browser runtime/data limitation, latency, or a
  deliberate deferral.
- **Offset conversions are part of the contract.** Rust spans are byte offsets;
  Monaco positions are UTF-16 line/column positions. Non-ASCII fixtures must
  test every browser editor feature that translates ranges.

## Current State

### Monaco demo

- `AlgrafEditor.tsx` registers the `algraf` Monaco language.
- TextMate highlighting is wired from the VS Code grammar.
- Diagnostics from the WASM render result are converted into Monaco markers.
- Hover support is imported from Monaco, but no Algraf hover provider is
  registered yet.
- Completion, signature help, semantic tokens, code actions, formatting,
  symbols, rename, references, highlights, definition, and inlay hints are not
  yet wired in the browser demo.

### Native LSP

The native LSP already advertises and implements:

- diagnostics;
- completion;
- hover;
- document symbols;
- document and range formatting;
- semantic tokens;
- code actions;
- go to definition;
- references and document highlights;
- signature help;
- prepare rename and rename;
- inlay hints;
- the custom `algraf/preview` request.

That list is the parity target for Monaco. The implementation should start from
"wire all of it" and narrow only when a concrete browser or Monaco limitation is
documented.

## v0.35.5 Must

### 1. Define the browser editor-service boundary

Status: Implemented.

- Add a Rust-facing API that takes current source text, an in-memory file map,
  a URI-like document id, and a feature request, then returns LSP-shaped JSON or
  another stable serialized result that TypeScript can map into Monaco.
- Reuse the existing LSP feature modules (`hover`, `completion`, `signature`,
  `semantic_tokens`, `code_actions`, `navigation`, `symbols`, `inlay`, and
  formatting) instead of duplicating their behavior.
- The native LSP hover handler and the browser editor-service hover endpoint
  must both call one shared Rust hover function. If the current
  `crate::hover::hover_at` API is not the right public boundary, extract it
  behind a shared `editor_services::hover` API rather than copying its logic.
- If `algraf-lsp` cannot compile cleanly for `wasm32-unknown-unknown` because
  of native server dependencies, split the pure feature helpers into a shared
  editor-services module or crate used by both `algraf-lsp` and `algraf-wasm`.
- Keep `tower-lsp`, Tokio stdio serving, and the native `Backend` as the native
  server adapter. The browser path does not need to run JSON-RPC over stdio.

### 2. Share document analysis with the browser editor service

Status: Implemented.

- The browser editor service must parse and analyze the same source text that
  Monaco is editing.
- In-memory host files used by the playground preview must also be visible to
  completion and hover so column names, inferred types, and sample values match
  native LSP behavior where the same data is available.
- Diagnostics should continue to drive Monaco markers, but the same document
  state should feed hover/completion/navigation instead of recomputing unrelated
  partial state in TypeScript.
- The service must tolerate incomplete source and missing data the same way the
  LSP does: parser diagnostics still publish, semantic features degrade
  gracefully, and no request panics.

### 3. Wire Monaco providers in `AlgrafEditor.tsx`

Status: Implemented.

Register Monaco providers for every native LSP feature that has a practical
Monaco/browser equivalent:

- hover provider from the shared Rust hover result; the Monaco adapter only maps
  LSP `Hover` Markdown/range into Monaco's hover shape and contains no Algraf
  hover decision logic;
- completion item provider with docs, snippets, kinds, and trigger characters
  matching the native LSP;
- signature help provider for call arguments;
- document and range formatting providers using the Algraf formatter;
- semantic tokens provider using the Rust token classifier, with TextMate
  grammar retained as the static fallback;
- code action provider for existing quick fixes and rewrites;
- definition, references, and document-highlight providers;
- rename provider with prepare-rename behavior;
- document-symbol provider for outline-like integrations;
- inlay hints where Monaco support is stable enough for the bundled version.

Every provider must map ranges through tested byte-offset <-> UTF-16
conversions. Any native LSP feature not wired by the end of the release must
appear in the deferred list with a specific reason.

### 4. Preserve the existing playground flow

Status: Implemented.

- The editor must still render diagnostics as squiggles while authors type.
- Preview rendering and chart interactivity must keep using the existing WASM
  render path and sidecar consumer.
- Editor feature requests must not block preview rendering longer than the
  current render/check loop already does. If synchronous WASM calls are too
  noticeable, move editor services behind a web worker before enabling the full
  provider set by default.
- Monaco setup must remain self-contained inside the demo. VS Code extension
  behavior should not change in this release.

### 5. Add parity and offset tests

Status: Implemented.

- Add Rust tests that compare browser editor-service responses against the
  native LSP feature helpers for representative documents.
- Add explicit hover parity tests for operators, geometry names, property keys,
  source constructors, primary columns, named-table columns, and non-ASCII
  identifiers. The asserted Markdown and ranges must match the native LSP hover
  response.
- Add fixtures with non-ASCII identifiers and strings to prove byte spans map
  to Monaco ranges correctly.
- Add TypeScript checks or focused unit tests for Monaco adapter mapping where
  practical; at minimum `cd demo && npm run check` must cover the provider
  types.
- Keep existing `algraf-lsp` tests as the source-of-truth coverage for feature
  behavior.

### 6. Document the shipped browser editor contract

Status: Implemented.

- Update spec §24.7 or a nearby browser-runtime section to describe the
  playground editor-service boundary once it exists.
- Update spec §21 only if native LSP behavior changes. Pure browser adapter
  work should be documented as an additional client surface, not as a new LSP
  protocol requirement.
- Update `demo/README.md` with the supported editor features and any browser
  limitations that remain after implementation.

## v0.35.5 Should

### Browser worker isolation

Status: Not required for v0.35.5; documented as the latency fallback.

Run editor-service requests in a worker if hover/completion/semantic-token
requests create visible typing latency. This should share the existing WASM
module rather than load separate copies unless measurement proves the simpler
shape is too slow.

The shipped provider set runs scoped synchronous requests through the same WASM
instance as preview rendering. Focused Rust tests, `npm run check`, and the demo
build keep this path covered; `demo/README.md` records the worker fallback if
larger documents make latency visible.

### LSP-shaped JSON compatibility audit

Status: Implemented.

Prefer response shapes that stay close to `lsp_types` JSON so native LSP tests,
browser editor-service tests, and future clients can reuse fixtures. Document
any intentional differences caused by Monaco APIs.

### Demo smoke coverage

Status: Not applicable; no Playwright or equivalent browser harness exists in the project.

Add a small browser smoke test if the project gains a Playwright or equivalent
test harness. The smoke should verify that diagnostics, hover, completion, and
formatting all work from the built demo bundle.

## Explicitly Deferred Past v0.35.5

- A full browser JSON-RPC LSP transport.
- A hosted multi-file workspace with arbitrary filesystem access.
- SQLite-backed schema reads in the browser.
- Publishing a standalone `@algraf/editor` or `@algraf/lsp-wasm` package.
- New language features, diagnostics, or examples.
- VS Code extension UI changes.
- On-type formatting, which remains deferred by spec §21.10.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
cd demo && npm run check
cd demo && npm run build
```

The demo build is required for implementation because Monaco provider wiring and
the WASM editor-service boundary are both user-facing browser behavior.

## Promotion Workflow

1. Keep this patch release scoped to editor integration; do not mix in roadmap
   language features from v0.36 or later.
2. Identify the smallest Rust boundary that lets native LSP and WASM/browser
   editor services share feature logic.
3. Add parity tests before wiring the corresponding Monaco provider.
4. Wire Monaco providers one feature family at a time, preserving diagnostics
   and preview behavior after each step.
5. Update spec and demo docs to describe the implemented browser editor
   contract.
6. Update this plan's `Status:` lines as items land, defer, or are rejected.
