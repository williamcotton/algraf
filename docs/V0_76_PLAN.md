# Algraf v0.76.0 Plan

Status: Implemented
Target version: 0.76.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_75_PLAN.md`](V0_75_PLAN.md)
Cross-repo coordination: mirrors the PDL v0.51 agent-template plan.

## Purpose

Algraf v0.76 is an agent ergonomics release. It adds a safe project-level
initialization surface for LLM coding agents and ships a concise Algraf language
reference template that downstream projects can keep at their root.

The template exists to prevent agents from hallucinating JavaScript, Python,
Vega-Lite JSON, ggplot2/R syntax, SQL, or unimplemented Algraf source forms
when asked to create `.ag` source. It should be useful for Codex, Claude, Google
Antigravity (`agy`), and any future tool that reads root-level agent
instructions.

This release does not change Algraf source syntax, rendering semantics, data
formats, LSP protocol surface, WASM behavior, or browser packages. It does
carry one LSP maintenance fix (document-sync race; see Maintenance below).

## Release Thesis

Projects that use Algraf should be able to opt into agent guidance without
clobbering existing agent instruction files. The CLI should generate one
authoritative language guide and have tool-specific instruction files point to
it.

## Must

- Add a root language guide template for Algraf.

  Status: Implemented.

  The CLI must be able to write `ALGRAF_LANG.md` into a target project
  directory. The file should explain enough Algraf syntax, chart structure,
  algebra, geometry calls, scale/guide rules, CLI checks, and common agent
  pitfalls that an LLM can create valid `.ag` source without assuming another
  language.

- Add safe agent-file initialization.

  Status: Implemented.

  `algraf init --codex`, `algraf init --claude`, and `algraf init --agy` must
  generate root-level agent references:

  - `--codex` writes or updates `AGENTS.md`.
  - `--agy` writes or updates `AGENTS.md`.
  - `--claude` writes or updates `CLAUDE.md`.

  The command must not overwrite an existing different `ALGRAF_LANG.md`.
  Existing `AGENTS.md` or `CLAUDE.md` files must be appended with a short
  reference block unless they already mention `ALGRAF_LANG.md`.

## Should

- Allow combined targets in one command, such as
  `algraf init --codex --claude --agy`.

  Status: Implemented.

- Keep the feature separate from source-language semantics and render output.

  Status: Implemented.

## Could

- Add additional target flags later if more agent tools settle on separate
  root-level instruction filenames.

  Status: Deferred.

## Maintenance

- Fix the LSP document-sync race that produced stale, misaligned semantic
  tokens in recently edited regions.

  Status: Implemented.

  `upsert_document` only made new text visible in the document cache after the
  blocking analysis (CSV load, schema resolution) finished, while tower-lsp
  serves requests concurrently. A `semanticTokens/full` request racing the
  analysis was answered from the pre-edit text, so the editor painted tokens
  shifted relative to the current buffer (mid-word color splits after the edit
  point). The backend now inserts the latest text (with carried-over or
  pending analysis state) before spawning analysis, discards analysis results
  for superseded versions or closed documents, and ignores stale lower-version
  `didChange` notifications. Spec §21.3 documents the ordering requirements.
  The original protocol regression is now covered by the focused
  `stale_upsert_does_not_clobber_newer_document_text` backend test added in
  v0.81.0. No protocol, capability, or language change.

## Non-Goals

- No Algraf source syntax changes.
- No new chart semantics, geometries, stats, themes, scales, guides, or data
  formats.
- No browser/WASM ABI change.
- No package publication or npm dependency pin change.
- No overwrite or merge automation for existing customized language-reference
  files.
