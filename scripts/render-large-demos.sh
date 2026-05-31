#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/debug/algraf"
OUT="$ROOT/bench-output/large-demos"
REPORT="$OUT/report.tsv"

mkdir -p "$OUT"

if [[ ! -f "$ROOT/target/algraf-large-fixtures/smoke/manifest.json" ]]; then
    "$ROOT/scripts/generate-large-fixtures.sh" --tier smoke
fi

needs_tlc_prepare=false
needs_sfo_prepare=false
if [[ -f "$ROOT/benchdata/raw/tlc/yellow_tripdata_2024-01.parquet" && ! -f "$ROOT/benchdata/prepared/tlc/yellow_tripdata_2024-01_chart.parquet" ]]; then
    needs_tlc_prepare=true
fi
if [[ -f "$ROOT/benchdata/raw/sfo/sfomuseum-data-flights-2026-03.parquet" && ! -f "$ROOT/benchdata/prepared/sfo/sfomuseum-data-flights-2026-03_chart.parquet" ]]; then
    needs_sfo_prepare=true
fi
if [[ "$needs_tlc_prepare" == "true" || "$needs_sfo_prepare" == "true" ]]; then
    "$ROOT/scripts/prepare-large-fixtures.sh"
fi

cd "$ROOT"
cargo build -p algraf-cli

printf 'chart\tstatus\tsvg\tpng\tmarks\telapsed_seconds\n' > "$REPORT"

mark_count() {
    local svg="$1"
    if [[ ! -f "$svg" ]]; then
        printf '0'
        return
    fi
    grep -Eo '<(circle|rect|path|line|polyline|polygon|text)\b' "$svg" | wc -l | tr -d ' '
}

render_success() {
    local chart="$1"
    local name
    name="$(basename "$chart" .ag)"
    local out="$OUT/$name.svg"
    local png="$OUT/$name.png"
    local log="$OUT/$name.log"
    local start end elapsed marks
    start="$(date +%s)"
    if "$BIN" render "$chart" --output "$out" >"$log" 2>&1; then
        "$BIN" render "$chart" --output "$png" >>"$log" 2>&1
        end="$(date +%s)"
        elapsed=$((end - start))
        marks="$(mark_count "$out")"
        printf '%s\tok\t%s\t%s\t%s\t%s\n' "$name" "$out" "$png" "$marks" "$elapsed" | tee -a "$REPORT"
    else
        end="$(date +%s)"
        elapsed=$((end - start))
        printf '%s\tfailed\t%s\t-\t0\t%s\n' "$name" "$log" "$elapsed" | tee -a "$REPORT"
        return 1
    fi
}

render_expected_budget_failure() {
    local chart="$1"
    local name
    name="$(basename "$chart" .ag)"
    local out="$OUT/$name.svg"
    local log="$OUT/$name.log"
    local start end elapsed
    start="$(date +%s)"
    if "$BIN" render "$chart" --mark-budget 500 --output "$out" >"$log" 2>&1; then
        printf '%s\tunexpected-success\t%s\t-\t%s\t0\n' "$name" "$out" "$(mark_count "$out")" | tee -a "$REPORT"
        return 1
    fi
    end="$(date +%s)"
    elapsed=$((end - start))
    if grep -q 'E2001' "$log"; then
        printf '%s\texpected-E2001\t%s\t-\t0\t%s\n' "$name" "$log" "$elapsed" | tee -a "$REPORT"
    else
        printf '%s\tfailed-without-E2001\t%s\t-\t0\t%s\n' "$name" "$log" "$elapsed" | tee -a "$REPORT"
        return 1
    fi
}

for chart in \
    bench/examples/large/synthetic_bin2d_density.ag \
    bench/examples/large/synthetic_nullable_histogram.ag \
    bench/examples/large/synthetic_projection_smoke.ag
do
    render_success "$chart"
done

render_expected_budget_failure bench/examples/large/synthetic_raw_mark_budget.ag

if [[ -f benchdata/prepared/tlc/yellow_tripdata_2024-01_chart.parquet ]]; then
    for chart in \
        bench/examples/large/tlc_trip_distance_histogram.ag \
        bench/examples/large/tlc_fare_distance_density.ag \
        bench/examples/large/tlc_payment_type_counts.ag \
        bench/examples/large/tlc_pickup_time_bins.ag
    do
        render_success "$chart"
    done
else
    printf 'skip\ttlc-missing\tbenchdata/prepared/tlc/yellow_tripdata_2024-01_chart.parquet\t-\t0\t0\n' | tee -a "$REPORT"
fi

if [[ -f benchdata/prepared/sfo/sfomuseum-data-flights-2026-03_chart.parquet ]]; then
    for chart in \
        bench/examples/large/sfo_daily_flights.ag \
        bench/examples/large/sfo_event_counts.ag \
        bench/examples/large/sfo_airline_counts.ag \
        bench/examples/large/sfo_route_density.ag
    do
        render_success "$chart"
    done
else
    printf 'skip\tsfo-missing\tbenchdata/prepared/sfo/sfomuseum-data-flights-2026-03_chart.parquet\t-\t0\t0\n' | tee -a "$REPORT"
fi

printf '\nLarge demo report: %s\n' "$REPORT"
