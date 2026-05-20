# Algraf

Algraf is a block-scoped, algebraic grammar-of-graphics DSL. You describe a
chart declaratively in a `.ag` file, point it at a CSV, and the `algraf`
binary parses the source, validates it against the data, trains scales, and
emits deterministic SVG.

The normative reference is [`docs/ALGRAF_SPEC.md`](docs/ALGRAF_SPEC.md).

```bash
cargo run -p algraf-cli -- render examples/scatter.ag --output /tmp/scatter.svg
```

This tutorial walks through every example in [`examples/`](examples/) from
the simplest scatter plot up through statistical layers, derived tables,
faceting, and theme overrides. Each section shows the `.ag` source followed
by its rendered SVG.

---

## A first chart: scatter with a categorical fill

A `Chart` block holds a `Space`, and a `Space` holds geometries. The
algebra operator `*` is *cross*: it pairs two columns into Cartesian x/y.
Mapping `fill: species` colors each point by the categorical column and
produces a legend automatically.

```algraf
Chart(data: "penguins.csv", width: 760, height: 500) {
    Theme(name: "minimal")

    Space(flipper_length * body_mass) {
        Point(fill: species, alpha: 0.82, size: 4)
    }
}
```

![scatter](examples/scatter.svg)

## Line series over time

Temporal columns are detected from the CSV schema and get a time-aware
axis. Mapping `stroke: series` splits one line per group.

```algraf
Chart(data: "series.csv", width: 760, height: 460) {
    Space(day * value) {
        Line(stroke: series, strokeWidth: 2)
    }
}
```

![line](examples/line.svg)

## Layered marks: connected scatter

Geometries inside a `Space` render in source order, so listing `Line`
before `Point` draws the points on top of the connecting lines.

```algraf
Chart(data: "timeseries.csv") {
    Space(time * value) {
        Line(stroke: series, strokeWidth: 2)
        Point(fill: series, size: 4)
    }
}
```

![connected_scatter](examples/connected_scatter.svg)

## Statistical overlay: linear smooth

`Smooth(method: "lm")` fits a linear model and draws the resulting line
on top of the data, sharing the same x/y space as the points.

```algraf
Chart(data: "penguins.csv", width: 760, height: 500) {
    Space(flipper_length * body_mass) {
        Point(fill: species, alpha: 0.55, size: 3)
        Smooth(method: "lm", stroke: "#333333", strokeWidth: 2)
    }
}
```

![smooth](examples/smooth.svg)

## Grouped bars via the `/` (nest) operator

`/` is *nest*: `quarter / type` makes `type` a sub-band inside each
`quarter` band. The result is a dodged/grouped bar chart with no extra
configuration.

```algraf
Chart(data: "financials.csv", width: 760, height: 460) {
    Theme(name: "classic")

    Space((quarter / type) * amount) {
        Bar(fill: type)
    }
}
```

![grouped_bar](examples/grouped_bar.svg)

## Stacked bars

Switching the bar `layout` from the default `"identity"` to `"stack"`
keeps the single `quarter` band and stacks the type contributions.

```algraf
Chart(data: "financials.csv", width: 760, height: 460) {
    Space(quarter * amount) {
        Bar(fill: type, layout: "stack")
    }
}
```

![stacked_bar](examples/stacked_bar.svg)

## Proportional fill bars

`layout: "fill"` normalizes each stack to 1.0 so the bars compare shares
instead of totals.

```algraf
Chart(data: "financials.csv", width: 760, height: 460) {
    Space(quarter * amount) {
        Bar(fill: type, layout: "fill")
    }
}
```

![fill_bar](examples/fill_bar.svg)

## Counting categories: `Bar(stat: "count")`

When you only have a categorical column, `stat: "count"` aggregates rows
per category and uses the count as the y value — no explicit `Derive`
needed.

```algraf
Chart(
    data: "demographics.csv",
    width: 640,
    height: 420,
    title: "Sample distribution by gender",
) {
    Space(gender) {
        Bar(stat: "count", fill: gender, alpha: 0.85)
    }
}
```

![bar_count](examples/bar_count.svg)

## Histograms the explicit way: `Derive` + `Rect`

`Derive` produces a named derived table from a stat. Here `Bin` returns
`bin_start`, `bin_end`, `count`, and `density`, and `Rect` draws the
bars by reading from `data: bins`.

```algraf
Chart(data: "distribution.csv", width: 760, height: 460) {
    Derive bins = Bin(value, binWidth: 1, boundary: 0)

    Space(bin_start * count, data: bins) {
        Rect(
            xmin: bin_start,
            xmax: bin_end,
            ymin: 0,
            ymax: count,
            fill: "steelblue",
            stroke: "#ffffff",
            strokeWidth: 1,
            alpha: 0.86,
        )
    }
}
```

![histogram](examples/histogram.svg)

## Histograms the short way: the `Histogram` geometry

`Histogram` desugars to the same `Derive` + `Rect` pair above, so you
get binning, count, density and the rectangles in a single line.

```algraf
Chart(data: "distribution.csv", width: 760, height: 460) {
    Space(value) {
        Histogram(
            binWidth: 1,
            boundary: 0,
            fill: "steelblue",
            stroke: "#ffffff",
            strokeWidth: 1,
            alpha: 0.86,
        )
    }
}
```

![histogram_direct](examples/histogram_direct.svg)

## Heatmap with `Tile`

Two categorical axes plus a continuous fill give you a heatmap.

```algraf
Chart(data: "heatmap.csv", width: 700, height: 460) {
    Space(day * hour) {
        Tile(fill: value, alpha: 0.92)
    }
}
```

![heatmap](examples/heatmap.svg)

## Boxplot with a rug

`Boxplot` summarizes a continuous distribution per categorical level,
and `Rug` adds tick marks along an axis to show raw values. `sides: "l"`
puts the rug on the left.

```algraf
Chart(data: "demographics.csv", width: 700, height: 460) {
    Space(gender * height) {
        Boxplot(fill: gender, alpha: 0.78)
        Rug(sides: "l", alpha: 0.35)
    }
}
```

![boxplot](examples/boxplot.svg)

## Ribbon: confidence band

`Ribbon` closes a band between `ymin` and `ymax` per x value. The `+`
operator in the algebra is *blend*: it tells the y scale to consider
both columns when training its domain.

```algraf
Chart(data: "ribbon.csv", width: 760, height: 460) {
    Space(day * (lower + upper)) {
        Ribbon(ymin: lower, ymax: upper, fill: "steelblue", alpha: 0.25)
    }
}
```

![ribbon](examples/ribbon.svg)

## Filled area under a line

`Area` closes a polygon down to its `baseline` — when the baseline is 0
the bottom edge sits flush against the x axis. Layering a `Line` on top
preserves the original series outline.

```algraf
Chart(data: "timeseries.csv", width: 760, height: 420) {
    Space(time * value) {
        Area(baseline: 0, fill: series, alpha: 0.35)
        Line(stroke: series, strokeWidth: 2)
    }
}
```

![area](examples/area.svg)

## Categorical strip / barcode

A categorical x paired with a continuous y and a low-alpha `Point` makes
a strip plot — useful for inspecting distributions without binning.

```algraf
Chart(data: "demographics.csv") {
    Space(gender * height) {
        Point(fill: gender, alpha: 0.4, size: 3)
    }
}
```

![barcode](examples/barcode.svg)

## Floating intervals with `Rect`

`Rect` is the general rectangle primitive: any combination of
`xmin/xmax/ymin/ymax` from columns or literals.

```algraf
Chart(data: "intervals.csv") {
    Space(time * value) {
        Rect(
            xmin: start_time,
            xmax: end_time,
            ymin: 0,
            ymax: peak_value,
            fill: "steelblue",
            alpha: 0.5
        )
    }
}
```

![floating](examples/floating.svg)

## Faceting via nested algebra

Nesting the space with `/ region` produces one panel per region, all
sharing the same scales and axes.

```algraf
Chart(data: "regional_sales.csv") {
    Space((time * sales) / region) {
        Line(stroke: product)
    }
}
```

![facet](examples/facet.svg)

## Reference marks: title, `HLine`, `VLine`, `Rug`

`HLine` and `VLine` accept literal data values and optional labels.
`Guide(legend: false)` suppresses the auto-generated legend when you
don't need it. `Chart(title: ...)` puts a title at the top.

```algraf
Chart(data: "penguins.csv", width: 760, height: 500, title: "Penguin measurements") {
    Guide(legend: false)

    Space(flipper_length * body_mass) {
        Point(fill: species, alpha: 0.62, size: 3)
        HLine(y: 4200, stroke: "#b22222", label: "4.2 kg")
        VLine(x: 45, stroke: "#555555", label: "45 mm")
        Rug(sides: "bl", alpha: 0.3)
    }
}
```

![reference](examples/reference.svg)

## Line segments between literal points

`Segment` draws a straight line between (`x`, `y`) and (`xend`, `yend`).
The endpoints participate in scale training, so the segment always
stays inside the plot rect.

```algraf
Chart(data: "penguins.csv", width: 720, height: 480) {
    Space(flipper_length * body_mass) {
        Point(fill: species, alpha: 0.7, size: 3)
        Segment(
            x: 175,
            y: 3000,
            xend: 230,
            yend: 6000,
            stroke: "#d62728",
            strokeWidth: 2,
        )
    }
}
```

![segment](examples/segment.svg)

## Text labels per row

`Text` places one label per data row at its (`x`, `y`) position, using
the column you map to `label`.

```algraf
Chart(data: "penguins.csv", width: 720, height: 480) {
    Space(flipper_length * body_mass) {
        Point(fill: species, alpha: 0.4, size: 3)
        Text(label: species, anchor: "middle", dy: -8, size: 10)
    }
}
```

![labels](examples/labels.svg)

## Overriding axis labels with `Guide`

`Guide(axis: x, label: ...)` and `Guide(axis: y, label: ...)` replace
the default column-name axis titles with custom text.

```algraf
Chart(data: "penguins.csv", width: 720, height: 480) {
    Guide(axis: x, label: "Flipper length (mm)")
    Guide(axis: y, label: "Body mass (g)")

    Space(flipper_length * body_mass) {
        Point(fill: species, alpha: 0.7, size: 3)
    }
}
```

![guide_labels](examples/guide_labels.svg)

## Space-local themes

`Theme` can appear at the chart level or inside a `Space`. A
space-scoped `Theme` overrides the chart theme just for that panel —
useful when one space wants a stripped-down look.

```algraf
Chart(data: "timeseries.csv", width: 760, height: 420) {
    Theme(name: "minimal")

    Space(time * value) {
        Theme(name: "void")
        Line(stroke: series, strokeWidth: 2)
    }
}
```

![space_theme](examples/space_theme.svg)

---

## Running the examples yourself

From the repository root:

```bash
# Render a single example
cargo run -p algraf-cli -- render examples/scatter.ag --output /tmp/scatter.svg

# Regenerate every committed SVG and PNG
./examples/generate.sh

# Validate without rendering
cargo run -p algraf-cli -- check examples/scatter.ag

# Inspect inferred schema and IR
cargo run -p algraf-cli -- schema examples/scatter.ag --json
```

## Workspace layout

Cargo workspace with seven crates under [`crates/`](crates/):

| Crate | Responsibility |
| --- | --- |
| `algraf-core` | Shared primitives: `Span`, `Diagnostic`, `Severity` |
| `algraf-syntax` | Lexer, parser, AST/CST (rowan), parse diagnostics, formatter |
| `algraf-data` | CSV loading, schema inference, dataframe |
| `algraf-semantics` | Name resolution, validation, IR, geometry registry |
| `algraf-render` | Scale training, layout, stats, geometries, SVG emission |
| `algraf-lsp` | tower-lsp backend, document cache, completion, hover |
| `algraf-cli` | The `algraf` binary |
