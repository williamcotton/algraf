# Algraf v0.18.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_17_PLAN.md`](V0_17_PLAN.md)
Follow-on plan: [`V0_19_PLAN.md`](V0_19_PLAN.md)

## Purpose

This document defines the intended v0.18.0 release shape: hardening semantic
and registry boundaries after the driver, diagnostics, schema cache, and render
execution boundary refactors have landed.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.18.0 is a **semantic surface hardening** release: make the registries,
property validation, source-constructor metadata, and LSP test ownership strong
enough to support later language and backend features without another round of
stringly-typed glue.

This release intentionally avoids new source syntax, data sources, render
features, and output formats. It turns several items that have been repeatedly
listed as deferred refactors into concrete Must items: full typed
geometry-property IR, source-constructor registry cleanup, LSP feature tests by
module, and resilience/resource guardrails.

## Current Debt Surface

The plan/spec audit found:

- v0.10, v0.11, and v0.12 all deferred full typed geometry-property IR.
- v0.10 deferred semantic string/display helpers.
- v0.12 deferred LSP feature tests by module after the module split.
- `algraf-syntax::SourceFormat` still names runtime formats directly
  (`GeoJson`, `Shapefile`), and the driver maps that enum into
  `algraf-data::Format`.
- The spec mentions future source constructors such as `Sqlite(...)`; adding
  those without a shared registry would widen syntax/data coupling.
- The spec recommends parser/property resilience tests and denial-of-service
  limits, but those are not yet a Must item in any release plan.

## Scope Rules

- No new Algraf source syntax, source constructors, data formats, render
  features, or LSP protocol features.
- Existing diagnostics, CLI/LSP wire shapes, and rendered examples should remain
  stable unless a deliberate correction is documented.
- Keep source constructors explicit language constructs; this release may
  centralize metadata, but it MUST NOT turn arbitrary runtime strings into
  accepted constructors.
- Do not introduce SQL, async loading, Polars, lazy data materialization,
  plugins, or new output backends.
- Prefer typed internal boundaries over new public APIs unless a current caller
  needs the API.

## Capstone Acceptance Target

The capstone is a stronger semantic boundary with no behavior drift:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

`git diff -- examples` must be empty.

## Design Decisions (settled)

1. **Type properties before adding more properties.** The current registry has
   enough surface area that future features need a typed semantic boundary.
2. **Centralize constructor metadata without generalizing syntax.** The syntax
   crate may know recognized constructor names, but data-format behavior should
   be described by shared metadata rather than scattered matches.
3. **Move LSP tests near feature logic.** Cross-feature protocol tests remain,
   but module-level behavior should be reviewable where it is implemented.
4. **Make resilience limits explicit.** Parser and analyzer guardrails should be
   tested before later releases add deeper syntax.

## v0.18.0 Must

### 1. Typed geometry-property IR

Status: Planned.

Acceptance criteria:

- Replace built-in geometry properties at the semantic/render boundary with a
  typed representation where doing so removes string-keyed lookup from render
  code.
- Preserve user-authored spans for every mapping and setting.
- Keep dynamic registry metadata for completions, hover, and signature help.
- Existing diagnostics for unknown properties, duplicate properties, missing
  required properties, and type mismatches keep their codes and spans unless a
  deliberate correction is documented.
- Render output for all checked-in examples remains byte-for-byte stable.

### 2. Registry display and documentation helpers

Status: Planned.

Acceptance criteria:

- Add `Display` or `as_str` helpers for semantic enums where CLI, LSP, render,
  or tests currently duplicate string matches.
- Geometry names, property names, stat names, source constructor names, scale
  targets, and theme override keys have one authoritative display spelling.
- LSP completion, hover, and signature help reuse registry metadata where that
  is practical.
- The change does not pull CLI, LSP, or render dependencies into semantics.

### 3. Source-constructor registry boundary

Status: Planned.

Acceptance criteria:

- Introduce a shared source-constructor metadata table for recognized
  constructors such as `GeoJson` and `Shapefile`.
- The metadata records constructor name, explicit format policy, path-argument
  rules, documentation, and completion text.
- The driver maps source metadata into data-loader format policy without
  hardcoding syntax enum variants in multiple places.
- Existing accepted syntax and diagnostics remain unchanged.
- The registry is ready for later `Sqlite(...)` work without accepting
  unplanned constructors in this release.

### 4. LSP feature tests by module

Status: Planned.

Acceptance criteria:

- Add focused tests near feature modules for completion, hover, semantic tokens,
  navigation, signature help, rename, inlay hints, diagnostics conversion, code
  actions, and preview helpers where coverage is currently only integration
  style.
- Keep a smaller integration test suite for cross-feature protocol behavior.
- Tests cover non-ASCII span conversion in at least one module-level test.
- No LSP capability advertisement changes unless a test documents an existing
  mismatch.

### 5. Resilience and resource guardrails

Status: Planned.

Acceptance criteria:

- Add explicit parser/analyzer tests or limits for algebra nesting depth, array
  nesting depth, and deeply malformed documents.
- Add property-style parser tests if the project has a suitable harness; if not,
  add deterministic fixture coverage that exercises the same risk.
- Ensure formatter behavior remains stable for invalid input.
- Resource-limit diagnostics or user-facing errors are added only if the spec is
  updated in the same change.

### 6. Spec, plan, and example hygiene

Status: Planned.

Acceptance criteria:

- Workspace version is bumped to `0.18.0` when the release branch is ready.
- Spec §13, §21, §23, §27, and §29 are updated only for intended architecture or
  guardrail clarifications.
- This plan is updated as each item completes, is rejected, or moves scope.
- Examples are regenerated with `./examples/generate.sh`; `git diff -- examples`
  must be empty.

## v0.18.0 Should

### Parser recovery fixture expansion

Status: Planned.

Add a small fixture corpus for recovery cases that are currently unit-test only,
especially malformed nested calls, map literals, and source constructors.

### On-type formatting policy note

Status: Planned.

Document why on-type formatting remains deferred or define the narrow safe cases
that could be enabled later. Do not implement on-type formatting in this
release.

## Explicitly Deferred Past v0.18.0

- New source syntax, source constructors, data formats, or chart features.
- SQL, network sources, environment-variable access, command sources, and
  feature gates.
- Async driver APIs, WASM runtime support, Polars, lazy execution, and
  query-driven compilation.
- Render backends, interactivity, plugins, custom stats, and user-defined
  functions.

## Optional-Item Audit

### Promote In v0.18.0 (Must)

- Full typed geometry-property IR.
- Semantic string/display helpers.
- Source-constructor registry boundary.
- LSP feature tests by module.
- Resilience and resource guardrails.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- Parser recovery fixture expansion.
- On-type formatting policy note.

### Keep Deferred

- User-facing language, data, render, backend, and plugin features.

## Promotion Workflow

1. Add semantic/render guard tests around current property behavior.
2. Introduce typed property IR while preserving registry metadata.
3. Centralize registry display strings and docs.
4. Add source-constructor metadata without changing accepted syntax.
5. Move LSP tests toward module ownership.
6. Add resilience/resource guard coverage.
7. Run formatter, clippy, workspace tests, `./examples/generate.sh`, and require
   an empty `git diff -- examples`.
