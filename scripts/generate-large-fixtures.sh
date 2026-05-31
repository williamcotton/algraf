#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TIER="${ALGRAF_LARGE_TIER:-smoke}"
OUT="$ROOT/target/algraf-large-fixtures"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --tier)
            TIER="$2"
            shift 2
            ;;
        --out)
            OUT="$2"
            shift 2
            ;;
        -h|--help)
            cat <<'USAGE'
Usage: scripts/generate-large-fixtures.sh [--tier smoke|local|stress] [--out DIR]

Generates deterministic synthetic Parquet fixtures plus CSV/NDJSON mirrors under
target/algraf-large-fixtures by default. No network access is used.
USAGE
            exit 0
            ;;
        *)
            echo "unexpected argument: $1" >&2
            exit 2
            ;;
    esac
done

cd "$ROOT"
cargo run -p algraf-data --features parquet --example generate_large_fixtures -- \
    --tier "$TIER" \
    --out "$OUT"
