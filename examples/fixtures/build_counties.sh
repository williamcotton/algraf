#!/usr/bin/env bash
#
# Builds the US county choropleth fixtures for examples/ from public-domain
# Census sources:
#
#   examples/us_counties.geojson          (GeoJson source for the capstone)
#   examples/cb_2018_us_county_20m.shp    (Shapefile source — same map)
#   examples/cb_2018_us_county_20m.{dbf,shx,prj,cpg}
#
# Both decode to the identical geometry + `population` column, so the
# Space/Geo/Scale body of the capstone is the same for either source.
#
# This is a ONE-TIME prep step. It is NOT run by generate.sh or CI, and the
# algraf binary itself never touches the network (spec §10.8) — only this
# script does, to fetch the source data once. Re-run it only to refresh the
# checked-in fixtures.
#
# Requires: curl, unzip, python3, and `npx mapshaper`.
#
# Sources (both public domain, U.S. Government works):
#   - Census Cartographic Boundary counties, 1:20,000,000 (2018)
#   - Census county population estimates, 2018 vintage
#
set -euo pipefail

EXAMPLES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "==> Downloading Census county boundaries (20m)…"
curl -sL -o "$WORK/counties.zip" \
  'https://www2.census.gov/geo/tiger/GENZ2018/shp/cb_2018_us_county_20m.zip'
unzip -oq "$WORK/counties.zip" -d "$WORK"

echo "==> Downloading county population estimates…"
curl -sL -o "$WORK/co-est2019.csv" \
  'https://www2.census.gov/programs-surveys/popest/datasets/2010-2019/counties/totals/co-est2019-alldata.csv'

echo "==> Slimming population CSV to GEOID,population…"
python3 - "$WORK/co-est2019.csv" "$WORK/pop.csv" <<'PY'
import csv, sys
src, dst = sys.argv[1], sys.argv[2]
with open(src, encoding='latin-1', newline='') as fh, open(dst, 'w', newline='') as out:
    reader = csv.DictReader(fh)
    writer = csv.writer(out)
    writer.writerow(['GEOID', 'population'])
    for row in reader:
        if row['COUNTY'] == '000':          # skip state-total rows
            continue
        writer.writerow([row['STATE'] + row['COUNTY'], row['POPESTIMATE2018']])
PY

echo "==> Joining population, filtering to lower-48 + DC, exporting both formats…"
# Drop Alaska (02), Hawaii (15), and territories (FIPS >= 60): a plain lower-48
# `albers` projection sends those to absurd coordinates and blows out the bbox.
# AK/HI return with the albersUsa composite-projection work (V0.8 Must #5).
npx -y mapshaper@latest "$WORK/cb_2018_us_county_20m.shp" \
  -join "$WORK/pop.csv" keys=GEOID,GEOID field-types=GEOID:str,population:number \
  -filter 'STATEFP != "02" && STATEFP != "15" && STATEFP < "60"' \
  -filter-fields NAME,GEOID,population \
  -o force precision=0.0001 "$EXAMPLES_DIR/us_counties.geojson" \
  -o force precision=0.0001 "$EXAMPLES_DIR/cb_2018_us_county_20m.shp"

echo "==> Done. Wrote:"
ls -la "$EXAMPLES_DIR/us_counties.geojson" "$EXAMPLES_DIR"/cb_2018_us_county_20m.*
