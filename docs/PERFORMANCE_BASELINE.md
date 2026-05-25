# Performance Baseline

v0.19.0 adds a lightweight timing script rather than CI-enforced benchmarks.
The script is intended to catch large local regressions and to collect reference
numbers before promoting Polars, lazy execution, or query-driven compilation.

## Reference Environment

- Date recorded: 2026-05-25
- Host: `aarch64-apple-darwin`, Darwin 24.6.0
- Rust: `rustc 1.95.0 (59807616e 2026-04-14)`
- Cargo: `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`
- CPU details: unavailable in the sandboxed environment; `sysctl` was denied.

No timing thresholds are enforced in CI in this release.

## Script

Run:

```bash
scripts/perf-baseline.sh
```

The script builds `algraf-cli`, records basic toolchain details, and times:

- parser/schema/semantic path via `algraf check`;
- schema loading via `algraf schema`;
- representative render cases (`scatter`, `histogram`, `bin2d`, `smooth`).

Outputs are written under `target/perf-baseline/` and may be deleted at any
time.
