# Algraf v0.68.0 Plan

Status: Implemented
Target version: 0.68.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_67_PLAN.md`](V0_67_PLAN.md)
Related PDL plan: [`V0_31_PLAN.md`](../../pdl/docs/V0_31_PLAN.md)
Follow-on performance plan: [`V0_69_PLAN.md`](V0_69_PLAN.md)
Roadmap theme: Benchmark infrastructure and cross-repo baseline alignment.
Cross-repo coordination: `../pdl/` for matching dataset lifecycle, workload
layout, CSV report schema, and baseline snapshot conventions.

## Purpose

Algraf v0.68 establishes the benchmark process needed before the Arrow,
columnar-stat, and aggregate-rendering performance work starts. The goal is not
to optimize rendering in this release. The goal is to make before/after
performance evidence repeatable across Algraf and PDL by aligning datasets,
workload locations, generated outputs, report files, and baseline snapshots.

Before this release, benchmark assets were split across `bench/`,
`bench-output/`, `benchdata/`, `target/algraf-large-fixtures/`, shell scripts,
and Rust examples in `algraf-data`. That made it hard to know which inputs were
authoritative, which results were comparable, or which cleanup was safe.

This release creates a single benchmark lifecycle:

```text
download/generate source data
  -> prepare chart-ready external fixtures
  -> run tracked workloads
  -> write ignored per-run CSV reports
  -> snapshot selected reports as tracked baselines
```

The follow-on [`V0_69_PLAN.md`](V0_69_PLAN.md) owns the Arrow-reader,
column-view, stat/domain, and aggregate rendering performance changes. This
plan owns the measurement harness those changes will be evaluated against.

## Scope

### Benchmark Crate

Status: Implemented.

Acceptance criteria:

- Add a workspace `algraf-bench` crate.
- Provide Rust commands for `generate`, `download`, `prepare`, `run`, and
  `snapshot` so the primary benchmark lifecycle is not shell-script driven.
- Keep legacy scripts as thin compatibility wrappers around `cargo run -p
  algraf-bench`.
- Support repeatable run labels so Algraf and PDL can be benchmarked with the
  same label.
- Write report output as CSV, not TSV.

### Directory Layout

Status: Implemented.

Acceptance criteria:

- Tracked workloads live under `bench/workloads/`.
- Ignored source and generated data live under `bench/data/`.
- Ignored per-run outputs live under `bench/runs/<run-label>/`.
- Tracked curated baselines live under `bench/baselines/<baseline>/`.
- Old `benchdata/`, `bench-output/`, `bench/examples/`, and
  `target/algraf-large-fixtures/` paths are removed or migrated.
- `.gitignore` documents only the active ignored benchmark directories.

### Dataset Family

Status: Implemented.

Acceptance criteria:

- Generate deterministic synthetic large-data fixtures.
- Keep the shared million-row CSV fixture aligned with PDL.
- Download and preserve external TLC and SFO source files under
  `bench/data/raw/` when requested.
- Prepare chart-ready external Parquet fixtures under `bench/data/prepared/`.
- Keep large downloaded/generated/prepared inputs out of git.
- Make full-size and smaller smoke/local tiers available from the same command
  surface.

### Workloads And Reports

Status: Implemented.

Acceptance criteria:

- Run the large workload suite from `bench/workloads/large/`.
- Include generated synthetic, TLC, and SFO benchmark cases.
- Emit `bench/runs/<run-label>/report.csv` with one row per benchmark case.
- Capture command, workload, input/output format, status, elapsed time, byte
  counts, SVG element counts, and output path in the report.
- Treat expected diagnostics, such as raw mark-budget rejection, as report rows
  rather than silent failures.

### Baseline Snapshots

Status: Implemented.

Acceptance criteria:

- Add `algraf-bench snapshot --run-label <label> --baseline <name>`.
- Copy a selected ignored run report into
  `bench/baselines/<baseline>/report.csv`.
- Record environment metadata in
  `bench/baselines/<baseline>/environment.txt`.
- Capture git ref, git status, toolchain versions, host information, source
  report path, and run command.
- Capture git status before writing the baseline files so snapshots do not
  describe themselves as new untracked files.

### Baseline Run

Status: Implemented.

Acceptance criteria:

- Run the full benchmark lifecycle from scratch using downloaded/generated data.
- Capture a shared baseline label that can be compared with PDL.
- Store the tracked baseline report under
  `bench/baselines/full-baseline-20260606/report.csv`.

Observed baseline:

- Run label: `full-baseline-20260606`.
- Report path: `bench/runs/full-baseline-20260606/report.csv`.
- Snapshot path: `bench/baselines/full-baseline-20260606/report.csv`.
- Result: 13 benchmark rows; 12 `ok`, 1 `expected-diagnostic`.
- Dataset families covered: generated synthetic, TLC, and SFO.

## Non-Goals

- Implementing reader-oriented Arrow ingestion.
- Implementing column-view/stat/domain fast paths.
- Changing Algraf source-language semantics or rendering semantics.
- Changing the normative format or CLI behavior in `ALGRAF_SPEC.md` beyond
  documenting benchmark process if desired.
- Treating local baseline timings as CI thresholds before a reference machine,
  variance policy, and release-mode benchmark process are defined.

## Validation

- `cargo fmt --all`
- `cargo check -p algraf-bench`
- `cargo run -p algraf-bench -- generate --tier smoke`
- `cargo run -p algraf-bench -- run --suite large --tier smoke --run-label smoke-algraf-bench --no-generate --no-prepare`
- `cargo run -p algraf-bench -- download --dataset all --force`
- `cargo run -p algraf-bench -- generate --tier stress`
- `cargo run -p algraf-bench -- prepare --dataset all`
- `cargo run -p algraf-bench -- run --suite large --tier stress --run-label full-baseline-20260606 --no-generate --no-prepare`
- `cargo run -p algraf-bench -- snapshot --run-label full-baseline-20260606 --baseline full-baseline-20260606`
