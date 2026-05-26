# Algraf v0.21.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_20_PLAN.md`](V0_20_PLAN.md)
Follow-on plan: [`V0_22_PLAN.md`](V0_22_PLAN.md)

## Purpose

This document defines the intended v0.21.0 release shape: promoting the
data-backend features that have been deferred since v0.7, starting with local
SQLite and then adding the security model needed for opt-in remote sources.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.21.0 is a **data backends and source security** release. It fills the
highest-value hole from the old v0.7 data-backends plan: embedded SQL via
SQLite, behind the same source-expression and dataframe boundaries as file
formats and geospatial sources.

Remote SQL, URL sources, credential lookup, and command sources are security
sensitive. This plan defines their opt-in model, but only promotes remote
fetching when the guardrails are concrete and tested.

## Current Debt Surface

The plan/spec audit found:

- v0.7's embedded SQLite Must item was deferred after TSV/JSON/NDJSON shipped.
- v0.8 deferred networked SQL, PostGIS geometry columns, `env("VAR")`
  credentials, async DB drivers, and deterministic SQL row-ordering rules.
- The spec lists SQL sources and feature gates as future work.
- The driver I/O provider deliberately excludes network, environment, process,
  and async operations until a later release designs them.
- Larger-data handling and sampled/chunked schema inference remain exploratory.

## Scope Rules

- New data sources must stay behind the dataframe/table boundary.
- Parser, LSP, semantics, and render must not gain backend-specific row access.
- SQL sources are opt-in through explicit source syntax and, if required,
  feature gates.
- Network access remains off by default.
- Arbitrary command execution remains deferred unless a separate security review
  promotes it.
- Deterministic row ordering is a release blocker for SQL examples.

## Capstone Acceptance Target

The capstone is a checked-in SQLite example whose output is deterministic:

```ag
Algraf(version: "0.21", features: ["sql"])

Chart(data: Sqlite("sales.db", "SELECT region, revenue FROM sales ORDER BY region")) {
    Space(region * revenue) {
        Bar(stat: "identity")
    }
}
```

The release must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

## Design Decisions (settled)

1. **SQLite first.** Local embedded SQL is valuable without adding network or
   credentials.
2. **Queries must be deterministic.** Either require `ORDER BY`, document row
   order, or impose a stable ordering policy.
3. **Security is source-level and CLI-visible.** Network and environment access
   require explicit opt-in; they must not appear as hidden behavior.
4. **Geometry columns reuse Simple Features.** Future spatial SQL feeds the
   existing `Geometry` column type and `Geo` mark.

## v0.21.0 Must

### 1. SQLite source constructor

Status: Implemented.

Acceptance criteria:

- Add a `Sqlite("path.db", "SELECT ...")` source constructor usable in
  `Chart(data:)` and `Table name = ...`.
- Database paths resolve with the same source/base-dir rules as file sources.
- Query results load into the existing dataframe with deterministic column order
  and type inference.
- Missing database files, unreadable databases, invalid queries, and unsupported
  types produce diagnostics with useful spans.
- Tests cover schema inference, full load, named tables, malformed queries, and
  LSP completions.

### 2. SQL determinism and safety rules

Status: Implemented.

Acceptance criteria:

- Specify whether queries must contain `ORDER BY` or how Algraf stabilizes row
  order when they do not.
- SQL execution is read-only; write statements are rejected.
- Multi-statement SQL is rejected unless the spec explicitly allows it.
- Error messages do not leak unnecessary local environment details.
- Examples use small checked-in fixtures.

### 3. SQL feature gate

Status: Implemented.

Acceptance criteria:

- If v0.20 feature gates exist, SQL uses a documented gate or is explicitly
  declared stable enough to need none.
- CLI and LSP diagnostics explain when SQL syntax is gated off.
- Completion and hover document the constructor only under the chosen policy.
- The feature-gate mechanism remains reusable for later network and plugin
  features.

### 4. Larger-data schema and loading policy

Status: Implemented.

Acceptance criteria:

- Define sample-size behavior for SQL schema inference.
- Make LSP schema reads bounded and cancellable where practical.
- Record which backends support streaming, chunked reads, or only eager loads.
- CLI render behavior remains deterministic and complete for supported sources.

### 5. Opt-in remote source security model

Status: Implemented as spec/security design. Network, credentials, and command
sources remain disabled by default.

Acceptance criteria:

- Specify how network sources are enabled from CLI, LSP, and config surfaces.
- Define whether URL-valued data sources are accepted and which schemes are
  allowed.
- Define credential access, including whether `env("VAR")` is supported and how
  it is gated.
- No network source is enabled by default.
- This item may ship as design/spec only if implementation risk is too high.

### 6. Spec, plan, and example hygiene

Status: Implemented.

Acceptance criteria:

- Workspace and VS Code versions are bumped to `0.21.0` when the release branch
  is ready.
- Spec §7, §10, §21, §22, §23, §26, §29, and §30 are updated for promoted SQL
  and security behavior.
- README and examples include a SQLite tutorial with checked-in fixtures.
- Examples are regenerated with `./examples/generate.sh`.

## v0.21.0 Should

### Postgres/PostGIS prototype

Status: Deferred past v0.21. The security model is specified, but no remote SQL
source ships in this release.

Prototype a gated `Postgres(...)` or design note for networked SQL, including
ordinary columns and PostGIS geometry columns decoded into `geo_types`.

### Polars-backed file adapter

Status: Deferred past v0.21. The SQLite path did not require changing the file
adapter backend.

If v0.19's Polars spike is favorable, add an optional feature-gated adapter for
one file format and prove snapshots and diagnostics stay stable.

## Explicitly Deferred Past v0.21.0

- Arbitrary command sources.
- Network access by default.
- Unbounded SQL result loading in LSP hot paths.
- SQL writes or user-defined SQL functions.
- Full data warehouse connectors beyond the first remote-SQL prototype.

## Optional-Item Audit

### Promote In v0.21.0 (Must)

- SQLite source constructor.
- SQL determinism and safety rules.
- SQL feature gate.
- Larger-data schema/loading policy.
- Opt-in remote source security model.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- Postgres/PostGIS prototype.
- Optional Polars-backed file adapter.

### Keep Deferred

- Command sources, default network access, and broad connector sprawl.

## Promotion Workflow

1. Add source-constructor and diagnostics tests before implementing SQLite.
2. Add SQLite loading behind the dataframe boundary.
3. Define deterministic query and read-only rules.
4. Wire LSP schema/completion and CLI commands through the driver.
5. Document and gate network/security follow-ups.
6. Add examples and fixtures.
7. Run formatter, clippy, workspace tests, regenerate examples, and review
   intentional example diffs.
