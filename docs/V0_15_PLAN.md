# Algraf v0.15.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_14_PLAN.md`](V0_14_PLAN.md)
Follow-on plan: [`V0_16_PLAN.md`](V0_16_PLAN.md)

## Purpose

This document defines the intended v0.15.0 release shape: consolidating
diagnostic and preparation reporting after the driver has an injectable I/O
boundary.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.15.0 is a **diagnostic pipeline and partial-preparation** release: reduce the
number of places where parse diagnostics, driver errors, data warnings, semantic
diagnostics, and render diagnostics are assembled by hand.

The release is intentionally conservative. It should improve internal reporting
structure while preserving CLI output, LSP wire diagnostics, `--strict`
behavior, and generated examples.

## Current Debt Surface

The deferred-item audit and source survey found:

- v0.13 deferred a unified diagnostic sink.
- `prepare_chart` returns `Result<PreparedChart, DriverError>`, while parse and
  semantic diagnostics are stored elsewhere.
- Data inference warnings live inside `LoadResult` and `NamedTable`, not in the
  same stream as parser or semantic warnings.
- The CLI manually prints parse diagnostics, semantic diagnostics, data
  warnings, render diagnostics, JSON diagnostics, and strict-mode decisions in
  several command paths.
- The LSP has local driver-error-to-diagnostic mapping in
  `crates/algraf-lsp/src/document.rs`.
- Editor use cases need partial information from invalid documents, but strict
  render preparation still short-circuits in several places.

## Scope Rules

- No new diagnostic codes are required.
- Existing diagnostic codes, severities, spans, and ordering should remain
  stable unless a test and spec update document a deliberate correction.
- CLI human output, CLI JSON output, LSP diagnostic wire shape, and strict-mode
  exit behavior should remain stable.
- Do not change parser recovery rules.
- Do not make data warnings pretend to have source spans when only a data-column
  name is known.
- Do not add source-language, render, data-format, or editor features.

## Capstone Acceptance Target

The capstone is centralized reporting with no output drift:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

`git diff -- examples` must be empty. Running `examples/generate.sh` should not
change what happens for any checked-in example.

## Design Decisions (settled)

1. **Keep `Diagnostic` as the wire type.** v0.12 already centralized codes; this
   release should not redesign serialized diagnostics.
2. **Separate collection from rendering.** CLI and LSP adapters may remain
   distinct, but they should consume one assembled report when possible.
3. **Preserve strict mode.** `--strict` continues to decide whether warnings fail
   a command; the report model should make that decision easier to audit.
4. **Allow partial preparation beside strict preparation.** Existing render
   callers keep a strict path, while LSP-like callers can inspect partial state.
5. **Do not force data warnings into source spans.** Use structured buckets until
   a real source or data-range mapping exists.

## v0.15.0 Must

### 1. Shared preparation report model

Status: Planned.

Acceptance criteria:

- Introduce a report type, such as `DriverReport`, `PreparationReport`, or
  `DiagnosticBundle`, that can hold:
  - parse diagnostics;
  - driver/data load diagnostics;
  - data warnings with their table/source context;
  - semantic diagnostics;
  - optional render diagnostics where the caller supplies them.
- The report preserves deterministic order and enough phase/context information
  for CLI and LSP adapters.
- Existing public `PreparedChart` APIs remain available unless callers are
  migrated in the same release.
- The report type does not depend on CLI or LSP crates.

### 2. Central driver-error diagnostic mapping

Status: Planned.

Acceptance criteria:

- Driver/data loading errors map to `algraf_core::Diagnostic` or a structured
  intermediate in one shared place.
- CLI and LSP stop carrying divergent missing-file, unreadable-file, malformed
  CSV/JSON, and geospatial parse mappings where practical.
- Current codes and messages are preserved unless a test documents an intentional
  correction.
- Named-table load failures keep enough context to report the table name.
- Stdin usage errors keep their current user-facing behavior.

### 3. Partial preparation path

Status: Planned.

Acceptance criteria:

- The driver exposes a preparation path that returns partial document/chart state
  plus diagnostics instead of failing at the first recoverable phase boundary.
- Parser errors are still returned to callers even when semantic analysis cannot
  produce IR.
- Semantic analysis still runs with available schemas where current LSP behavior
  does so today.
- Strict render preparation remains available and continues to block on parse,
  load, and semantic errors as today.
- Tests cover malformed source, missing data, malformed data, unknown columns,
  and named-table failures.

### 4. Data-warning normalization

Status: Planned.

Acceptance criteria:

- Data warnings from primary data and named tables are represented with source
  context, table context, and column context.
- CLI human and JSON output stay compatible with current behavior unless a
  deliberate output-shape change is promoted into the spec.
- `--strict` continues to fail on data warnings exactly where it fails today.
- The LSP does not publish misleading source diagnostics for data warnings that
  have no source span.

### 5. CLI and LSP report adapters

Status: Planned.

Acceptance criteria:

- At least `check`, `schema`, `ir`, and render preparation use the shared report
  assembly where doing so reduces duplication.
- LSP document analysis uses the shared mapping for schema/load errors.
- Existing LSP tests for missing data and semantic diagnostics continue to pass.
- CLI integration tests prove current human/JSON diagnostic output and strict
  behavior remain stable.
- The adapters do not pull CLI or LSP dependencies into the driver.

### 6. Spec, plan, and example hygiene

Status: Planned.

Acceptance criteria:

- Workspace version is bumped to `0.15.0` when the release branch is ready.
- Spec §23.4, §24, and §26 are updated only for intended responsibility or
  diagnostic-shape clarifications.
- This plan is updated as each item completes, is rejected, or moves scope.
- Examples are regenerated with `./examples/generate.sh`; `git diff -- examples`
  must be empty.

## v0.15.0 Should

### Render diagnostic adapter

Status: Planned.

If the report model is small enough, add a render-result adapter so CLI render
can append render diagnostics through the same report structure. Do not make the
driver depend on `algraf-render`.

### Diagnostic order snapshots

Status: Planned.

Add focused snapshots or integration tests for diagnostic ordering across parse,
load, semantic, data-warning, and render-warning phases.

## Explicitly Deferred Past v0.15.0

- New diagnostic codes or rich data-file spans unless promoted deliberately.
- Changing CLI JSON shape or LSP diagnostic wire shape.
- Query-driven compilation.
- Schema/data caching.
- Async loading.
- New editor features, data formats, source syntax, or render behavior.

## Optional-Item Audit

### Promote In v0.15.0 (Must)

- Shared preparation report model.
- Central driver-error diagnostic mapping.
- Partial preparation path.
- Data-warning normalization.
- CLI and LSP report adapters.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- Render diagnostic adapter.
- Diagnostic order snapshots.

### Keep Deferred

- Wire-shape changes, query databases, caches, async loading, and new
  user-facing capabilities.

## Promotion Workflow

1. Add current-output guard tests for CLI and LSP diagnostics.
2. Introduce the shared report model without migrating command behavior.
3. Centralize driver/data error mapping.
4. Add the partial preparation path beside strict preparation.
5. Normalize data-warning context.
6. Migrate CLI and LSP adapters one path at a time.
7. Run formatter, clippy, workspace tests, `./examples/generate.sh`, and require
   an empty `git diff -- examples`.
