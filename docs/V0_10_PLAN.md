# Algraf v0.10.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_9_PLAN.md`](V0_9_PLAN.md)
Follow-on plans: [`V0_11_PLAN.md`](V0_11_PLAN.md),
[`V0_12_PLAN.md`](V0_12_PLAN.md)

## Purpose

This document defines the intended v0.10.0 release shape: decomposing the
semantic analyzer and making the stat/lowering boundary typed.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when syntax, diagnostics, tests, and
examples land together.

## Release Thesis

v0.10.0 is a **semantic architecture** release: turn
`crates/algraf-semantics/src/analyzer.rs` from one 3,700+ line stateful object
into explicit semantic passes, while avoiding gratuitous churn at current crate
call sites.

The release starts after v0.9 centralizes driver/source behavior. v0.10 owns the
semantic internals: duplicate argument checking, value extraction, stat option
typing, synthetic names, and high-level geometry lowering.

It still avoids new source-language and render features. Because this roadmap is
pre-release, `algraf ir` JSON and internal APIs may change when the typed stat
boundary improves the design. The contract is traceability: diagnostic,
render-output, and IR-shape changes must be deliberate and covered by tests and
spec updates, not accidental side effects of file moves.

## Current Debt Surface

The refactor survey identified `analyzer.rs` as the largest god object:

- chart argument parsing and defaults;
- source expression recognition;
- named table and derived table resolution;
- let binding evaluation/substitution;
- frame construction and column resolution;
- guide, theme, and scale validation;
- geometry property validation;
- explicit stat validation;
- high-level geometry lowering;
- synthetic name allocation;
- repeated argument duplicate checks and value extractors.

The largest semantic functions are also the most coupled:

- `scale_decl` at roughly 369 lines;
- `space` at roughly 156 lines;
- `desugar_count_bar` at roughly 137 lines;
- `derive` at roughly 132 lines;
- `guide_decl` at roughly 124 lines;
- `collect_bin_settings` at roughly 94 lines.

## Scope Rules

- Existing semantic entrypoints remain available unless a call-site cleanup is
  deliberately included:
  `analyze`, `analyze_with_tables`, `analyze_chart`,
  `analyze_chart_with_tables`, and `analyze_source`.
- Keep diagnostic codes, severities, and spans stable unless tests document a
  deliberate correction.
- Keep IR semantics stable except for the typed stat-option representation
  called out below.
- Do not add source syntax, geometry names, stat names, data formats, or render
  behavior.
- Prefer small pass modules over abstract frameworks. A trait/registry is only
  justified where it removes real complexity.

## Capstone Acceptance Target

The capstone is semantic equivalence:

```bash
cargo test -p algraf-semantics
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

The current checked-in examples are the visual regression baseline:
`git diff -- examples` must be empty after regeneration. If the typed stat IR
changes `algraf ir` JSON, tests must document the new shape.

## Design Decisions (settled)

1. **Split by semantic concern.** Organize analyzer internals around chart,
   sources/tables, frames, properties, scales, guides, themes, stats, and
   lowering.
2. **Keep one analysis context.** Shared state should be explicit: diagnostics,
   primary and named schemas, derived schemas, let scopes, and synthetic names.
3. **Type stat options before geometry properties.** Stats have a small finite
   option surface and are currently parsed twice. Geometry properties remain
   dynamic for now.
4. **Move lowering before abstracting it.** First isolate high-level geometry
   lowering into one module; then extract helpers for repeated frame validation,
   synthetic derive creation, and setting passthrough.

## v0.10.0 Must

### 1. Semantic behavior guard tests

Status: Not started.

Acceptance criteria:

- Add or strengthen semantic tests that cover:
  - chart arguments and duplicate diagnostics;
  - named tables and derived table dependency ordering;
  - `let` scope and substitution;
  - frames, facets, spatial spaces, and projection arguments;
  - guide/theme/scale validation;
  - explicit `Derive` stats;
  - high-level lowering for `Histogram`, `FreqPoly`, `Bin2D`, `Density`, and
    `Bar(stat: "count")`;
  - diagnostic spans for lowered nodes and stat settings.
- Tests should make later code moves reviewable without relying only on rendered
  SVG snapshots.

### 2. Analyzer module decomposition

Status: Not started.

Acceptance criteria:

- Existing semantic callers are migrated deliberately if API cleanup is part of
  the split.
- `Analyzer` state is reduced to a small context or coordinator.
- Analyzer code is split into modules along these lines:
  - `context.rs`;
  - `chart.rs`;
  - `sources.rs` / `tables.rs`;
  - `frames.rs`;
  - `properties.rs`;
  - `scales.rs`;
  - `guides.rs`;
  - `themes.rs`;
  - `stats.rs`;
  - `lowering.rs`.
- No single semantic source file retains the current analyzer's broad
  responsibility set.
- The move is behavior-preserving before any deeper stat/lowering changes land.

### 3. Unique argument and value extraction helpers

Status: Not started.

Acceptance criteria:

- Repeated `seen: HashMap<String, Span>` duplicate checks are replaced with a
  helper that can emit the correct code/message for:
  - ordinary duplicate arguments (`E1002`);
  - duplicate table names (`E1105`);
  - duplicate geometry properties (`E1203`);
  - duplicate stat settings (`E1404`).
- Typed value extractors cover common argument forms: string, bool, number,
  positive integer, null flag, enum string, color string, axis selector, and
  map/numeric bounds where appropriate.
- The four `theme_scalar_*` functions are collapsed into one generic helper or
  equivalent local abstraction inside `themes.rs`.
- These helpers reduce repetition without hiding diagnostic span choices.

### 4. Typed stat options in IR

Status: Not started.

Replace string-keyed stat settings at the semantic/render boundary.

Minimum target:

```rust
enum StatOptionsIr {
    Bin { bins: Option<f64>, bin_width: Option<f64>, boundary: Option<f64>, closed: BinClosedIr },
    Bin2D { bins: Option<f64> },
    HexBin { bins: Option<f64> },
    Smooth { method: SmoothMethodIr },
    Density { bandwidth: Option<f64>, grid_points: Option<f64> },
    Count,
}
```

Acceptance criteria:

- `StatCallIr` no longer carries built-in stat settings as generic
  `Vec<Setting>`.
- Explicit `Derive` declarations and high-level geometry lowering share one stat
  option parser/defaulting path.
- Fixed domains become enums where useful, e.g. bin closure (`left`/`right`) and
  smooth method (`lm`).
- Invalid stat settings preserve current diagnostic codes and spans.
- Renderer derived-table execution matches typed options rather than looking up
  strings such as `"bins"` and `"closed"`.
- CLI `ir` JSON/debug output remains understandable; tests document any field
  shape changes.

### 5. High-level geometry lowering module

Status: Not started.

Acceptance criteria:

- Lowering for `Histogram`, `FreqPoly`, `Bin2D`, `Density`, and
  `Bar(stat: "count")` lives outside the chart/space analyzer.
- The lowering module owns synthetic derived table names and synthetic output
  columns.
- The five `next_*_name` functions are replaced with one
  `next_synthetic(prefix)` helper.
- Repeated setting-copy helpers are replaced with a small allowlist helper.
- Repeated frame validation is extracted into readable helpers, not one
  over-general closure abstraction.
- Lowered diagnostics continue to point at the original source call or the most
  precise user-authored setting span.

### 6. Spec, version, and example hygiene

Status: Not started.

Acceptance criteria:

- Workspace and VS Code extension versions are bumped to `0.10.0` when the
  release branch is ready.
- Any IR shape changes caused by typed stat options are reflected in the spec.
- This plan is updated as each item completes, is rejected, or moves scope.
- Examples are regenerated; `git diff -- examples` must be empty for current
  checked-in examples.

## v0.10.0 Should

### Semantic string/display helpers

Status: Not started.

Add `as_str`/`Display` helpers for semantic enums where this reduces match
duplication in CLI or LSP without pulling those crates into semantics internals.

### Parser recovery table cleanup

Status: Not started.

The parser's misspelled chart-item recovery can be table-driven for
`Scale`/`Guide`/`Theme`/`Layout`. This is safe and local, but not central to the
semantic release. If it lands here, mark the corresponding v0.12 item complete;
otherwise it remains v0.12 scope.

## Explicitly Deferred Past v0.10.0

- Driver/source pipeline work: v0.9.
- Renderer planner, geometry renderer, SVG writer, and render helper cleanup:
  v0.11.
- Full LSP module split and diagnostic code registry: v0.12.
- Full typed geometry-property IR.
- New user-facing language or rendering features.

## Optional-Item Audit

### Promote In v0.10.0 (Must)

- Semantic guard tests.
- Analyzer module decomposition.
- Unique argument and typed value helpers.
- Typed stat options in IR.
- High-level geometry lowering module.
- Spec/version/example hygiene.

### Consider If Capacity Allows (Should)

- Semantic string/display helpers.
- Parser recovery table cleanup.

### Keep Deferred

- Renderer/LSP/diagnostic registry work.
- Full typed geometry-property IR.
- New language features.

## Promotion Workflow

1. Add guard tests.
2. Split files without changing behavior.
3. Introduce duplicate-argument and value-extraction helpers.
4. Add typed stat options and update renderer execution.
5. Move and simplify high-level lowering.
6. Run workspace tests, regenerate examples, and require an empty
   `git diff -- examples`.
