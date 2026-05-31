#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TLC_URL="${ALGRAF_TLC_URL:-https://d37ci6vzurychx.cloudfront.net/trip-data/yellow_tripdata_2024-01.parquet}"
TLC_ZONES_URL="${ALGRAF_TLC_ZONES_URL:-https://d37ci6vzurychx.cloudfront.net/misc/taxi_zone_lookup.csv}"
SFO_URL="${ALGRAF_SFO_URL:-https://static.sfomuseum.org/parquet/sfomuseum-data-flights-2026-03.parquet}"

TLC_DIR="$ROOT/benchdata/raw/tlc"
SFO_DIR="$ROOT/benchdata/raw/sfo"
mkdir -p "$TLC_DIR" "$SFO_DIR"

download() {
    local url="$1"
    local dest="$2"
    if [[ -f "$dest" ]]; then
        printf 'exists: %s\n' "$dest"
        return
    fi
    printf 'downloading: %s\n' "$url"
    printf '        to: %s\n' "$dest"
    curl -L --fail --output "$dest" "$url"
}

cat <<NOTES
External data sources:
- NYC Taxi & Limousine Commission trip records, Yellow Taxi January 2024:
  $TLC_URL
- NYC TLC taxi zone lookup:
  $TLC_ZONES_URL
- SFO Museum flight data, March 2026:
  $SFO_URL

Files are written under benchdata/raw/ and are intentionally gitignored.
Review upstream source pages for current licensing and citation requirements.
NOTES

download "$TLC_URL" "$TLC_DIR/yellow_tripdata_2024-01.parquet"
download "$TLC_ZONES_URL" "$TLC_DIR/taxi_zone_lookup.csv"
download "$SFO_URL" "$SFO_DIR/sfomuseum-data-flights-2026-03.parquet"

printf '\nDownloaded large fixtures under %s\n' "$ROOT/benchdata/raw"
