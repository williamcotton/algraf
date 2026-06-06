# Algraf Benchmarks

`algraf-bench` owns the benchmark lifecycle for this repo.

```bash
cargo run -p algraf-bench -- generate --tier smoke
cargo run -p algraf-bench -- download --dataset all
cargo run -p algraf-bench -- prepare --dataset all
cargo run -p algraf-bench -- run --suite large --tier smoke --run-label before-v0.69
cargo run -p algraf-bench -- compare --before before-v0.69 --after after-v0.69
cargo run -p algraf-bench -- snapshot --run-label before-v0.69 --baseline v0.68.0-before-v0.69
```

## Layout

- `bench/workloads/` is tracked source: `.ag` charts and local fixtures grouped
  by suite.
- `bench/data/generated/` is ignored deterministic synthetic data.
- `bench/data/raw/` is ignored downloaded source data.
- `bench/data/prepared/` is ignored derived chart-ready data.
- `bench/runs/<run-label>/` is ignored benchmark output.
- `bench/baselines/<baseline>/` is tracked curated benchmark history.

Every run writes `bench/runs/<run-label>/report.csv`. Reports use the same
column contract as `pdl-bench` and include `git describe --tags --always
--dirty` so before/after runs can be compared by tag or commit.

Use `compare` to print per-workload elapsed-time deltas and improvement
percentages for two ignored run reports.

Use `snapshot` to promote an ignored run report into `bench/baselines/`.
Snapshots copy `report.csv` and write `environment.txt` with the git ref,
system, Rust/Cargo versions, source report, and snapshot timestamp.

The current generated source families are `million-row` and Algraf synthetic
Parquet fixtures. Downloaded source families currently include NYC TLC January
2024 trips and SFO Museum March 2026 flights.
