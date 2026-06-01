# Algraf v0.47.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_46_1_PLAN.md`](V0_46_1_PLAN.md)
Roadmap theme: explicit derived-table input sources.

## Purpose

v0.47.0 makes derived-table dependencies explicit. Before this release, a
`Derive` could silently read columns produced by another `Derive` when its stat
inputs happened to match those output names. That made the active input table
hard to see from source.

The new surface spells the dependency in the declaration:

```ag
Derive bins = Bin(value, bins: 12)
Derive trend from bins = Smooth(bin_center, count)
```

## Scope

### Explicit `from` Input

Status: Implemented.

`Derive name from source = Stat(...)` sets the stat's active input table to
`source`.

Rules:

- `source` may name a chart-scoped `Table` or derived table.
- Omitting `from source` keeps the existing primary-table default.
- Stat input columns resolve only against the active input table.
- Derived-table output columns are not injected into chart scope or sibling
  `Derive` declarations.
- Cycles through `from` references remain `E1501`.
- Unknown `from` sources use the ordinary unknown-table diagnostic `E1103`.

## Non-Goals

- No join syntax or multi-source derived stat input.
- No `data:` option inside stat calls.
- No change to `Space(..., data: name)` binding.

## Validation

- Parser tests cover `Derive name from source = ...`.
- Semantic tests cover explicit derived chaining, named-table inputs, cycle
  detection, and rejection of implicit column injection.
- Renderer tests cover chained derived execution through explicit `from`.
- Example charts and README snippets use the explicit syntax.
