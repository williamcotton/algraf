#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/debug/algraf"
OUT="$ROOT/bench-output/large-demos"
REPORT="$OUT/report.tsv"
REPORT_HEADER=$'chart\tstatus\tsvg\tpng\tmarks\telapsed_ms\trun_timestamp_utc\tgit_ref'
RUN_TIMESTAMP_UTC="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
RUN_GIT_REF="$(git -C "$ROOT" describe --tags --always --dirty 2>/dev/null || printf 'unknown')"

mkdir -p "$OUT"

if [[ ! -f "$ROOT/target/algraf-large-fixtures/smoke/manifest.json" ]]; then
    "$ROOT/scripts/generate-large-fixtures.sh" --tier smoke
fi
if [[ ! -f "$ROOT/benchdata/generated/million-row.csv" ]]; then
    "$ROOT/scripts/generate-million-row-csv.sh"
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

if [[ ! -s "$REPORT" ]]; then
    printf '%s\n' "$REPORT_HEADER" > "$REPORT"
elif ! head -n 1 "$REPORT" | grep -qx "$REPORT_HEADER"; then
    printf 'warning: %s already exists with a different header; appending rows with current schema\n' "$REPORT" >&2
fi

now_ms() {
    if command -v python3 >/dev/null 2>&1; then
        python3 -c 'import time; print(time.time_ns() // 1000000)'
    elif command -v perl >/dev/null 2>&1; then
        perl -MTime::HiRes=time -e 'printf "%.0f\n", time() * 1000'
    else
        printf '%s000\n' "$(date +%s)"
    fi
}

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
    local start_ms end_ms elapsed_ms marks
    start_ms="$(now_ms)"
    if "$BIN" render "$chart" --output "$out" >"$log" 2>&1; then
        "$BIN" render "$chart" --output "$png" >>"$log" 2>&1
        end_ms="$(now_ms)"
        elapsed_ms=$((end_ms - start_ms))
        marks="$(mark_count "$out")"
        printf '%s\tok\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$out" "$png" "$marks" "$elapsed_ms" "$RUN_TIMESTAMP_UTC" "$RUN_GIT_REF" | tee -a "$REPORT"
    else
        end_ms="$(now_ms)"
        elapsed_ms=$((end_ms - start_ms))
        printf '%s\tfailed\t%s\t-\t0\t%s\t%s\t%s\n' "$name" "$log" "$elapsed_ms" "$RUN_TIMESTAMP_UTC" "$RUN_GIT_REF" | tee -a "$REPORT"
        return 1
    fi
}

render_expected_budget_failure() {
    local chart="$1"
    local name
    name="$(basename "$chart" .ag)"
    local out="$OUT/$name.svg"
    local log="$OUT/$name.log"
    local start_ms end_ms elapsed_ms
    start_ms="$(now_ms)"
    if "$BIN" render "$chart" --mark-budget 500 --output "$out" >"$log" 2>&1; then
        printf '%s\tunexpected-success\t%s\t-\t%s\t0\t%s\t%s\n' "$name" "$out" "$(mark_count "$out")" "$RUN_TIMESTAMP_UTC" "$RUN_GIT_REF" | tee -a "$REPORT"
        return 1
    fi
    end_ms="$(now_ms)"
    elapsed_ms=$((end_ms - start_ms))
    if grep -q 'E2001' "$log"; then
        printf '%s\texpected-E2001\t%s\t-\t0\t%s\t%s\t%s\n' "$name" "$log" "$elapsed_ms" "$RUN_TIMESTAMP_UTC" "$RUN_GIT_REF" | tee -a "$REPORT"
    else
        printf '%s\tfailed-without-E2001\t%s\t-\t0\t%s\t%s\t%s\n' "$name" "$log" "$elapsed_ms" "$RUN_TIMESTAMP_UTC" "$RUN_GIT_REF" | tee -a "$REPORT"
        return 1
    fi
}

for chart in \
    bench/examples/large/million_row_summary_bin.ag \
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
    printf 'skip\ttlc-missing\tbenchdata/prepared/tlc/yellow_tripdata_2024-01_chart.parquet\t-\t0\t0\t%s\t%s\n' "$RUN_TIMESTAMP_UTC" "$RUN_GIT_REF" | tee -a "$REPORT"
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
    printf 'skip\tsfo-missing\tbenchdata/prepared/sfo/sfomuseum-data-flights-2026-03_chart.parquet\t-\t0\t0\t%s\t%s\n' "$RUN_TIMESTAMP_UTC" "$RUN_GIT_REF" | tee -a "$REPORT"
fi

printf '\nLarge demo report: %s\n' "$REPORT"
