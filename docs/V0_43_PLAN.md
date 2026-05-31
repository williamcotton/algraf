# Algraf v0.43.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_42_PLAN.md`](V0_42_PLAN.md)
Follow-on plan: [`V0_44_PLAN.md`](V0_44_PLAN.md)
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
column-oriented scale/stat execution, and native CLI Parquet workflows for
checking, schema inspection, aggregate rendering, and large demos. Parquet is a
release requirement for the native CLI path, not a speculative evaluation,
though the exact Arrow/Polars adapter details may remain optional or
feature-gated. It must preserve deterministic SVG output and keep heavy data
engines optional for parser, semantics, LSP basics, WASM, and non-Parquet
rendering.

The goal is not to turn Algraf into a dataframe language. The goal is to remove
the architectural decisions that would make a future Arrow or Polars backend
useful only as a slow scalar adapter.

## Scope Rules

- Keep the parser, semantic analyzer, ordinary LSP analysis, and renderer
  planning decoupled from concrete dataframe internals.
- Keep Polars optional. A future Polars backend may be supported, but core
  parser, LSP, CLI, and SVG rendering must not require it.
- Treat Parquet support as required for native CLI. The implementation may hide
  heavy crates behind a `parquet` or backend feature, but the v0.43.0 release
  binary and workspace checks must exercise native Parquet schema loading and
  rendering. Browser and WASM support may follow after the native path proves
  useful.
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
- Keep large demo data and rendered benchmark output out of git. Store
  downloaded/generated files under gitignored local directories and make every
  reproducible demo script able to recreate its inputs.
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
| Large-format coverage | CSV/TSV/JSON/SQLite/Geo are supported, but Parquet is not | The roadmap lacks a realistic large columnar fixture that exercises scan-oriented APIs. | Add native CLI Parquet support plus generated and downloaded large fixtures/demos. |
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
- **Columnar file workflows:** Parquet must become the first explicit
  large-format target, either through an Algraf-native reader or an optional
  Arrow/Polars-backed adapter. Tests should include a deterministic generated
  Parquet fixture large enough to make scalar-row execution visibly wrong.
- **Large demo workflows:** v0.43.0 should ship opt-in scripts and `.ag` demo
  specs that render bounded SVG charts from synthetic data, NYC TLC trip data,
  and SFO Museum flight data without committing those large inputs or outputs.
- **Raw mark workflows:** plotting every row remains allowed for moderate data,
  but the renderer should warn or fail predictably when a chart would emit an
  excessive number of marks.
- **Out-of-core workflows:** full out-of-core execution is deferred. This
  release should make the APIs compatible with streaming and future chunked
  execution without promising distributed or remote query behavior.

## Large Demo Data Policy

Large demo data is part of the release plan but must not become repository
weight. The repository should contain scripts, chart specs, documentation,
checksums where practical, and small smoke fixtures only.

- Store local large inputs under `benchdata/` or
  `target/algraf-large-fixtures/`; both are gitignored or already covered by
  `target/`.
- Store large rendered demo outputs under `bench-output/`; generated SVG/PNG
  benchmark artifacts are not committed unless a future release explicitly
  promotes a compact visual fixture.
- Add `scripts/generate-large-fixtures.sh` for deterministic synthetic data.
  This script is the only large-data path that normal CI may depend on, and CI
  should use a small or medium row-count tier unless a dedicated benchmark job
  opts into larger tiers.
- Add `scripts/download-large-fixtures.sh` for external data. Downloads are
  opt-in, must print source URLs and licensing/citation notes, and should verify
  checksums or source metadata when the upstream artifact is stable enough to
  do so. Network downloads are not required for `cargo test`.
- Add `scripts/render-large-demos.sh` to render the large demo chart suite into
  `bench-output/large-demos/` and report source row counts, derived row counts,
  emitted mark counts, elapsed time, and peak memory when available.
- Large demo chart specs should live under `bench/examples/large/`, not the
  tutorial `examples/` directory, unless they use only checked-in compact data.
  The top-level README should link to these demos and explain how to generate
  their data rather than embedding every large rendered artifact.

The successful SVG demos must be aggregate-first: they may scan millions of
input rows, but the rendered scene should usually contain hundreds or low
thousands of marks. The suite should also include at least one intentionally
rejected raw-mark chart to prove the mark-budget diagnostics work.

## Large Demo Sources

### Synthetic generated fixtures

The synthetic generator should create deterministic fixtures from a fixed seed
with configurable size tiers. Parquet is the primary output; CSV/NDJSON mirrors
may be generated for streaming-loader comparisons.

Required fixture shapes:

- **Tall events:** millions of rows with timestamp, numeric value, group, and
  nullable fields.
- **Wide metrics:** many columns with only a small referenced subset, proving
  projection and schema inspection do not force unnecessary column reads.
- **Sparse nullable:** numeric, temporal, and categorical columns with validity
  bitmaps exercised heavily.
- **High-cardinality categories:** a controlled category domain large enough to
  test scale/legend guardrails.
- **Dense points:** numeric `x`/`y` rows for `Bin2D` success and raw-point
  budget failure.

Planned demos:

- `bench/examples/large/synthetic_bin2d_density.ag` renders a bounded
  rectangular density plot from dense generated points.
- `bench/examples/large/synthetic_nullable_histogram.ag` bins sparse nullable
  numeric values and verifies missing values are skipped consistently.
- `bench/examples/large/synthetic_projection_smoke.ag` references a few columns
  from a wide Parquet file and reports whether unused columns were skipped.
- `bench/examples/large/synthetic_raw_mark_budget.ag` intentionally maps raw
  points and must emit the large-render diagnostic instead of pathological SVG.

### NYC TLC trip records

Use NYC Taxi & Limousine Commission trip records as the primary real-world
Parquet demo source. The default download should use a fixed historical Yellow
Taxi monthly Parquet file, such as:

```text
https://d37ci6vzurychx.cloudfront.net/trip-data/yellow_tripdata_2024-01.parquet
```

The downloader may also fetch the taxi zone lookup CSV for labels:

```text
https://d37ci6vzurychx.cloudfront.net/misc/taxi_zone_lookup.csv
```

The raw Parquet file should remain unchanged under `benchdata/raw/tlc/`. Any
prepared helper files, such as projected columns or compact aggregates for demo
comparison, should be generated under `benchdata/prepared/tlc/` and treated as
rebuildable artifacts.

Planned demos:

- `bench/examples/large/tlc_trip_distance_histogram.ag` bins
  `trip_distance`.
- `bench/examples/large/tlc_fare_distance_density.ag` renders `Bin2D` over
  `trip_distance` and `total_amount`.
- `bench/examples/large/tlc_payment_type_counts.ag` counts trips by
  `payment_type`.
- `bench/examples/large/tlc_pickup_time_bins.ag` bins
  `tpep_pickup_datetime` by a calendar interval if direct temporal binning is
  available for the Parquet timestamp representation; otherwise the preparation
  script should materialize an explicit ISO datetime column for the same chart.

### SFO Museum flight data

Use SFO Museum flight data as the second real-world Parquet demo source. The
default download should use a fixed monthly public Parquet file, such as:

```text
https://static.sfomuseum.org/parquet/sfomuseum-data-flights-2026-03.parquet
```

The downloader should keep the raw Parquet file under `benchdata/raw/sfo/`.
The preparation step should materialize a chart-ready projected Parquet table
under `benchdata/prepared/sfo/` with at least date, event, airline, journey,
longitude, and latitude columns.

Planned demos:

- `bench/examples/large/sfo_daily_flights.ag` bins flights by day.
- `bench/examples/large/sfo_event_counts.ag` counts arrivals and departures.
- `bench/examples/large/sfo_airline_counts.ag` counts flights by grouped
  airline.
- `bench/examples/large/sfo_route_density.ag` renders bounded 2D bins over
  longitude and latitude.

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

This source constructor is implemented for the native CLI path. It is specified
in the spec, added to source resolution, covered by source-constructor tests and
native CLI Parquet schema/render tests, and backed by deterministic generated
Parquet fixtures. Extension inference from `Chart(data: "events.parquet")` is
also supported.

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

Status: Done.

- Add non-normative planning targets for row counts, byte sizes, and mark counts
  that each workflow class should support.
- Define size tiers for generated fixtures: smoke, local benchmark, and stress.
  Only the smoke tier should be expected in ordinary CI.
- Define baseline target files for external demos: one fixed NYC TLC monthly
  Parquet file and one fixed SFO Museum monthly Parquet file, plus their
  prepared Parquet projections where applicable.
- Decide which limits are diagnostics, warnings, CLI flags, or hard runtime
  errors.
- Reserve spec diagnostics before implementing any new user-visible errors.
- Document that large raw SVG output is not the same as large-data support.

### 2. Redesign the table execution boundary

Status: Done.

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

Status: Done.

- Replace `Vec<Option<bool>>`, `Vec<Option<i64>>`, `Vec<Option<f64>>`, and
  `Vec<Option<DateTimeValue>>` with dense value buffers plus validity bitmaps.
- Preserve current null semantics: out-of-range row access is absent, while a
  present missing cell reads as `DataValueRef::Null`.
- Keep deterministic ordering for categorical domains and mixed values.
- Add memory-layout tests or assertions that catch regressions in scalar column
  storage size.

### 4. Stream native data loading

Status: Done.

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

Status: Done.

- Rework numeric, temporal, and categorical domain collection around typed
  column scans.
- Rework `Count`, `Bin`, `Bin2D`, smoothing inputs, summaries, and passthrough
  derived tables to avoid unnecessary owned-cell cloning.
- Ensure grouped aggregations use stable maps or explicit source-order tracking
  so category order remains deterministic.
- Add performance regression tests or benchmarks for representative wide,
  tall, sparse, and mixed datasets.

### 6. Add large-render guardrails

Status: Done.

- Add a mark-budget model for raw SVG and draw-list output.
- Make the limit deterministic and inspectable in CLI/report output.
- Include emitted mark counts in large-demo reports, distinguishing input rows,
  derived rows, and actual SVG/draw-list primitives.
- Allow an explicit override for advanced users, but require the default path to
  fail or warn before generating pathological SVG.
- Add examples that show aggregation, binning, or sampling as the recommended
  solution for large sources.

### 7. Add native CLI Parquet support

Status: Done.

- Add Parquet as the concrete large columnar file target for v0.43.0. Native
  CLI Parquet schema loading and aggregate rendering are required release
  scope.
- Decide the user-facing source surface before implementation: either
  `Parquet("events.parquet")`, extension inference from
  `Chart(data: "events.parquet")`, or both. Specify the chosen behavior before
  shipping it.
- Support schema loading, type mapping, missing-value handling, and bounded
  row/column projection for native CLI tests before broadening to LSP, WASM, or
  browser runtimes.
- Exercise the path with both generated Parquet fixtures and downloaded TLC
  Parquet data.
- Add a deterministic fixture generator that creates Parquet files with tall,
  wide, sparse, categorical, numeric, temporal, and nullable columns.
- Prove that core scale and stat paths use column scans rather than scalar
  `value` calls when running against the Parquet-backed adapter.
- Do not make Polars a required dependency for parser, semantics, LSP, CLI
  basics, WASM, or non-Parquet SVG rendering. Arrow/parquet crates may be
  required by the native Parquet feature if they remain isolated from those
  layers.
- Document which operations remain Algraf-native because they depend on
  deterministic SVG, category ordering, or Algraf-specific missing-value rules.
- If advanced projection pushdown, row-group pruning, or browser/WASM Parquet
  support is too large for v0.43.0, defer those pieces explicitly; do not close
  the release without a native CLI Parquet path that can render the required
  aggregate demos.

### 8. Add large demo data and chart suite

Status: Done.

- Add `scripts/generate-large-fixtures.sh` for deterministic synthetic fixtures
  and generated Parquet files.
- Add `scripts/download-large-fixtures.sh` for NYC TLC and SFO Museum source
  files.
- Add `scripts/prepare-large-fixtures.sh` if raw source files need normalized
  Parquet or compact aggregate helper tables before charting.
- Add `scripts/render-large-demos.sh` to render every large demo spec and write
  outputs to `bench-output/large-demos/`.
- Add large demo `.ag` specs under `bench/examples/large/` for synthetic, TLC,
  and SFO sources.
- Include at least one successful bounded SVG demo for each source family and
  at least one intentional raw-mark budget failure.
- Document the source citations and local disk/network expectations for every
  external download.

### 9. Spec, docs, examples, and release hygiene

Status: Done.

- Update spec sections for data storage, table access, driver I/O, diagnostics,
  Parquet sources, source preparation, and rendering limits only as behavior
  lands.
- Add compact tutorial examples for aggregate rendering and large-source
  guardrails when they can run from checked-in data.
- Add README guidance for the opt-in large demo suite, including why large
  source files are downloaded/generated locally rather than checked into git.
- Explain when to aggregate, bin, sample, query through SQLite, or use Parquet
  directly.
- Keep plan examples runnable except explicitly marked feature target sketches.

## v0.43.0 Should

### Driver context cleanup

Status: Deferred.

- Consider replacing broad public `_with_io` wrapper duplication with a
  `DriverContext` or similar object after streaming I/O requirements are clear.
- Preserve simple compatibility wrappers for common CLI/native use.

### LSP cancellation and cache refinement

Status: Deferred.

- Ensure large schema sampling can be cancelled or skipped in editor paths when
  users keep typing.
- Use metadata and fingerprints to avoid resampling unchanged large files.

### Opportunistic parser allocation cleanup

Status: Deferred.

- Replace small heap allocations in keyword edit-distance recovery with a
  stack-bounded implementation if the parser is already being touched.
- Treat this as polish, not a big-data blocker.

### Benchmark fixtures

Status: Done.

- Add synthetic fixture generation for tall, wide, sparse, categorical, and
  temporal datasets.
- Include Parquet output in the fixture generator for smoke, local benchmark,
  and stress tiers.
- Add a local fixture manifest that records generated/downloaded file paths,
  source URLs, source metadata or checksums where practical, row counts, and
  preparation steps.
- Keep benchmark fixtures generated or compact so the repository does not grow
  unnecessarily.

## Follow-on Candidate After v0.43.0

After v0.43.0 lands the required native CLI Parquet baseline, the immediate
follow-on should be a production hardening release:

- harden the Parquet source syntax and format inference with more compatibility
  fixtures;
- optimize projection pushdown for only the columns referenced by the chart;
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
- Production-grade Parquet row-group predicate pushdown beyond the v0.43.0
  referenced-column projection baseline.
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
generation commands added by this release. The smoke tier should be runnable
without network access; downloaded TLC/SFO demos remain opt-in unless a
release checklist explicitly requests them.

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
6. Add the Parquet fixture generator and native CLI Parquet adapter only after
   the table API can exercise column scans.
7. Add the large demo source scripts and `bench/examples/large/` chart suite,
   then verify smoke generated demos before any downloaded-data run.
8. Do not close v0.43.0 until native CLI Parquet can load schemas, render the
   required aggregate demos, and participate in the release checks without
   leaking heavy backend dependencies into parser, semantics, or WASM paths.
