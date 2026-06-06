# Algraf v0.69.0 Plan

Status: In progress
Target version: 0.69.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_68_PLAN.md`](V0_68_PLAN.md)
Promoted from: [`ARROW_PERFORMANCE.md`](ARROW_PERFORMANCE.md)
Neighboring PDL plan: [`V0_34_PLAN.md`](../../pdl/docs/V0_34_PLAN.md)
Roadmap theme: Arrow-stream and large-data aggregate performance.
Cross-repo coordination: `../pdl/` for Polars-backed execution and Arrow IPC
stream output.

## Purpose

Algraf v0.69 makes the PDL-to-Algraf typed pipe and large aggregate rendering
path the active implementation target:

```bash
pdl run prep.pdl --stdout-format arrow-stream \
  | algraf render chart.ag --data - --data-format arrow-stream --output chart.svg
```

PDL's neighboring v0.34 production native-pipeline plan targets native Polars
coverage, Arrow-stream input, lazy scans, writer-oriented Arrow IPC stream
output, and clean stdout. Algraf's side is the consumer and renderer: it should
accept caller-provided Arrow streams without avoidable full buffering, process
large source tables through column-oriented stats and domains, and render
visual-sized aggregate output instead of millions of raw SVG nodes.

This release does not make Algraf parse or execute PDL. The format boundary is
Arrow IPC stream bytes, and the source-language boundary remains
`Chart(data: input)` or `Chart(data: stdin)` plus the CLI caller-data flags.

The Arrow, Parquet, and PDL pipe work in this plan is native-only. Browser/WASM
builds do not need this support in v0.69 and MUST NOT pull Arrow, Parquet,
Polars, SQLite, or native file-format dependencies into `algraf-wasm`.

## Release Thesis

v0.69 is the **Arrow and aggregate performance** release. It should make large
typed inputs useful by improving ingest, dataframe access, stat/domain scans,
and aggregate-first rendering UX while preserving Algraf's current crate
boundaries and deterministic SVG output.

The practical success path is:

```text
large source data
  -> PDL or Algraf native reduction/stat/binning
  -> typed columnar data
  -> bounded marks
  -> deterministic SVG/draw-list/raster output
```

The release should prove that a million-row input can contribute to a small
rendered scene without forcing every hot path through scalar row lookups. It
should also keep raw million-row mark charts guarded and diagnosable because
static SVG is not the target for unbounded raw marks.

## Current State

Algraf already has a better data shape than a row-only renderer:

- `algraf-data::DataFrame` is columnar.
- Scalar nullable columns use dense value buffers plus validity bitmaps.
- Renderer-facing code depends on the `Table` trait rather than concrete Arrow,
  Parquet, Polars, or dataframe engine types.
- `arrow-stream` is accepted as caller-provided data for `Chart(data: input)`,
  `Chart(data: stdin)`, and `--data -`.
- Raw per-row rendering has a default mark budget.
- Large demos and generated fixtures already favor aggregate-first output.

The current large-data costs remain significant:

- caller-provided stdin now has a reader path, but many non-stdin and
  in-memory paths still materialize owned data before render planning;
- Arrow stream bytes are decoded into an owned `DataFrame`;
- strings are copied out of Arrow arrays into `Vec<Option<String>>`;
- numeric columns are copied from Arrow buffers into Algraf-owned buffers;
- render planning, domains, scales, stats, facets, and marks repeatedly scan
  materialized tables;
- many stats and geometry paths still iterate row indices and ask for scalar
  values;
- SVG and draw-list outputs remain proportional to emitted mark count.

The existing implementation is correct and well-separated. v0.69 should keep
that shape while making the hot paths more columnar and stream friendly.

## Observed Million-Row Baseline

The large-demo suite includes a deterministic CSV aggregate benchmark:

```text
cargo run -p algraf-bench -- generate --tier stress
bench/workloads/large/million_row_summary_bin.ag
bench/data/generated/million-row.csv
```

The fixture has 1,000,000 data rows, is about 28 MB on disk, and renders
through:

```ag
Derive bins = SummaryBin(x, score, by: [segment], bins: 64, reducer: "mean")
```

This is the intended visual shape for large-data Algraf: the input is large,
but the rendered SVG is bounded. On the local debug build used for the baseline,
the chart produced a roughly 29 KB SVG with 313 SVG mark/label/axis elements.
The observed timings were:

- CSV generation: about 1.8 seconds.
- `target/debug/algraf check`: about 10.4 seconds.
- `target/debug/algraf render --output out.svg`: about 14.6 seconds.
- Older `scripts/render-large-demos.sh` shell flow: 31 seconds for this chart's
  SVG plus PNG render pair.

These numbers are not CI thresholds. They are useful because the output is
small, so most elapsed time is in data loading, materialization, derived-stat
execution, scale/domain training, and repeated table scans rather than final SVG
emission. A release build should be measured separately before this baseline is
used for regression decisions.

Expected improvement bands for this workload:

- Release-mode current code may already cut standalone SVG render to roughly
  3-6 seconds.
- CSV loading plus column-oriented stat/domain fast paths should plausibly land
  around 1.5-3 seconds.
- Arrow or Parquet input plus columnar stat paths should plausibly land around
  0.5-1.5 seconds for the same aggregate chart.
- If PDL or another producer pre-aggregates to the 256 summary rows and Algraf
  only renders the summary table, render time should be well below one second.

CSV remains a useful baseline, but it is also a hard floor: Algraf must parse
text, allocate values, and decode categorical strings before it can summarize.
The Arrow and Parquet paths are where typed input, less copying, and columnar
scans should produce the largest end-to-end win.

## Scope

### Baseline And Benchmark Instrumentation

Status: In progress.

Implementation notes:

- Added `algraf-bench compare --before <run> --after <run>` so ignored
  `bench/runs/<run-label>/report.csv` files can be compared without ad hoc
  spreadsheet work.
- Added phase timing columns to benchmark reports: `parse_ms`, `prepare_ms`,
  `render_ms`, and `timing_total_ms`.
- `algraf-bench run` now builds the native `render-timing` binary alongside the
  CLI for release/profile-compatible phase measurement.
- `algraf-render` forwards the native `parquet` feature so `render-timing` can
  measure the same Parquet workloads as the CLI without changing the WASM
  feature surface.
- Before/after release runs are recorded locally as
  `v0_69_before_release`, `v0_69_after_release_rerun`,
  `v0_69_after_release_reader_timing3`, and
  `v0_69_after_release_count_fastpath`.
- Memory and derived-row reporting remain pending.

Acceptance criteria:

- The million-row `SummaryBin` benchmark records input rows, derived rows,
  emitted marks, output bytes, and elapsed time.
- Benchmarks separate data load, analysis, derived stat execution, scale/domain
  training, SVG emission, and PNG rasterization when practical.
- CSV, Arrow stream, and Parquet variants of the same aggregate chart can be
  compared without checking large generated inputs into git.
- Baseline numbers are documented as local observations, not CI thresholds,
  unless a reference machine and variance policy are added.

### Reader-Oriented Caller Data

Status: In progress.

Implementation notes:

- Added `DriverIo::open_stdin` so native `OsDriverIo` can provide stdin as a
  `Read` stream instead of forcing the caller-data path through `read_stdin`.
- Explicit caller-data formats now dispatch directly over the reader.
- Sniffed caller-data from stdin reads a bounded prefix, sniffs format, and then
  chains the preserved prefix back in front of the remaining reader.
- Schema loading uses the same reader-oriented stdin path.
- Added driver tests proving explicit and sniffed caller stdin can work when
  byte-buffer `read_stdin` is unavailable.

Acceptance criteria:

- Explicit `--data-format arrow-stream` from `--data -` starts Arrow decoding
  from a reader path instead of requiring a full stdin `Vec<u8>` first.
- Omitted-format sniffing still uses a bounded prefix buffer, preserves peeked
  bytes, detects Arrow IPC stream and Parquet magic, and falls back to CSV.
- Native in-memory hosts continue to support byte-buffer caller data.
- Browser/WASM remains feature-gated and does not compile Arrow stream,
  Parquet, Polars, SQLite, or native filesystem support for this release.
- The command continues to reject using stdin for both Algraf source and
  caller-provided data in the same invocation.
- Driver/data diagnostics remain stable for malformed Arrow streams,
  unsupported Arrow stream types, and unsupported sniffed Arrow IPC file bytes.

### Arrow Loader Efficiency

Status: In progress.

Implementation notes:

- Arrow stream column builders now reserve per-batch capacity before appending
  arrays.
- The current loader still copies Arrow arrays into Algraf-owned columns; this
  keeps the API boundary strict while leaving zero-copy or Arrow-backed columns
  as a later benchmark-driven decision.

Acceptance criteria:

- Arrow IPC stream loading can append record batches directly from a `Read`
  source.
- Builders reserve capacity when batch or stream row counts are known.
- Numeric and boolean arrays append in larger slices where Arrow exposes
  efficient access.
- Example extraction and diagnostic formatting stay bounded for large streams.
- Temporal units are normalized once per batch where possible.
- Dictionary/categorical inputs are either cast to strings or rejected
  deterministically until Algraf has a native category representation.
- Zero-copy Arrow-backed storage remains optional unless benchmarks prove the
  copy into Algraf columns is the dominant bottleneck after reader and stat
  changes.

### Column Handles And Column Views

Status: In progress.

Implementation notes:

- The first implementation slice keeps the existing `Table` API and resolves
  typed `ColumnView`s once per `Summary`, `SummaryBin`, `Ecdf`, `Qq`, and `Cut`
  execution.
- Explicit reusable column handles remain pending. The current slice proves the
  intended boundary: render hot paths can become more columnar without importing
  Arrow, Parquet, Polars, SQLite, or concrete dataframe internals.

Acceptance criteria:

- `Table::value(column, row)` remains available for compatibility, final mark
  emission, metadata, and edge cases.
- Renderer hot paths can resolve column names to handles once per plan.
- Scale training, domain collection, stats, and common geometries can borrow
  typed column views without repeated string lookup and scalar enum wrapping.
- The renderer remains decoupled from concrete Arrow, Parquet, Polars, or
  dataframe engine symbols.
- Missing-value semantics and source-order category behavior remain
  deterministic.

### Columnar Stat And Domain Fast Paths

Status: In progress.

Implementation notes:

- `SummaryBin` now reuses borrowed views for x, value, and grouping columns,
  and precomputes a deterministic group-key index instead of searching the
  group-domain vector for every row.
- `Summary`, `Ecdf`, `Qq`, and `Cut` now use typed column views when available
  rather than repeatedly resolving scalar values by column name.
- `count_by` now has typed single-column fast paths for `Bar(stat: "count")`
  over borrowed column views and preserves the semantic output dtype contract
  instead of stringifying all group keys.
- Category-domain collection keeps deterministic first-appearance output order
  while using a hash membership set to avoid linear `Vec::contains` scans.
- Added tests that panic if the new summary hot paths fall back to scalar
  `Table::value` access when a column view is available.
- Added count tests covering integer key dtype preservation and nested
  zero-count combinations.
- Broader stat coverage is still pending for z-field stats, regular-grid
  construction, density scans, mark emission, and any derived-domain caching.

Acceptance criteria:

- One-dimensional numeric bins, grouped `SummaryBin`, and common domain
  training use typed column scans for numeric, temporal, and categorical inputs.
- Multiple aesthetics referencing the same source column avoid redundant raw
  scans where practical.
- Binned/stat charts train domains from derived aggregate tables unless language
  semantics require raw source domains.
- Category collectors remain bounded and report deterministic diagnostics for
  overflow.
- Aggregate charts over generated large inputs are faster and lower allocation
  than the scalar-row path while producing the same deterministic SVG.

### Aggregate-First Rendering UX

Status: Planned.

Acceptance criteria:

- Raw mark budget diagnostics suggest applicable alternatives such as `Bin`,
  `Bin2D`, `SummaryBin`, sampling, SQLite/Parquet aggregation, or PDL
  pre-aggregation.
- Large demo charts continue to render bounded output from large generated
  inputs.
- At least one large demo or smoke script demonstrates PDL-style Arrow stream
  input reduced to a visual-sized chart.
- `--allow-large-render` remains an explicit expert escape hatch if raw large
  output is requested.
- Interaction metadata stays bounded by emitted marks, not source rows.

### PDL Arrow Stream Interop Validation

Status: Planned.

Acceptance criteria:

- The canonical PDL-to-Algraf command remains documented:

  ```bash
  pdl run prep.pdl --stdout-format arrow-stream \
    | algraf render chart.ag --data - --data-format arrow-stream --output chart.svg
  ```

- Algraf assumes stdout contains only Arrow IPC stream bytes; diagnostics,
  progress, and logs belong on stderr on the producer side.
- The smoke fixture covers a realistic PDL-prepared table with numeric,
  categorical string, boolean or flag, temporal, and nullable fields.
- Algraf verifies that the Arrow field names and field order it receives match
  the PDL-produced schema expected by the chart.
- Algraf validates multi-batch Arrow streams, null-heavy arrays, temporal
  columns, string columns, and unsupported Arrow types at the caller-data
  boundary.
- Algraf covers two PDL handoff shapes:
  - a large typed table where Algraf performs `SummaryBin`, domain training,
    and bounded mark reduction;
  - a visual-ready pre-aggregated table where Algraf mostly validates schema,
    trains scales, and renders bounded marks.
- The smoke path compares PDL Arrow stream output with CSV or Parquet input for
  the same aggregate chart when a local `pdl` binary is available.
- The smoke path exercises both explicit `--data-format arrow-stream` and the
  sniffed caller-data path when sniffing is available.
- Dictionary/categorical and nested Arrow inputs are cast or rejected
  deterministically according to the supported Algraf caller-data subset; they
  must not silently coerce to a different chart-visible value shape.
- The Algraf plan remains aligned with the neighboring
  [`V0_34_PLAN.md`](../../pdl/docs/V0_34_PLAN.md): PDL owns native
  transformation and writer-oriented Arrow output; Algraf owns visualization,
  stats, domains, mark budgets, and rendering.

### Browser/WASM Boundary

Status: In progress.

Implementation notes:

- `algraf-wasm` was checked for `wasm32-unknown-unknown` with the same rustup
  compiler override pattern used by the browser build scripts.
- The wasm dependency tree was searched for `arrow`, `parquet`, `polars`, and
  `libsqlite3-sys`; none were present.
- Native Arrow/Parquet/Polars/SQLite support remains outside the browser/WASM
  release surface.

Acceptance criteria:

- `algraf-wasm` continues to depend on feature-bearing workspace crates with
  default features disabled.
- `algraf-wasm` does not enable `arrow-stream`, `parquet`, `sql`, or any future
  native format feature as part of v0.69.
- `cargo check -p algraf-wasm --target wasm32-unknown-unknown` passes.
- `cargo tree -p algraf-wasm --target wasm32-unknown-unknown` has no `arrow`,
  `parquet`, `polars`, or `libsqlite3-sys` entries.
- Browser/WASM callers that request native-only data formats receive registered
  unsupported-format diagnostics instead of partial Arrow/Parquet support.
- Demo, editor, and npm package docs do not advertise browser Arrow-stream or
  Parquet caller-data support for v0.69.

### Strict API And Boundary Discipline

Status: In progress.

Implementation notes:

- `AGENTS.md` now records the general DSL/API boundary: Algraf syntax and public
  browser/editor surfaces stay visualization-focused and engine-independent;
  native format work stays in `algraf-data`/`algraf-driver`; render consumes
  table/column-view abstractions.
- The first performance slice does not expose any concrete Arrow, Parquet,
  Polars, SQLite, or native-format type outside its existing data/driver
  ownership boundary.

Acceptance criteria:

- Public APIs outside `algraf-data` and format-selection code in
  `algraf-driver` do not expose concrete Arrow, Parquet, Polars, SQLite, or
  native file-format types.
- `algraf-render` consumes stable table, column-handle, typed-column-view, scan,
  or aggregate APIs. It does not construct readers, inspect Arrow arrays, or
  depend on file-format crates.
- `algraf-driver` owns caller-data source resolution, format selection,
  sniffing, reader construction, and unsupported-format diagnostics. It does
  not own stat, scale, mark-budget, or rendering behavior.
- `algraf-data` owns concrete loader implementations and any Arrow/Parquet
  conversion details. New loader APIs should be narrow, additive, and written in
  terms of Algraf logical schemas and dataframe abstractions.
- Any new cross-crate API required for performance work has parity tests and a
  short plan/spec note explaining its ownership boundary before implementation
  is marked complete.
- Boundary tests or static checks fail if parser, syntax, semantics,
  editor-services, ordinary LSP, renderer public APIs, or WASM APIs import or
  name concrete native data-engine types.

### Spec, Version, And Release Metadata

Status: In progress.

Implementation notes:

- Workspace Cargo metadata, `Cargo.lock`, `docs/ALGRAF_SPEC.md`, VS Code
  package metadata, and the demo app's own package version are aligned to
  `0.69.0`.
- Verified npm publications on 2026-06-06:
  - `algraf-wasm`: `0.67.0`, `0.68.0`, `0.68.5`
  - `algraf-editor`: `0.67.0`, `0.68.1`, `0.68.5`
- Because `algraf-wasm@0.69.0` and `algraf-editor@0.69.0` are not published,
  browser package manifests and consumer dependency pins were not moved to
  `0.69.0`. Browser package publication remains independent from the native
  Rust/CLI release.

Acceptance criteria:

- `docs/ALGRAF_SPEC.md` records `0.69.0` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- Workspace/package version stamps that track the active release are aligned to
  `0.69.0`, including Cargo metadata, Cargo lockfile, VS Code package metadata,
  first-party browser package manifests/lockfiles, and demo package
  manifests/lockfiles.
- Normative spec sections are updated alongside each implementation item before
  that item is marked implemented.

## Architecture Boundary

Concrete data engines stop at `algraf-data`. `algraf-render` may ask for typed
column views, grouped scans, aggregate results, or column handles, but it should
not import Arrow, Parquet, Polars, or other engine symbols.

`algraf-driver` owns source resolution, caller-data policy, format selection,
and I/O shape. It may provide readers instead of byte buffers. It should not own
rendering logic or dataframe engine decisions beyond selecting formats and
contexts.

`algraf-render` owns visual semantics: which columns are needed, which stats are
requested, which bins or aggregations are appropriate, and how many marks are
safe to emit.

The `Table` trait remains the compatibility surface. Large-data internals
should prefer additive APIs such as column handles, typed column views, and scan
visitors rather than replacing the current dataframe abstraction in one step.
Strict API boundaries are part of the release scope, not cleanup. Performance
work should improve the internals without making renderer, language, editor, or
browser surfaces depend on native data-engine details.

## Non-Goals

- No PDL parser, executor, or `.pdl` awareness in Algraf.
- No source-language syntax change for caller-provided data.
- No Polars, Arrow, or Parquet symbols exposed in parser, syntax, semantics,
  ordinary LSP, or renderer public APIs.
- No promise that static SVG output will handle millions of raw marks.
- No distributed, remote, GPU, or streaming-rendering engine requirement.
- No removal of the existing `DataFrame` or `Table` abstractions.
- No browser/WASM dependency on native Arrow, Parquet, Polars, SQLite, or
  native filesystem format support in this release.

## Testing Strategy

Format and loader tests:

- explicit `--data-format arrow-stream` from stdin;
- sniffed Arrow stream from caller input;
- malformed Arrow stream diagnostics;
- unsupported Arrow types;
- multi-batch Arrow streams;
- null-heavy numeric, boolean, string, and temporal columns;
- large generated fixtures that do not live in git.

Performance-sensitive correctness tests:

- binning and aggregation from large generated data;
- category overflow diagnostics;
- mark budget rejection for raw points;
- bounded mark output for `Bin`, `Bin2D`, and aggregate charts;
- PDL-style Arrow stream fixtures consumed as caller data, including a
  mixed-type schema with numeric, categorical string, boolean or flag, temporal,
  and nullable fields;
- pre-aggregated PDL-style fixtures where Algraf should render bounded marks
  without repeating million-row reduction work.

Boundary tests:

- no Arrow, Parquet, or Polars imports outside data/driver crates unless a
  format-specific CLI test explicitly owns that dependency;
- WASM feature gates continue to produce registered diagnostics when a format is
  unavailable;
- `algraf-wasm` target dependency-tree checks reject Arrow, Parquet, Polars, and
  SQLite native dependencies;
- render output remains deterministic for aggregate charts.

## Benchmark Strategy

Ingest benchmarks:

- Arrow stream from stdin reader versus full-buffer byte path;
- Arrow stream multi-batch append;
- Parquet path-backed load;
- CSV load as a baseline;
- CSV versus Arrow stream versus Parquet for the same million-row aggregate
  chart.

Stat and binning benchmarks:

- `Bin` over millions of numeric values;
- `Bin2D` over dense points;
- grouped count over high-cardinality categories;
- grouped `SummaryBin` over millions of rows with a small number of groups;
- temporal bins;
- top-N category reduction.

End-to-end benchmarks:

- PDL Arrow stream into an Algraf aggregate chart, recording the PDL version or
  run label used to produce the stream;
- PDL Arrow stream into an Algraf chart from a visual-ready pre-aggregated
  table;
- CSV pipe into the same chart;
- Parquet path into the same chart;
- CSV, Parquet, and Arrow handoffs from equivalent PDL-produced data where
  practical;
- intentionally rejected raw point chart;
- raster heatmap from binned data.

`algraf-bench` reports MUST be CSV files under `bench/runs/<run-label>/`.
Reports should include wall-clock time, phase timings where practical, memory
where practical, input rows, derived rows, emitted marks, output bytes,
diagnostic counts, run label, and git ref. For aggregate charts, reports must
make row reduction explicit so large-data performance is not confused with
large raw-SVG output.

## Local Benchmark Results

Commands run for this implementation pass:

```bash
cargo run -p algraf-bench -- run --suite large --tier stress \
  --run-label v0_69_before_release --profile release --no-generate --no-prepare
cargo run -p algraf-bench -- run --suite large --tier stress \
  --run-label v0_69_after_release_count_fastpath --profile release --no-generate --no-prepare
cargo run -p algraf-bench -- compare \
  --before v0_69_before_release --after v0_69_after_release_count_fastpath
```

The first after run, `v0_69_after_release`, overlapped with another local
compile and showed large unrelated Parquet regressions. A later rerun failed
while rebuilding release binaries because the local filesystem had only about
308 MiB free; `cargo clean` removed 8.2 GiB of Cargo build artifacts before the
final samples were taken. Use `v0_69_after_release_count_fastpath` as the
comparison for this implementation pass.

Final release comparison:

| Workload | Before ms | After ms | Change | Notes |
| -------- | --------- | -------- | ------ | ----- |
| `million_row_summary_bin` | 2413 | 2086 | +13.6% | `SummaryBin` hot path; phase timing total 1382 ms. |
| `synthetic_bin2d_density` | 1001 | 921 | +8.0% | Parquet load still dominates; SVG unchanged. |
| `synthetic_nullable_histogram` | 541 | 499 | +7.8% | Column-domain/stat path; SVG unchanged. |
| `synthetic_projection_smoke` | 937 | 835 | +10.9% | Wide Parquet input; phase timing total 581 ms. |
| `synthetic_raw_mark_budget` | 1039 | 977 | +6.0% | Expected diagnostic path; no output SVG. |
| `tlc_trip_distance_histogram` | 1448 | 1118 | +22.8% | Large Parquet histogram; phase timing total 1030 ms. |
| `tlc_fare_distance_density` | 1214 | 1141 | +6.0% | Large Parquet density; output unchanged. |
| `tlc_payment_type_counts` | 1647 | 1603 | +2.7% | Count fast path removed the earlier render-phase regression. |
| `tlc_pickup_time_bins` | 1240 | 1284 | -3.5% | Remaining small regression; temporal bin load/render phases need another sample. |
| `sfo_daily_flights` | 105 | 98 | +6.7% | Small absolute delta. |
| `sfo_event_counts` | 111 | 111 | 0.0% | Count fast path render phase is about 19 ms. |
| `sfo_airline_counts` | 118 | 114 | +3.4% | Small absolute delta. |
| `sfo_route_density` | 100 | 93 | +7.0% | Small absolute delta. |

All successful workloads emitted the same mark counts and SVG byte counts in the
before and after reports. The phase timing columns show that large Parquet
charts are still dominated by prepare/data loading: TLC workloads spend roughly
1.0 second in prepare before render-specific work begins. The count fast path
was necessary because a generic native-key implementation preserved dtype but
regressed render time; the final typed single-column path brought
`tlc_payment_type_counts` render timing down to about 490 ms and SFO count
render timing to about 19-20 ms.

Next performance work should target the remaining prepare-side cost: Parquet
projection/predicate behavior, Arrow/Parquet append copies, temporal-bin scans,
derived-domain reuse, z-field stat scans, and multi-iteration benchmark
reporting so one-shot noise is easier to separate from real regressions.

## Deferred

- Arrow-backed or Polars-backed columns inside `algraf-data`, unless benchmarks
  show owned `DataFrame` copies are the dominant remaining bottleneck.
- Arrow IPC file input parity beyond existing unsupported-format diagnostics.
- Browser/WASM Arrow stream or Parquet decoding.
- Retained-canvas, WebGL, or other specialized large-mark render backends.
- Streaming scale/stat passes that never materialize full source tables.
- Approximate or sampled previews beyond explicit, documented opt-in features.
