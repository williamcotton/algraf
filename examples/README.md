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
| `heatmap.ag` | `heatmap.csv` | `heatmap.svg`, `heatmap.png` | Categorical tile grid with continuous fill |
| `histogram.ag` | `distribution.csv` | `histogram.svg`, `histogram.png` | Explicit `Derive` + `Rect` histogram primitive |

## Notes

The examples avoid unsupported renderer paths and should pass `algraf check`
without diagnostics. They are intentionally small enough to inspect by hand and
large enough to exercise scale training, legends, axes, and derived data.
