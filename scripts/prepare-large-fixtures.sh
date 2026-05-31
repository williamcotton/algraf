#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RAW_TLC="$ROOT/benchdata/raw/tlc/yellow_tripdata_2024-01.parquet"
PREP_TLC="$ROOT/benchdata/prepared/tlc/yellow_tripdata_2024-01_chart.parquet"
RAW_SFO="$ROOT/benchdata/raw/sfo/sfomuseum-data-flights-2026-03.parquet"
PREP_SFO="$ROOT/benchdata/prepared/sfo/sfomuseum-data-flights-2026-03_chart.parquet"

mkdir -p "$(dirname "$PREP_TLC")" "$(dirname "$PREP_SFO")"

if [[ ! -f "$RAW_TLC" && ! -f "$RAW_SFO" ]]; then
    cat >&2 <<EOF
missing external sources:
- $RAW_TLC
- $RAW_SFO
Run scripts/download-large-fixtures.sh first, or place local Parquet files at those paths.
EOF
    exit 1
fi

cargo run -p algraf-data --features parquet --example prepare_large_fixtures -- \
    --tlc-in "$RAW_TLC" \
    --tlc-out "$PREP_TLC" \
    --sfo-in "$RAW_SFO" \
    --sfo-out "$PREP_SFO"
