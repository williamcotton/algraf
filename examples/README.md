# Algraf examples

This directory contains small, runnable `.ag` charts with matching CSV data.
Each chart uses a relative `Chart(data: "...")` path, so it can be rendered
from the repository root without extra flags.

Run one example:

```bash
cargo run -p algraf-cli -- render examples/scatter.ag --output /tmp/scatter.svg
cargo run -p algraf-cli -- render examples/scatter.ag --output /tmp/scatter.png
```

Regenerate all committed SVG and PNG outputs:

```bash
./examples/generate.sh
```

Check an example and inspect its schema:

```bash
cargo run -p algraf-cli -- check examples/scatter.ag
cargo run -p algraf-cli -- schema examples/scatter.ag --json
```

Render with debug layout metadata:

```bash
cargo run -p algraf-cli -- render examples/grouped_bar.ag --debug-layout --emit-metadata --output /tmp/grouped-bar.svg
```

## Files

| Chart | Data | Rendered outputs | Demonstrates |
| --- | --- | --- | --- |
| `scatter.ag` | `penguins.csv` | `scatter.svg`, `scatter.png` | Cartesian scatter plot, categorical fill legend |
| `line.ag` | `series.csv` | `line.svg`, `line.png` | Temporal x axis, multiple line series by stroke |
| `grouped_bar.ag` | `financials.csv` | `grouped_bar.svg`, `grouped_bar.png` | Nested x bands for grouped/dodged bars |
| `stacked_bar.ag` | `financials.csv` | `stacked_bar.svg`, `stacked_bar.png` | Stacked bar layout |
| `fill_bar.ag` | `financials.csv` | `fill_bar.svg`, `fill_bar.png` | Normalized fill/proportion bar layout |
| `heatmap.ag` | `heatmap.csv` | `heatmap.svg`, `heatmap.png` | Categorical tile grid with continuous fill |
| `histogram.ag` | `distribution.csv` | `histogram.svg`, `histogram.png` | Explicit `Derive` + `Rect` histogram primitive |
| `histogram_direct.ag` | `distribution.csv` | `histogram_direct.svg`, `histogram_direct.png` | High-level `Histogram` desugaring |
| `facet.ag` | `regional_sales.csv` | `facet.svg`, `facet.png` | Facet wrap via `(x * y) / group` |
| `connected_scatter.ag` | `timeseries.csv` | `connected_scatter.svg`, `connected_scatter.png` | Layered line and point marks |
| `barcode.ag` | `demographics.csv` | `barcode.svg`, `barcode.png` | Point strip/barcode-style categorical plot |
| `floating.ag` | `intervals.csv` | `floating.svg`, `floating.png` | Primitive `Rect` intervals over temporal data |
| `gantt.ag` | `gantt.csv` | `gantt.svg`, `gantt.png` | Gantt chart / timeline with `Rect` intervals and custom labels |
| `smooth.ag` | `penguins.csv` | `smooth.svg`, `smooth.png` | Linear `Smooth(method: "lm")` overlay |
| `boxplot.ag` | `demographics.csv` | `boxplot.svg`, `boxplot.png` | Boxplot summaries with rug ticks |
| `violin.ag` | `demographics.csv` | `violin.svg`, `violin.png` | Violin geometry with quantile lines |
| `density.ag` | `distribution.csv` | `density.svg`, `density.png` | Kernel density estimation (KDE) of a single numeric column |
| `multiple_density.ag` | `astronauts.csv` | `multiple_density.svg`, `multiple_density.png` | Overlaid kernel density estimates (KDE) of multiple numeric columns using the blend operator |
| `freqpoly.ag` | `distribution.csv` | `freqpoly.svg`, `freqpoly.png` | Frequency polygon over binned counts |
| `derived_chain.ag` | `distribution.csv` | `derived_chain.svg`, `derived_chain.png` | Chained `Derive` declarations |
| `gradient.ag` | `heatmap.csv` | `gradient.svg`, `gradient.png` | Source header and positioned continuous gradient stops |
| `group_line.ag` | `series.csv` | `group_line.svg`, `group_line.png` | `group` aesthetic with constant line stroke |
| `shapes.ag` | `series.csv` | `shapes.svg`, `shapes.png` | Categorical point shape mapping |
| `bin2d.ag` | `penguins.csv` | `bin2d.svg`, `bin2d.png` | Rectangular 2D binning |
| `hexbin.ag` | `penguins.csv` | `hexbin.svg`, `hexbin.png` | Hexagonal binning |
| `zfield_raster.ag` | `surface_grid.csv` | `zfield_raster.svg`, `zfield_raster.png` | Regular raster-style field rendered with primitive `Rect` cells |
| `contour_lines.ag` | `surface_grid.csv` | `contour_lines.svg`, `contour_lines.png` | `ContourLines` z-field stat feeding grouped `Path` segments |
| `contour_bands.ag` | `surface_grid.csv` | `contour_bands.svg`, `contour_bands.png` | `ContourBands` filled z-field bands rendered as geometry |
| `density2d_contours.ag` | `samples.csv` | `density2d_contours.svg`, `density2d_contours.png` | `Density2DContours` KDE stat overlaid on sample points |
| `summary2d_z.ag` | `sensor_z_samples.csv` | `summary2d_z.svg`, `summary2d_z.png` | Rectangular x/y/z summary bins with a mean reducer |
| `summaryhex_z.ag` | `sensor_z_samples.csv` | `summaryhex_z.svg`, `summaryhex_z.png` | Hexagonal x/y/z summary bins with a mean reducer |
| `ribbon.ag` | `ribbon.csv` | `ribbon.svg`, `ribbon.png` | Closed ribbon interval path |
| `reference.ag` | `penguins.csv` | `reference.svg`, `reference.png` | `HLine`, `VLine`, `Rug`, title, and legend suppression |
| `satisfaction_slope.ag` | `satisfaction.csv` | `satisfaction_slope.svg`, `satisfaction_slope.png` | Slopegraph with line grouping and text labels |
| `flight_dumbbell.ag` | `flights.csv` | `flight_dumbbell.svg`, `flight_dumbbell.png` | Dumbbell plot showing category-continuous range segments |
| `violin_boxplot.ag` | `demographics.csv` | `violin_boxplot.svg`, `violin_boxplot.png` | Layered violin and boxplot distributions |
| `faceted_sales_performance.ag` | `regional_sales.csv` | `faceted_sales_performance.svg`, `faceted_sales_performance.png` | Faceted line chart with point markers and a horizontal daily target reference line (`HLine`) |
| `binned_heatmap_overlay.ag` | `samples.csv` | `binned_heatmap_overlay.svg`, `binned_heatmap_overlay.png` | Continuous 2D binned density heatmap (`Bin2D`) overlaid with raw data points and threshold limit lines |
| `faceted_violin_boxplot.ag` | `regional_sales.csv` | `faceted_violin_boxplot.svg`, `faceted_violin_boxplot.png` | Faceted distribution chart overlaying `Violin`, narrow `Boxplot`, and marginal data `Rug` marks |
| `annotated_intervals.ag` | `intervals.csv` | `annotated_intervals.svg`, `annotated_intervals.png` | Shaded interval rectangles (`Rect`) and trend markers aligned to temporal x and value-constrained y axes |
| `binned_regression_chain.ag` | `samples.csv` | `binned_regression_chain.svg`, `binned_regression_chain.png` | Multi-stage statistical chaining: 2D binning (`Bin2D`) chained to a regression `Smooth` trend line |
| `weather_forecast.ag` | `weather_forecast.csv` | `weather_forecast.svg`, `weather_forecast.png` | Layered area range (`Ribbon`), trend (`Line`), and actual observed temperatures (`Point`) |
| `candlestick.ag` | `stock_prices.csv` | `candlestick.svg`, `candlestick.png` | Candlestick stock chart using two custom `Rect` layers and gain/loss conditional coloring |
| `lollipop.ag` | `programming_languages.csv` | `lollipop.svg`, `lollipop.png` | Layered bar and point chart using background `Bar` and overlaid `Point` markers |
| `bubble.ag` | `co2_gdp.csv` | `bubble.svg`, `bubble.png` | Bubble chart showing GDP, emissions, population size mapping, and labels |
| `diverging_bar.ag` | `monthly_profit.csv` | `diverging_bar.svg`, `diverging_bar.png` | Diverging bar chart showing monthly profits/losses relative to a zero baseline |


## Notes

The examples avoid unsupported renderer paths and should pass `algraf check`
without diagnostics. They are intentionally small enough to inspect by hand and
large enough to exercise scale training, legends, axes, and derived data.
