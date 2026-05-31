# Algraf v0.43.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_42_PLAN.md`](V0_42_PLAN.md)
Roadmap theme: Big-data readiness and backend-friendly data execution.

## Purpose

This release starts the big-data architecture track. Algraf's current language
and renderer are deterministic and well-factored, but the data plane is tuned
for small and moderate in-memory datasets. The existing `Table` boundary exposes
scalar cell reads, nullable numeric columns use `Vec<Option<T>>`, driver I/O
often buffers entire files before parsing, and scale/stat code repeatedly scans
tables row by row.

Those choices are acceptable for a compact SVG renderer, but they become the
wrong defaults once Algraf needs to check, summarize, or render from larger
CSV/TSV/JSON/SQLite/Geo sources and columnar files such as Parquet. v0.43.0
should make big-data support an explicit design target without pretending that
static SVG can sensibly contain millions of raw marks.

## Release Thesis

v0.43.0 is the **data-plane scale** release. It should make Algraf capable of
bounded schema inspection, streaming ingest, memory-efficient typed storage,
column-oriented scale/stat execution, and at least one concrete large columnar
file path for testing, with Parquet as the preferred target. It must preserve
deterministic SVG output and keep heavy data engines optional.

The goal is not to turn Algraf into a dataframe language. The goal is to remove
the architectural decisions that would make a future Arrow or Polars backend
useful only as a slow scalar adapter.

## Scope Rules

- Keep the parser, semantic analyzer, ordinary LSP analysis, and renderer
  planning decoupled from concrete dataframe internals.
- Keep Polars optional. A future Polars backend may be supported, but core
  parser, LSP, CLI, and SVG rendering must not require it.
- Treat Parquet support as native/CLI-first and feature-gated if needed. Browser
  and WASM support may follow after the native path proves useful.
- Treat the scalar `Table::value` API as a compatibility and final-mark
  fallback, not the primary execution surface for domains, stats, and derived
  tables.
- Prefer column scans, typed iterators, and aggregate/fold interfaces over
  repeated dynamic cell lookups by column name and row index.
- Preserve missing-value behavior, source-order category ordering, stable
  diagnostics, and byte-for-byte deterministic SVG output.
- Make large raw mark output explicit and guarded. Big-data support should
  favor aggregation, binning, sampling, and summary marks over unbounded SVG
  node generation.
- Streaming support must coexist with in-memory and WASM providers. WASM may
  keep byte-buffer paths where browser APIs require them, but native file paths
  should not be forced through `Vec<u8>`.
- Do not add arbitrary dataframe expressions, SQL dialects for non-SQL sources,
  or user-provided code execution as part of this release.

## Current Bottleneck Audit

| Area | Current shape | Big-data concern | Release direction |
| ---- | ------------- | ---------------- | ----------------- |
| Table access | `Table::value(column, row)` scalar reads | Per-cell dynamic lookup and enum wrapping prevent vectorized backends and slow repeated scans. | Add column views, typed scanners, and pre-resolved column handles. |
| Nullable storage | Numeric and temporal columns store `Vec<Option<T>>` | Doubles numeric memory on common Rust layouts and hurts cache locality. | Use dense value buffers plus validity bitmaps for scalar types. |
| CSV ingest | Reads raw `String` cells into per-column vectors, classifies into another cell vector, then builds typed columns | Multiple full-column allocations exist before final typed storage. | Build through streaming typed builders and avoid intermediate full-file byte buffers. |
| Driver I/O | `DriverIo::read_path` and `read_stdin` return full `Vec<u8>` | Large files are buffered before parsers can stream records. | Add reader-oriented I/O for native paths and keep byte reads only where necessary. |
| Scale/stat execution | Domain and stat code loops over row indices and calls `value` repeatedly | Large tables pay per-cell dynamic dispatch, string lookup, and enum costs. | Rework domains and stats around typed column scans and grouped aggregators. |
| SVG output | Raw row-to-mark rendering can produce one SVG node per row | Static SVG is the wrong target for unbounded raw points. | Add mark budgets, diagnostics, and examples that aggregate or sample first. |
| Large-format coverage | CSV/TSV/JSON/SQLite/Geo are supported, but Parquet is not | The roadmap lacks a realistic large columnar fixture that exercises scan-oriented APIs. | Add opt-in Parquet support or a Parquet-backed adapter for native tests. |
| Parser typo recovery | Keyword edit distance allocates small vectors on misspelled keyword checks | Not a big-data bottleneck; a minor cleanup opportunity only. | Fix opportunistically if touched, but do not treat it as release-critical. |
| Driver API shape | Many `_with_io` wrappers exist for testing/WASM injection | Public API is noisy, but not the main scale blocker. | Consider a context object after the streaming I/O boundary is clear. |

## Big-Data Capability Targets

v0.43.0 should classify supported workflows explicitly:

- **Large schema/check workflows:** `algraf check`, LSP schema hover, and
  completion should use bounded reads, metadata caches, and cancellation where
  possible. They must not fully load a large source just to infer provisional
  columns.
- **Aggregate render workflows:** charts built from `Bin`, `Bin2D`, `Count`,
  `Smooth`, or SQLite aggregate queries should be able to process larger inputs
  while materializing only the derived result needed for SVG.
- **Columnar file workflows:** Parquet should become the first explicit
  large-format target, either through an Algraf-native reader or an optional
  Arrow/Polars-backed adapter. Tests should include a deterministic generated
  Parquet fixture large enough to make scalar-row execution visibly wrong.
- **Raw mark workflows:** plotting every row remains allowed for moderate data,
  but the renderer should warn or fail predictably when a chart would emit an
  excessive number of marks.
- **Out-of-core workflows:** full out-of-core execution is deferred. This
  release should make the APIs compatible with streaming and future chunked
  execution without promising distributed or remote query behavior.

## Current Recipes

These sketches use current Algraf surfaces and remain the preferred workaround
until v0.43.0 features land.

### Pre-aggregate with SQLite

```text
Chart(data: Sqlite("events.db",
                   "SELECT date, category, COUNT(*) AS n
                    FROM events
                    GROUP BY date, category
                    ORDER BY date, category"),
      width: 820, height: 460,
      title: "Events by day") {
    Scale(fill: category, label: "Category")
    Space(date * n) {
        Bar(fill: category, layout: "stack")
    }
}
```

This charts a bounded aggregate result instead of rendering every source row.
SQLite remains the best current path for large local tabular data because the
query engine can reduce data before Algraf materializes it.

### Pre-bin outside Algraf

```text
Chart(data: "points_binned.csv", width: 760, height: 520,
      title: "Point density") {
    Scale(fill: count, gradient: ["#edf8fb", "#b2e2e2", "#66c2a4",
                                  "#2ca25f", "#006d2c"])
    Space(x_center * y_center) {
        Rect(xmin: x_min, xmax: x_max,
             ymin: y_min, ymax: y_max,
             fill: count,
             stroke: null)
    }
}
```

This charts density with one rectangle per bin. Native streaming `Bin2D` should
eventually make this external preprocessing unnecessary for common cases.

### Sample before rendering raw rows

```text
Chart(data: "sampled_points.csv", width: 760, height: 460,
      title: "Sampled raw observations") {
    Space(x * y) {
        Point(fill: group, alpha: 0.28, size: 1.8)
    }
}
```

This charts a representative sample when the visual goal is distribution shape,
not exact per-row inspection.

## Feature Target Sketches

These sketches are non-runnable design targets. They distinguish API and runtime
work from source-level language additions.

### Column-oriented table surface

```rust
pub trait Table {
    fn schema(&self) -> &[ColumnDef];
    fn row_count(&self) -> usize;

    // Compatibility and final-mark fallback.
    fn value(&self, column: &str, row: usize) -> Option<DataValueRef<'_>>;

    // New execution-oriented surface.
    fn column(&self, column: &str) -> Option<ColumnView<'_>>;
    fn scan(&self, columns: &[&str], visitor: &mut dyn TableScan);
}
```

The exact trait shape may differ, but stats and scale training should no longer
need to perform name lookup and dynamic scalar conversion for every cell.

### Nullable scalar buffers

```rust
pub struct NullableColumn<T> {
    values: Vec<T>,
    validity: NullBitmap,
}
```

Dense values plus a bit-level validity mask should back booleans, integers,
floats, and temporal values. String and geometry columns may keep owned values,
but their missingness should still be represented consistently.

### Reader-oriented driver I/O

```rust
pub trait DriverIo {
    fn open_path(&self, path: &Path) -> io::Result<Box<dyn io::Read + '_>>;
    fn read_path(&self, path: &Path) -> io::Result<Vec<u8>>;
}
```

Native path loads should prefer `open_path`; embedded and WASM providers may
continue to implement `read_path` where they only have bytes.

### Parquet source target

```text
Chart(data: Parquet("events.parquet"), width: 820, height: 460,
      title: "Events by day") {
    Space(date * n) {
        Bar(fill: category, layout: "stack")
    }
}
```

This source constructor is a target sketch, not current syntax. If promoted, it
must be specified in the spec, added to source resolution, covered by LSP
completion/hover, and backed by deterministic tests using generated Parquet
fixtures. If the first implementation instead uses extension inference from
`Chart(data: "events.parquet")`, the same spec and test requirements apply.

### Mark-budget diagnostics

```text
Chart(data: "events.csv", width: 760, height: 460,
      title: "Too many raw events") {
    Space(x * y) {
        Point(alpha: 0.1, size: 1)
    }
}
```

If this source would emit millions of `Point` marks, the renderer should produce
a clear diagnostic that recommends binning, aggregation, sampling, or a higher
explicit mark budget. Any new diagnostic code must be reserved in the spec
before implementation.

## v0.43.0 Must

### 1. Define big-data contracts and budgets

Status: Planned.

- Add non-normative planning targets for row counts, byte sizes, and mark counts
  that each workflow class should support.
- Decide which limits are diagnostics, warnings, CLI flags, or hard runtime
  errors.
- Reserve spec diagnostics before implementing any new user-visible errors.
- Document that large raw SVG output is not the same as large-data support.

### 2. Redesign the table execution boundary

Status: Planned.

- Add a column-oriented API that exposes typed column views or typed scan/fold
  operations.
- Keep `Table::value` available for compatibility and final mark property
  resolution.
- Refactor hot-path callers to pre-resolve column handles rather than looking
  up column names for each row.
- Add tests with an instrumented table implementation that fails if scale and
  stat code accidentally falls back to scalar cell access where a column scan is
  expected.

### 3. Replace nullable scalar storage

Status: Planned.

- Replace `Vec<Option<bool>>`, `Vec<Option<i64>>`, `Vec<Option<f64>>`, and
  `Vec<Option<DateTimeValue>>` with dense value buffers plus validity bitmaps.
- Preserve current null semantics: out-of-range row access is absent, while a
  present missing cell reads as `DataValueRef::Null`.
- Keep deterministic ordering for categorical domains and mixed values.
- Add memory-layout tests or assertions that catch regressions in scalar column
  storage size.

### 4. Stream native data loading

Status: Planned.

- Add reader-oriented native path I/O to `DriverIo` and route CSV/TSV/JSON/
  NDJSON readers through it where the format supports streaming.
- Avoid buffering a native file into `Vec<u8>` before passing it to parsers.
- Rework CSV/TSV typed builders to avoid retaining both raw string columns and
  final typed columns when possible.
- Keep bounded schema sampling for LSP and `check`; sampled schemas must remain
  clearly provisional where the spec requires it.
- Explicitly document formats that still require whole-document parsing, such
  as TopoJSON or GeoJSON if streaming is not implemented for them.

### 5. Make domains and stats column-oriented

Status: Planned.

- Rework numeric, temporal, and categorical domain collection around typed
  column scans.
- Rework `Count`, `Bin`, `Bin2D`, smoothing inputs, summaries, and passthrough
  derived tables to avoid unnecessary owned-cell cloning.
- Ensure grouped aggregations use stable maps or explicit source-order tracking
  so category order remains deterministic.
- Add performance regression tests or benchmarks for representative wide,
  tall, sparse, and mixed datasets.

### 6. Add large-render guardrails

Status: Planned.

- Add a mark-budget model for raw SVG and draw-list output.
- Make the limit deterministic and inspectable in CLI/report output.
- Allow an explicit override for advanced users, but require the default path to
  fail or warn before generating pathological SVG.
- Add examples that show aggregation, binning, or sampling as the recommended
  solution for large sources.

### 7. Add an opt-in Parquet/Arrow/Polars path

Status: Planned.

- Add Parquet as the concrete large columnar file target for v0.43.0, preferably
  through an optional Arrow or Polars-backed reader behind a feature flag.
- Support schema loading and bounded row/column projection for native CLI tests
  before broadening to LSP, WASM, or browser runtimes.
- Add a deterministic fixture generator that creates Parquet files with tall,
  wide, sparse, categorical, numeric, temporal, and nullable columns.
- Prove that core scale and stat paths use column scans rather than scalar
  `value` calls when running against the Parquet-backed adapter.
- Do not make Arrow or Polars a required dependency for parser, semantics, LSP,
  CLI basics, or SVG rendering.
- Document which operations remain Algraf-native because they depend on
  deterministic SVG, category ordering, or Algraf-specific missing-value rules.
- If production Parquet support is too large for v0.43.0, ship the adapter as
  explicitly experimental and open a follow-on plan for hardening rather than
  leaving the backend work as an abstract evaluation.

### 8. Spec, docs, examples, and release hygiene

Status: Planned.

- Update spec sections for data storage, table access, driver I/O, diagnostics,
  and rendering limits only as behavior lands.
- Add examples for aggregate rendering and large-source guardrails.
- Update README guidance to explain when to aggregate, bin, sample, or query
  through SQLite.
- Keep plan examples runnable except explicitly marked feature target sketches.

## v0.43.0 Should

### Driver context cleanup

Status: Planned.

- Consider replacing broad public `_with_io` wrapper duplication with a
  `DriverContext` or similar object after streaming I/O requirements are clear.
- Preserve simple compatibility wrappers for common CLI/native use.

### LSP cancellation and cache refinement

Status: Planned.

- Ensure large schema sampling can be cancelled or skipped in editor paths when
  users keep typing.
- Use metadata and fingerprints to avoid resampling unchanged large files.

### Opportunistic parser allocation cleanup

Status: Planned.

- Replace small heap allocations in keyword edit-distance recovery with a
  stack-bounded implementation if the parser is already being touched.
- Treat this as polish, not a big-data blocker.

### Benchmark fixtures

Status: Planned.

- Add synthetic fixture generation for tall, wide, sparse, categorical, and
  temporal datasets.
- Include Parquet output in the fixture generator when the optional backend
  feature is enabled.
- Keep benchmark fixtures generated or compact so the repository does not grow
  unnecessarily.

## Follow-on Candidate After v0.43.0

If v0.43.0 lands only experimental Parquet/Arrow/Polars support, the immediate
follow-on should be a production hardening release:

- stabilize the Parquet source syntax and format inference;
- support projection pushdown for only the columns referenced by the chart;
- support row-group pruning where the backend exposes it;
- extend LSP schema sampling to Parquet metadata without reading full columns;
- add browser/WASM behavior only if dependency size and memory usage are
  acceptable;
- document backend feature flags and fallback behavior.

## Explicitly Deferred Past v0.43.0

- Distributed execution, remote object stores, and cluster schedulers.
- A dataframe expression language inside Algraf source.
- Arbitrary SQL execution against CSV/JSON files.
- GPU rendering or WebGL chart output.
- Full out-of-core rendering of raw marks.
- Required Polars dependency in the core workspace.
- Production-grade Parquet predicate pushdown if the v0.43.0 backend ships as
  experimental only.
- Automatic approximate algorithms that change chart values without explicit
  source or option-level consent.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

Large-data implementation changes should also run any new benchmark or fixture
generation commands added by this release.

## Promotion Workflow

1. Specify the table execution API and missing-value invariants before changing
   renderer or stats call sites.
2. Reserve any new diagnostics for mark budgets, large-source warnings, or
   streaming limitations in the spec before emitting them.
3. Implement dense nullable storage and streaming loaders behind compatible
   public APIs, then migrate hot-path scale and stat code.
4. Add instrumented tests that prove hot paths use column scans and preserve
   deterministic category ordering.
5. Add large-source guardrail examples and README guidance after diagnostics are
   implemented.
6. Add the Parquet fixture generator and optional Arrow/Polars-backed adapter
   only after the table API can exercise column scans.
7. Decide at release close whether Parquet support is stable enough for v0.43.0
   or should be marked experimental with a follow-on hardening plan.
