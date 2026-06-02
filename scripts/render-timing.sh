#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/release/render-timing"
ITERATIONS="${ALGRAF_TIMING_ITERATIONS:-200}"
WARMUP="${ALGRAF_TIMING_WARMUP:-20}"

cd "$ROOT"
cargo build --release -p algraf-render --bin render-timing

run_case() {
    local name="$1"
    shift
    printf '\n== %s ==\n' "$name"
    "$BIN" --warmup "$WARMUP" --iterations "$ITERATIONS" "$@"
}

run_case \
    "weather alias embedded JSON" \
    bench/examples/local/weather_alias.ag \
    --input bench/examples/local/weather_hourly.json \
    --data-format json

run_case \
    "weather alias embedded JSON interactive" \
    bench/examples/local/weather_alias.ag \
    --input bench/examples/local/weather_hourly.json \
    --data-format json \
    --interactive

run_case "scatter path data" examples/scatter.ag
run_case "bin2d path data" examples/bin2d.ag
run_case "smooth path data" examples/smooth.ag
