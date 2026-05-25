# Algraf v0.19.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_18_PLAN.md`](V0_18_PLAN.md)
Follow-on plan: [`V0_20_PLAN.md`](V0_20_PLAN.md)

## Purpose

This document defines the intended v0.19.0 release shape: preparing the runtime
data boundary for larger data, editor responsiveness, WASM, and future backend
swaps after schema caching and semantic/property boundaries have been cleaned
up.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.19.0 is a **data execution boundary** release. It does not add SQL, Polars,
remote fetching, or streaming user features. It makes those later releases
possible by separating schema-first work from full-frame execution, tightening
the table abstraction, and making blocking I/O choices explicit.

This is the last planned refactor-first release before new language and data
features resume in v0.20+.

## Current Debt Surface

The plan/spec audit and architecture notes found:

- v0.7 deferred larger-data handling and a Polars backend.
- v0.13 through v0.17 defer lazy data engines and renderer-delayed
  materialization.
- v0.14 adds injected synchronous I/O but deliberately excludes async loading.
- v0.16 plans schema caching, but not full-frame caching or query-driven
  compilation.
- The spec says derived table schemas SHOULD be computable without expensive
  full-data transforms where possible.
- The spec mentions WASM support, but no release yet attempts a compile target
  or feature-flag audit beyond v0.14's I/O-readiness note.

## Scope Rules

- No new Algraf source syntax, source constructors, data formats, render
  features, or LSP capabilities.
- Existing CLI one-shot commands continue to load fresh data unless a local
  cache is proven behavior-neutral.
- No required Polars dependency.
- No persistent disk cache.
- No network access, environment access, command execution, or SQL.
- Any async API must coexist with current synchronous compatibility wrappers.
- Rendered examples must remain unchanged.

## Capstone Acceptance Target

The capstone is a clearer data-execution boundary with no output drift:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

`git diff -- examples` must be empty.

## Design Decisions (settled)

1. **Schema-first remains separate from frame execution.** Editor validation
   should not need full data materialization when a schema is enough.
2. **Do not require a new engine.** The homegrown dataframe remains the default;
   the work is to make the boundary substitutable.
3. **Make async additive.** Existing synchronous public helpers stay available.
4. **Treat WASM as a build-shape audit first.** A browser runtime is a later
   product feature; this release removes obvious compile blockers where it can.
5. **Avoid a query framework until there is measured need.** Demand-driven work
   can be audited without committing to `salsa`.

## v0.19.0 Must

### 1. Data engine boundary audit and trait cleanup

Status: Planned.

Acceptance criteria:

- Audit all places that require concrete `DataFrame` access outside
  `algraf-data` and `algraf-render`.
- Tighten the existing `Table` abstraction or introduce a small adjacent
  `DataEngine`/`TableStore` boundary only where it removes concrete coupling.
- Parser, LSP, semantics, and driver APIs continue to avoid dataframe internals.
- Existing schemas, category ordering, missing-value behavior, and diagnostics
  remain unchanged.

### 2. Schema-only derived planning

Status: Planned.

Acceptance criteria:

- Identify built-in stats whose output schema can be computed from input schema
  and typed options without full data execution.
- Move schema-only planning for those stats into semantics or a shared planning
  helper where it belongs.
- Keep full data execution in render/runtime.
- Tests prove LSP-style analysis can obtain derived schemas without executing
  expensive transforms where possible.

### 3. Frame-cache policy design

Status: Planned.

Acceptance criteria:

- Define the difference between schema cache, full-frame cache, render-result
  cache, and persistent cache.
- Add an optional in-memory frame-cache interface only if there is a clear
  caller; otherwise document why full-frame caching remains deferred.
- Cache keys reuse the v0.16 source identity/fingerprint work.
- CLI one-shot behavior remains unchanged by default.

### 4. Additive async loading boundary

Status: Planned.

Acceptance criteria:

- Define an async-capable loading trait or adapter shape that can wrap the
  synchronous v0.14 `DriverIo` provider.
- Existing public synchronous driver helpers remain available.
- LSP preview/schema paths can opt into async or blocking-task execution without
  changing protocol behavior.
- Tests prove cancellation or supersession still prevents stale preview output.
- No network source is added in this release.

### 5. WASM build-shape audit

Status: Planned.

Acceptance criteria:

- Document crates and features that currently block `wasm32-unknown-unknown`.
- Add feature gates or dependency splits where doing so is behavior-neutral.
- Prove at least `algraf-syntax`, `algraf-core`, and schema-free semantic logic
  can be compiled or audited for WASM readiness.
- Do not promise a browser runtime, JS bindings, or web preview in this release.

### 6. Performance and resource baseline

Status: Planned.

Acceptance criteria:

- Add lightweight benchmarks or documented timing scripts for parser, schema
  loading, common stats, and representative render cases.
- Record reference machine details when thresholds are documented.
- Avoid brittle CI failures on machine-specific timing.
- Use the baseline to decide whether Polars, lazy execution, or query-driven
  compilation should be promoted later.

### 7. Spec, plan, and example hygiene

Status: Planned.

Acceptance criteria:

- Workspace version is bumped to `0.19.0` when the release branch is ready.
- Spec §10, §21, §23, §24, and §28 are updated for intended architecture and
  performance clarifications.
- This plan is updated as each item completes, is rejected, or moves scope.
- Examples are regenerated with `./examples/generate.sh`; `git diff -- examples`
  must be empty.

## v0.19.0 Should

### Query-driven compilation spike

Status: Planned.

Prototype or document how a demand-driven query database could represent source
text, parse trees, schemas, analysis, and render plans. Do not add `salsa` as a
required dependency in this release.

### Polars adapter spike

Status: Planned.

Build a private experiment or design note showing what a Polars-backed table
would need to implement to preserve Algraf's diagnostics, category ordering,
missing-value behavior, and SVG determinism.

## Explicitly Deferred Past v0.19.0

- SQL, network, URL, command, or environment-variable data sources.
- Required Polars backend.
- Lazy renderer-delayed materialization as a user-visible capability.
- Persistent caches and render-result caches.
- Browser/WASM runtime product surface.
- New chart syntax, geometries, stats, scales, projections, or output formats.

## Optional-Item Audit

### Promote In v0.19.0 (Must)

- Data engine boundary audit and trait cleanup.
- Schema-only derived planning.
- Frame-cache policy design.
- Additive async loading boundary.
- WASM build-shape audit.
- Performance and resource baseline.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- Query-driven compilation spike.
- Polars adapter spike.

### Keep Deferred

- User-visible data backend features and browser/runtime features.

## Promotion Workflow

1. Add guard tests around current data/schema/stat behavior.
2. Audit concrete dataframe coupling and tighten the table boundary.
3. Move schema-only derived planning where it can be behavior-preserving.
4. Define cache policy and add only the minimal interfaces justified by callers.
5. Add async-compatible adapters without removing synchronous wrappers.
6. Run WASM and performance audits.
7. Run formatter, clippy, workspace tests, `./examples/generate.sh`, and require
   an empty `git diff -- examples`.
