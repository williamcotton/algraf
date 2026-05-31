#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
algraf="$repo_root/target/debug/algraf"

charts=(
    scatter
    line
    sparkline
    inset_sparklines
    grouped_bar
    stacked_bar
    horizontal_bar
    horizontal_grouped_bar
    horizontal_stacked_bar
    horizontal_fill_bar
    horizontal_diverging_bar
    upside_down_bar
    horizontal_reversed_bar
    horizontal_lollipop
    heatmap
    histogram
    histogram_direct
    grouped_histogram
    dodged_histogram
    astronauts
    facet
    connected_scatter
    barcode
    floating
    gantt
    fill_bar
    smooth
    loess_smooth
    grouped_loess
    boxplot
    boxplot_outliers
    ribbon
    tapered_flow
    reference
    bar_count
    area
    labels
    text
    segment
    guide_labels
    clean_canvas
    log_scale
    sqrt_scale
    scale_domain
    coordinate_zoom
    fixed_aspect
    facet_grid
    free_scales
    jitter
    nudge
    reversed_axis
    temporal_formats_auto
    temporal_parse_custom
    temporal_histogram
    space_theme
    presentation_theme
    density
    multiple_density
    violin
    horizontal_boxplot
    horizontal_violin
    horizontal_violin_boxplot
    horizontal_faceted_violin_boxplot
    freqpoly
    ecdf
    qq
    summary_intervals
    summary_bin
    derived_chain
    gradient
    breaks_labels_expansion
    binned_scale
    identity_color
    group_line
    shapes
    penguin_channels
    bin2d
    hexbin
    zfield_raster
    contour_lines
    contour_bands
    density2d_contours
    summary2d_z
    summaryhex_z
    legend_merge
    scale_label
    satisfaction_slope
    dumbbell
    uncertainty_intervals
    horizontal_intervals
    labeled_points
    flight_dumbbell
    violin_boxplot
    faceted_sales_performance
    binned_heatmap_overlay
    faceted_violin_boxplot
    annotated_intervals
    binned_regression_chain
    top_down_icicle
    variables
    custom_theme
    path
    step_vertices
    primitive_links
    manual_colors
    minard
    sales_tsv
    sqlite_sales
    temperatures_json
    events_ndjson
    choropleth
    choropleth_shapefile
    choropleth_graticule
    county_centroids
    topojson_grid
    spatial_join
    spatial_overlay
    weather_forecast
    candlestick
    lollipop
    bubble
    diverging_bar
    pie
    donut
    coxcomb
    wind_rose
    circular_histogram
    polar_scatter
    annular_heatmap
    radar
    radial_bar
    polar_start_angle
    nested_insets
    temporal_literal
    off_axis_time
    time_only_anchor
    us_city_bubbles
    inset_city_pies
    flight_routes_map
    tooltips
    highlight
)

cd "$repo_root"
cargo build -p algraf-cli

for chart in "${charts[@]}"; do
    "$algraf" render "examples/$chart.ag" --output "examples/$chart.svg"
    "$algraf" render "examples/$chart.ag" --output "examples/$chart.png"
done

"$algraf" render "examples/highlight.ag" \
    --output "examples/highlight.svg" \
    --metadata "examples/highlight.meta.json"

# Multi-chart documents render one file per chart, with a 1-based suffix
# inserted before the extension (multi_chart-1.svg, multi_chart-2.svg, ...).
for chart in multi_chart map_simplification us_urban_population projection_comparison; do
    "$algraf" render "examples/$chart.ag" --output "examples/$chart.svg"
    "$algraf" render "examples/$chart.ag" --output "examples/$chart.png"
done
