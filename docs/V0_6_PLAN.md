# Algraf v0.6.0 Plan

Status: Planned (not started)
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_5_PLAN.md`](V0_5_PLAN.md)

## Purpose

This document defines the intended v0.6.0 release shape: broadening the data
story beyond a single CSV file.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when syntax, diagnostics, tests, and
examples land together.

## Release Thesis

v0.6.0 is a **data backends** release: get data into Algraf from more than one
CSV file, and keep the rest of the pipeline unchanged.

This is the highest-risk release in the roadmap and is intentionally last. The
parser, semantics, scales, and renderer are decoupled from concrete dataframe
internals (spec §10.5), which is exactly the seam this release exploits: new
sources and backends must plug in behind the existing `DataFrame` boundary
without leaking into parser, LSP, semantics, or render.

It does NOT add new chart types (v0.3) or new authoring syntax (v0.5) beyond what
is needed to name a data source.

## Scope Rules

- New data sources and backends MUST sit behind the dataframe boundary (spec
  §10.5); parser/LSP/semantics/render MUST NOT gain backend-specific knowledge.
- Output MUST stay deterministic regardless of backend (spec §18.12, §23.6).
- Schema inference, type inference, and diagnostics MUST behave identically
  across backends for equivalent data.
- Prefer one well-integrated source type over several shallow ones.
- Security: any new source path/handle goes through the source-security rules
  (spec §10.8); network and arbitrary-command sources stay deferred.

## Current Data Surface (baseline)

Version 0.1+ supports CSV files and `stdin` (spec §10.1). Schema and type
inference operate on the in-memory dataframe (spec §10.3–10.5). No SQL, no
columnar formats, no alternative backend.

## v0.6.0 Must

### 1. Embedded SQL Data Sources

Status: Not started. Listed under the standing deferred list and as a future
feature gate (spec §30.3).

Allow a chart to source rows from an embedded SQL query (SQLite to start), with
no external server.

Minimum target:

```ag
Chart(data: sqlite("sales.db", "SELECT region, revenue FROM sales")) {
    Space(region * revenue) {
        Bar(stat: "identity")
    }
}
```

Acceptance criteria:

- A new `data:` source form names a SQLite database file plus a query; the exact
  surface syntax is specified in §10.1 before implementation.
- Query results are loaded into the existing `DataFrame`; schema/type inference
  reuses the CSV path's logic (spec §10.3) where types are ambiguous.
- The source resolves relative to the source file directory, like CSV paths, and
  honors the source-security model (spec §10.8).
- Diagnostics for missing database file, invalid query, and unsupported column
  types map to useful spans. Reserve new diagnostic codes in the spec first.
- Determinism: row ordering is defined (e.g. requires explicit `ORDER BY`, or the
  loader sorts deterministically) and documented.
- Tests cover schema inference, type mapping, and error cases; an example using
  a small checked-in SQLite fixture.

### 2. Additional File Formats

Status: Not started.

Support at least one columnar/structured format beyond CSV — a tractable first
target is delimited variants (TSV) and/or JSON/NDJSON — reusing the dataframe and
inference pipeline.

Acceptance criteria:

- Format is selected by extension or an explicit source form, specified in §10.1.
- Loading produces the same `DataFrame` shape as CSV; downstream behavior is
  identical for equivalent data.
- Schema/type inference rules for the new format are specified (spec §10.3).
- Diagnostics for malformed input map to useful locations.
- Tests for parsing, inference, and errors; an example with a checked-in fixture.

### 3. Spec, Version, and Example Hygiene

Status: Not started; mirrors prior releases.

Acceptance criteria:

- `Cargo.toml` workspace version bumped to `0.6.0` when the release branch is ready.
- Spec §10.1 (data source model), §10.3 (schema inference), §10.8 (source
  security), and any new diagnostic codes made normative.
- If a feature gate is introduced for SQL (spec §30.3), specify it explicitly.
- README gains examples for the new sources; fixtures checked into `examples/`.
- Examples regenerated via `./examples/generate.sh`.
- This document updated as each item completes, is rejected, or moves scope.

## v0.6.0 Should

### Polars Backend

Status: Not started. Listed under the standing deferred list.

Replace or supplement the in-house dataframe internals with Polars for
performance, keeping the dataframe boundary (spec §10.5) and all observable
behavior unchanged. Large effort; keep a Should and treat as an internal swap
that must not change rendered output or diagnostics.

Acceptance criteria (if implemented):

- No change to parser/LSP/semantics/render code beyond the dataframe boundary.
- Identical rendered output and diagnostics on existing examples (snapshot tests
  must not change).
- Determinism preserved (spec §18.12).

### Larger-Data Handling

Status: Not started.

Investigate handling datasets larger than comfortable for full in-memory
rendering — chunked loading or sampling for schema inference. Keep exploratory;
streaming/million-row architecture remains a non-goal for this release.

## Explicitly Deferred Past v0.6.0

Carried forward and unchanged unless a later planning decision moves them:

- Network/URL data sources and arbitrary-command sources (security surface).
- Streaming or million-row rendering architecture.
- WebAssembly runtime.
- Interactive or animated output.
- Plugins and custom stats.
- Everything under the standing deferred list in [`V0_3_PLAN.md`](V0_3_PLAN.md)
  not promoted here.

## Optional-Item Audit

### Promote In v0.6.0 (Must)

- Embedded SQL (SQLite) data sources.
- At least one additional file format (TSV and/or JSON/NDJSON).

### Consider If Capacity Allows (Should)

- Polars backend (behavior-preserving internal swap).
- Larger-data handling (chunked load / sampled inference).

### Keep Deferred

- Network/URL and command sources.
- Streaming / million-row architecture.
- WASM, interactive output, plugins, custom stats.

## Promotion Workflow

1. Move the chosen behavior into the relevant normative section of
   [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) (data model §10, security §10.8,
   feature gates §30.3).
2. Reserve or add diagnostic codes before implementation if behavior can fail.
3. Implement behind the dataframe boundary; keep parser/LSP/semantics/render
   backend-agnostic (spec §10.5).
4. Add focused tests in `algraf-data` and snapshot tests proving unchanged output.
5. Add or update examples with checked-in fixtures when sources change.
6. Regenerate examples when rendered output changes.
7. Update this document when a candidate is completed, rejected, or moved scope.
