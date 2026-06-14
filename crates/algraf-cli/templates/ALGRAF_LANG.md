# Algraf Language Reference

Algraf is a declarative grammar-of-graphics DSL. Files use the `.ag`
extension. Algraf loads tabular data, validates chart declarations against the
data schema, trains scales, and emits deterministic SVG or related render
outputs.

## Read This First

- Algraf is not JavaScript, Python, Vega-Lite JSON, ggplot2 R code, SQL, or
  PDL.
- Do not invent loops, functions, imports, executable scripts, callbacks,
  mutable variables, or network data fetching. Algraf chart source is
  declarative.
- A useful file usually has one or more `Chart` blocks. Each chart has a data
  source, one or more `Space` blocks, and geometry calls inside spaces.
- Algebra inside `Space(...)` defines the coordinate frame. Geometry calls draw
  inside that inherited frame.
- Use unquoted identifiers for data columns and quoted strings for literal enum
  values, colors, labels, and file paths.
- Use `algraf check chart.ag` before rendering.
- Use `algraf format chart.ag` to print canonical formatting, or
  `algraf format chart.ag --write` to rewrite a file.
- Use `algraf schema chart.ag --json` when you need to inspect resolved column
  names and types.

## Minimal Chart

```ag
Chart(data: "penguins.csv", width: 760, height: 500) {
    Theme(name: "minimal")

    Space(flipper_length * body_mass) {
        Point(fill: species, alpha: 0.82, size: 4)
    }
}
```

The expression `flipper_length * body_mass` defines x and y position axes.
`Point(fill: species)` maps the `species` column to fill color.

## Comments

```ag
// Single-line comment
/* Block comment */
```

Block comments may span lines but do not nest. Keep comments plain.

## Program Structure

```ag
Chart(data: "data.csv") {
    // Chart-scoped declarations
    Theme(name: "minimal")
    Scale(axis: x, type: "linear")
    Guide(axis: x, label: "X value")

    Space(x * y) {
        // Space-scoped declarations and geometry calls
        Point(fill: group)
    }
}
```

Top-level items are an optional `Algraf(...)` source header, document-scope
`Table` declarations, and one or more `Chart` blocks. Chart bodies can contain
`Table`, `Parse`, `Derive`, `Glyph`, `let`, `Scale`, `Guide`, `Theme`,
`Layout`, and `Space` declarations where supported by the current
implementation.

## Data Sources

Common chart data forms:

```ag
Chart(data: "points.csv") { }
Chart(data: "points.tsv") { }
Chart(data: "points.json") { }
Chart(data: "points.ndjson") { }
Chart(data: Parquet("points.parquet")) { }
Chart(data: input) { }
Chart(data: stdin) { }
Chart {
    Table main = "points.csv"
}
```

`Chart(data: input)` and `Chart(data: stdin)` mean the caller supplies data,
typically with `algraf render chart.ag --data data.csv` or `--data -`. If a
chart omits `data:`, a visible `Table main = ...` is used as the primary data
source.

Native and geospatial constructors include:

```ag
Table points = Parquet("points.parquet")
Table shapes = GeoJson("shapes.geojson")
Table shapes = TopoJson("shapes.topojson")
Table shapes = Shapefile("shapes.shp")
```

Local SQLite sources are gated syntax. Do not use `Sqlite(...)` unless the file
has an appropriate `Algraf(version: "0.21", features: ["sql"])` header and the
runtime supports the native SQL feature.

Arrow IPC stream input is a caller-data format, not path-inferred chart syntax.
Use `Chart(data: input)` with `--data - --data-format arrow-stream`, or use a
`--data <path> --data-format arrow-stream` override. Do not expect
`Chart(data: "events.arrow")` to infer Arrow stream format by extension.

## Algebra

Algebra defines visual topology.

```ag
Space(x * y) { }          // Cartesian x by y
Space((quarter / type) * amount) { }  // type nested inside quarter
Space(a + b) { }          // blend compatible dimensions
Space((a + b) * y) { }    // parenthesize blends
```

Operators:

- `*` crosses dimensions into Cartesian axes.
- `/` nests one dimension inside another, useful for grouped bars.
- `+` blends compatible dimensions and should be parenthesized when combined
  with other operators.

Physical order matters. In `Space(a * b)`, `a` is the screen x axis and `b` is
the screen y axis.

## Values And Properties

```ag
Point(fill: species, alpha: 0.7, size: 3)
Point(fill: "#4E79A7", shape: "circle")
Guide(axis: x, label: "Body mass")
```

Unquoted identifiers usually mean column mappings when a mapping is allowed.
String literals mean literal settings or enum values. For example,
`Bar(layout: "stack")` is correct; `Bar(layout: stack)` is not.

Literal values:

```ag
"text"
123
123.45
true
false
null
[value, other_value]
date("2024-01-01")
datetime("2024-01-01T00:00:00Z")
```

Column names that are not plain identifiers should be backtick-quoted:

```ag
Space(`body mass` * `flipper-length`) {
    Point(fill: `species name`)
}
```

Map literals use bracketed `key => value` entries, not JSON object syntax:

```ag
Scale(stroke: direction, range: ["A" => "burlywood", "R" => "black"])
Scale(stroke: direction, labels: ["A" => "Advance", "R" => "Retreat"])
Layout(facetLabels: ["raw" => "Readable label"])
```

## Geometry Calls

Geometry calls live inside a `Space` block.

```ag
Space(x * y) {
    Point(fill: group)
    Line(stroke: group)
}
```

Common geometries include `Point`, `Line`, `Path`, `Bar`, `Rect`, `Text`,
`Label`, `Area`, `Ribbon`, `Boxplot`, `Violin`, `Smooth`, `Histogram`,
`FreqPoly`, `Density`, `Image`, `Geo`, `Tile`, `Segment`, `ErrorBar`,
`LineRange`, `PointRange`, `CrossBar`, `HLine`, `VLine`, `Rug`, and
`Graticule` depending on the current release.

Use documented geometry properties. Do not invent ggplot-style aesthetics or
Vega-Lite keys unless they exist in Algraf.

## Bars

Dodged bars are expressed in algebra:

```ag
Chart(data: "financials.csv") {
    Space((quarter / type) * amount) {
        Bar(fill: type)
    }
}
```

Stacked bars are expressed with a geometry layout setting:

```ag
Chart(data: "financials.csv") {
    Space(quarter * amount) {
        Bar(fill: type, layout: "stack")
    }
}
```

Fill bars use `layout: "fill"`.

## Derived Tables And Stats

Use `Derive` for data generated by stats.

```ag
Chart(data: "distribution.csv") {
    Derive bins = Bin(value, binWidth: 1, boundary: 0)

    Space(bin_start * count, data: bins) {
        Rect(
            xmin: bin_start,
            xmax: bin_end,
            ymin: 0,
            ymax: count,
            fill: "steelblue",
            stroke: "#ffffff",
        )
    }
}
```

Common stats include `Bin`, `Bin2D`, `HexBin`, `Smooth`, `Density2D`,
`Density2DContours`, `Density2DBands`, `Summary`, and related explicit
transform declarations. Generated columns such as `bin_start`, `bin_end`,
`bin_center`, and `count` come from the stat.

## Scales

```ag
Scale(axis: x, type: "linear")
Scale(axis: y, type: "log10")
Scale(axis: x, type: "temporal")
Scale(fill: species, palette: "accent")
Scale(axis: x, domain: [0, 100])
Scale(axis: x, breaks: [0, 25, 50, 75, 100])
Scale(axis: x, tickInterval: "1 month")
```

Use `axis: x` and `axis: y` as bare language selectors. Enum-valued options
such as scale type are string literals.

Temporal axes are continuous over elapsed time. Use full date or datetime values
for timelines; partial labels such as `"2024-03"` are categorical unless
explicitly parsed by the language.

## Guides

```ag
Guide(axis: x, label: "Date", timeFormat: "%b %Y")
Guide(axis: y, label: "Revenue")
Scale(fill: species, label: "Species")
Guide(axis: x, tickLabelAngle: -45)
```

Guides control axis and legend presentation. Tick positions are scale concerns;
text formatting and label layout are guide concerns.

## Themes And Layout

```ag
Theme(name: "minimal")
Theme(name: "classic")
Theme(name: "dark")

Layout(facetColumns: 2)
```

Theme names are string literals. Theme and layout declarations do not execute
code.

## Glyphs

Use `Glyph` for chart-scoped reusable mark templates where supported.

```ag
Chart(data: "nodes.csv") {
    Table mix = "node_mix.csv"

    Glyph pie(data: mix, key: [id], size: 32) {
        Space(value, coords: "polar", theta: "y") {
            Bar(fill: category, layout: "fill")
        }
    }

    Space(x * y) {
        pie(clip: "circle")
    }
}
```

Do not use removed or older `Inset` block syntax in new code.

## Interactivity

Chart source can declare inert interaction metadata, not executable handlers.

```ag
Space(x * y) {
    Point(tooltip: [group, y], highlight: "group")
    On(event: "click", emit: group)
}
```

Interactive SVG output is opt-in at render time with `--interactive`. Static
SVG is script-free by default.

## Complete Declaration Reference

### Source header

```ag
Algraf(version: "0.21", features: ["sql"])
```

Arguments:

- `version`: string. Required when the header is present.
- `features`: array of strings. Recognized feature gates are `sql`, `network`,
  `plugins`, and `experimental`; only `sql` enables a shipped source feature.

### Chart

```ag
Chart(data: "data.csv", width: 800, height: 520) { ... }
Chart { Table main = "data.csv" ... }
```

Arguments:

```text
data            source path, input sentinel, table reference, or constructor
width           number
height          number
title           string
subtitle        string
caption         string   (honors \n for stacked lines)
source          string   (de-emphasized attribution line below the caption; honors \n)
alt             string
description     string
marginTop       number
marginRight     number
marginBottom    number
marginLeft      number
```

`data` is the only data-source argument; `source:` is the editorial
attribution line, not a data source. If `data` is omitted, `Table main = ...`
is used as the chart's primary data source when present. Do not use `dataset`,
`url`, or Vega-Lite-style `data: { ... }`.

### Table

```ag
Table name = "path.csv"
Table name = Parquet("path.parquet")
```

The left side is a table identifier. The right side is a source expression.
Document-scope tables are visible to every chart in the file; chart-scope
tables are visible only within that chart.

### Source constructors

```ag
GeoJson("features.geojson")
Shapefile("shapes.shp")
Parquet("points.parquet")
TopoJson("map.topojson")
TopoJson("map.topojson", object: "counties")
Sqlite("data.db", "SELECT x, y FROM points ORDER BY x")
```

`Sqlite(...)` requires `Algraf(version: "0.21", features: ["sql"])` and a
native runtime with SQL support. `GeoJson`, `Shapefile`, `Parquet`, and
`TopoJson` are explicit loader constructors. Bare string paths infer by
extension.

### Parse

```ag
Parse(column: started_at, as: "datetime", format: "%m/%d/%Y %I:%M %p", timezone: "UTC")
Parse(column: settled_on, as: "date", format: "%d/%m/%Y")
Parse(column: epoch_ms, as: "datetime", unit: "milliseconds")
Parse(table: trades, column: executed_at, as: "datetime", formats: ["%FT%T%:z", "%F %T"])
```

Arguments:

```text
table       table name
column      column name
as          "date" | "datetime"
format      string
formats     array of strings
unit        "seconds" | "milliseconds" | "microseconds" | "nanoseconds"
timezone    timezone string, such as "UTC"
onError     "warn" | "missing" | "error"
anchor      date string used for time-only formats
```

### Space

```ag
Space(x * y, data: binned, coords: "cartesian") { ... }
```

Arguments:

```text
data          table name
coords        "cartesian" | "polar"
theta         "x" | "y"
innerRadius   number in [0, 1)
startAngle    number in [-360, 360]
direction     "clockwise" | "counterclockwise"
projection    string
zoomX         [number | null, number | null]
zoomY         [number | null, number | null]
aspect        positive number
```

The first positional item in `Space(...)` is always algebra, not a named
argument.

### Derive

```ag
Derive bins = Bin(value, bins: 30)
Derive grouped = Summary(value, by: [species], reducer: "mean")
Derive trend from bins = Smooth(bin_center, count, method: "lm")
```

`Derive` creates a chart-scoped named table from a stat call. `Derive name from
table = Stat(...)` reads a chart-scoped `Table` or earlier `Derive`; without
`from`, the stat reads the chart's primary table. Use
`Space(..., data: derived_name)` to draw the result.

### let and Style

```ag
let muted = Style(fill: "#6b7280", alpha: 0.55)

Space(x * y) {
    Point(style: muted)
}
```

`Style(...)` is a literal property bag, not a function. Accepted style keys:

```text
fill stroke strokeWidth alpha size shape group label dx dy nudge nudgeData
```

Do not put `style:` inside `Style(...)`.

### Theme

```ag
Theme(name: "minimal")
Theme(name: "minimal", grid: false, fontSize: 12)
```

Theme names:

```text
minimal classic light dark void gray bw linedraw
```

Theme override keys:

```text
axisText axisTitle plotTitle plotSubtitle plotCaption plotSource stripText
legendTitle legendText panelBackground gridMajor gridMinor legendPosition
legendSpacing fontFamily fontSize titleSize pointSize lineWidth background
plotBackground axisColor gridColor textColor grid gridX gridY axes
axisYPosition axisXPosition
```

`legendPosition` values are `"right"`, `"bottom"`, `"top"`, and `"left"`.
`plotSource` is a `Text(...)` style for the `source:` line. `gridX`/`gridY` are
booleans for per-axis grid-line defaults. `axisYPosition` is `"left"`/`"right"`
and `axisXPosition` is `"top"`/`"bottom"`, setting the house default axis side
that `Guide(axis:, position:)` overrides.

### Scale

```ag
Scale(axis: x, type: "linear")
Scale(axis: x, type: "temporal", tickInterval: "1 month")
Scale(fill: species, palette: "default")
Scale(fill: score, gradient: ["#3366cc", "#cc3333"])
Scale(fill: score, gradient: [Stop(value: 0, color: "#3366cc"), Stop(value: 100, color: "#cc3333")])
```

Arguments:

```text
axis           x | y
type           "linear" | "log10" | "sqrt" | "categorical" | "temporal"
domain         [number | null, number | null] or [string, ...]
mode           "binned" | "identity"
breaks         array
tickInterval   "<count> <unit>"
expand         number or [number, number]
expansion      alias accepted by implementation
reverse        boolean
integer        boolean
fill           column
stroke         column
size           column
strokeWidth    column
palette        "default" | "accent"
gradient       array of colors or array of Stop(...)
range          array or map
labels         array or map
label          string
train          "shared" | "local"
```

`Stop(...)` arguments:

```text
value    finite number
color    color string
```

Temporal `tickInterval` units are millisecond, second, minute, hour, day, week,
month, quarter, and year, with plural forms accepted in interval strings.

### Guide

```ag
Guide(axis: x, label: "Date", timeFormat: "%b %Y")
Guide(fill: null)
Guide(axis: x, tickLabelAngle: -45, tickLabelRows: 2)
Guide(axis: y, position: "right", format: ".0f")
Guide(axis: x, grid: false)
```

Arguments:

```text
axis             x | y
label            string | null
timeFormat       string
position         y: "left" | "right"; x: "top" | "bottom"
format           numeric tick format: .0f .1f .2f $.2f .0% .1% .2%
tickLabelAngle   number between -90 and 90
tickLabelRows    integer row count
legend           boolean
fill             null
stroke           null
grid             boolean (with axis:, sets only that axis's grid lines)
gridShape        "circle" | "polygon"
```

Use `Guide(fill: null)` or `Guide(stroke: null)` to suppress legends.
Use `Guide(axis: y, position: "right")` to move the value axis to the right edge,
and `Guide(axis: x, position: "top")` to move the x axis to the top.
Use `Guide(axis: y, format: ".0f")` for integer value-axis labels (numeric,
non-temporal axes only). Use `Guide(axis: x, grid: false)` to hide vertical grid
lines (or `axis: y` to hide horizontal lines); a bare `Guide(grid: false)` hides
all grid lines.
Use `Guide(gridShape: "polygon")` inside polar spaces for radar-style polygon
grid rings; `"circle"` is the default.

### Layout

```ag
Layout(facetColumns: 2)
Layout(facetRows: region, facetCols: channel, facetScales: "free-y")
```

Arguments:

```text
facetColumns   number
facetRows      column
facetCols      column
facetScales    "fixed" | "free-x" | "free-y" | "free"
facetLabel     "value" | "name-value" | null
facetLabels    map
panelSpacing   number or [number, number]
```

### Glyph declaration and glyph calls

```ag
Glyph pie(data: mix, key: [id], scales: "shared") {
    Space(value, coords: "polar", theta: "y") {
        Bar(fill: category, layout: "fill")
    }
}

Space(x * y) {
    pie(size: weight, clip: "circle", padding: 2)
}
```

Glyph declaration arguments:

```text
data     table name
key      column, [column, ...], or key mapping list
scales   "shared" | "local"
```

Glyph call arguments:

```text
size      column
width     number
height    number
clip      "rect" | "circle" | false
padding   number
at        "position" | "mark-center" | "centroid"
dx        number
dy        number
legend    boolean
```

## Complete Stat Reference

Stat calls appear on the right side of `Derive`.

```text
Bin(input, bins?, binWidth?, boundary?, closed?, interval?)
Smooth(x, y, method?, span?, se?)
Bin2D(x, y, bins?)
HexBin(x, y, bins?)
ContourLines(x, y, z:, levels?)
ContourBands(x, y, z:, levels?)
Density2D(x, y, bandwidth?, grid?)
Density2DContours(x, y, bandwidth?, grid?, levels?)
Density2DBands(x, y, bandwidth?, grid?, levels?)
Distinct(columns...)
Ecdf(value)
Qq(value, distribution?, reference?)
Summary(value, by?, reducer?)
SummaryBin(x, value, by?, bins?, binWidth?, boundary?, closed?, reducer?)
Cut(value, breaks?, labels?, output?)
Summary2D(x, y, z:, bins?, reducer?)
SummaryHex(x, y, z:, bins?, reducer?)
StepVertices(x, y, direction?)
JitterPoints(x, y, width?, height?)
VectorEndpoints(x, y, angle, length, lengthScale?)
CurveSample(x, y, xend, yend, curvature?, points?)
IntervalSegments(x, low, high, orientation?, capWidth?)
IntervalRects(x, low, high, orientation?, width?)
IntervalMiddles(x, mid, orientation?, width?)
Centroid(geom)
Simplify(geom, tolerance?)
SpatialJoin(point_geom, table:, predicate?)
```

Important enum values:

```text
closed:        "left" | "right"
method:        "lm" | "loess"
distribution:  "normal"
reducer:       "mean" | "count" | "min" | "max" | "sum" | "median" | "mean_se"
direction:     "hv" | "vh"
orientation:   "vertical" | "horizontal"
predicate:     "within"
```

Use the implemented stat names exactly, including capitalization.

## Complete Geometry Property Reference

Required properties are marked with `*`.

```text
Point:
  fill, stroke, alpha, size, shape, jitter, nudge, nudgeData

Line:
  stroke, strokeWidth, dash, alpha, group, taper

Path:
  stroke, strokeWidth, dash, alpha, group, taper

Bar:
  fill, stroke, strokeWidth, alpha, layout, stat, radius

Rect:
  xmin*, xmax*, ymin*, ymax*, fill, stroke, strokeWidth, alpha

Histogram:
  bins, binWidth, boundary, closed, interval, orientation, fill, stroke,
  strokeWidth, alpha, group

FreqPoly:
  bins, binWidth, boundary, closed, interval, orientation, stroke, strokeWidth,
  alpha, group

Bin2D:
  bins, fill, stroke, strokeWidth, alpha

HexBin:
  bins, fill, stroke, strokeWidth, alpha

Smooth:
  method, span, se, fill, stroke, strokeWidth, dash, alpha, group

Boxplot:
  fill, stroke, strokeWidth, alpha, width, outliers

Violin:
  bandwidth, n, quantiles, fill, stroke, strokeWidth, alpha, width

Density:
  bandwidth, n, fill, stroke, strokeWidth, alpha

ErrorBar:
  xmin, xmax, ymin, ymax, orientation, capWidth, stroke, strokeWidth, dash,
  alpha

LineRange:
  xmin, xmax, ymin, ymax, orientation, stroke, strokeWidth, dash, alpha

PointRange:
  xmin, xmax, ymin, ymax, orientation, fill, stroke, strokeWidth, dash, alpha,
  size, shape

CrossBar:
  xmin, xmax, ymin, ymax, orientation, width, fill, stroke, strokeWidth, dash,
  alpha

Ribbon:
  ymin*, ymax*, fill, stroke, strokeWidth, alpha

Tile:
  fill, stroke, strokeWidth, alpha

HLine:
  y*, stroke, strokeWidth, dash, alpha, label, labelPosition, labelShape,
  labelFill, labelStroke

VLine:
  x*, stroke, strokeWidth, dash, alpha, label, labelPosition, labelShape,
  labelFill, labelStroke

Rug:
  sides, stroke, strokeWidth, alpha

Area:
  baseline, fill, stroke, strokeWidth, alpha, layout, group

Text:
  label*, x, y, fill, alpha, size, anchor, dx, dy, nudge, nudgeData, declutter,
  format, timeFormat

Label:
  label*, at, group, fill, alpha, size, anchor, dx, dy, format

Image:
  src*, alpha, size, jitter, nudge, nudgeData

Segment:
  x*, y*, xend*, yend*, stroke, strokeWidth, dash, alpha

Geo:
  fill, stroke, strokeWidth, alpha

Graticule:
  stroke, strokeWidth, alpha, step
```

Interaction properties accepted by `Point`, `Image`, `Bar`, `Rect`, and `Tile`:

```text
tooltip      column or [column, ...]
highlight    column
```

`On(...)` event emitter arguments:

```text
event    "click"
emit     column
```

## Property Value Forms And Enums

Common property value forms:

```text
fill, stroke       color string or column mapping
alpha              number or column mapping
size               number or column mapping
shape              string or column mapping
strokeWidth        number; Line/Path also accept a column mapping
group              column mapping
label              string or column mapping where accepted
labelFill, labelStroke   color string (HLine/VLine callout badge)
src                local image path string or column mapping
x, y, xmin, xmax   number or column mapping where accepted
ymin, ymax
xend, yend
dx, dy             number or column mapping
jitter             [x, y]
nudge              [dx, dy] in pixels
nudgeData          [dx, dy] in data units
```

Known enum values:

```text
layout:       "identity" | "stack" | "fill"
stat:         "identity" | "count"
dash:         "solid" | "dotted" | "dashed"
closed:       "left" | "right"
method:       "lm" | "loess"
orientation:  "vertical" | "horizontal"
anchor:       "start" | "middle" | "end"
at:           "start" | "end"                 # Label geometry
labelPosition: VLine "top" | "bottom"; HLine "start" | "end"
labelShape:   "none" | "circle" | "square"   # HLine/VLine callout badge
sides:        string such as "b" for Rug
clip:         "rect" | "circle" | false       # glyph calls
coords:       "cartesian" | "polar"
theta:        "x" | "y"
direction:    "clockwise" | "counterclockwise"
```

Colors must be valid color strings. Local `Image(src: ...)` paths are embedded
as data URLs by Algraf; arbitrary source-authored external URLs are not a
general network-fetch mechanism.

## CLI Commands

```bash
algraf check chart.ag
algraf render chart.ag --output chart.svg
algraf render chart.ag --format svg+json --output chart
algraf render chart.ag --output chart.png
algraf format chart.ag
algraf format chart.ag --write
algraf schema chart.ag --json
algraf ast chart.ag
algraf ir chart.ag
algraf init --codex
algraf init --claude
algraf init --agy
algraf lsp
```

Caller-provided data:

```bash
algraf render chart.ag --data data.csv --output chart.svg
cat data.csv | algraf render chart.ag --data - --output chart.svg
pdl run prep.pdl --stdout-format arrow-stream | algraf render chart.ag --data - --data-format arrow-stream --output chart.svg
```

## Common Agent Pitfalls

- Do not write JSON chart specs. Algraf uses block syntax, not Vega-Lite JSON.
- Do not write ggplot2 code such as `ggplot(...) + geom_point(...)`.
- Do not use JavaScript callbacks, event handlers, or inline scripts in source.
- Do not use Python syntax, imports, loops, or functions.
- Do not use SQL unless the documented gated `Sqlite(...)` path is explicitly
  required and enabled.
- Do not treat `x` and `y` as strings in `Guide(axis: x)` or `Scale(axis: x)`;
  they are bare language selectors.
- Do quote enum values such as `"stack"`, `"fill"`, `"circle"`, `"linear"`, and
  `"temporal"`.
- Do not assume an identifier value is a string. If it is meant to be literal
  text, quote it.
- `algraf check` validates against the chart's data source. If the data source
  is generated by PDL, run the PDL pipeline first so the CSV exists and has the
  expected columns.
- `Parse(...)` for dates and datetimes requires an explicit `format:`,
  `formats:`, or `unit:`. For ISO date CSV columns such as `2024-04-01`, use
  `Parse(column: author_month, as: "date", format: "%Y-%m-%d")`.
- Do not collapse missing dates on a temporal axis by converting dates to month
  labels unless a categorical axis is intended.
- Do not put geometries directly at top level; put them inside `Space`.
- Do not put `Derive` inside `Space` unless the current language explicitly
  supports that shape. Prefer chart-scoped `Derive`.
- Stacked `Area(...)` charts can use `layout: "stack"`. If a chart needs
  separate colors for both a split period and a measure type, create one
  categorical column in the data such as `change_segment` instead of trying to
  map two fields to one fill channel.
- For stacked additions/deletions by a date split, a useful long table has rows
  like `Before cutoff additions`, `Before cutoff deletions`,
  `After cutoff additions`, and `After cutoff deletions`; map `fill` to that
  combined segment and use an explicit `Scale(fill: ..., range: [...])`.
- If legend text is clipped, shorten the category labels first, then increase
  chart `width` or `marginRight` as needed.
- If a diagnostic says a column is unknown, run `algraf schema chart.ag --json`
  and verify the data source path and column spelling.

## Project Agent Setup

If this file was generated by `algraf init`, keep it at the project root and
have agent instruction files reference it. Do not paste this whole reference
into every agent file.

This reference documents the language surface implemented by the `algraf`
binary it shipped with — every geometry, property, scale/guide key, theme
token, chart argument, enum value, and CLI command listed here is accepted by
that binary. It does not describe planned or deferred features. In the Algraf
source repository this file is the canonical template at
`crates/algraf-cli/templates/ALGRAF_LANG.md` and is updated in lockstep with the
language whenever new surface lands (see `AGENTS.md`/`CLAUDE.md`); if a feature
is missing here, run `algraf check` to confirm whether your binary supports it
before relying on it.
