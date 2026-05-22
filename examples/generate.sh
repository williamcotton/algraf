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
    gantt
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
    violin
    freqpoly
    derived_chain
    gradient
    group_line
    shapes
    bin2d
    hexbin
    legend_merge
    scale_label
    satisfaction_slope
    labeled_points
    flight_dumbbell
    violin_boxplot
    faceted_sales_performance
    binned_heatmap_overlay
    faceted_violin_boxplot
    annotated_intervals
    binned_regression_chain
)

cd "$repo_root"
cargo build -p algraf-cli

for chart in "${charts[@]}"; do
    "$algraf" render "examples/$chart.ag" --output "examples/$chart.svg"
    "$algraf" render "examples/$chart.ag" --output "examples/$chart.png"
done
