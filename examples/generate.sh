#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
algraf="$repo_root/target/debug/algraf"

charts=(
    scatter
    line
    grouped_bar
    stacked_bar
    heatmap
    histogram
    histogram_direct
    facet
    connected_scatter
    barcode
    floating
    fill_bar
    smooth
    boxplot
    ribbon
    reference
    bar_count
    area
    labels
    segment
    guide_labels
    clean_canvas
    log_scale
    scale_domain
    reversed_axis
    temporal_histogram
    space_theme
    density
    legend_merge
    scale_label
)

cd "$repo_root"
cargo build -p algraf-cli

for chart in "${charts[@]}"; do
    "$algraf" render "examples/$chart.ag" --output "examples/$chart.svg"
    "$algraf" render "examples/$chart.ag" --output "examples/$chart.png"
done
