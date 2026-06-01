# Algraf v0.46.1 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_46_PLAN.md`](V0_46_PLAN.md)
Roadmap theme: table-source spelling consistency.

## Purpose

v0.46.1 is a small language-surface patch that lets named `Table`
declarations participate in primary chart data binding. It keeps the existing
`Chart(data: "file.csv")` spelling, but also accepts named-table forms so
authors can make all data sources look like table declarations when that reads
better.

## Scope

### Named table primary sources

Status: Implemented.

Support these equivalent primary-source spellings:

```ag
Chart(data: "some.csv") {
    Space(x * y) { Point() }
}
```

```ag
Table main = "some.csv"

Chart(data: main) {
    Space(x * y) { Point() }
}
```

```ag
Chart {
    Table main = "some.csv"
    Space(x * y, data: main) { Point() }
}
```

Rules:

- `Table` may appear at document scope or chart scope.
- `Chart(data: name)` resolves `name` as a visible `Table`.
- A `Chart` with no argument list uses a visible `Table main = ...` as its
  primary source.
- Existing source expressions, named-table binding in `Space(data:)`, and
  derived-table binding remain unchanged.

## Non-Goals

- No implicit joins or relational operations between tables.
- No change to `Derive` dependency syntax.
- No change to data loading formats or source security policy.

## Validation

- Parser tests cover top-level `Table`, `Chart(data: main)`, and `Chart { ... }`.
- Semantic tests cover table-backed primary data and explicit `Space(data: main)`.
- Driver tests cover loading primary data through a named table.

