# Algraf v0.39.5 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) §10.6, §10.9, §21.7, §21.17, §24.7, §27.7
Predecessor plan: [`V0_39_PLAN.md`](V0_39_PLAN.md)
Feature-roadmap follow-on: [`V0_40_PLAN.md`](V0_40_PLAN.md)
Roadmap theme: Rust editor-service hover quality before the ggplot2-comparability
feature roadmap resumes.

## Purpose

This patch release overhauls Algraf hover behavior in the shared Rust editor
service. The immediate authoring pain is that derived-table schemas are exposed
as grey inlay text after `Derive` declarations, while the actual hover surface
does not explain the derived table when the user points at the table name.

For example:

```text
Chart(data: "samples.csv") {
    Derive binned = Bin2D(x, y, bins: 10)
    Derive trend = Smooth(x_center, y_center, method: "lm")

    Space(x_center * y_center, data: binned) {
        Rect(xmin: x_min, xmax: x_max, ymin: y_min, ymax: y_max)
    }
    Space(x * y, data: trend) {
        Line()
    }
}
```

Hovering `binned` or `trend` should show the derived table's output columns and
types. Hovering `"samples.csv"` should show a bounded preview of the source
schema and sample rows. Hovering declarations and calls such as `Chart`, `Theme`,
`Space`, and `Rect` should show the available attributes and concise examples,
not just a one-line description.

The release does not change Algraf source semantics. It moves schema and
registry knowledge into the hover surface that authors already reach for, and it
removes duplicate derived-table schema inlay text that competes with the source.

## Release Thesis

v0.39.5 is a **Rust editor hover overhaul** patch. Hover should become the
authoritative lightweight inspection surface for table schemas, source samples,
declaration attributes, geometry properties, and usage examples.

The implementation belongs in `algraf-editor-services`, with the native
`algraf-lsp` server and the WASM/browser editor-service ABI both calling the
same Rust hover helper. VS Code and Monaco adapters may display LSP-shaped hover
Markdown, but they must not decide Algraf hover meaning, build schema previews,
or maintain separate documentation tables.

Success means an Algraf author can point at the symbol they are already reading
and learn:

- what table a `Derive` name produces;
- what columns and types are available from that table;
- what a source CSV appears to contain from the bounded schema/sample read;
- what attributes a declaration or geometry accepts;
- a small valid example for common calls.

## Scope Rules

- **No new language behavior.** Do not add syntax, stats, geometries,
  properties, diagnostics, CLI flags, or renderer behavior for this patch.
- **Rust owns hover semantics.** All new hover decision logic and Markdown
  assembly must live in shared Rust editor-service code used by native LSP and
  WASM/browser editor-service requests.
- **No TypeScript hover fork.** VS Code and Monaco glue may map LSP hover ranges
  and Markdown to client APIs, but they must not inspect Algraf syntax or data
  schemas to synthesize Algraf documentation.
- **Hover replaces derived-schema inlay text.** Derived-table output schemas
  should be available on hover over the derived-table name and references. The
  grey inlay text that redundantly lists derived-table columns after each
  `Derive` declaration should be removed or disabled by default.
- **Samples are bounded and provisional.** Source-file hover may show sampled
  column types and rows, but it must respect existing schema-cache limits, avoid
  large hot-path reads, and label sampled information as provisional where the
  spec requires it.
- **Registry docs stay single-source.** Attribute lists and examples for
  `Chart`, `Theme`, `Space`, `Rect`, and other calls must come from shared
  registry/declaration metadata or a shared Rust documentation table, not from
  per-client strings.
- **Incomplete source remains safe.** Hover must degrade gracefully while the
  document is partially typed, while data is missing, or while semantic analysis
  is unavailable.
- **Ranges remain part of the contract.** Hover ranges must continue using LSP
  UTF-16 positions externally and Algraf byte spans internally. Non-ASCII
  fixtures are required for any new hover target kind.

## Current State

### Shared editor services

- `crates/algraf-editor-services/src/hover.rs` already owns native and browser
  hover behavior for operators, source constructors, geometry names, property
  keys, string-valued options, and sampled source columns.
- Column hover can show type, source, and examples when a primary or named-table
  schema is available.
- Hover over a `data:`-bound derived-table reference does not yet show the
  derived table's output schema as a table-level object.
- Hover over a `Derive` declaration name does not yet summarize the produced
  table.
- Hover over source strings such as `"samples.csv"` does not yet show a compact
  schema and row preview.
- Hover over declaration names such as `Chart`, `Space`, and `Theme` is not as
  complete as geometry/property registry hover.

### Inlay hints

- `crates/algraf-editor-services/src/inlay.rs` emits derived-table output
  schemas as inline grey text after each in-document `Derive`.
- This duplicates information that belongs on hover and creates visual noise in
  normal editing.

### Native LSP and browser editor service

- `algraf-lsp` calls the shared Rust `hover_at` helper.
- The WASM editor-service ABI returns LSP-shaped hover results from the same
  editor-services crate.
- This shared path is the required implementation route for the overhaul.

## Hover Target Sketches

These sketches describe the intended Markdown content shape. Exact wording may
change, but tests should lock the useful facts and ranges.

### Derived-table declaration name

Hovering `binned` in:

```text
Derive binned = Bin2D(x, y, bins: 10)
```

should show:

```markdown
**Derived table `binned`**

Produced by `Bin2D(x, y, ...)`.

Columns:

| Column | Type |
| ------ | ---- |
| x_min | float |
| x_max | float |
| x_center | float |
| y_min | float |
| y_max | float |
| y_center | float |
| count | integer |
```

### Derived-table reference

Hovering `trend` in:

```text
Space(x * y, data: trend) {
    Line()
}
```

should show the same table-level schema summary, with enough context to make it
clear that `Space` is bound to that derived table.

### Source string

Hovering `"samples.csv"` in:

```text
Chart(data: "samples.csv") {
    Space(x * y) { Point() }
}
```

should show a bounded source preview:

```markdown
**Data source `samples.csv`**

Sampled schema:

| Column | Type | Examples |
| ------ | ---- | -------- |
| x | float | 1.2, 1.8 |
| y | float | 4.0, 4.6 |
| group | string | A, B |

Sample rows:

| x | y | group |
| - | - | ----- |
| 1.2 | 4.0 | A |
| 1.8 | 4.6 | B |

Provisional LSP sample.
```

If row samples are unavailable but schema is available, the hover should still
show columns and types. If neither is available, it should explain that the
source could not be sampled without replacing diagnostics.

### Declaration and geometry calls

Hovering `Chart`, `Theme`, `Space`, or `Rect` should show:

- the call kind and short description;
- every accepted attribute/property with type or accepted value set;
- defaults where the registry defines them;
- one concise valid example.

`Rect` should include its required bound properties (`xmin`, `xmax`, `ymin`,
`ymax`) and commonly used style properties. `Theme` should include theme names
and override properties. `Space` should explain its algebra argument and
supported arguments such as `data`, `coords`, `theta`, `innerRadius`, and
projection where applicable.

## v0.39.5 Must

### 1. Add derived-table schema hover

Status: Implemented.

- Hover over a `Derive` declaration name must show the derived table name, stat
  producer, and output schema from semantic analysis.
- Hover over `data: derived_name` must show the same table schema summary.
- Hover over columns inside a `Space(..., data: derived_name)` block must keep
  resolving against the derived table schema.
- Chained derived tables must work: `trend` produced from `binned` should show
  the `Smooth` output schema, while `x_center` and `y_center` continue to resolve
  against `binned` where that table is in scope.
- Ambiguous or failed analysis must return no misleading schema. Prefer a short
  fallback hover only when the name is known but output schema is unavailable.

### 2. Replace derived-schema inlay hints with hover

Status: Implemented.

- Stop emitting the grey inlay hint that lists derived-table columns after each
  `Derive`.
- Update spec §21.17 so derived-table schema inlay hints are no longer described
  as the active editor behavior.
- Remove or rewrite tests that assert derived-schema inlay text. Add replacement
  hover tests for the same information.
- Keep the LSP inlay-hint capability only if another useful inlay-hint family
  remains. Otherwise stop advertising it from the native server and browser
  editor-service surface for now.

### 3. Add source-string schema and row previews

Status: Implemented.

- Hover over `Chart(data: "file.csv")` and `Table name = "file.csv"` source
  strings must show the resolved source label, sampled columns, inferred types,
  and a bounded row preview when available.
- Browser editor-service hover must use host-supplied in-memory files for these
  previews, matching the existing browser data boundary.
- Native LSP hover must use the existing driver/schema cache and file-size caps;
  it must not add unbounded filesystem reads on the hover hot path.
- Missing, unreadable, unsupported, or too-large sources must degrade gracefully
  and must not duplicate diagnostics as noisy hover errors.
- Tests must cover primary CSV sources, named `Table` CSV sources, in-memory
  browser files, unavailable sources, and non-ASCII path or header text.

### 4. Add complete call hovers for declarations and geometries

Status: Implemented.

- Hover over `Chart`, `Space`, `Theme`, `Scale`, `Guide`, `Layout`, and `Table`
  must use shared declaration metadata to list accepted attributes and examples.
- Hover over geometry and stat call names, including `Rect`, must list accepted
  properties/arguments and examples from the registry.
- Attribute lists must include value kinds or accepted enum values where the
  registry already knows them.
- Examples must be valid Algraf snippets for the currently implemented language.
- The hover text must stay compact enough for editor popovers; long property
  families may be grouped, but required properties and common properties must be
  visible without client-specific expansion behavior.

### 5. Centralize hover Markdown helpers

Status: Implemented.

- Add shared Rust helpers for Markdown tables, schema summaries, source previews,
  property lists, and examples.
- Keep formatting deterministic so hover snapshot tests are stable.
- Escape Markdown-sensitive characters from column names, paths, and sample
  values.
- Preserve existing operator, property, source-constructor, string-option, and
  column hover behavior unless the new helper intentionally improves the same
  content.

### 6. Add native LSP and WASM editor-service parity tests

Status: Implemented.

- Add editor-services unit tests for each new hover target kind.
- Add native `algraf-lsp` request tests proving hover returns the same content
  and ranges over UTF-16 positions.
- Add WASM/browser editor-service tests proving in-memory files feed source
  string previews and that hover Markdown matches the shared helper.
- Add non-ASCII fixtures for derived-table names, source headers, string paths,
  and hover ranges.
- Add regression tests that prove derived-schema inlay text is gone or no longer
  advertised.

### 7. Update spec and docs

Status: Implemented.

- Update spec §21.7 to make derived-table name hover, source-string preview
  hover, and call attribute/example hover explicit.
- Update spec §21.17 to remove the old derived-schema inlay-hint behavior or
  mark inlay hints as having no active v0.39.5 surface if the provider is
  disabled.
- Update spec §24.7 only if the browser editor-service ABI result set or
  advertised feature set changes.
- Update the VS Code extension README and demo README only if user-visible
  feature lists mention derived-table inlay hints or omit the new hover
  behavior.

## v0.39.5 Should

### Hover content snapshots

Status: Implemented.

Add focused snapshot-style tests for representative hover Markdown. The tests
should assert stable facts and layout without making harmless prose edits
unnecessarily expensive.

### Registry metadata completeness audit

Status: Implemented.

Audit declaration and geometry metadata for missing accepted-value lists,
defaults, and examples. Fill gaps needed by `Chart`, `Theme`, `Space`, and
`Rect` first, then extend the same pattern to the rest of the registry.

### Source preview row cap setting

Status: Implemented.

Use the existing schema/sample cap if it already covers row previews. Add a
small internal constant only if the current cap is column-focused and would make
hover too large.

## Explicitly Deferred Past v0.39.5

- New Algraf language constructs or renderer behavior.
- A custom VS Code or Monaco hover UI beyond standard LSP Markdown hover.
- Client-side TypeScript schema readers.
- Full-data previews or sortable/filterable table inspection in hover.
- Network-backed source sampling.
- SQLite row previews in WASM, because the browser build does not enable native
  SQLite support.
- User-configurable hover verbosity unless the default compact hover proves
  insufficient after implementation.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
cd demo && npm run check
cd demo && npm run build
```

The demo checks are required if the implementation changes the browser
editor-service ABI or advertised provider set. Pure native/server hover changes
still need Rust formatting, lint, and tests.

## Promotion Workflow

1. Keep this patch release scoped to Rust editor-service hover behavior; do not
   mix in v0.40 scale/guide language work.
2. Specify hover target kinds and Markdown facts before coding.
3. Add or extend shared registry/declaration metadata before writing per-target
   hover formatting.
4. Implement hover behavior once in `algraf-editor-services`.
5. Route native LSP and WASM/browser editor-service tests through that shared
   implementation.
6. Remove or disable duplicate derived-schema inlay text only after equivalent
   hover coverage exists.
7. Update spec and docs to describe the new hover contract.
8. Update this plan's `Status:` lines as items land, defer, or are rejected.
