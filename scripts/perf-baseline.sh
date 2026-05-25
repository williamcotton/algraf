#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/debug/algraf"
OUT="${ALGRAF_PERF_OUT:-$ROOT/target/perf-baseline}"
TIME_BIN="${TIME_BIN:-/usr/bin/time}"

mkdir -p "$OUT"

printf 'Algraf performance baseline\n'
printf 'date: %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
printf 'system: %s\n' "$(uname -a)"
printf 'rustc: %s\n' "$(rustc -V)"
printf 'cargo: %s\n' "$(cargo -V)"
printf '\n'

cargo build -p algraf-cli

run_case() {
    local name="$1"
    shift
    printf '\n== %s ==\n' "$name"
    "$TIME_BIN" -p "$@" >/dev/null
}

run_case "check scatter" "$BIN" check "$ROOT/examples/scatter.ag"
run_case "schema scatter sample" "$BIN" schema "$ROOT/examples/scatter.ag" --sample-size 200
run_case "render scatter" "$BIN" render "$ROOT/examples/scatter.ag" --output "$OUT/scatter.svg"
run_case "render histogram" "$BIN" render "$ROOT/examples/histogram.ag" --output "$OUT/histogram.svg"
run_case "render bin2d" "$BIN" render "$ROOT/examples/bin2d.ag" --output "$OUT/bin2d.svg"
run_case "render smooth" "$BIN" render "$ROOT/examples/smooth.ag" --output "$OUT/smooth.svg"

printf '\noutputs: %s\n' "$OUT"
