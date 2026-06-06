# Algraf v0.68.0 Plan

Status: Planned
Target version: 0.68.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_67_PLAN.md`](V0_67_PLAN.md)
Promoted from: [`ARROW_PERFORMANCE.md`](ARROW_PERFORMANCE.md)
Neighboring PDL plan: [`V0_31_PLAN.md`](../../pdl/docs/V0_31_PLAN.md)
Neighboring PDL source note: `pdl/docs/POLARS_PERFORMANCE.md`
Roadmap theme: Arrow-stream and large-data aggregate performance.
Cross-repo coordination: `../pdl/` for Polars-backed execution and Arrow IPC
stream output.

## Purpose

Algraf v0.68 makes the PDL-to-Algraf typed pipe and large aggregate rendering
path the active implementation target:

```bash
pdl run prep.pdl --stdout-format arrow-stream \
  | algraf render chart.ag --data - --data-format arrow-stream --output chart.svg
```

PDL's neighboring v0.31 Polars performance plan, promoted from
`pdl/docs/POLARS_PERFORMANCE.md`, targets native reduction, lazy scans,
writer-oriented Arrow IPC stream output, and clean stdout. Algraf's side is the
consumer and renderer: it should accept caller-provided Arrow streams without
avoidable full buffering, process large source tables through column-oriented
stats and domains, and render visual-sized aggregate output instead of millions
of raw SVG nodes.

This release does not make Algraf parse or execute PDL. The format boundary is
Arrow IPC stream bytes, and the source-language boundary remains
`Chart(data: input)` or `Chart(data: stdin)` plus the CLI caller-data flags.

## Release Thesis

v0.68 is the **Arrow and aggregate performance** release. It should make large
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

- caller-provided stdin is read into a full `Vec<u8>` before format dispatch;
- Arrow stream bytes are decoded into an owned `DataFrame`;
- strings are copied out of Arrow arrays into `Vec<Option<String>>`;
- numeric columns are copied from Arrow buffers into Algraf-owned buffers;
- render planning, domains, scales, stats, facets, and marks repeatedly scan
  materialized tables;
- many stats and geometry paths still iterate row indices and ask for scalar
  values;
- SVG and draw-list outputs remain proportional to emitted mark count.

The existing implementation is correct and well-separated. v0.68 should keep
that shape while making the hot paths more columnar and stream friendly.

## Observed Million-Row Baseline

The large-demo suite includes a deterministic CSV aggregate benchmark:

```text
scripts/generate-million-row-csv.sh
bench/examples/large/million_row_summary_bin.ag
benchdata/generated/million-row.csv
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
- `scripts/render-large-demos.sh`: 31 seconds for this chart's SVG plus PNG
  render pair.

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

Status: Planned.

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

Status: Planned.

Acceptance criteria:

- Explicit `--data-format arrow-stream` from `--data -` starts Arrow decoding
  from a reader path instead of requiring a full stdin `Vec<u8>` first.
- Omitted-format sniffing still uses a bounded prefix buffer, preserves peeked
  bytes, detects Arrow IPC stream and Parquet magic, and falls back to CSV.
- In-memory hosts and WASM continue to support byte-buffer caller data.
- The command continues to reject using stdin for both Algraf source and
  caller-provided data in the same invocation.
- Driver/data diagnostics remain stable for malformed Arrow streams,
  unsupported Arrow stream types, and unsupported sniffed Arrow IPC file bytes.

### Arrow Loader Efficiency

Status: Planned.

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

Status: Planned.

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

Status: Planned.

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
- Algraf validates multi-batch Arrow streams, null-heavy arrays, temporal
  columns, string columns, and unsupported Arrow types at the caller-data
  boundary.
- A local smoke path can compare PDL Arrow stream output with CSV or Parquet
  input for the same aggregate chart when a local `pdl` binary is available.
- The Algraf plan remains aligned with the neighboring
  [`V0_31_PLAN.md`](../../pdl/docs/V0_31_PLAN.md): PDL owns native
  transformation and writer-oriented Arrow output; Algraf owns visualization,
  stats, domains, mark budgets, and rendering.

### Spec, Version, And Release Metadata

Status: Planned.

Acceptance criteria:

- `docs/ALGRAF_SPEC.md` records `0.68.0` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- Workspace/package version stamps that track the active release are aligned to
  `0.68.0`, including Cargo metadata, Cargo lockfile, VS Code package metadata,
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

## Non-Goals

- No PDL parser, executor, or `.pdl` awareness in Algraf.
- No source-language syntax change for caller-provided data.
- No Polars, Arrow, or Parquet symbols exposed in parser, syntax, semantics,
  ordinary LSP, or renderer public APIs.
- No promise that static SVG output will handle millions of raw marks.
- No distributed, remote, GPU, or streaming-rendering engine requirement.
- No removal of the existing `DataFrame` or `Table` abstractions.
- No default browser/WASM dependency on native Arrow, Parquet, or Polars.

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
- PDL-style Arrow stream fixtures consumed as caller data.

Boundary tests:

- no Arrow, Parquet, or Polars imports outside data/driver crates unless a
  format-specific CLI test explicitly owns that dependency;
- WASM feature gates continue to produce registered diagnostics when a format is
  unavailable;
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

- PDL Arrow stream into Algraf aggregate chart;
- CSV pipe into the same chart;
- Parquet path into the same chart;
- intentionally rejected raw point chart;
- raster heatmap from binned data.

Reports should include wall-clock time, phase timings where practical, memory
where practical, input rows, derived rows, emitted marks, output bytes, and
diagnostic counts. For aggregate charts, reports must make row reduction
explicit so large-data performance is not confused with large raw-SVG output.

## Deferred

- Arrow-backed or Polars-backed columns inside `algraf-data`, unless benchmarks
  show owned `DataFrame` copies are the dominant remaining bottleneck.
- Arrow IPC file input parity beyond existing unsupported-format diagnostics.
- Browser/WASM Arrow stream decoding when dependency footprint remains too
  costly.
- Retained-canvas, WebGL, or other specialized large-mark render backends.
- Streaming scale/stat passes that never materialize full source tables.
- Approximate or sampled previews beyond explicit, documented opt-in features.
