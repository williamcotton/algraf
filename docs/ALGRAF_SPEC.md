# Algraf Detailed Specification

Status: 0.82.0
Audience: implementers, language designers, runtime engineers, LSP authors, and test authors
Scope: block-scoped algebraic grammar-of-graphics DSL, single Rust binary, resilient parser, language server, CSV-backed runtime, and SVG renderer

## 0. Document Contract

This document specifies Algraf, a block-scoped algebraic graphics language inspired by Wilkinson's Grammar of Graphics and modern declarative UI DSLs.

Algraf is designed around one core idea:

```ag
Chart(data: "financials.csv") {
    Space((quarter / type) * amount) {
        Bar(fill: type)
    }
}
```

The algebra defines the visual space.

The block defines scope.

The geometry draws inside the inherited space.

The Rust binary parses, validates, serves editor intelligence, evaluates data, trains scales, and emits SVG.

The specification is intentionally detailed.

It is written to support implementation without relying on the original chat conversation.

Released version 0.1 behavior is preserved by repository tags.

This working copy is the 0.82.0 specification.

The staged release plans and optional-item audits live under `docs/` as
`V0_*_PLAN.md` files. The earliest unreleased plan is the active implementation
target, and later unreleased plans are sequencing guidance.

Items in the plan are planning guidance until they are promoted into normative sections of this specification.

The keyword `MUST` means required behavior.

The keyword `SHOULD` means recommended behavior.

The keyword `MAY` means optional behavior.

The keyword `MUST NOT` means prohibited behavior.

The keyword `implementation-defined` means behavior may vary, but the implementation must document the chosen behavior.

The keyword `diagnostic` means a machine-readable error or warning with source span information.

The keyword `resilient` means parsing or analysis continues after an error where practical.

The keyword `source span` means a byte range into the source document.

The keyword `frame` means an algebraic coordinate space before scale training.

The keyword `space` means either an algebraic frame or a trained spatial context depending on phase.

The keyword `geometry` means a drawable layer such as `Point`, `Line`, `Bar`, or `Smooth`.

The keyword `aesthetic` means a visual property such as `fill`, `stroke`, `alpha`, `shape`, or `size`.

The keyword `mapping` means binding an aesthetic to a data column.

The keyword `setting` means binding an aesthetic or option to a literal value.

The keyword `scale` means a mapping from data domain to visual range.

The keyword `guide` means visible explanatory non-data ink such as an axis or legend.

The keyword `layout` means how trained spaces are allocated into a viewport.

The keyword `renderer` means the runtime component that emits SVG.

The keyword `LSP` means Language Server Protocol.

The keyword `CLI` means command-line interface.

The keyword `AST` means abstract syntax tree.

The keyword `CST` means concrete syntax tree.

The keyword `IR` means intermediate representation.

The initial implementation target is a single Rust executable named `algraf`.

The executable supports at least two modes.

`algraf render` renders a source file to SVG.

`algraf lsp` runs a language server over standard input and standard output.

The parser, semantic analyzer, schema resolver, and runtime are shared by both modes.

LSP diagnostics and CLI diagnostics MUST derive from the same analysis engine.

LSP completions and CLI schema validation MUST derive from the same schema model.

SVG rendering and SVG previews MUST derive from the same render pipeline.

## 1. Executive Summary

Algraf is a domain-specific language for declarative data visualization.

It combines three design choices.

First, Algraf uses Wilkinson-style algebra to define coordinate space.

Second, Algraf uses block scoping to attach geometry to an inherited space.

Third, Algraf ships parser, LSP, runtime, and renderer as one Rust binary.

The smallest useful program is a `Chart` block containing a `Space` block containing one geometry.

```ag
Chart(data: "penguins.csv") {
    Space(flipper_length * body_mass) {
        Point(fill: species)
    }
}
```

The expression `flipper_length * body_mass` defines a two-dimensional Cartesian space.

The `Point` geometry inherits that space.

The property `fill: species` maps a categorical data column to fill color.

The data source is loaded from `penguins.csv`.

The compiler resolves the schema.

The analyzer validates that referenced columns exist.

The evaluator trains scales from the data.

The renderer emits SVG elements.

The same source also drives completions, hover, diagnostics, and formatting in an editor.

Algraf separates spatial layout from geometry behavior.

Dodged bars are expressed by changing the algebraic space.

```ag
Chart(data: "financials.csv") {
    Space((quarter / type) * amount) {
        Bar(fill: type)
    }
}
```

Stacked bars are expressed by keeping the ordinary Cartesian space and asking the geometry to stack collisions.

```ag
Chart(data: "financials.csv") {
    Space(quarter * amount) {
        Bar(fill: type, layout: "stack")
    }
}
```

The expression `(quarter / type) * amount` means:

`quarter` is the primary x dimension.

`type` is nested inside each `quarter`.

`amount` is the y dimension.

The bar geometry does not implement ad hoc dodging.

It receives a trained nested scale.

It asks the scale to resolve row coordinates.

The syntax is designed to be readable, parseable, and editor-friendly.

The implementation is designed to be fast, deterministic, and testable.

## 2. Design Goals

Algraf MUST make the coordinate algebra explicit.

Algraf MUST keep geometry code free of axis-layout hacks where algebra can express the layout.

Algraf MUST support resilient parsing for incomplete code.

Algraf MUST provide source spans for every syntax node.

Algraf MUST provide diagnostics that point to exact source ranges.

Algraf MUST provide schema-aware completions for data columns.

Algraf MUST support a single-binary installation story.

Algraf MUST support deterministic SVG output.

Algraf MUST support CSV input in the first implementation.

Algraf SHOULD support other tabular inputs later.

Algraf SHOULD keep syntax stable once examples are published.

Algraf SHOULD permit forward-compatible extensions.

Algraf SHOULD support precise snapshot testing.

Algraf SHOULD favor pure transformations in core evaluation.

Algraf SHOULD isolate filesystem and database I/O at clear boundaries.

Algraf SHOULD preserve enough syntax trivia for formatting.

Algraf SHOULD be pleasant to hand-write.

Algraf SHOULD be easy for LLMs to generate correctly.

Algraf SHOULD be easy to parse incrementally.

Algraf SHOULD produce helpful errors for common grammar-of-graphics mistakes.

Algraf supports declarative, opt-in interactive output (tooltips, hover
highlighting, host-owned click emitters, and Cartesian plot crosshairs with
axis value readouts) through inert mark metadata, emitted plot/axis geometry,
and a fixed, audited runtime; static SVG stays script-free by default (spec
§14.25, §24.6, §29.3).

Algraf MAY support raster output through a separate backend in later versions.

Algraf MAY support SQL-backed data sources in later versions.

Algraf MAY support a WebAssembly runtime in later versions.

Algraf MAY support IDE preview panes in later versions.

## 3. Non-Goals

Algraf is not a general-purpose programming language.

Algraf is not a spreadsheet formula language.

Algraf is not a replacement for SQL.

Algraf is not initially a replacement for ggplot2, plotnine, or Vega-Lite.

Algraf does not initially execute arbitrary user code.

Algraf does not initially support user-defined functions.

Algraf does not initially support mutable variables.

Algraf does not initially support loops.

Algraf does not initially support conditional statements.

Algraf does not initially support dynamic imports.

Algraf does not initially support network data fetching by default.

Algraf does not initially support database query execution.

Algraf does not initially support animated SVG.

Algraf does not initially support HTML canvas.

Algraf does not initially support WebGL.

Algraf supports *declarative* interactivity as opt-in metadata and emitted
chart geometry (tooltips, hover highlighting, host-owned click emitters, and
Cartesian plot crosshairs; spec §14.25, §24.6, §29.3): a chart declares *what*
data participates, and a viewer interprets inert metadata plus the rendered
plot/axis elements. Algraf does not support event-handler source, an embedded
scripting language, or user-authored runtime code; static SVG remains
script-free unless interactive output is explicitly requested.

Algraf does not initially support automatic statistical inference beyond explicitly specified statistics.

Algraf does not initially support arbitrary theme scripting.

Algraf does not initially support plugin execution.

Algraf does not initially support every grammar-of-graphics feature from Wilkinson's GPL.

## 4. Core Concepts

### 4.1 Chart

A chart is the root visual object.

A chart owns a data source.

A chart owns zero or more derived table declarations.

A chart owns zero or more space blocks.

A chart owns optional guide, scale, theme, and layout declarations.

A chart produces one SVG document by default.

A chart MAY later produce multiple pages or panels.

A chart MUST have a deterministic root viewport.

A chart MUST be the top-level construct in version 0.1.

### 4.2 Space

A space is a block-scoped algebraic frame.

A space has exactly one algebraic expression.

A space owns zero or more geometry declarations.

A space MAY own local scale, guide, or annotation declarations in later versions.

A geometry inside a space inherits the nearest parent space.

A space has a **coordinate system**. The default is Cartesian: a frame maps to
planar x/y pixels exactly as in earlier versions. A space MAY opt into a polar
coordinate system with `coords: "polar"`, which remaps scale *ranges* — not
domains — so that one frame axis wraps around an angle and the other extends
along a radius (spec §16.16). Polar accepts two further arguments:

- `theta`: `"x"` (default) or `"y"`, selecting which frame axis maps to the
  angle; the other maps to the radius. Invalid values are `E1902`.
- `innerRadius`: a number in `[0, 1)`, the fraction of the maximum radius left
  empty at the center (`0` = pie, `> 0` = donut). Out-of-range or non-numeric
  values are `E1903`.
- `startAngle` (since 0.31): a number of degrees in `[-360, 360]` (default `0`),
  rotating the angular origin clockwise from 12 o'clock. Out-of-range or
  non-numeric values are `E1909`.
- `direction` (since 0.31): `"clockwise"` (default) or `"counterclockwise"`, the
  angular sweep sense. Invalid values are `E1910`.

`coords` MUST be `"cartesian"` or `"polar"` (otherwise `E1901`). Polar requires a
1D or 2D (`a * b`) frame; a faceted frame is `E1904` and a 3D+ frame is rejected
as for Cartesian. Circular charts (pie, donut, coxcomb, wind rose, polar
scatter/line, annular heatmap, radar) arise from applying this transform to the
ordinary geometries, not from dedicated geometries. Cartesian output MUST be
byte-for-byte unchanged when `coords` is absent.

Cartesian frame order is physical. In `Space(a * b)`, `a` trains the physical
x axis and `b` trains the physical y axis. Scale declarations, guide
declarations, render metadata, draw-list axes, and interaction sidecars MUST
interpret `axis: x` and `axis: y` as those physical screen axes.

Version 0.46.0 removes frame-operator calls from source algebra. A source such
as `transpose(a * b)` MUST NOT be lowered or rendered through a compatibility
path. It MUST emit `E1912`; when the operand is a valid two-dimensional
Cartesian frame, diagnostics SHOULD include the physical-order replacement
(`b * a`) as help text.

Nested spaces are legal only as children of a `Glyph` declaration (§7.11). A
`Space` nested directly in another `Space` is reserved for later versions and
SHOULD produce a diagnostic.

### 4.3 Algebra

Algebra is the language of visual topology.

Identifiers name data columns.

Operators combine columns into coordinate structures.

`*` is cross.

`/` is nest.

`+` is blend.

Parentheses override precedence.

Algebra produces a frame before data is loaded.

Frame evaluation produces a dimensional tree.

Scale training hydrates the dimensional tree with data domains.

### 4.4 Geometry

A geometry is a drawable layer.

A geometry consumes an inherited trained space.

A geometry consumes its own properties.

A geometry emits SVG fragments.

A geometry MUST NOT mutate the chart.

A geometry MUST NOT read files directly.

A geometry SHOULD be deterministic.

A geometry SHOULD be independently snapshot-testable.

### 4.5 Derived Data

Derived data is a named table produced by a statistical transform.

Derived data declarations live in chart scope.

Derived data declarations are introduced with `Derive`.

Example:

```ag
Derive bins = Bin(value, bins: 25)
```

The declaration above creates a chart-scoped derived table named `bins`.

The `Bin` stat reads the chart's primary data table by default.

The `Bin` stat emits columns such as `bin_start`, `bin_end`, `bin_center`, and `count`.

A `Space` block may bind to a derived table with `data`.

Example:

```ag
Space(bin_start * count, data: bins) {
    Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)
}
```

Derived data is the primitive-level escape hatch for high-level statistical geometries.

High-level geometries such as `Histogram` MUST have a specified desugaring into derived data plus primitive marks where practical.

### 4.6 Property

A property is a key-value pair inside a geometry or configuration call.

```ag
Point(fill: species, alpha: 0.7, size: 3)
```

`fill: species` is a mapping when `species` resolves to a column.

`alpha: 0.7` is a setting because `0.7` is a literal.

`size: 3` is a setting because `3` is a literal.

The analyzer decides whether an identifier value is a column reference, selector, sentinel, or symbol reference using context.

Ambiguous identifier values MUST produce deterministic resolution.

The default resolution order SHOULD be:

1. Column reference where a data mapping is allowed.
2. Language selector where the property explicitly accepts selectors, such as `Guide(axis: x)`.
3. Language sentinel where the property explicitly accepts sentinels, such as `Chart(data: input)`.
4. Symbol reference where the property explicitly accepts chart symbols, such as `Space(..., data: bins)`.
5. Diagnostic if unresolved.

User-facing enum-valued options MUST use string literals in version 0.1.

Examples include `Bar(layout: "stack")`, `Smooth(method: "lm")`, and `Theme(name: "minimal")`.

Bare identifiers MUST NOT be accepted as enum values for ordinary properties in version 0.1.

If a user writes `Bar(layout: stack)`, the analyzer MUST produce a diagnostic suggesting `Bar(layout: "stack")`.

The bare `x` and `y` values in guide declarations are language selectors, not general enum values.

The bare `input` value in `Chart(data: input)` is a language sentinel, not a
general enum value. `stdin` is accepted as a compatibility alias for CLI-era
charts that already use `Chart(data: stdin)`.

### 4.7 Scale

A scale maps raw data values to visual values.

Position scales map data values to pixel coordinates.

Categorical position scales map categories to bands.

Continuous position scales map numbers to ranges.

Fill and stroke scales map data values to colors.

Alpha scales map data values to opacity.

Size scales map data values to marker radii or line widths.

Shape scales map data values to marker shapes.

Scales are trained from domains.

Domains are extracted from data referenced by spaces and properties.

Scales are used by guides.

Scales are used by geometries.

Scales are recorded in render metadata for debug output.

### 4.8 Guide

A guide is a visual explanation of a scale.

Axes guide position scales.

Legends guide fill, stroke, size, alpha, and shape scales.

Guides are generated by default.

Guides MAY be configured by explicit declarations.

Guide generation MUST be deterministic.

Guide layout MUST be included in viewport measurement.

### 4.9 Theme

A theme is a set of visual defaults.

Themes define fonts, colors, line widths, grid visibility, and spacing.

Themes MUST be data-independent.

Themes SHOULD be serializable.

Themes SHOULD be composable.

The initial implementation MUST ship with `minimal`.

The initial implementation SHOULD ship with `classic`, `light`, `dark`, and `void`.

Version 0.42.0 MUST also ship neutral presentation presets `gray`, `bw`, and
`linedraw`.

## 5. Syntax Overview

Algraf syntax resembles a SwiftUI or Kotlin DSL block structure.

Blocks are named calls followed by braces.

Calls use named arguments.

Algebra uses infix operators.

String literals use double quotes.

Quoted column identifiers use backticks.

Numbers are decimal or integer literals.

Booleans use `true` and `false`.

Comments use `//` for line comments.

Block comments MAY be supported later.

The canonical file extension is `.ag`.

The examples in this spec use the `ag` fenced code block language tag.

### 5.1 Minimal Scatter Plot

```ag
Chart(data: "penguins.csv") {
    Space(flipper_length * body_mass) {
        Point(fill: species, alpha: 0.7, size: 3)
    }
}
```

### 5.2 Dodged Bar Chart

```ag
Chart(data: "financials.csv") {
    Space((quarter / type) * amount) {
        Bar(fill: type)
    }
}
```

### 5.3 Stacked Bar Chart

```ag
Chart(data: "financials.csv") {
    Space(quarter * amount) {
        Bar(fill: type, layout: "stack")
    }
}
```

### 5.4 Faceted Scatter Plot

```ag
Chart(data: "penguins.csv") {
    Space((flipper_length * body_mass) / species) {
        Point(fill: species, alpha: 0.7, size: 3)
        Smooth(method: "lm", stroke: "#333333", strokeWidth: 2)
    }
}
```

### 5.5 Ribbon Plot

```ag
Chart(data: "intervals.csv") {
    Space(time * (lower + upper)) {
        Ribbon(fill: "steelblue", alpha: 0.25)
    }

    Space(time * estimate) {
        Line(stroke: "steelblue", strokeWidth: 2)
    }
}
```

### 5.6 Histogram

```ag
Chart(data: "distribution.csv") {
    Space(value) {
        Histogram(bins: 25, fill: "steelblue", alpha: 0.8)
    }
}
```

### 5.7 Primitive Histogram

```ag
Chart(data: "distribution.csv") {
    Derive bins = Bin(value, bins: 25)

    Space(bin_start * count, data: bins) {
        Rect(
            xmin: bin_start,
            xmax: bin_end,
            ymin: 0,
            ymax: count,
            fill: "steelblue",
            alpha: 0.8
        )
    }
}
```

This is the primitive form of `Histogram`.

`Bin` produces a derived table.

`Rect` draws the bins from explicit bounds.

`Histogram(bins: 25)` MUST desugar to this lower-level model.

### 5.8 Boxplot

```ag
Chart(data: "demographics.csv") {
    Space(gender * height) {
        Boxplot(fill: gender)
    }
}
```

### 5.9 Violin Plot

```ag
Chart(data: "demographics.csv") {
    Space(gender * height) {
        Violin(fill: gender, quantiles: [0.25, 0.5, 0.75])
    }
}
```

### 5.10 Heatmap

```ag
Chart(data: "heatmap_data.csv") {
    Space(day * hour) {
        Tile(fill: value)
    }
}
```

### 5.11 Reference Lines

```ag
Chart(data: "timeseries.csv") {
    Space(time * value) {
        Line(stroke: series)
        HLine(y: 12, stroke: "red", label: "Target")
        VLine(x: 3, stroke: "gray40", label: "Marker")
    }
}
```

## 6. Lexical Structure

### 6.1 Source Encoding

Source files MUST be valid UTF-8.

The parser MUST report invalid UTF-8 when reading from filesystem paths.

The parser MAY assume valid UTF-8 when receiving text from an LSP client.

Source spans MUST be byte offsets.

LSP conversions MUST map byte offsets to UTF-16 line-column positions.

The implementation MUST test byte-offset conversion with non-ASCII strings even if examples are ASCII.

### 6.2 Whitespace

Whitespace separates tokens.

Whitespace includes space, tab, carriage return, and newline.

Whitespace is insignificant outside string literals.

Newlines do not terminate statements.

The formatter controls canonical indentation.

### 6.3 Comments

Line comments begin with `//`.

Line comments run until the next newline or end of file.

Comments MUST be preserved in the CST.

Comments MAY be omitted from the AST.

The formatter SHOULD preserve comments near their original logical position.

Block comments begin with `/*` and run until the first `*/`.

Block comments MAY span multiple lines.

Block comments MUST NOT nest: the first `*/` closes the comment, and any
`/*` inside the comment body is ordinary comment text.

An unterminated block comment runs to end of input and MUST emit `E0020`.

Block comments are trivia: like line comments they MUST be preserved in the
CST, MAY be omitted from the AST, and the formatter SHOULD preserve them near
their original logical position.

> Promoted from v0.1 (where block comments were optional) to a v0.2.0
> requirement; see `docs/V0_2_PLAN.md`.

### 6.4 Identifiers

Identifiers name declarations, keywords, geometry types, property keys, and data columns.

Identifier regex:

```text
[A-Za-z_][A-Za-z0-9_]*
```

Identifiers are case-sensitive.

Keywords are reserved where grammar expects keywords.

Data columns with spaces, punctuation, reserved words, or other non-identifier characters MUST be referenced with quoted column identifiers.

Quoted column identifiers are part of version 0.1.

Quoted column identifiers use backticks.

Example:

```ag
Chart(data: "penguins.csv") {
    Space(`flipper length` * `body mass`) {
        Point(fill: `species name`)
    }
}
```

Backticks distinguish column identifiers from string literals.

Double quotes are always string literals.

Backticks inside quoted identifiers MUST be escaped with a backslash.

Version 0.20.0 quoted identifiers MUST also support Unicode scalar escapes in
the form `\u{...}`. Quoted identifiers MUST NOT support string-only control
escapes such as `\n` or `\t`.

### 6.5 Keywords

The following identifiers are reserved in version 0.1:

`Algraf`

`Chart`

`Derive`

`Space`

`Scale`

`Guide`

`Theme`

`Layout`

`Table`

`let`

`true`

`false`

`null`

`let` is a contextual keyword that introduces a variable binding (spec §7.10,
§9.6). It is reserved at the start of a chart-body or space-body item; the lexer
emits it as an identifier and the parser retags it as a keyword in that position.
`let` MUST NOT be used as a plain column identifier; a column literally named
`let` MUST be referenced with backticks.

`Table` introduces a named data table (spec §7.4, §10.10). It is reserved at
top level and at the start of a chart-body item; like the other block keywords
it is lexed as an identifier and retagged by the parser. A column literally
named `Table` MUST be referenced with backticks.

`from` is a contextual keyword only in `Derive name from table = ...` (spec
§7.4). Outside that position, `from` is an ordinary plain identifier.

`input` and `stdin` are contextual keywords only in `Chart(data: input)` and
`Chart(data: stdin)`.

Outside those source positions, `input` and `stdin` are ordinary plain
identifiers.

`Derive input = Bin(value, bins: 25)` and `Derive stdin = Bin(value, bins: 25)`
are syntactically valid, though style guides SHOULD discourage them because
they are visually confusing.

`Algraf` introduces the optional source header in version 0.20.0 and is
reserved only at top level before the first `Chart`.

Reserved words MUST NOT be used as plain column identifiers.

Reserved words MAY be used as column identifiers when quoted with backticks.

Geometry names are not globally reserved.

Property keys are not globally reserved.

### 6.6 String Literals

String literals begin and end with double quotes.

String literals support escape sequences.

Required escape sequences:

`\n`

`\r`

`\t`

`\"`

`\\`

Version 0.20.0 MUST support Unicode scalar escapes in string literals:
`\u{...}`. The escape body MUST contain one to six ASCII hex digits and MUST
decode to a valid Unicode scalar value.

Invalid escapes MUST produce diagnostics.

Unterminated strings MUST produce diagnostics with recovery.

### 6.7 Quoted Column Identifiers

Quoted column identifiers begin and end with backticks.

Quoted column identifiers name data columns exactly.

Quoted column identifiers are valid anywhere a column identifier is valid.

Examples:

```ag
Space(`body mass (g)` * `flipper length (mm)`) {
    Point(fill: `species name`)
}
```

The parser MUST produce a distinct token for quoted identifiers.

The semantic analyzer MUST resolve quoted identifiers against CSV headers by exact string match after escape processing.

Quoted identifiers MUST NOT be interpreted as string literals.

Unterminated quoted identifiers MUST produce diagnostics with recovery.

### 6.8 Number Literals

Integer examples:

`0`

`1`

`25`

`1000`

Decimal examples:

`0.5`

`3.14`

`-10.2`

Scientific notation SHOULD be supported:

`1e3`

`2.5e-4`

Negative numbers SHOULD lex as a signed number when the minus appears immediately before digits.

There is no binary subtraction operator in version 0.1.

### 6.9 Boolean Literals

`true` and `false` are boolean literals.

They MUST NOT resolve as identifiers.

### 6.10 Null Literal

`null` denotes absence of value.

It MAY be used to suppress labels or guides.

Example:

```ag
Guide(fill: null)
```

### 6.11 Punctuation

The lexer recognizes:

`(`

`)`

`{`

`}`

`[`

`]`

`:`

`,`

`=`

`=>` (fat arrow) — separates a key from its value in a map literal (spec §7.8).
The lexer matches `=>` in preference to `=` (longest match).

`*`

`/`

`+`

`` ` ``

`.` MAY be recognized for future qualified names.

`-` MAY be part of number literals.

### 6.12 Token Spans

Every token MUST include:

token kind

lexeme text or parsed value

start byte offset

end byte offset

line and column MAY be cached.

The parser MUST use token byte spans when constructing AST spans.

Diagnostics MUST include source spans.

## 7. Grammar

This grammar is normative for version 0.1 except where marked implementation-defined.

The grammar is expressed as pseudo-EBNF.

It is intentionally simple enough for recursive descent plus Pratt expression parsing.

### 7.1 Program

```ebnf
Program        ::= Trivia* SourceHeader? TopLevelItem (Trivia* TopLevelItem)* Trivia* EOF
TopLevelItem   ::= TableDecl | ChartBlock
SourceHeader   ::= "Algraf" "(" SourceHeaderArgs ")"
SourceHeaderArgs ::= Arg ("," Arg)* ","?
```

`Trivia` means whitespace and comments retained by the lexer/CST layer.

Trivia is not represented as typed AST children.

A source file MUST contain at least one chart block.

Since version 0.46.1, a source file MAY contain document-scope `Table`
declarations before or between chart blocks. Document-scope tables are visible
to every chart in the file and use the same source-expression rules as
chart-scope `Table` declarations (§7.4.1, §10.10).

Version 0.20.0 MAY begin with a single source header before the first chart.
Version 0.21.0 uses the same header form:

```ag
Algraf(version: "0.21", features: ["sql"])
```

If present, `version` is required and MUST be a string literal. `features` is an
optional array of string literals. Version 0.20.0 recognizes `sql`, `network`,
`plugins`, and `experimental` as reserved feature gates; these gates do not
enable SQL, network access, plugins, or experimental syntax in version 0.20.0.
Version 0.21.0 enables local SQLite sources only when the source header declares
`features: ["sql"]`; `network`, `plugins`, and `experimental` remain reserved.
Unknown or duplicate feature gates MUST emit diagnostics. A gated source used
without the required version and feature gate is `E0025`.

A source file MAY contain more than one top-level chart block. Each chart is a
complete, independent chart: it has its own data source, scales, guides, theme,
and layout, and renders separately. Charts do not share layout or a viewport;
multiple charts are distinct from multiple `Space` blocks within one chart
(spec §17.5).

When multiple charts are present, the CLI `render` command writes one output per
chart. With a single chart the `--output` path is used verbatim; with multiple
charts a 1-based `-{n}` suffix is inserted before the extension (so
`--output out.svg` produces `out-1.svg`, `out-2.svg`, …). Rendering a
multi-chart document to stdout (no `--output`) is a usage error, since several
SVG documents cannot be concatenated into one. Each chart resolves its own
`data` source; sharing the `stdin` data sentinel across charts is a usage error.

If extra top-level tokens appear between or after top-level chart/table items,
the parser MUST emit diagnostics and recover.

### 7.2 Chart Block

```ebnf
ChartBlock     ::= "Chart" ( "(" ChartArgs? ")" )? BlockStart ChartBody BlockEnd
ChartArgs      ::= Arg ("," Arg)* ","?
ChartBody      ::= ChartItem*
ChartItem      ::= SpaceBlock
                 | DeriveDecl
                 | TableDecl
                 | GlyphDecl
                 | LetDecl
                 | ScaleDecl
                 | GuideDecl
                 | ThemeDecl
                 | LayoutDecl
                 | ErrorItem
```

`Chart` MUST declare a primary data source. The primary source may be written as
`Chart(data: <source>)`, where `<source>` is a source expression or a bare table
name, or by omitting the argument list when a visible table named `main` exists.
`Chart { Table main = "some.csv" ... }` is therefore equivalent to
`Chart(data: main) { Table main = "some.csv" ... }` for primary-data loading.

If neither `Chart(data: ...)` nor a visible `Table main = ...` declaration is
available, semantic analysis MUST emit `E1001`.

`Chart` MAY include `width` and `height` arguments.

`Chart` MAY include `title`, `subtitle`, and `caption` arguments.

`Chart` MAY include `alt` and `description` string arguments. `alt` is a short
accessible label for the chart. `description` is a longer accessible
description and, when present, MUST be preferred over the older subtitle/caption
fallback for SVG `<desc>` and render metadata.

`Chart` MAY include `marginTop`, `marginRight`, `marginBottom`, and `marginLeft`
arguments. Each is a non-negative integer giving a per-side plot margin in
pixels (see §17.3). With axes they act as a floor — useful to reserve room for
annotations outside the plot area, such as direct end-labels on a slope chart.
With no axes they set the margin exactly and MAY be `0`, letting an embedded
sparkline bleed to the viewport edge.

`Chart` MUST NOT include `theme` as a shorthand in version 0.1.

Themes are declared with `Theme(name: "minimal")` inside the chart body.

Example:

```ag
Chart(data: "penguins.csv", width: 800, height: 520) {
    Theme(name: "minimal")

    Space(flipper_length * body_mass) {
        Point()
    }
}
```

Equivalent primary-table spellings:

```ag
Table main = "penguins.csv"

Chart(data: main, width: 800, height: 520) {
    Space(flipper_length * body_mass) {
        Point()
    }
}
```

```ag
Chart {
    Table main = "penguins.csv"

    Space(flipper_length * body_mass, data: main) {
        Point()
    }
}
```

### 7.3 Space Block

```ebnf
SpaceBlock     ::= "Space" "(" Algebra SpaceArgs? ")" BlockStart SpaceBody BlockEnd
SpaceArgs      ::= "," Arg ("," Arg)* ","?
SpaceBody      ::= SpaceItem*
SpaceItem      ::= GeometryCall
                 | LetDecl
                 | ScaleDecl
                 | GuideDecl
                 | ThemeDecl
                 | ErrorItem
```

A `GeometryCall` head names either a built-in geometry or a chart-scoped `Glyph`
(spec §7.11, §13.8); the two share call syntax and are distinguished during
semantic analysis. A `Space` MUST NOT be nested directly inside another `Space`;
subordinate charts are expressed as glyph marks (§14.27), not nested spaces.

`Space` MUST include exactly one algebra expression.

`Space` with an empty expression MUST produce a diagnostic and an error expression node.

`Space` body MAY be empty during editing.

An empty `Space` body SHOULD produce a warning in CLI render mode.

`Space` MAY include a `data` argument.

`Space(..., data: name)` binds that space to a chart-scoped named table or
derived table.

`Space` MAY include `coords`, `theta`, and `innerRadius` arguments for a polar
coordinate system (§4.2, §16.16). These are ordinary named `Arg` nodes — no
grammar change — validated in semantics (`E1901`–`E1905`).

`Space` MAY include Cartesian coordinate-view arguments `zoomX`, `zoomY`, and
`aspect` (§16.17). These are coordinate controls, not scale declarations:
`zoomX`/`zoomY` clip the visible panel range after stats and scale training, and
`aspect` controls the final plot rectangle's x/y unit ratio.

The `data` argument MUST be a bare identifier that resolves to a named or
derived table.

Example:

```ag
Space(bin_start * count, data: bins) {
    Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)
}
```

`Space` MAY include `Theme` declarations in its body.

Space-local themes override chart-level theme values for that space only.

Space-local themes MUST NOT mutate chart-level theme state.

A subordinate chart anchored at a host row — a map glyph, a sparkline on a
point, a mini-pie — is expressed as a **glyph mark** (spec §7.11, §14.27): a
chart-valued mark declared once in chart scope with `Glyph` and invoked inside a
`Space` body with ordinary geometry-call syntax. A glyph mark draws at the host
row's anchor like `Point` and participates in the existing mark, scale, and
legend systems; it is not a raw SVG/HTML injection surface.

Example:

```ag
Chart(data: "timeseries.csv") {
    Theme(name: "minimal")

    Space(time * value) {
        Theme(name: "void")
        Line(stroke: series)
    }
}
```

### 7.4 Derive Declaration

```ebnf
DeriveDecl     ::= "Derive" Ident DeriveSource? "=" StatCall
DeriveSource   ::= "from" Ident
StatCall       ::= Ident "(" StatInput? StatArgs? ")"
StatInput      ::= Algebra
StatArgs       ::= "," Arg ("," Arg)* ","?
```

`Derive` creates a named derived table in chart scope.

The derived table name MUST be unique among derived tables.

The derived table name MUST NOT conflict with reserved keywords.

Since version 0.47.0, a `Derive` MAY include `from name` before `=`. The `name`
MUST resolve to a chart-scoped named table or derived table. If omitted, the
stat reads the chart's primary table.

The stat call name is PascalCase.

Version 0.1 MUST support `Bin`.

Example:

```ag
Derive bins = Bin(value, bins: 25)
Derive trend from bins = Smooth(bin_center, count, method: "lm")
```

#### 7.4.1 Table Declaration

```ebnf
TableDecl      ::= "Table" Ident "=" SourceExpr
SourceExpr     ::= String
                 | GeoJsonSource
                 | ShapefileSource
                 | ParquetSource
                 | SqliteSource
                 | TopoJsonSource
GeoJsonSource  ::= "GeoJson" "(" String ")"
ShapefileSource ::= "Shapefile" "(" String ")"
ParquetSource  ::= "Parquet" "(" String ")"
SqliteSource   ::= "Sqlite" "(" String "," String ")"
TopoJsonSource ::= "TopoJson" "(" String ("," "object" ":" String)? ")"
```

`Table` creates a named data table from an independent source (spec §10.10). A
`Table` may appear at document scope or inside a chart body. Document-scope
tables are visible to every chart in that source file; chart-scope tables are
visible only within their chart. The `SourceExpr` position is a *source
expression*: a string path uses extension-based format selection, geospatial and
Parquet constructors select their loader explicitly, `Sqlite(path, query)`
selects a local SQLite query (spec §10.12), and `TopoJson(path, object:)`
selects a TopoJSON object.

A `Table` name MUST be unique among `Table` declarations (`E1105`) and MUST NOT
conflict with a derived table (`E1108`). A `Table` is bound to a
`Space` by bare identifier in `data:`, exactly as a derived table is (spec §7.3).
A `Chart(data: name)` value MAY name a visible `Table`; in that position the
identifier is a table reference, not a column reference or string path.

Example:

```ag
Table cities = "minard_cities.csv"
```

### 7.5 Geometry Call

```ebnf
GeometryCall   ::= Ident "(" ArgList? ")"
ArgList        ::= Arg ("," Arg)* ","?
Arg            ::= Ident ":" Value
Value          ::= Algebra
                 | Literal
                 | StdinSentinel
                 | Array
                 | Map
                 | CallValue
CallValue      ::= Ident "(" ArgList? ")"
```

A `CallValue` is a nested `Name(args)` value. It is used by custom theme
overrides (spec §20.8), e.g. `axisText: Text(size: 12)`. Semantic analysis
decides which properties accept a call value; most geometry properties do not.

This grammar admits algebra expressions as property values.

`StdinSentinel` is the bare token `input`, or the compatibility alias `stdin`.

Semantic analysis decides whether algebra is allowed for that property.

Version 0.1 SHOULD allow only identifiers or literals for most geometry properties.

### 7.6 Declarations

```ebnf
ScaleDecl      ::= "Scale" "(" ArgList? ")"
GuideDecl      ::= "Guide" "(" ArgList? ")"
ThemeDecl      ::= "Theme" "(" ArgList? ")"
LayoutDecl     ::= "Layout" "(" ArgList? ")"
```

Declaration syntax is intentionally call-like.

Example:

```ag
Scale(axis: x, type: "log10")
Guide(axis: x, label: "Flipper Length (mm)")
Theme(name: "minimal")
Layout(padding: 24)
```

### 7.7 Algebra

```ebnf
Algebra        ::= BlendExpr
BlendExpr      ::= CrossExpr ("+" CrossExpr)*
CrossExpr      ::= NestExpr ("*" NestExpr)*
NestExpr       ::= PrimaryExpr ("/" PrimaryExpr)*
PrimaryExpr    ::= QualifiedName
                 | "(" Algebra ")"
                 | ErrorExpr
QualifiedName  ::= Ident
                 | QuotedIdent
                 | Ident "." Ident
                 | Ident "." QuotedIdent
```

Operator precedence from tightest to loosest:

`/`

`*`

`+`

All operators are left-associative in version 0.1.

Thus:

`a / b / c` parses as `(a / b) / c`.

`a * b * c` parses as `(a * b) * c`.

`a + b + c` parses as `(a + b) + c`.

`a / b * c` parses as `(a / b) * c`.

`a * b + c` parses as `(a * b) + c`.

Blend expressions using `+` MUST be written inside explicit parentheses in version 0.1.

Examples:

```ag
Space(time * (lower + upper)) {
    Ribbon(ymin: lower, ymax: upper)
}
```

```ag
Space((lower + upper)) {
    Rug()
}
```

Unparenthesized blend expressions such as `time * lower + upper` and `lower + upper` MUST produce semantic diagnostics.

The formatter MUST NOT remove parentheses that make a blend expression valid.

Qualified algebra names are reserved for row-context references such as
`outer.id` inside a glyph mark's `key` resolution (§7.11, §14.27). Outside those
contexts, a qualified name MUST produce `E2204`.

### 7.8 Literals

```ebnf
Literal        ::= String
                 | Number
                 | Boolean
                 | Null
Array          ::= "[" ValueList? "]"
ValueList      ::= Value ("," Value)* ","?
Map            ::= "[" Entry ("," Entry)* ","? "]"
Entry          ::= Value "=>" Value
```

Arrays MAY be nested.

Array element types SHOULD be homogeneous where the receiving property requires homogeneity.

Heterogeneous arrays MAY produce semantic diagnostics.

A bracketed value is a **map** when it contains a top-level `=>`, and an
**array** otherwise. A map associates ordered key/value pairs; its keys define
iteration (and legend-entry) order. A malformed map entry — a missing `=>` or a
stray separator — MUST produce diagnostic `E0021`. Maps are accepted only where
a property documents map support (currently a categorical color `Scale`'s
`range:` and `labels:`, spec §16.13); used elsewhere they produce `E1606`.

**Temporal literals (since version 0.31).** `datetime("…")` and `date("…")` are
typed value constructors written as a call value with a single quoted string
argument. They parse their contents with the same conservative automatic rules as
schema inference (§10.3) and yield a UTC-equivalent instant; `date(...)` truncates
to midnight. A temporal literal is a value, not an algebra primitive: it is valid
only where a numeric position or scale-domain bound is accepted (at least
`HLine`/`VLine` `x:`/`y:` and `Scale(domain: [...])` bounds, §16.11). Contents the
rules do not recognize, or the wrong argument shape, produce `E1017`. A temporal
literal used in any other value position produces `E1018`.

### 7.9 Error Items

`ErrorItem` is not written by users.

It represents recovered syntax.

The parser SHOULD create an error node when it cannot parse a chart item, space item, property, or expression.

The parser MUST advance at least one token when creating an error node to avoid infinite loops.

The parser SHOULD synchronize at:

`)`

`}`

`,`

known block keywords

known geometry identifiers if inside a space

EOF

### 7.10 Let Binding

```ebnf
LetDecl        ::= "let" Ident "=" Value
```

A `let` declaration binds a name to a constant value. It is valid as a chart-body
item and as a space-body item.

`let` MUST be followed by an identifier name, then `=`, then a `Value`.

The bound value MUST be a constant: a string, number, boolean, null literal, or
an array of those (spec §7.8). Column mappings, algebra expressions, the `stdin`
sentinel, and references to other variables MUST produce diagnostic `E1701`.
Version 0.1 of variables intentionally excludes user-defined functions and
column shadowing; the first cut is constant values only.

Version 0.20.0 also permits a `let` binding to hold a `Style(...)` fragment:

```ag
let muted = Style(fill: "#6b7280", alpha: 0.55)
```

`Style(...)` is a property bag, not a user-defined function. Its entries MUST be
named arguments. A style fragment MUST NOT contain `style:`. A `style:` argument
inside a geometry call applies a style fragment at that source position; later
explicit properties or later style fragments override earlier expanded
properties.

The bound name lives in a variable namespace separate from columns and derived
tables (spec §9.6). A variable name MUST be unique within its scope; a second
binding of the same name in the same scope MUST produce diagnostic `E1702`.

A `Value` parser error after `=` follows the usual value recovery (spec §12.13);
a missing `=` MUST produce diagnostic `E0021`.

Example:

```ag
Chart(data: "penguins.csv") {
    let primary = "#3366cc"
    let dim_alpha = 0.4

    Space(flipper_length * body_mass) {
        Point(fill: primary, alpha: dim_alpha)
    }
}
```

### 7.11 Glyph Declaration

A `Glyph` declaration (since version 0.71) is a chart-scoped, reusable,
chart-valued mark template. It supersedes the removed `Inset` block.

```ebnf
GlyphDecl   ::= "Glyph" Ident "(" GlyphArgs ")" BlockStart GlyphBody BlockEnd
GlyphArgs   ::= GlyphArg ("," GlyphArg)* ","?
GlyphArg    ::= Arg
GlyphBody   ::= GlyphItem*
GlyphItem   ::= SpaceBlock
             | LetDecl
             | ScaleDecl
             | GuideDecl
             | ThemeDecl
             | ErrorItem
```

Rules:

- A `Glyph` MUST be declared in chart scope (alongside `Table`, `Derive`,
  `Scale`, `Theme`, `Guide`, `Layout`).
- The declaration arguments are ordinary named `Arg` nodes validated in
  semantics. `data` is REQUIRED and MUST name a chart-scoped `Table` or
  `Derive`. `key` is REQUIRED. `scales` is OPTIONAL. Any other argument is
  `E2201`.
- `key` lists the correlation columns and accepts one of three forms: a single
  bare identifier (`key: store`); a bracketed bare list whose entries are
  identifiers each equi-matched against a host-row column of the same name
  searched outward through the row-context chain (`key: [id, category]`); or a
  bracketed map of explicit `child => hostRef` pairs (`key: [id => region]`).
  In the map form `hostRef` is an unqualified host-row column or an
  `outer.`-qualified ancestor column (`key: [id => outer.id]`). An invalid or
  missing `key` is `E2203`.
- `scales` sets the default training scope for the glyph's internal scales and
  MUST be `"shared"` or `"local"`, defaulting to `"shared"`. Per-`Scale`
  `train:` (§16.18) overrides it.
- A glyph body MUST contain one or more `Space` blocks, identical in form to any
  other space (exactly one algebra expression each, §7.3). `let`, `Scale`,
  `Guide`, and `Theme` declarations directly inside a glyph body apply as
  inherited defaults to each child `Space`; declarations inside a child `Space`
  override those defaults. A glyph body MUST NOT contain user-authored
  JavaScript, CSS, HTML, external images, or raw SVG.
- A glyph name MUST NOT shadow a built-in geometry name (§13.8); a collision is
  `E2201` at the declaration site.
- A glyph MUST NOT invoke itself, directly or transitively (`E2209`/`E2210`).

Example:

```ag
Chart(data: "stores.csv") {
    Table mix = "store_category_mix.csv"

    Glyph pie(data: mix, key: store, scales: "shared") {
        Space(share, coords: polar, theta: y) {
            Bar(fill: category, layout: "fill")
        }
    }

    Scale(size: footfall, range: [16, 44])

    Space(revenue * satisfaction) {
        Point(alpha: 0.15, size: 2)
        pie(size: footfall, clip: "circle")
    }
}
```

## 8. Algebra Semantics

### 8.1 Overview

Algebra expressions define frames.

Frames are symbolic before data loading.

Frames are evaluated into `DimensionalSpace`.

`DimensionalSpace` is then hydrated into `ScaledSpace`.

The algebra operators are not arithmetic.

`a * b` does not multiply values.

`a / b` does not divide values.

`a + b` does not add values.

They are structural operators over data domains.

### 8.2 Identifier Frame

An identifier frame names a single column.

Expression:

```ag
value
```

Dimensional form:

```rust
Vector("value")
```

The analyzer MUST verify that `value` exists in the active data schema.

The analyzer SHOULD infer whether the vector is categorical, continuous, temporal, or unknown.

### 8.3 Cross Operator

The cross operator creates a Cartesian product.

Expression:

```ag
x * y
```

Dimensional form:

```rust
Cartesian(Vector("x"), Vector("y"))
```

`Cross` is usually used to define x and y position.

Left operand maps to physical horizontal position.

Right operand maps to physical vertical position.

For more than two operands, the result is a higher-dimensional Cartesian space.

Version 0.1 renderer MUST support 1D and 2D spaces.

Version 0.1 analyzer MUST reject unsupported 3D Cartesian spaces with a diagnostic.

If the analyzer sees `x * y * group`, the diagnostic MUST explain that 3D Cartesian spaces are unsupported and SHOULD suggest `(x * y) / group` when `group` is categorical.

The LSP SHOULD expose that suggestion as a quick fix.

### 8.4 Nest Operator

The nest operator conditions one domain inside another domain.

Expression:

```ag
quarter / type
```

Dimensional form:

```rust
Nested(Vector("quarter"), Vector("type"))
```

In a position context, `Nested(a, b)` allocates a band for each `a`, then sub-bands for each `b` within that band.

This is the algebraic basis for dodged bars.

Nested spaces represent facets when applied to a whole Cartesian plane.

Expression:

```ag
(x * y) / group
```

Dimensional form:

```rust
Nested(Cartesian(Vector("x"), Vector("y")), Vector("group"))
```

Version 0.1 MUST support nested x-axis bands.

Version 0.1 MUST support faceting.

Expression `(x * y) / group` MUST produce a facet-wrap layout by default.

Facets MUST share x and y scales by default.

Version 0.41 MUST support panel-local facet scales through
`Layout(facetScales: "fixed" | "free-x" | "free-y" | "free")`. `"fixed"` is
the default and shares both axes; `"free-x"` trains x panel-locally and shares
y; `"free-y"` shares x and trains y panel-locally; `"free"` trains both axes
from each panel's rows. Legends remain chart-level and are not retrained per
facet.

Facet columns MUST be chosen automatically when not specified.

Facet column count MAY be overridden with `Layout(facetColumns: n)`.

Version 0.41 MUST support facet grids through `Layout(facetRows: col,
facetCols: col)`. Either side MAY be omitted; when both are present the renderer
lays panels out row-major over the row-domain cross product with the
column-domain. Empty combinations MUST still reserve a panel. `facetRows` and
`facetCols` columns MUST be categorical (or unknown during editing); otherwise
`E1303`.

### 8.5 Blend Operator

The blend operator unions domains into a shared dimension.

Source-level blend MUST be explicitly parenthesized in version 0.1.

Valid blend examples:

```ag
time * (lower + upper)
```

```ag
(lower + upper)
```

Invalid blend examples:

```ag
time * lower + upper
```

```ag
lower + upper
```

Expression:

```ag
lower + upper
```

Dimensional form:

```rust
Union(Vector("lower"), Vector("upper"))
```

In a continuous position context, the union domain spans the minimum and maximum of both operands.

In a categorical context, the union domain contains unique categories from both operands in deterministic order.

Blend is used for intervals, ribbons, ranges, and multi-measure axes.

Example:

```ag
Chart(data: "intervals.csv") {
    Space(time * (lower + upper)) {
        Ribbon(fill: "steelblue", alpha: 0.25)
    }
}
```

The y scale domain includes both `lower` and `upper`.

The `Ribbon` geometry uses properties or conventions to know which columns are lower and upper.

If a geometry cannot infer lower and upper columns from the union expression, it MUST require explicit properties.

Example:

```ag
Chart(data: "intervals.csv") {
    Space(time * (lower + upper)) {
        Ribbon(ymin: lower, ymax: upper, fill: "steelblue", alpha: 0.25)
    }
}
```

### 8.6 Operator Precedence Rationale

Nest binds tighter than cross because grouped slots are usually formed before crossing with a measure.

`quarter / type * amount` should mean `(quarter / type) * amount`.

Cross binds tighter than blend only for parser recovery and diagnostics.

`time * lower + upper` parses as `(time * lower) + upper`, but the analyzer MUST reject it because the blend is not explicitly parenthesized.

For common interval syntax, users MUST write `time * (lower + upper)`.

### 8.7 Associativity

All operators are left-associative.

`a / b / c` means nested `c` inside `b` inside `a`.

`a * b * c` means a 3D Cartesian space and is unsupported by the version 0.1 SVG renderer.

`(a + b + c)` means a union of three domains.

`a + b + c` without enclosing parentheses MUST be rejected in version 0.1.

The analyzer SHOULD flatten associative operators into normalized IR where useful.

### 8.7.1 Removed Frame Operator Calls

Version 0.46.0 removes prefix frame-operator calls from source algebra. The
parser MAY retain a recovered call-shaped CST node so diagnostics and editor
code actions can target legacy source, but the analyzer MUST NOT treat any
call-shaped frame expression as a valid frame.

The removed v0.33 spelling:

```ag
transpose(a * b)
```

MUST emit `E1912` and MUST NOT lower to `b * a`. An unknown call such as
`flip(a * b)`, an empty call such as `transpose()`, and a malformed call in
frame position MUST also emit `E1912`.

A bare `transpose` remains an ordinary column reference, and
`` `transpose` `` MUST always be a quoted column reference. Only the
call-shaped form is removed.

When the removed call wraps a valid two-dimensional Cartesian frame, diagnostics
SHOULD explain that frame order is physical and include the equivalent
replacement frame. LSP code actions MAY offer the same mechanical rewrite; for
example, `transpose((a * b)) / group` may be rewritten to `(b * a) / group`.

### 8.8 Algebra Type System

Every algebra expression has an algebra kind.

Suggested kinds:

`Vector1`

`Cartesian2`

`CartesianN`

`Nested`

`Union`

`Faceted`

`Invalid`

Every algebra expression has data domain information after schema analysis.

Suggested domain kinds:

`Continuous`

`Categorical`

`Temporal`

`Boolean`

`Unknown`

`Mixed`

`Spatial` — since 0.8: a 1-D frame (`Vector1`) over a geometry column
(spec §10.11). A spatial frame trains the spatial scale (§16.15) rather than an
ordinary position axis; it is the algebra kind of `Space(geom)`. A geometry
column used in any other position (e.g. as a Cartesian axis) is `E1801`.

The analyzer MUST propagate `Invalid` without cascading excessive errors.

The analyzer SHOULD report one primary diagnostic for a malformed expression.

### 8.9 Frame Normalization

The semantic analyzer SHOULD normalize frames into a canonical representation.

Canonical representation examples:

`a * b` becomes `Cartesian { axes: [a, b] }`.

`a * b * c` becomes `Cartesian { axes: [a, b, c] }` before the version 0.1 renderer rejects it as unsupported.

`(a + b + c)` becomes `Union { members: [a, b, c] }`.

`a / b / c` becomes `Nested { root: a, children: [b, c] }`.

Canonicalization makes scale training simpler.

Canonicalization makes equality checks simpler.

Canonicalization improves caching.

Canonicalization MUST preserve source spans for diagnostics.

### 8.10 Unsupported Algebra

The analyzer MUST reject:

empty algebra expression

unknown column identifier

unsupported 3D render target

invalid blend of incompatible domains where no scale can support it

unparenthesized blend expressions

nesting continuous dimensions inside continuous dimensions unless explicitly supported

faceting expressions whose panel variable cannot be treated as categorical

The analyzer SHOULD warn:

blend expression used where no geometry consumes both sides

nested expression used with geometry that cannot use banded coordinates

categorical y dimension with continuous-only geometry

continuous x dimension with bar geometry and no binning/stat

## 9. Block Scope Semantics

### 9.1 Chart Scope

The chart scope contains:

data source

derived table names

schema

global theme

global scales

global guides

global layout settings

space blocks

The chart scope is the root for name resolution.

Column names live in chart scope.

Derived table names live in chart scope.

Geometry names live in language scope.

Properties live in geometry-specific scope.

### 9.2 Space Scope

The space scope contains:

active table

algebraic frame

trained space

space-local scales

space-local guides

space-local theme

geometry list

Space-local declarations override chart declarations where applicable.

Space-local declarations MUST NOT mutate chart-level declarations.

### 9.3 Geometry Scope

The geometry scope contains:

geometry type

properties

inherited frame

inherited scales

resolved mappings

resolved settings

statistical transform if any

Geometries do not define child scopes in version 0.1.

### 9.4 Name Resolution

Name resolution depends on position.

In `Chart(data: "file.csv")`, `data` is a property key.

In `Chart(data: main)`, `main` is a visible `Table` reference.

In `Space(x * y)`, `x` and `y` are column references.

In `Derive bins = Bin(value, bins: 25)`, `bins` is a derived table name and `value` is a column reference in the chart's primary table.

In `Derive trend from bins = Smooth(bin_center, count)`, `trend` is a derived table name, `bins` is a derived-table source reference, and `bin_center` and `count` are column references in `bins`.

In `Space(bin_start * count, data: bins)`, `bins` is a derived table reference, while `bin_start` and `count` are column references in that derived table.

In `Point(fill: species)`, `fill` is a property key and `species` is a property value.

In `Point(fill: "red")`, `fill` is a property key and `"red"` is a literal.

In `Point(shape: "circle")`, `"circle"` is a string literal used as an enum-valued option.

If a property accepts both column mappings and enum values, unquoted identifiers MUST resolve only as column mappings.

Enum values MUST be written as strings in version 0.1.

### 9.5 Inheritance

Geometries inherit:

active data source

active frame

active theme

active layout viewport

active scale defaults

active guide defaults

Geometries do not inherit properties from other geometries.

Geometries MAY inherit aesthetic defaults from theme.

### 9.6 Shadowing

A `let` declaration introduces a variable in a namespace separate from columns
and derived tables (spec §7.10). Variables hold constant values only.

Variables are resolved up front per scope, so a `let` MAY be referenced
regardless of its position within the same block.

Scopes:

- A chart-scope `let` (a chart-body item) is visible in every space of the
  chart.
- A space-scope `let` (a space-body item) is visible only within that space and
  MUST NOT leak into sibling spaces.

A space-scope `let` MUST shadow a chart-scope `let` of the same name for the
duration of that space.

Variable resolution applies only in property value positions. A bare,
unquoted identifier in a property value position MUST resolve to an in-scope
variable when one exists with that name, taking precedence over a column of the
same name; otherwise it resolves as a column reference (spec §9.4). A
backtick-quoted identifier is always a column reference and is never resolved as
a variable.

Variables MUST NOT be resolved inside algebra (`Space(...)` frames and stat
inputs); identifiers there are always columns.

After resolution, a variable's value is type-checked against the property's
accepted forms (spec §13.9); a mismatch produces `E1204` at the use site.

Column names SHOULD NOT shadow keywords inside grammar positions.

Quoted identifiers MUST be used to reference keyword-like column names.

## 10. Data Sources

### 10.1 Initial Data Source Model

Version 0.1 supports CSV files. Version 0.7 adds TSV, JSON, and NDJSON files
(spec §10.2). Version 0.43 adds native CLI Parquet loading (spec §10.13).
Version 0.57 adds Arrow IPC stream loading for caller-provided data
(spec §10.14). All formats load into the same dataframe abstraction and behave
identically downstream once materialized.

`Chart(data: "path.csv")` resolves `path.csv` relative to the source file
directory by default. Since version 0.46.1, `Chart(data: name)` MAY instead
name a visible `Table`; in that case the table's source is the chart's primary
data source. If `Chart` omits its argument list and a visible `Table main = ...`
exists, `main` is used as the primary source.

The data source format is selected by the path's file extension (spec §10.2).
`Chart(data: "sales.json")` loads JSON; `Chart(data: "sales.tsv")` loads TSV.
The same rule applies to the `--data` override and to `Table name = "..."`
declarations (spec §10.10).

The data crate MUST keep path-oriented compatibility APIs for all supported
formats. Streamable single-file formats (`csv`, `tsv`, `json`, `ndjson`,
`geojson`, and `arrow-stream`) MUST also be loadable from an already-open reader
or byte slice so callers can provide bytes without forcing the data crate to
re-open a filesystem path. Parquet MUST have native path and byte APIs; native
filesystem paths SHOULD use the Parquet reader's path/chunk interface rather
than first buffering the entire file into a `Vec<u8>`.

Version 0.8 and later geospatial releases add geospatial **source constructors**
on the same seam, selected explicitly rather than by extension (spec §10.11):
`GeoJson("path")` loads a GeoJSON `FeatureCollection`,
`Shapefile("path.shp")` loads an ESRI shapefile bundle, and
`TopoJson("path.topojson", object: "name")` loads a TopoJSON topology object.
They MAY appear wherever a data source is accepted — for example
`Chart(data: GeoJson("us.geojson"))` and `Table counties = Shapefile("us.shp")`.
A source constructor's path is its first positional string argument.

Version 0.21 adds `Sqlite("path.db", "SELECT ... ORDER BY ...")` on the same
source-expression seam (spec §10.12). It MAY appear in `Chart(data:)` and in
`Table name = ...` only when the source header enables `features: ["sql"]`.
The first positional string is a local database path; the second positional
string is the SQL query.

Version 0.43 adds `Parquet("path.parquet")` on the same source-expression seam
(spec §10.13). It MAY appear in `Chart(data:)` and in `Table name = ...`.
The `.parquet` and `.parq` extensions also select the Parquet loader.

`Chart(data: input)` reads caller-provided primary data. In the CLI, caller
input is supplied with `--data -`; in an embedded host, caller input is the byte
buffer or structured JSON value provided to the Rust facade.

`stdin` is accepted as a compatibility alias for `input`.

`input` and `stdin` are bare sentinels, not string paths.

`Chart(data: "input")` and `Chart(data: "stdin")` refer to files literally
named `input` and `stdin`.

If source is read from stdin, relative paths resolve against the current working directory.

The canonical command for CSV data from stdin is:

```bash
cat data.csv | algraf render chart.ag --data -
```

The canonical PDL-to-Algraf pipe uses Arrow IPC stream bytes on standard input:

```bash
pdl run prep.pdl --stdout-format arrow-stream | algraf render chart.ag --data - --data-format arrow-stream --output chart.svg
```

When `--data -` is used, it overrides the chart's `data` argument for the render
command and supplies caller-provided bytes from standard input.

The recommended source pattern for piped data is `Chart(data: input)`.

When `Chart(data: input)` or `Chart(data: stdin)` is used with no explicit
format override, the CLI and embedded facade MUST inspect caller-provided bytes
for supported binary stream magic before falling back to CSV. The CLI
`--data-format <csv|tsv|json|ndjson|geojson|topojson|parquet|arrow-stream>`
option MUST select the stream format for `--data -`, `Chart(data: input)`, and
`Chart(data: stdin)`. The alias `arrow` MAY be accepted for `arrow-stream`.
The same option also overrides extension inference for a primary
`--data <path>` override. Path-backed chart declarations continue to select
format by source syntax or file extension.

`Chart` MUST include `data` in version 0.1 even when the CLI supplies `--data`.

The CLI `--data` option is an override of the source declaration, not a replacement for it.

Version 0.1 MUST NOT support reading both Algraf source and CSV data from the same standard input stream in a single command.

If the source path is `-`, then `--data -` MUST produce a CLI usage error.

The CLI MUST offer a way to override base directory.

Suggested CLI:

```bash
algraf render chart.ag --base-dir examples --output chart.svg
```

Standard input examples:

```bash
cat chart.ag | algraf render - --data data.csv --output chart.svg
```

```bash
cat data.csv | algraf render chart.ag --data - --output chart.svg
```

Source syntax example:

```ag
Chart(data: input) {
    Space(time * value) {
        Line(stroke: series)
    }
}
```

### 10.2 CSV Parsing

CSV parser MUST support headers.

CSV parser MUST reject headerless CSV by default.

CSV parser SHOULD support configurable delimiter later.

CSV parser SHOULD support quoted fields.

CSV parser SHOULD support escaped quotes.

CSV parser SHOULD support UTF-8.

CSV parser SHOULD report row and column numbers for malformed CSV.

CSV parser SHOULD preserve original string values where type inference fails.

#### 10.2.1 Format Selection

The data source format MUST be selected by the path's file extension, matched
case-insensitively:

| Extension          | Format |
| ------------------ | ------ |
| `.csv`             | CSV    |
| `.tsv`, `.tab`     | TSV    |
| `.json`            | JSON   |
| `.ndjson`, `.jsonl`| NDJSON |
| `.geojson`         | GeoJSON |
| `.topojson`        | TopoJSON |
| `.shp`             | Shapefile |
| `.parquet`, `.parq`| Parquet |

An unrecognized or absent extension MUST be treated as CSV. Caller-provided
bytes (`Chart(data: input)`, the `stdin` compatibility alias, or `--data -`) use
explicit `--data-format` first; without it, the driver MUST sniff Arrow IPC
stream and Parquet magic bytes, SHOULD reject sniffed Arrow IPC file bytes with
a deterministic unsupported-format diagnostic, and MUST then fall back to CSV.
Sniffing MUST preserve all consumed bytes before handing the stream to the
selected decoder. JSON and NDJSON caller input require explicit `--data-format`
in version 0.57; text sniffing for those formats is deferred.

#### 10.2.2 TSV

TSV is delimited data with a tab (`\t`) field separator. All other CSV parsing
rules (required header row, quoted fields, missing-token handling, type
inference) apply unchanged. TSV MUST produce the same dataframe shape as the
equivalent CSV.

#### 10.2.3 JSON and NDJSON

A JSON source MUST be a top-level array of row objects, e.g.
`[{"a": 1, "b": "x"}, {"a": 2, "b": "y"}]`. A top-level value that is not an
array is `E1010`; an array element that is not an object is `E1010`; input that
is not valid JSON is `E1009`.

An NDJSON source is one JSON row object per line. Blank lines MUST be skipped. A
line that is not valid JSON is `E1009`; a line that is valid JSON but not an
object is `E1010`.

Columns MUST be discovered in first-seen key order across rows; a key absent
from a row is a missing cell. This ordering is deterministic (spec §18.12).

Each JSON value MUST be rendered to its canonical text and run through the same
type-inference pipeline as CSV (spec §10.3): `null` becomes a missing cell;
booleans become `true`/`false`; numbers and strings become their textual form;
nested arrays and objects serialize to compact JSON and infer as strings.
Consequently JSON does NOT preserve a distinction between the number `1` and the
string `"1"` — both infer as an integer, exactly as in CSV. This keeps schema and
type inference identical across formats for equivalent data.

### 10.3 Schema Inference

The schema resolver reads enough data to infer column names and basic types.

For LSP completion, reading only headers is sufficient.

For LSP hover, inferred types and sample values SHOULD be shown when the sampled
schema is available.

LSP type inference from sampled rows is provisional.

For SQL sources, schema inference MUST execute the declared read-only query and
inspect at most the requested sample size of result rows. A sample size of `N`
MUST NOT step more than `N` rows, though preparing the SQLite statement is
allowed so result-column names are available.

For Parquet sources, schema inference MUST use Parquet/Arrow metadata when
available and MUST NOT decode every data row just to list column names and basic
types. Example values MAY be empty for metadata-only schema results.

The LSP MUST label sampled types as provisional in internal analysis state.

The LSP SHOULD avoid hard error diagnostics that depend only on provisional sampled types.

The LSP MAY emit hints or warnings for likely type mismatches from sampled types.

For CLI render, full type inference is required before scale training.

CLI render type inference is authoritative.

CLI render MUST use a deterministic policy for values that disagree with the inferred column type.

Recommended type inference order:

boolean

integer

float

temporal

categorical string

empty or null

If a column has mixed numeric and string values, infer `Mixed` and prefer categorical unless a continuous scale is required.

Version 0.75.0: a column whose non-missing values would otherwise infer
`Mixed` from only temporal and string cells MUST infer `Temporal` when the
temporal cells form the majority and the unparseable cells are at most 10% of
the non-missing values. The unparseable cells become missing and the loader
MUST emit one aggregated data warning naming up to three offending values.
Mixtures involving numeric or boolean cells keep their existing
classification; columns past the 10% allowance stay `Mixed` so genuinely
messy data is not silently reclassified. Blank cells and recognized missing
tokens never count against temporal inference.

If a column is selected for a continuous or temporal scale and contains late non-empty values that cannot be parsed as the selected type, the renderer MUST treat those values as missing, preserve the selected scale type, and emit one aggregated warning.

The renderer MUST NOT dynamically recast a trained continuous or temporal column to categorical after scale training begins.

The renderer MUST NOT invalidate already-trained scales because of late invalid values.

Recognized missing tokens SHOULD include the empty string, `NA`, `N/A`, `NaN`, `null`, and `NULL`.

Recognized missing tokens in otherwise numeric or temporal columns SHOULD be treated as missing rather than causing `Mixed`.

Empty values SHOULD be represented as missing.

Missing values SHOULD be skipped by geometries unless the geometry has explicit missing-value behavior.

Temporal inference is required in version 0.1.

Version 0.1 temporal inference MUST recognize RFC3339 timestamps.

Version 0.1 temporal inference MUST recognize ISO dates in `YYYY-MM-DD` form.

Version 0.1 temporal inference MUST recognize ISO datetimes without time zone in `YYYY-MM-DDTHH:MM:SS` form.

Version 0.1 temporal inference SHOULD recognize ISO datetimes with a space separator in `YYYY-MM-DD HH:MM:SS` form.

Version 0.20.0 temporal inference MUST also recognize strict minute-precision
naive datetimes in `YYYY-MM-DD HH:MM` form.

Version 0.28.0 temporal inference MUST additionally recognize the following
unambiguous automatic forms:

- ISO-like datetimes `YYYY-MM-DDTHH:MM`, `YYYY-MM-DDTHH:MM:SS`,
  `YYYY-MM-DDTHH:MM:SS.sss`, and the same space-separated forms;
- RFC3339-compatible offset forms with `Z` or `+/-HH:MM`;
- dates `YYYY/MM/DD` and `YYYYMMDD`;
- RFC2822 timestamps such as `Wed, 27 May 2026 14:30:00 -0500`;
- fixed-English month forms such as `May 27, 2026`, `27 May 2026`, and those
  forms with 24-hour time.

Automatic inference MUST NOT guess ambiguous localized date orders such as
`01/02/2026`, `02-01-2026`, two-digit years, bare years, bare year-month values,
or time-only values.

Version 0.28.0 MUST support chart-body explicit temporal parse declarations:

```ag
Parse(column: started_at, as: "datetime", format: "%m/%d/%Y %I:%M %p", timezone: "UTC")
Parse(column: settled_on, as: "date", format: "%d/%m/%Y")
Parse(column: epoch_ms, as: "datetime", unit: "milliseconds")
Parse(table: trades, column: executed_at, as: "datetime", formats: ["%FT%T%:z", "%F %T"])
```

`column:` is a bare column identifier. `table:` is an optional bare chart-scoped
`Table` name; omitted `table:` targets the primary data source. `as:` accepts
`"date"` or `"datetime"`. `format:` and `formats:` accept chrono/strftime-style
patterns and are mutually exclusive with each other and with `unit:`. `unit:`
accepts `"seconds"`, `"milliseconds"`, `"microseconds"`, and `"nanoseconds"`.
`timezone:` applies to naive datetime parses and MUST support `"UTC"` and fixed
offsets such as `"+02:00"` or `"-05:30"`.

Version 0.31.0 MUST also support, on `timezone:`, named IANA zones such as
`"America/Chicago"`. An IANA zone interprets a *naive* declared datetime (one
whose selected pattern produces no offset) at the specific local instant,
applying that zone's rules including daylight saving, and resolves it to a
UTC-equivalent instant. Storage stays UTC-equivalent microseconds and no
DST-aware scale arithmetic is introduced (§16.4). A local time that is ambiguous
or does not exist (a DST transition) fails to parse for that cell. An
unrecognized zone name is `E1014`.

Version 0.31.0 MUST support a time-only `format:` (e.g. `"%H:%M"`) when an
`anchor:` argument supplies a date (a string parsed by the §10.3 automatic
rules); each time is combined with the anchor date, interpreted in the declared
`timezone:`. Without an anchor, a time-only format leaves cells unparsed.
Automatic inference of time-only columns stays rejected (a temporal scale needs a
date anchor). An invalid `anchor:` date is `E1014`.

Version 0.31.0 MUST support `onError:` on `Parse(...)` with `"warn"` (default),
`"error"`, or `"missing"`. `"warn"` keeps the aggregated-warning behavior below;
`"missing"` coerces failures to missing without a warning; `"error"` turns any
per-column parse failure into a blocking diagnostic (`E1019`). An invalid
`onError:` value is `E1014`.

Declared temporal columns MUST remain temporal even when some non-missing cells
fail to parse. Failed cells become missing values and, under the default
`onError: "warn"`, MUST produce an aggregated data warning with the column name
and failure count.

Temporal inference MUST distinguish date-only columns from datetime columns where all non-missing values are date-only.

If a column mixes date-only values and datetime values, version 0.1 MUST infer a datetime column and lift date-only values to midnight at `00:00:00`.

Naive datetime values without an offset MUST be interpreted as timezone-free Gregorian timestamps.

Naive datetime values MUST NOT be interpreted in the user's local timezone.

RFC3339 datetime values with offsets MUST be converted to UTC instants for ordering and scale mapping.

If a column mixes naive datetime values and offset-aware RFC3339 datetime values, version 0.1 MUST convert offset-aware values to UTC instants and treat naive values as UTC-equivalent instants for scale mapping.

The analyzer SHOULD emit a warning when a column mixes naive and offset-aware datetime values.

### 10.4 Column Definition

Recommended Rust structure:

```rust
pub struct ColumnDef {
    pub name: String,
    pub dtype: DataType,
    pub nullable: bool,
    pub examples: Vec<String>,
}
```

Recommended data type enum:

```rust
pub enum DataType {
    Boolean,
    Integer,
    Float,
    Temporal,
    String,
    Mixed,
    Unknown,
}
```

### 10.5 Data Frame

Version 0.1 MUST use a homegrown columnar dataframe.

The dataframe implementation MUST be internal to the runtime.

Parser, LSP, semantic analysis, and rendering APIs MUST NOT expose concrete dataframe internals.

The dataframe API MUST be designed so a future optional Polars backend can implement the same access traits without changing the language syntax or renderer interfaces.

Recommended initial structure:

```rust
pub struct DataFrame {
    pub columns: Vec<Series>,
    pub name_to_index: IndexMap<String, usize>,
}
```

The `columns` vector is the canonical column order.

`name_to_index` MUST preserve deterministic iteration order if it is ever iterated.

Recommended row view representation:

```rust
pub struct RowView<'a> {
    pub frame: &'a DataFrame,
    pub row_index: usize,
}
```

Row-oriented iteration MAY be exposed as views.

Full row maps SHOULD NOT be the primary storage format.

Future Polars integration:

Polars MAY be added behind an optional feature after version 0.1.

Polars MUST NOT become required for the core parser, LSP, or SVG renderer.

Polars-backed execution MUST preserve diagnostics, category ordering, missing-value behavior, and SVG determinism.

As of version 0.43, scalar cell access remains available for compatibility and
final mark property resolution, but scale training and built-in stats SHOULD use
pre-resolved column views or table scans instead of repeated name lookup plus
dynamic scalar reads.

Suggested trait boundary:

```rust
pub trait Table {
    fn schema(&self) -> &[ColumnDef];
    fn row_count(&self) -> usize;
    fn value(&self, column: &str, row: usize) -> Option<DataValueRef<'_>>;
    fn column(&self, column: &str) -> Option<ColumnView<'_>>;
    fn scan(&self, columns: &[&str], visitor: &mut dyn TableScan);
}
```

`Table::value` returns `None` only when the column or row is absent. A present but
missing cell MUST return `Some(DataValueRef::Null)`. `ColumnView::get` MUST
preserve the same distinction. `ColumnView` variants SHOULD expose dense typed
storage for booleans, integers, floats, and temporals, plus borrowed string and
geometry slices for existing owned values.

Nullable scalar columns MUST use a dense value buffer plus a validity bitmap:

```rust
pub struct NullableColumn<T> {
    values: Vec<T>,
    validity: NullBitmap,
}
```

The validity bitmap is the source of missingness. The stored sentinel value used
for a missing scalar cell MUST NOT be observable through `value`, `ColumnView`, or
`Table::scan`. String and geometry columns MAY keep `Vec<Option<T>>` storage.

As of version 0.19, concrete `DataFrame` ownership is limited to the data
crate, driver load results, CLI/LSP preview handoff, and renderer materialized
stat outputs. Parser, syntax, semantic analysis, and ordinary LSP analysis MUST
continue to use schemas and IR rather than dataframe internals. Renderer
planning reads loaded data through the `Table` trait.

### 10.6 Derived Tables

Derived tables are produced by `Derive` declarations.

Derived tables live in memory during one render or LSP analysis session.

Derived tables MUST use the same dataframe abstraction as primary CSV data.

Derived tables MUST have schemas.

Derived table schemas MUST be available to semantic analysis after the stat declaration is validated.

Derived table schemas MUST be available to LSP completions inside
`Space(..., data: derived_name)` blocks and after `Derive name from `.

Derived table schemas MUST be available to LSP hover on the `Derive` table name
and on `Derive name from derived_name` or `data: derived_name` references. The
hover MUST identify the producer stat and list the output columns and types
when semantic analysis has a valid schema. If analysis is incomplete or invalid,
hover MUST NOT invent an output schema.

Derived tables MAY be lazily computed by the renderer.

Derived table schemas SHOULD be computed without running expensive full-data transforms where possible.

Schema-only stat planning lives in semantic planning helpers. The built-in
schemas for `Bin`, `Smooth`, `StepVertices`, `JitterPoints`,
`VectorEndpoints`, `CurveSample`, `Bin2D`, `HexBin`, `Density`, and `Count` MUST
be derivable from typed input frames and options without reading row values.
Full stat execution remains a render/runtime responsibility.

Derived table names are chart-scoped.

Derived table names MUST be unique within a chart.

Derived table names MUST NOT shadow the primary data source.

Derived table columns are referenced like ordinary columns inside spaces bound to that derived table.

Since version 0.47.0, a derived declaration MAY read from a named table or
another derived table in the same chart by spelling `from name` before `=`.

If `from name` is omitted, the derived declaration MUST read from the chart's
primary table.

The stat input expressions inside a `Derive` MUST resolve only against the
declared input table. Derived-table columns MUST NOT be injected into chart
scope or into another `Derive` merely because their names match.

The analyzer MUST build a dependency graph between derived declarations from
explicit `from` references.

The graph resolution order MUST be deterministic.

If a cycle exists between derived declarations, the analyzer MUST emit `E1501`
and MUST NOT loop.

If a derived declaration references a column absent from its active input table,
the analyzer MUST emit the ordinary unknown-column diagnostic `E1101`.

Example:

```ag
Chart(data: "distribution.csv") {
    Derive bins = Bin(value, bins: 25)
    Derive trend from bins = Smooth(bin_center, count, method: "lm")

    Space(bin_start * count, data: bins) {
        Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)
    }
}
```

### 10.7 Data Values

Recommended enum:

```rust
pub enum DataValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Temporal(DateTimeValue),
    String(String),
    Geometry(geo_types::Geometry<f64>), // since 0.8 (spec §10.11)
}
```

`DataValue` MUST support deterministic ordering for categorical domains.

Continuous comparisons MUST handle NaN carefully.

NaN SHOULD be treated as missing.

`DataValue::Geometry` (and the borrowed `DataValueRef::Geometry`) carry a spatial
geometry value (spec §10.11). Geometry is not orderable: all geometry values
compare equal, so a geometry column never forms a continuous or categorical
domain.

### 10.8 Source Security

The renderer MUST NOT fetch network resources by default.

The LSP MUST NOT fetch network resources by default.

The CLI MUST restrict data reads to explicit paths.

The CLI SHOULD provide an option to allow network sources if implemented later.

Driver-level data loading MUST keep a synchronous, injectable I/O provider
that can open resolved paths as readers, read resolved path bytes, read stdin
bytes, inspect path metadata, read shapefile sidecars, and open local SQLite and
Parquet database/file paths. The default provider uses the operating system.
Native path-backed formats that support streaming SHOULD use `open_path` rather
than `read_path` to avoid whole-file byte buffers. Embedded and WASM providers MAY
continue to supply bytes where host APIs only expose bytes. The synchronous
provider MUST NOT add network access, environment-variable access, command
execution, async operations, or caching policy.

Version 0.35 removes the unused async loading adapter. A future async driver
boundary MAY be reintroduced, but it MUST mirror the synchronous local-source
surface and MUST NOT add new source kinds, network access, environment-variable
access, command execution, or cache policy.

SQLite sources are local file sources. They MUST open databases read-only, MUST
reject write statements, MUST reject multiple SQL statements, MUST NOT register
user-defined SQL functions, and MUST NOT read credentials or environment
variables. URL-valued sources, remote SQL connections, `env("VAR")`, and
command sources are not enabled by default and remain gated/deferred.

The LSP SHOULD avoid reading very large files on the hot path.

The LSP SHOULD cap schema preview read size.

### 10.9 Schema Cache

A schema cache maps a data source to its resolved schema. Since version 0.16
the cache is owned by the `driver` crate so the LSP, tests, and future callers
share one keying and invalidation policy; the LSP holds a concrete instance
(spec §21.3). The cache stores schemas and load errors only — never full data
frames.

The cache key (`DataSourceKey`) MUST include the resolved source path and the
explicit source-constructor format policy, so the same path read as inferred CSV
and as `GeoJson(...)` occupy distinct slots. The path SHOULD be normalized
(lexically, without consulting the filesystem) so equivalent spellings share one
slot.

For SQLite sources, `DataSourceKey` MUST also include the SQL query string. The
same database path queried with two different `SELECT` statements MUST occupy
distinct schema-cache slots.

Cache validity is decided by a separate source fingerprint
(`SourceFingerprint`), which SHOULD include:

file size

last modified timestamp

optional content hash

The cache MUST invalidate when the source file changes: a cached schema is
reused only when a freshly observed fingerprint equals the one stored when the
entry was created. Invalidation MUST be conservative — when metadata is
unavailable or ambiguous (for example a missing or unreadable file), the cache
MUST NOT serve the entry and the source is reloaded.

The cache MUST keep load outcomes distinguishable: a missing file, an unreadable
file, malformed data, and a successful schema are stored as distinct results
(cached errors carry their stable diagnostic code and message; spec §23.4).

The cache MUST distinguish parse errors from missing files.

The cache implementation MUST be injectable: callers that want fresh one-shot
loads (for example CLI render) MAY use a no-op cache, and the LSP uses an
in-memory, fingerprint-validated cache.

Completion requests SHOULD return cached schemas if available.

Completion requests SHOULD NOT block for full data loading.

Hover source previews SHOULD reuse the same cached, bounded schema-sampling
path. A hover MAY include a few raw source rows for compact CSV/TSV previews,
but the sample MUST be bounded, labeled provisional, and omitted when the source
is unavailable, unsupported for row preview, or too large for the editor path.
Since version 0.48.0, this same preview behavior MUST apply to the path string
inside recognized source constructors such as `Parquet("file.parquet")` and
`TopoJson("map.topojson", object: "name")`, for both chart primary sources and
named table sources. Constructor-backed previews MAY omit raw rows when the
format's editor path exposes schema metadata but no bounded row sample.

### 10.10 Named Tables

A chart or document MAY declare named data tables with `Table name = <source>`
(spec §7.4.1). Chart-scope tables are visible only to their chart;
document-scope tables are visible to every chart in the file. Each named table
is an independent source, loaded the same way as `Chart(data:)` — including
format selection by extension (spec §10.2), explicit source constructors
(`GeoJson`, `Shapefile`, `TopoJson`, `Parquet`, and gated `Sqlite`), path
resolution, `--base-dir`, and source security in §10.8, all unchanged.

A `Space` binds to a named table by bare identifier in its `data:` argument,
exactly as it binds to a derived table; the space's algebra and geometry
properties resolve their columns against that table's schema. Named tables stay
behind the dataframe boundary (§10.5): the parser, semantics, LSP, and renderer
gain no source-specific knowledge beyond the table's name and resolved schema.
Since version 0.48.0, LSP hover on a named table declaration name,
`Chart(data: name)`, `Space(..., data: name)`, or
`Derive output from name = ...` reference MUST identify the named table and show
its sampled schema when one is available. The hover SHOULD include the source
spelling and MAY include bounded sample rows under the same limits as source
preview hover (§10.9). If schema analysis is incomplete or invalid, the hover
MUST NOT invent a schema.

`Chart(data: name)` binds the chart's primary data source to a visible named
table. A missing `Chart(data:)` binds to `Table main = ...` when such a table is
visible. These primary-table spellings do not change `Space(data:)`: a space
with no `data:` reads the primary table, while `Space(..., data: name)` reads
the named table directly.

Named tables are independent overlays: version 0.6 defines no join or relational
operation between tables. When two compatible spaces overlay but back onto
different tables, their shared position scales MUST be unioned across all
contributing tables so the layers align (spec §17.5).

Diagnostics: a duplicate `Table` name is `E1105`; a name that conflicts with the
derived table is `E1108`; a `Table` source file that cannot be
found is `E1106`, and one that cannot be read is `E1107`. (`E1103` still covers
an unknown identifier passed to a space's `data:`.)

### 10.11 Geometry Values (Simple Features)

Since version 0.8, a column MAY hold spatial **geometry** (the Simple Features
model). Geometry is its own data type, distinct from continuous and categorical:

- `DataType::Geometry` MUST report `is_continuous() == false` and
  `is_categorical() == false`. A geometry column trains no position or aesthetic
  scale — only the spatial scale (spec §16.15).
- Geometry is stored columnar behind the dataframe boundary
  (`Column::Geometry(Vec<Option<Geometry<f64>>>)` wrapping
  `geo_types::Geometry<f64>`), one feature per row. The `Table` trait (§10.5) is
  unchanged, so parser, semantics, LSP, and render see geometry only through
  `DataValueRef::Geometry`.

Three geometry **source constructors** decode to this representation; all are
pure-Rust and offline, preserving the single-binary (spec §2) and
no-network-by-default (§10.8) guarantees. They differ only in ingestion — all
produce a `geom` geometry column plus scalar attribute columns — so they share
the spatial scale, projection, and `Geo` render path identically:

- **`GeoJson("path.geojson")`** parses a `FeatureCollection` (a lone `Feature` is
  also accepted): one row per feature in file order, each `properties` key
  becomes a scalar column via the shared type-inference pipeline (§10.3), and
  each feature's `geometry` becomes the `geom` column.
- **`Shapefile("path.shp")`** reads the `.shp` for geometry and the sidecar
  `.dbf` for attributes (the `.dbf`/`.shx`/`.prj`/`.cpg` sidecars resolve next
  to the named `.shp`), one row per record in file order. A shapefile's polygon
  type normalizes to `MultiPolygon`. Attributes run through the same inference
  pipeline, so a shapefile and an equivalent GeoJSON produce the same dataframe
  shape.
- **`TopoJson("path.topojson", object: "name")`** parses a TopoJSON `Topology`.
  TopoJSON shares boundaries as **arcs** referenced by index; the loader stitches
  arcs (negative indices reference an arc reversed; consecutive arcs share an
  endpoint), applies the optional quantization `transform` (`q = position ·
  scale + translate`), and produces the same `geom` + attribute columns as
  GeoJSON. The optional `object:` argument names the entry in the topology's
  `objects` map; it MAY be omitted when the topology defines exactly one object.
  A selected `GeometryCollection` object yields one row per member geometry (like
  a `FeatureCollection`); any other object is a single feature. The `.topojson`
  file extension also selects this loader, decoding the topology's sole object. A
  named object absent from the topology, an ambiguous omitted `object:` when the
  topology defines several, malformed topology, and unsupported geometry types
  are `E1805`.

The implementation MAY load shapefiles from a resolved in-memory sidecar bundle
instead of a filesystem path. When using the default operating-system provider,
path-relative sidecar behavior MUST remain the same as path-backed shapefile
loading.

Recognized source constructors MUST be described by a single shared
constructor-metadata table in the syntax layer. The metadata records the
constructor's authoritative name, source kind, argument rule, documentation, and
editor completion text. The set of
recognized constructors is closed: a name absent from the table is not a
constructor (and falls through to the usual invalid-source diagnostics). The
driver maps a constructor's format policy into a data-loader format at one
boundary; syntax MUST NOT depend on the data crate's runtime format type, and
runtime strings MUST NOT be promoted to constructors outside the table. Adding a
future constructor means adding a table entry, not widening accepted syntax in
scattered matches.

Path resolution, `--base-dir`, and source security (§10.8) apply unchanged. A
missing source file is `E1106`; an unreadable one is `E1107`. A malformed
document or unsupported geometry type is `E1805`.

### 10.12 SQLite Sources

Version 0.21 supports local SQLite data sources behind the `sql` feature gate:

```ag
Algraf(version: "0.21", features: ["sql"])

Chart(data: Sqlite("sales.db", "SELECT region, revenue FROM sales ORDER BY region")) {
    Space(region * revenue) {
        Bar(stat: "identity")
    }
}
```

`Sqlite(path, query)` MUST take exactly two positional string literals. Keyword
arguments, missing arguments, or non-string arguments are invalid source
expressions (`E1004`). The constructor MAY be used anywhere `SourceExpr` is
accepted, including `Chart(data:)` and `Table name = ...`.

The database path resolves with the same source-relative and `--base-dir` rules
as file sources. `--data <path>` remains a primary-source override and, when
present, replaces the chart's declared `Sqlite(...)` source with the ordinary
extension-selected file source named by `--data`.

SQLite execution MUST be local and read-only. The loader MUST open the database
read-only, MUST reject statements that SQLite reports as writable, and MUST
reject SQL that does not start with `SELECT` or `WITH`. A SQLite source query
MUST be exactly one statement; multiple statements are `E1012`.

SQLite source queries MUST include a top-level `ORDER BY` clause. Algraf does
not impose hidden row ordering because arbitrary SQL result ordering is backend
dependent. Missing top-level `ORDER BY` is `E1012`. The `ORDER BY` requirement
applies to both full render loads and schema-only LSP samples.

SQLite result columns MUST appear in SQLite result-column order, and duplicate
result column names are `E1008`. Values with SQLite storage classes `NULL`,
`INTEGER`, `REAL`, and `TEXT` are converted to the same raw textual inference
pipeline used by CSV/JSON (§10.3). `BLOB` values are unsupported and MUST emit
`E1013`. SQLite text that is not valid UTF-8 is `E1011`.

Malformed SQL, missing tables, missing columns, and SQLite execution failures
are `E1011`. Error messages SHOULD include the resolved database path and the
SQLite error message, but MUST NOT include environment variables, credentials,
or unrelated local process state.

Schema inference for SQLite MUST be bounded by the caller's sample size. CLI
render MUST fully load the query result before rendering; LSP schema reads MUST
sample and MUST remain cancellable/cooperative at the request boundary where
practical.

### 10.13 Parquet Sources

Version 0.43 supports native CLI Parquet sources:

```ag
Chart(data: Parquet("events.parquet")) {
    Derive bins = Bin(value, bins: 32)
    Space(bin_start * count, data: bins) {
        Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)
    }
}
```

`Parquet(path)` MUST take exactly one positional string literal. Keyword
arguments, missing arguments, or non-string arguments are invalid source
expressions (`E1004`). The constructor MAY be used anywhere `SourceExpr` is
accepted, including `Chart(data:)` and `Table name = ...`. Extension inference
from `.parquet` and `.parq` MUST select the same loader.

The native CLI build MUST include Parquet support. Heavy Arrow/Parquet
dependencies MAY be isolated behind a Cargo feature for libraries, WASM, or
minimal editor builds, but the released native CLI path MUST support schema
loading and rendering from Parquet.

Parquet schema loading MUST map Arrow booleans, signed and unsigned integers,
floats, UTF-8 strings, dates, and timestamps into Algraf `DataType` values.
Unsupported Parquet/Arrow physical or logical types MUST fail through `E1020`
rather than panic. Unsigned integer values too large for `i64` MAY be saturated or
rejected, but the chosen behavior MUST be deterministic.

Parquet `null` values MUST become Algraf missing cells. Date and timestamp
columns MUST become `Temporal` columns using UTC-equivalent instants. String and
numeric category formatting MUST follow the same deterministic domain rules as
CSV/JSON.

The Parquet reader SHOULD support column projection by requested top-level column
names. When projection is requested, omitted columns MUST be absent from the
materialized dataframe and `Table::value` for an omitted column returns `None`.
Unknown projected column names MUST fail deterministically. Full chart rendering
MAY still materialize more columns than strictly referenced until broader
projection planning is promoted, but the backend API MUST expose a projection
surface.

Advanced row-group pruning, predicate pushdown, remote object stores, and
browser/WASM Parquet loading are deferred.

### 10.14 Arrow IPC Stream Caller Data

Version 0.57 supports Apache Arrow IPC stream input for caller-provided primary
data. The promoted format name is `arrow-stream`.

`arrow-stream` is a data-loader format, not new Algraf source syntax. It is
selected by `--data-format arrow-stream`, by the optional `arrow` alias, by an
embedded host's explicit data-format override, or by caller-input sniffing. It
MUST work for `--data -`, `Chart(data: input)`, `Chart(data: stdin)`, and a
primary `--data <path>` override paired with `--data-format arrow-stream`.
Path-backed source extension inference does not select Arrow IPC streams in
version 0.57; a chart path such as `Chart(data: "events.arrow")` continues to
use the extension policy in §10.2.1 unless a CLI/host override supplies the
format.

Arrow IPC stream loading MUST stay behind the `algraf-data` facade. Parser,
semantics, renderer, editor-services, LSP, and source syntax MUST NOT depend on
Arrow IPC reader types. The native CLI build MUST include Arrow IPC stream
support. Libraries and WASM builds MAY gate the reader behind a Cargo feature;
when disabled, an explicit `arrow-stream` load MUST fail through a registered
diagnostic rather than panic.

Arrow stream schema loading MUST map Arrow booleans, signed and unsigned
integers, floats, UTF-8 strings, dates, and timestamps into Algraf `DataType`
values using the same policy as Parquet (§10.13). Arrow stream `null` values
MUST become Algraf missing cells. Date and timestamp columns MUST become
`Temporal` columns using UTC-equivalent instants. Unsupported Arrow physical or
logical types MUST fail through `E1021` rather than panic. Unsigned integer
values too large for `i64` MAY be saturated or rejected, but the chosen behavior
MUST be deterministic.

Malformed Arrow IPC streams, invalid stream schemas, truncated batches, and
Arrow IPC reader errors MUST fail through `E1021`. Sniffed Arrow IPC file bytes
MUST fail through `E1022` in version 0.57 because the promoted interop boundary
is the stream format. Sniffed Parquet bytes MAY be loaded when Parquet support
is enabled; otherwise they fail through the existing Parquet diagnostic path.

The Arrow IPC stream reader MAY materialize the full stream before rendering.
Streaming render, Arrow IPC file parity, Arrow stream output, and browser/WASM
Arrow stream decoding are deferred.

### 10.15 Runtime Cache Policy

Algraf distinguishes four cache kinds:

- A **schema cache** stores schemas and stable load errors. It is implemented in
  the driver and keyed by `DataSourceKey` plus `SourceFingerprint` (§10.9).
- A **full-frame cache** would store loaded table data. It is deferred in
  version 0.19 because no current caller reuses full frames without changing
  one-shot CLI behavior or increasing editor memory pressure.
- A **render-result cache** would store SVG or a planned render scene. It is
  deferred because render output depends on source text, data fingerprints,
  dimensions, theme, and output options.
- A **persistent cache** would survive process restarts. It is deferred until a
  storage location, format version, invalidation policy, and privacy policy are
  specified.

If full-frame caching is promoted later, its keys SHOULD reuse
`DataSourceKey`/`SourceFingerprint`, and cached frames MUST preserve data
warnings, category ordering, missing-value behavior, and deterministic render
output. CLI one-shot commands MUST continue to load fresh data by default unless
a cache is explicitly proven behavior-neutral.

## 11. AST Model

### 11.1 AST Requirements

Every AST node MUST have a kind.

Every AST node MUST have a source span.

Every AST node SHOULD retain child spans.

Every AST node SHOULD be serializable for debug output.

Every AST node SHOULD be cloneable or reference-counted for LSP use.

The rowan CST is the canonical syntax tree.

Typed AST nodes SHOULD be lightweight views over rowan syntax nodes, following the rust-analyzer pattern.

Typed AST views SHOULD expose methods that walk CST children rather than storing separate owned child vectors.

The owned structs shown in this document are structural sketches of node shape, not a required in-memory representation.

Implementations MUST NOT maintain two independently mutable syntax trees.

AST views MAY omit comments.

CST nodes MUST retain comments and whitespace for formatting.

### 11.2 Span Type

Recommended span type:

```rust
pub type ByteOffset = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub start: ByteOffset,
    pub end: ByteOffset,
}
```

Span ranges are half-open.

`start` is inclusive.

`end` is exclusive.

Zero-length spans are allowed for inserted recovery nodes.

### 11.3 Spanned Wrapper

Recommended wrapper:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}
```

### 11.4 Root Node

Recommended root:

```rust
pub struct Program {
    pub chart: Option<Spanned<ChartBlock>>,
    pub errors: Vec<ParseDiagnostic>,
    pub span: Span,
}
```

`chart` may be absent when parsing severely malformed files.

`errors` contains parse diagnostics only.

Semantic diagnostics live separately.

### 11.5 Chart Node

The following Rust structs describe the logical shape of typed nodes.

A rowan-based implementation SHOULD expose equivalent typed accessors over CST nodes rather than allocating this exact owned tree.

```rust
pub struct ChartBlock {
    pub args: Vec<Spanned<Argument>>,
    pub body: Vec<Spanned<ChartItem>>,
}
```

```rust
pub enum ChartItem {
    Space(Spanned<SpaceBlock>),
    Derive(Spanned<DeriveDecl>),
    Glyph(Spanned<GlyphDecl>),
    Scale(Spanned<Call>),
    Guide(Spanned<Call>),
    Theme(Spanned<Call>),
    Layout(Spanned<Call>),
    Error(ErrorNode),
}
```

```rust
pub struct GlyphDecl {
    pub name: Spanned<String>,
    pub args: Vec<Spanned<Argument>>,
    pub body: Vec<Spanned<GlyphItem>>,
}
```

### 11.6 Space Node

```rust
pub struct SpaceBlock {
    pub frame: Spanned<AlgebraExpr>,
    pub args: Vec<Spanned<Argument>>,
    pub body: Vec<Spanned<SpaceItem>>,
}
```

```rust
pub enum SpaceItem {
    Geometry(Spanned<GeometryCall>),
    Scale(Spanned<Call>),
    Guide(Spanned<Call>),
    Theme(Spanned<Call>),
    Error(ErrorNode),
}
```

A glyph mark is a `GeometryCall` whose name resolves to a chart-scoped `Glyph`
rather than a built-in geometry (§13.8); there is no distinct space-item node.

```rust
pub enum GlyphItem {
    Space(Spanned<SpaceBlock>),
    Let(Spanned<LetDecl>),
    Scale(Spanned<Call>),
    Guide(Spanned<Call>),
    Theme(Spanned<Call>),
    Error(ErrorNode),
}
```

### 11.7 Derive Node

```rust
pub struct DeriveDecl {
    pub name: Spanned<String>,
    pub source: Option<Spanned<String>>,
    pub stat: Spanned<StatCall>,
}
```

```rust
pub struct StatCall {
    pub name: Spanned<String>,
    pub input: Option<Spanned<AlgebraExpr>>,
    pub args: Vec<Spanned<Argument>>,
}
```

### 11.8 Geometry Node

```rust
pub struct GeometryCall {
    pub name: Spanned<String>,
    pub args: Vec<Spanned<Argument>>,
}
```

Geometry call names are case-sensitive.

Built-in geometry names use PascalCase.

Examples:

`Point`

`Line`

`Bar`

`Rect`

`Histogram`

`Smooth`

`Boxplot`

`Violin`

`Ribbon`

`Tile`

### 11.9 Argument Node

```rust
pub struct Argument {
    pub key: Spanned<String>,
    pub value: Spanned<ValueExpr>,
}
```

Argument keys are identifiers.

Duplicate keys are syntax-valid but semantic-invalid.

The analyzer MUST report duplicate keys.

### 11.10 Value Expression Node

```rust
pub enum ValueExpr {
    Algebra(Spanned<AlgebraExpr>),
    Literal(Literal),
    Stdin,
    Array(Vec<Spanned<ValueExpr>>),
    Error(ErrorNode),
}
```

The parser may initially parse identifiers in argument values as algebra expressions.

The parser SHOULD parse bare `input` and `stdin` as `ValueExpr::Stdin` only in
value positions.

The analyzer interprets them by property context.

The analyzer MUST accept `ValueExpr::Stdin` only for `Chart(data: input)` and
the compatibility alias `Chart(data: stdin)`.

Using `input` or `stdin` as a geometry property value MUST produce a semantic
diagnostic unless a future property explicitly allows it.

### 11.11 Algebra Node

```rust
pub enum AlgebraExpr {
    Identifier(Identifier),
    Binary {
        op: AlgebraOp,
        left: Box<Spanned<AlgebraExpr>>,
        right: Box<Spanned<AlgebraExpr>>,
    },
    Paren(Box<Spanned<AlgebraExpr>>),
    Error(ErrorNode),
}
```

```rust
pub struct Identifier {
    pub name: String,
    pub quoted: bool,
}
```

Plain identifiers and quoted column identifiers share the same semantic identifier type.

`quoted: true` means the source used backticks and the `name` field contains the unescaped column name.

`quoted: false` means the source used ordinary identifier syntax.

```rust
pub enum AlgebraOp {
    Cross,
    Nest,
    Blend,
}
```

The AST SHOULD retain `Paren` nodes.

The semantic IR MAY erase `Paren` nodes.

The formatter uses `Paren` nodes and precedence rules.

### 11.12 Literal Node

```rust
pub enum Literal {
    String(String),
    Number(NumberLiteral),
    Bool(bool),
    Null,
}
```

```rust
pub enum NumberLiteral {
    Integer(i64),
    Float(f64),
}
```

The lexer SHOULD retain original number lexeme for formatting.

### 11.13 Error Node

```rust
pub struct ErrorNode {
    pub message: String,
    pub expected: Vec<String>,
    pub found: Option<String>,
}
```

Error nodes MUST have spans.

Inserted error nodes MAY have zero-length spans.

Error nodes SHOULD allow LSP traversal to continue.

## 12. Parser Architecture

### 12.1 Parser Requirements

The parser MUST be resilient.

The parser MUST avoid panics on malformed user input.

The parser MUST advance on errors.

The parser MUST produce diagnostics with spans.

The parser MUST preserve enough structure for completions inside incomplete expressions.

The parser SHOULD use `logos` or an equivalent lexer.

The parser SHOULD use Pratt parsing for algebra expressions.

The parser SHOULD use recursive descent for blocks and calls.

The parser MUST use `rowan` from the start for a lossless concrete syntax tree.

The typed AST SHOULD be derived from the `rowan` CST.

The LSP, formatter, and diagnostics MUST use the lossless CST where preserving comments, whitespace, and incomplete syntax matters.

### 12.2 Lexing

Recommended dependency:

```toml
logos = "0.14"
rowan = "0.15"
```

Tokenization produces a vector:

```rust
Vec<TokenWithSpan>
```

```rust
pub struct TokenWithSpan {
    pub kind: TokenKind,
    pub span: Span,
    pub text: String,
}
```

Whitespace and comments MUST be represented in the `rowan` CST as trivia.

Whitespace and comments MAY be skipped when constructing the typed AST.

The formatter SHOULD use CST trivia where practical and fall back to pretty-printing only for malformed regions.

### 12.3 Token Kind

Recommended token kind:

```rust
pub enum TokenKind {
    Ident(String),
    QuotedIdent(String),
    String(String),
    Number(NumberLiteral),
    True,
    False,
    Null,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Comma,
    Equal,
    Star,
    Slash,
    Plus,
    Comment(String),
    Whitespace,
    Error(String),
    Eof,
}
```

The parser SHOULD use an explicit EOF token.

### 12.4 Parser State

Recommended parser state:

```rust
pub struct Parser {
    tokens: Vec<TokenWithSpan>,
    cursor: usize,
    diagnostics: Vec<ParseDiagnostic>,
}
```

The parser exposes:

`peek()`

`peek_n(n)`

`advance()`

`at(kind)`

`expect(kind)`

`expect_recover(kind, message)`

`synchronize(sync_set)`

### 12.5 Pratt Parser Binding Powers

Binding powers:

`+` blend: left 1, right 2

`*` cross: left 3, right 4

`/` nest: left 5, right 6

The parser accepts `+` so it can build recoverable syntax trees and precise diagnostics.

The semantic analyzer enforces the explicit-parentheses rule for blend.

The parser uses:

```rust
fn parse_algebra(&mut self, min_bp: u8) -> Spanned<AlgebraExpr>
```

The parser parses primary expressions first.

The parser loops while next token is an operator with binding power at least `min_bp`.

The parser recurses on right side with right binding power.

### 12.6 Parse Primary

Primary expression parsing accepts:

identifier

quoted identifier

left parenthesis algebra right parenthesis

error fallback

If parser sees `)` or `}` where a primary is expected, it SHOULD create an inserted error node without consuming the closing token.

If parser sees an unrelated token where primary is expected, it SHOULD consume the token and create an error node.

### 12.7 Parse Chart

Chart parser sequence:

expect `Chart`

expect `(`

parse optional arg list

expect `)`

expect `{`

parse chart items until `}` or EOF

expect `}`

Chart item parser MUST recognize `Derive`.

If `Chart` is missing, parser SHOULD search for first `Chart` token and report skipped tokens.

### 12.8 Parse Space

Space parser sequence:

expect `Space`

expect `(`

parse algebra

parse optional comma-prefixed space arguments

expect `)`

expect `{`

parse space items until `}` or EOF

expect `}`

If closing `)` is missing before `{`, parser SHOULD recover at `{`.

If closing `}` is missing, parser SHOULD synthesize it at EOF and report diagnostic.

### 12.9 Parse Derive

Derive parser sequence:

expect `Derive`

expect derived table name identifier

if `from` is present, expect input table name identifier

expect `=`

parse stat call

The stat call parser sequence:

read stat name identifier

expect `(`

parse optional stat input algebra

parse optional comma-prefixed argument list

expect `)`

If `=` is missing, parser SHOULD recover at the next identifier followed by `(` where practical.

If stat call is missing, parser SHOULD create an error stat node.

### 12.10 Parse Call

Call parser sequence:

read call name identifier

expect `(`

parse optional arg list

expect `)`

No semicolon is used.

Calls are separated by normal token boundaries.

Newlines are not significant.

### 12.11 Parse Argument List

Argument list parser:

parse argument

if comma, continue

if right paren, finish

if EOF, recover

if unexpected token, report and synchronize to comma or right paren

Trailing commas are allowed.

Duplicate keys are permitted syntactically.

### 12.12 Parse Argument

Argument parser:

expect identifier key

expect colon

parse value expression

If colon is missing and next token looks like value, emit diagnostic and continue.

If value is missing, create error value.

### 12.13 Parse Value

Value parser accepts:

string

number

boolean

null

caller-input sentinel

array

algebra expression

Value parser SHOULD prefer literal parsing when current token is literal.

Value parser SHOULD parse bare `input` and `stdin` as the caller-input sentinel
in value positions.

Value parser SHOULD parse other identifiers, quoted identifiers, and parenthesized identifiers as algebra.

### 12.14 Parse Array

Array parser sequence:

expect `[`

parse optional value list

expect `]`

Array parser MUST recover from missing commas.

Array parser SHOULD diagnose mixed trailing tokens.

### 12.15 Parser Diagnostics

Diagnostic fields:

code

severity

message

span

related spans

help text

Parser diagnostic examples:

`E0001 expected Chart block`

`E0002 expected '(' after Chart`

`E0003 expected ')' after argument list`

`E0004 expected algebra expression`

`E0005 expected property value`

`E0006 unterminated string literal`

`E0007 unexpected token in Space block`

`E0016 expected '=' after derived table name`

`E0017 expected stat call after '='`

### 12.16 Error Recovery Strategy

Error recovery MUST be local.

Error recovery MUST avoid deleting large valid regions.

Synchronize chart body on:

`Space`

`Derive`

`Scale`

`Guide`

`Theme`

`Layout`

`}`

Synchronize space body on:

known geometry call names

`Scale`

`Guide`

`Theme`

`}`

Synchronization MUST use hard stops to avoid consuming subsequent valid blocks.

Chart-body recovery MUST stop at `Space`, `Derive`, `Scale`, `Guide`, `Theme`, `Layout`, `Chart`, `}`, or EOF.

Space-body recovery MUST stop at known geometry call names, `Scale`, `Guide`, `Theme`, `Space`, `Derive`, `}`, or EOF.

Argument-list recovery MUST stop at `,`, `)`, `}`, a known chart-body keyword, or a known space-body item starter.

Algebra recovery MUST stop at `)`, `,`, `{`, `}`, `Space`, `Derive`, `Scale`, `Guide`, `Theme`, `Layout`, a known geometry call name, or EOF.

Synchronize argument list on:

`,`

`)`

Synchronize algebra on:

`)`

`,`

`{`

`}`

### 12.17 Partial AST Examples

The parser MUST produce useful partial CST/AST structure for common in-progress edits.

For `Chart(data: "fi`, the parser MUST produce a `Chart` node with a `data` argument whose value is an unterminated string error node.

For `Space(quarter / )`, the parser MUST produce a `Space` node whose frame contains a `Nest` expression with an error right-hand operand.

For `Derive bins = Bin(value, bins: )`, the parser MUST produce a `Derive` node, a `Bin` stat call, and an error value for `bins`.

For a missing closing `}` before a following `Space`, the parser SHOULD close the previous block synthetically and continue parsing the following `Space` as a sibling where possible.

These partial AST shapes are part of the LSP contract and SHOULD have fixtures.

### 12.18 Incremental Parsing

Version 0.1 MAY reparse whole files.

The parser SHOULD be written so incremental parsing can be added later.

The LSP SHOULD debounce parse operations.

The LSP SHOULD parse on document open and change.

The LSP SHOULD avoid parsing on every completion request if a parsed AST is already cached.

## 13. Semantic Analysis

### 13.1 Semantic Analyzer Responsibilities

The semantic analyzer validates AST against language rules.

It resolves data source paths.

It resolves schemas.

It resolves derived table declarations.

It resolves identifiers.

It validates geometry names.

It validates property names.

It validates property types.

It validates algebra support.

It builds semantic IR.

It produces semantic diagnostics.

### 13.2 Semantic IR

The semantic IR SHOULD be separate from AST.

The AST mirrors source.

The IR mirrors executable meaning.

Recommended IR root:

```rust
pub struct ChartIr {
    pub data_source: DataSourceIr,
    pub derived_tables: Vec<DeriveIr>,
    pub width: u32,
    pub height: u32,
    pub spaces: Vec<SpaceIr>,
    pub theme: ThemeIr,
    pub guides: GuideConfig,
    pub scales: ScaleConfig,
}
```

### 13.3 Space IR

```rust
pub struct SpaceIr {
    pub data: SpaceDataRef,
    pub frame: FrameIr,
    pub layers: Vec<SpaceLayerIr>,
    pub geometries: Vec<GeometryIr>,
    pub span: Span,
}
```

```rust
pub enum SpaceDataRef {
    Primary,
    Table(String),
    Derived(String),
}
```

```rust
pub enum SpaceLayerIr {
    Geometry(GeometryIr),
    Glyph(GlyphCallIr),
}
```

`layers` preserves source order for geometry calls and glyph marks. The legacy
`geometries` list is retained for scale training and existing geometry
lowerings, but emission MUST use `layers` when a space contains glyph marks so
marks and child scenes render in the order authored.

```rust
pub struct GlyphDeclIr {
    pub name: String,
    pub data: SpaceDataRef,
    pub key: Vec<GlyphKeyIr>,
    pub scale_policy: GlyphScalePolicyIr,
    pub child_spaces: Vec<SpaceIr>,
    pub span: Span,
}

pub struct GlyphKeyIr {
    pub child_column: String,
    pub host_ref: GlyphHostRefIr,
}

pub enum GlyphHostRefIr {
    Current(String),
    Outer(String),
}

pub struct GlyphCallIr {
    pub glyph: String,
    pub size: GlyphSizeIr,
    pub clip: GlyphClipIr,
    pub padding: f64,
    pub placement: GlyphPlacementIr,
    pub dx: f64,
    pub dy: f64,
    pub legend: bool,
    pub span: Span,
}
```

A `GlyphDeclIr` is lowered once per chart-scoped `Glyph` declaration. A
`GlyphCallIr` is lowered per call site and references a declaration by name. The
semantic analyzer MUST validate the glyph data table, key columns, host-row
references, viewport sizing, and type compatibility before rendering.

### 13.4 Derived Table IR

```rust
pub struct DeriveIr {
    pub name: String,
    pub data: SpaceDataRef,
    pub stat: StatCallIr,
    pub output_schema: Vec<ColumnDef>,
    pub span: Span,
}
```

```rust
pub struct StatCallIr {
    pub kind: StatKind,
    pub input: FrameIr,
    pub options: StatOptionsIr,
    pub span: Span,
}
```

```rust
pub enum StatKind {
    Bin,
    Bin2D,
    HexBin,
    Summary2D,
    SummaryHex,
    ContourLines,
    ContourBands,
    Density2D,
    Density2DContours,
    Density2DBands,
    Distinct,
    Ecdf,
    Qq,
    Summary,
    SummaryBin,
    Cut,
    Count,
    Smooth,
    StepVertices,
    VectorEndpoints,
    CurveSample,
    IntervalSegments,
    IntervalRects,
    IntervalMiddles,
    Boxplot,
    Density,
    Centroid,
    Simplify,
    SpatialJoin,
}
```

Built-in stat options MUST be carried as the typed `StatOptionsIr` enum rather than a string-keyed setting list. Each variant carries the user-specified values as `Option`s, where `None` means "use the renderer default"; fixed-domain settings (`closed`, smooth `method`) are enums:

```rust
pub enum StatOptionsIr {
    Bin { bins: Option<f64>, bin_width: Option<f64>, boundary: Option<f64>, closed: BinClosedIr },
    Bin2D { bins: Option<f64> },
    HexBin { bins: Option<f64> },
    Summary2D { bins: GridBinsIr, reducer: SummaryReducerIr },
    SummaryHex { bins: usize, reducer: SummaryReducerIr },
    ContourLines { levels: LevelSpecIr },
    ContourBands { levels: LevelSpecIr },
    Density2D { bandwidth: Option<f64>, grid: GridBinsIr },
    Density2DContours { bandwidth: Option<f64>, grid: GridBinsIr, levels: LevelSpecIr },
    Density2DBands { bandwidth: Option<f64>, grid: GridBinsIr, levels: LevelSpecIr },
    Distinct,
    Ecdf,
    Qq { distribution: QqDistributionIr, reference: bool },
    Summary { by: Vec<ColumnRef>, reducer: SummaryReducerIr },
    SummaryBin {
        by: Vec<ColumnRef>,
        bins: Option<f64>,
        bin_width: Option<f64>,
        boundary: Option<f64>,
        closed: BinClosedIr,
        reducer: SummaryReducerIr,
    },
    Cut { breaks: Vec<f64>, labels: Option<Vec<String>>, output: String },
    Smooth { method: SmoothMethodIr, span: Option<f64>, se: bool },
    StepVertices { direction: StepDirectionIr },
    VectorEndpoints { length_scale: Option<f64> },
    CurveSample { curvature: f64, points: usize },
    IntervalSegments { orientation: IntervalOrientationIr, cap_width: Option<f64> },
    IntervalRects { orientation: IntervalOrientationIr, width: Option<f64> },
    IntervalMiddles { orientation: IntervalOrientationIr, width: Option<f64> },
    Density { bandwidth: Option<f64>, grid_points: Option<f64> },
    Count,
    Centroid,
    Simplify { tolerance: Option<f64> },
    SpatialJoin { table: String, predicate: SpatialPredicateIr },
}

pub enum BinClosedIr { Left, Right }

pub enum SmoothMethodIr { Lm, Loess }

pub enum StepDirectionIr { Hv, Vh }

pub enum IntervalOrientationIr { Vertical, Horizontal }

pub enum SummaryReducerIr { Count, Mean, Min, Max, Sum, Median, MeanSe }

pub enum QqDistributionIr { Normal }
```

The explicit `Derive` stat-option parser and high-level geometry lowering MUST produce `StatOptionsIr` through the same defaulting path, so invalid settings keep identical diagnostic codes and spans regardless of which surface produced them. The renderer MUST read `StatOptionsIr` directly rather than looking up string keys.

Version 0.1 MUST support `StatKind::Bin` for explicit `Derive` declarations.

Other stat kinds MAY be exposed through high-level geometries before they are exposed through `Derive`.

Derived table IR MUST be ordered so every explicit derived dependency appears
before the derived table that reads from it. When no dependency ordering is
needed, source order MUST be preserved.

Derived table IR MUST be validated before spaces that reference derived data are validated.

Derived table names MUST be available to later `Derive` declarations and `Space` blocks.

`Derive name from source = ...` MUST set `DeriveIr.data` to `Table(source)` for
a named table source or `Derived(source)` for a derived source. A `Derive`
without `from` MUST set `DeriveIr.data` to `Primary`.

Forward references through `from` MAY be accepted if the analyzer still emits a
deterministic acyclic order.

### 13.5 Frame IR

```rust
pub enum FrameIr {
    Vector(ColumnRef),
    Cartesian(Vec<FrameIr>),
    Nested {
        outer: Box<FrameIr>,
        inner: Box<FrameIr>,
    },
    Union(Vec<FrameIr>),
    Invalid,
}
```

### 13.6 Geometry IR

```rust
pub struct GeometryIr {
    pub kind: GeometryKind,
    pub mappings: Vec<AestheticMapping>,
    pub settings: Vec<GeometrySetting>,
    pub span: Span,
    pub origin: IrOrigin,
}
```

```rust
pub enum IrOrigin {
    Source(Span),
    Desugared {
        source: Span,
        role: DesugaredRole,
    },
}
```

```rust
pub enum DesugaredRole {
    HistogramBinTable,
    HistogramRect,
    HistogramCountAxis,
}
```

IR produced by high-level geometry desugaring MUST carry an origin mapping.

Diagnostics on synthetic IR nodes MUST use the origin mapping to report source spans in the user-authored program.

A geometry's aesthetic mappings and literal settings are keyed by a typed
property key rather than a raw string:

```rust
pub struct AestheticMapping {
    pub aesthetic: PropertyKey,
    pub column: ColumnRef,
    pub span: Span,
}

pub struct GeometrySetting {
    pub name: PropertyKey,
    pub value: SettingValue,
    pub span: Span,
}
```

`PropertyKey` is a closed enum with one variant per built-in geometry property
(spec §13.9). The renderer and geometry lowering MUST match on `PropertyKey`
variants rather than comparing property-name strings. Every mapping and setting
MUST carry the `Span` of the user-authored argument that produced it; mappings
and settings synthesized during desugaring MUST carry the span of the
originating geometry call.

### 13.7 Column Reference

```rust
pub struct ColumnRef {
    pub name: String,
    pub dtype: DataType,
    pub span: Span,
}
```

Column refs MUST point to schema columns.

Unknown columns produce diagnostics.

Unknown column refs in IR SHOULD use `Invalid` sentinel values to avoid cascading failures.

### 13.8 Geometry Registry

The analyzer uses a geometry registry.

Each geometry definition includes:

name

supported space kinds

required aesthetics

optional aesthetics

settings

default stat

default position behavior

render implementation key

documentation string

completion metadata

The registry MUST include a `Path` geometry alongside `Line` (spec §14.3.1).
`Path` reuses `Line`'s group-splitting and stroke logic but preserves row order
rather than sorting by x.

For `Line` and `Path`, the `strokeWidth` property MUST accept a column mapping in
addition to a numeric literal; a mapped `strokeWidth` trains a continuous scale
and is drawn per segment (spec §16.8). For all other geometries `strokeWidth`
remains a numeric literal. A `strokeWidth` (or `size`) scale mapped to a
non-numeric column is `E1607`.

Since version 0.8 the registry MUST include a `Geo` geometry (spec §14.23),
supported only in a spatial space. Its properties are `fill` (column or color),
`stroke` (color), `strokeWidth` (number), and `alpha` (number).

Since version 0.71 the analyzer resolves a call head `Name(...)` inside a
`Space` body in this order:

1. a built-in geometry from the geometry registry, else
2. a chart-scoped `Glyph` declaration (spec §7.11, §14.27), else
3. `E1201` unknown geometry/glyph.

The registry is consulted first, so a `Glyph` can never silently redefine a
built-in geometry; a declared glyph whose name collides with a registry
geometry is rejected at the declaration site (`E2201`).

### 13.9 Property Registry

Each geometry property definition includes:

property name

kind: mapping, setting, or both

accepted value types

default value

whether required

whether scale-backed

documentation string

Examples:

`Point.fill` accepts column mapping or color literal.

`Point.alpha` accepts number literal or column mapping.

`Point.size` accepts number literal or column mapping.

`Bar.fill` accepts column mapping or color literal.

`Bar.layout` accepts string literal `"identity"`, `"stack"`, or `"fill"`.

`Image.src` accepts a local image path string literal or a string column mapping.

`Rect.xmin`, `Rect.xmax`, `Rect.ymin`, and `Rect.ymax` accept column mappings or numeric/temporal literals.

`Smooth.method` accepts string literals `"lm"` and `"loess"` (see §14.10 and
§15.7). `Smooth.span` accepts a number in `(0, 1]` and `Smooth.se` accepts a
boolean.

Before a property value is checked against its accepted forms, an unquoted
bare-identifier value MUST be resolved against in-scope `let` variables (spec
§9.6). When the identifier names a variable, the bound constant is substituted
and checked in its place; the type rules above then apply to the constant.

Each recognized property name maps to exactly one typed `PropertyKey` variant
(spec §13.6), and a property's name MUST be the single authoritative spelling of
its key (the registry derives the name from the key, so the two cannot drift).
Geometry names are likewise derived from a single authoritative
`GeometryKind` spelling. The registry remains the source of completion, hover,
and signature-help metadata; LSP features MUST reuse this metadata rather than
re-listing names.

### 13.10 Duplicate Argument Diagnostics

Duplicate argument keys MUST produce diagnostics.

Example:

```ag
Point(alpha: 0.5, alpha: 0.7)
```

Diagnostic:

`E1101 duplicate property alpha`

The later value SHOULD be ignored by semantic IR.

The diagnostic SHOULD reference both spans.

### 13.11 Unknown Geometry Diagnostics

Unknown geometry names MUST produce diagnostics.

The analyzer SHOULD suggest closest known geometry names.

Suggestion distance SHOULD use case-insensitive edit distance.

Example:

`Piont()` suggests `Point()`.

### 13.12 Unknown Property Diagnostics

Unknown property names MUST produce diagnostics.

The analyzer SHOULD suggest closest known property names for that geometry.

Example:

`Point(colour: species)` MAY suggest `fill` or `stroke`.

The diagnostic MUST make clear that `colour` is not an alias because `fill` and `stroke` have different semantics.

If aliases are supported later, they MUST resolve to explicit `fill` or `stroke` behavior rather than introducing a separate `color` aesthetic.

Version 0.1 MUST avoid property aliases.

### 13.13 Type Diagnostics

Type diagnostics MUST identify expected and found types.

Example:

```ag
Point(alpha: "high")
```

Diagnostic:

`E1201 alpha expects number between 0 and 1 or column mapping, found string`

### 13.14 Algebra Diagnostics

Algebra diagnostics MUST identify unsupported structures.

Example:

```ag
Space(x * y * z) {
    Point()
}
```

Diagnostic:

`E1306 3D Cartesian spaces are unsupported; use (x * y) / z to facet by z`

### 13.15 Schema Diagnostics

Schema diagnostics include:

data file not found

data file unreadable

CSV parse error

missing header

duplicate column names

unknown column reference

incompatible column type

Duplicate column names MUST produce diagnostics.

The resolver MAY disambiguate duplicate columns internally, but user-facing identifiers become ambiguous.

### 13.16 Diagnostic Severity

Errors block rendering.

Warnings do not block rendering.

Information diagnostics provide guidance.

Hints provide editor-only suggestions.

Version 0.1 CLI render MUST fail on errors.

Version 0.1 CLI render MAY proceed on warnings.

### 13.17 Semantic Analysis Phases

Recommended phases:

1. Parse source to AST.
2. Extract chart data source.
3. Resolve data source path.
4. Load or infer schema.
5. Build initial symbol table, including visible `Table` declarations.
6. Resolve and validate `Derive` declarations and their explicit `from`
   dependencies.
7. Add derived table schemas to the symbol table.
8. Resolve space data bindings.
9. Resolve algebra identifiers against each space's active table.
10. Validate frame kinds.
11. Resolve geometry calls.
12. Resolve properties.
13. Build scale requirements.
14. Build guide requirements.
15. Emit `ChartIr`.

Each phase SHOULD return diagnostics without panicking.

## 14. Geometry Specification

### 14.1 Geometry Interface

A geometry is declared by a call in a `Space` block.

Geometries are either primitive or high-level.

Primitive geometries draw marks directly from explicit coordinates or bounds.

High-level geometries may compute derived data before drawing primitive marks.

High-level geometries SHOULD document their primitive desugaring.

In a polar space (§16.16) geometries draw circular forms from the same data and
stat logic. `Bar`, `Rect`, and `Tile` draw wedges or annular segments (reusing
the Cartesian stacking/fill of `BarLayout`); `Line` and `Area` draw closed
polygons ordered by angle (radar); `Point` places markers at the polar-projected
position. `Histogram` desugars to `Rect` and so yields a circular histogram for
free. Built-in Cartesian geometry output is unchanged.

`Rect` is a primitive geometry.

`Histogram` is a high-level geometry.

All geometries share common optional properties where applicable.

Common properties:

`fill`

`stroke`

`alpha`

`size`

`strokeWidth`

`shape`

`label`

`layout`

`stat`

`position`

`nudge`

`nudgeData`

Geometry-specific properties are defined below.

### 14.2 Point

Syntax:

```ag
Point(fill: species, alpha: 0.7, size: 3)
```

Supported spaces:

1D Cartesian/vector

2D Cartesian

nested 2D Cartesian where x or y has nested bands

faceted Cartesian

Required inherited frame:

x coordinate

y coordinate for 2D spaces; 1D spaces place marks on the plot-center baseline
without creating a visible y axis

Optional mappings:

fill

stroke

alpha

size

shape

Optional settings:

fill color

stroke color

alpha number

size number

shape string option

jitter numeric array `[x, y]`

nudge numeric array `[dx, dy]`

nudgeData numeric array `[dx, dy]`

Default fill:

theme point fill

Default stroke:

none or theme point stroke

Default alpha:

1.0

Default size:

3

Default shape:

circle

Version 0.3.0 point shapes are:

circle

square

triangle

diamond

Unknown literal shapes SHOULD render as `circle` with a warning.

Categorical shape mappings MUST assign shapes deterministically in domain
order and wrap when there are more categories than supported shapes.

Point rendering emits SVG `circle`, `path`, or `use` elements.

Point MUST skip rows with missing x, or with missing y in 2D spaces.

Point SHOULD skip rows with non-finite x or y after scale mapping.

Version 0.41 point marks MAY use deterministic position adjustments. `jitter:
[x, y]` offsets the point by a stable, row-index-derived value in
`[-0.5, 0.5)` times the requested x/y amount. On continuous and temporal axes
the amount is in data units; on band and nested-band axes it is a fraction of
the resolved band width. `nudge: [dx, dy]` is a pixel offset applied after
data-space adjustments. `nudgeData: [dx, dy]` is a data-space offset converted
through the trained axis; on band axes it is a band-width fraction. These
adjustments are ignored for polar and spatial spaces except that pixel `nudge`
still applies after projection. The algorithm MUST be deterministic,
seed-free, and independent of wall-clock randomness.

### 14.3 Line

Syntax:

```ag
Line(stroke: series, strokeWidth: 2)
```

Supported spaces:

1D Cartesian/vector

2D Cartesian

faceted Cartesian

Required inherited frame:

x coordinate

y coordinate for 2D spaces; 1D spaces place vertices on the plot-center
baseline without creating a visible y axis

Optional mappings:

stroke

alpha

group

Optional settings:

stroke color

strokeWidth number

alpha number

dash string option

Default grouping:

group aesthetic if present

stroke mapping if present

fill mapping if present

otherwise all rows one group

Line MUST sort rows by x within each group unless `sort: false`.

Line MUST skip rows with missing x, or with missing y in 2D spaces.

Line MUST break paths on missing coordinates rather than connecting across gaps.

`Line` MUST accept a column-mapped `strokeWidth` (spec §13.8, §16.8); when
mapped, the width is drawn per segment as in `Path`.

Since version 0.36.0, `Line` MUST accept `dash: "solid" | "dotted" |
"dashed"`. The default is solid. The presets map to deterministic backend
dash patterns; arbitrary SVG dash arrays are not accepted. User-facing
`lineCap`, `lineJoin`, and miter-limit properties are deferred and MUST NOT be
accepted as geometry properties in this version.

#### 14.3.1 Path

Syntax:

```ag
Path(stroke: direction, strokeWidth: survivors, group: group)
```

`Path` is identical to `Line` except it connects rows in **source order** and
never sorts by x. It honors `group:` (separate sub-paths), `stroke:`
(categorical or continuous color mapping), and `strokeWidth:` (a numeric literal
or a column mapping). `Path` reuses `Line`'s registry shape and group-splitting;
the only difference is row ordering. `Line`'s automatic x-sort remains a
deliberate feature, so a path that must preserve a non-monotone trajectory (such
as a geographic route) uses `Path`.

When `strokeWidth` is a column mapping, `Path` (and `Line`) MUST emit a separate
segment per adjacent pair of points, each with a width derived from its
endpoints' scaled values (spec §16.8).

Version 0.23.0 MUST support `taper: true` on `Line`/`Path`, which renders a
mapped-`strokeWidth` series as a single filled polygon (a "tapered ribbon")
instead of per-segment strokes. The polygon offsets the polyline by ±½ the
scaled `strokeWidth` at each vertex along the per-vertex miter normal; miter
length MUST be capped so sharp turns do not spike. The ribbon is filled with the
series' `stroke` color. `taper` defaults to `false`, preserving per-segment
rendering. `taper` has no effect when `strokeWidth` is constant (the series
falls back to a plain stroked line). Grouping, missing values, and group
boundaries follow the same rules as the per-segment path.

Since version 0.36.0, `Path` MUST accept the same `dash` presets as `Line`.

### 14.4 Step Lines as Path Vertices

Algraf does not expose a `Step` geometry in version 0.36.0. Step lines are
expressed as a derived primitive table plus `Path`:

```ag
Derive step_rows = StepVertices(day, units, direction: "hv")
Space(day * units, data: step_rows) {
    Path(group: step_group)
}
```

`StepVertices` is specified as a derived stat in §15.15. It is the normative
source-level feature for step lines in this version. A future `Step` convenience
geometry MAY be added only if it lowers byte-for-byte to `StepVertices` plus
`Path`.

### 14.5 Rect

Syntax:

```ag
Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)
```

Supported spaces:

2D Cartesian

faceted Cartesian

Required properties:

`xmin`

`xmax`

`ymin`

`ymax`

Optional properties:

fill

stroke

alpha

strokeWidth

`Rect` is a primitive geometry.

`Rect` draws axis-aligned rectangles from explicit bounds.

`Rect` does not compute counts, bins, summaries, or stacks.

`Rect` MUST map `xmin` and `xmax` through the active x scale.

`Rect` MUST map `ymin` and `ymax` through the active y scale.

`Rect` MUST accept numeric literals for continuous bounds.

`Rect` MUST accept temporal literals once temporal literal syntax is added.

`Rect` MUST accept column mappings for all bounds.

`Rect` MUST skip rows with missing required bound values.

When a bound maps to a categorical column on a band or nested-band axis,
`Rect` MUST resolve that category to the category band edge: `xmin`/`ymin`
use the lower edge and `xmax`/`ymax` use the upper edge.

For nested bands, a bound mapped to the inner category column MUST use the
row's outer and inner category values to resolve the nested sub-band.

When a mapped rectangle has zero width or zero height, the renderer MUST skip
the rectangle rather than emitting a stroked marker. A histogram bin with count
zero is therefore invisible at the baseline.

`Rect` is the primitive mark used by histogram desugaring.

Example:

```ag
Chart(data: "distribution.csv") {
    Derive bins = Bin(value, bins: 25)

    Space(bin_start * count, data: bins) {
        Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)
    }
}
```

### 14.6 Bar

Syntax:

```ag
Bar(fill: type)
```

Supported spaces:

categorical x by continuous y

nested categorical x by continuous y

continuous x by categorical y

continuous x by nested categorical y

continuous x with explicit binning if stat count implemented

Required inherited frame:

x coordinate

y coordinate unless `stat: "count"`

Optional mappings:

fill

alpha

group

radius (polar radial bar mode only, §16.16)

Optional settings:

layout

width

baseline

Default layout:

`identity`

Supported layouts:

`identity`

`stack`

`fill`

`identity` draws each bar at its resolved coordinate.

`stack` stacks bars sharing the same x coordinate by grouping fill or group mapping.

`fill` stacks bars and normalizes the value axis extent to 100 percent.

Dodging is not a `Bar` layout in Algraf.

Dodging is represented algebraically with nesting.

Rationale:

Dodging changes the coordinate system by allocating sub-bands inside each primary band, so it belongs in algebra as `quarter / type`.

Stacking does not create new x coordinates; it resolves multiple rows that already share a coordinate by accumulating y extents, so it remains a bar layout policy.

This split keeps coordinate partitioning in `Space(...)` and collision resolution in geometry settings.

Example:

```ag
Space((quarter / type) * amount) {
    Bar(fill: type)
}
```

Bar MUST skip rows with missing position-axis values.

Bar MUST skip rows with missing value-axis values unless `stat: "count"`.

Bar MUST treat negative values according to stack rules.

Positive and negative stacks SHOULD be separated around baseline.

Version 0.77.0: for `stack` and `fill` layouts, the default legend order for
the stacked categorical aesthetic MUST follow rendered visual stack order
rather than raw scale/domain order (§19.5). Stack accumulation order continues
to control geometry placement from the baseline outward.

### 14.7 Histogram

Syntax:

```ag
Histogram(bins: 25, fill: "steelblue")
```

Supported spaces:

1D continuous vector

Required inherited frame:

x vector

Optional settings:

bins

binWidth

boundary

closed

orientation

fill

alpha

Histogram computes counts by bin.

Histogram produces an internal Cartesian frame of bin position by count by
default. Since version 0.46.0, `orientation: "vertical" | "horizontal"` selects
which physical axis is synthesized: vertical maps bins to physical x and count
to physical y; horizontal maps count to physical x and bins to physical y.
`orientation` defaults to `"vertical"`.

Histogram SHOULD expose the generated count axis label as `count`.

Version 0.23.0 MUST support grouping a Histogram by a categorical column in two
forms:

- **Stacked** — a `fill` column mapping or explicit `group` mapping on a single
  numeric vector space (`Space(value) { Histogram(fill: group) }`). An explicit
  `group` takes precedence; a literal `fill: "color"` is not a grouping. Each
  group is binned over the same shared edges (from the global value domain) so
  bars align, and the per-group counts render as a **stacked** bar in each bin.
- **Dodged** — nesting the group inside the binned value axis with the nest
  operator (`Space(value / group) { Histogram(...) }`). Each bin is split into
  side-by-side per-group sub-bars on a continuous x-axis, rising from a zero
  baseline to each group's count. This is the algebraic counterpart to dodged
  bars (§12); there is no `position`/`layout` keyword. When no `fill` is given,
  the dodged sub-bars are colored by the nested group.

Both forms bin per group over shared edges, color by the group column with a
categorical `fill` legend, and are deterministic (stacking and dodge slots
ordered by group first-appearance). A grouped Histogram requires a numeric input
column; a temporal input with grouping MUST emit `E1404`.

Version 0.77.0: the stacked form's default `fill` legend MUST follow the
rendered visual stack order of the generated stacked bars (§19.5), including
through the pre-stacked `Rect` desugaring below. The dodged form keeps
scale/domain legend order.

Grouped, dodged, blended, and annotated Histograms MUST honor the same
generated-axis orientation. In horizontal orientation, `count`/stack/density
bounds use x properties and bin boundaries use y properties; guide labels, scale
training, render metadata, draw-list axes, and interaction sidecars MUST match
that physical assignment.

`Histogram` with `orientation` in a two-dimensional Cartesian frame MUST remain
invalid (`E1302`); `orientation` is not a synonym for swapping physical axes.

Version 0.24.0 MUST support overlaid Histograms by blending numeric columns with
the blend operator:

```ag
Space((selection_age + mission_age)) {
    Histogram(binWidth: 1, alpha: 0.8)
}
```

This form bins every blended column over the same shared edges computed from the
combined numeric domain. It draws one full-width bar per `(bin, series)` from a
zero baseline to that series' count, with bars overlaid in deterministic
bin-major, series-minor order. The synthetic `series` column names the source
column and drives the categorical `fill` scale and legend. This is the algebraic
counterpart to an identity-position overlaid histogram; there is no
`position`/`layout` keyword.

When a Space contains exactly one Histogram plus annotation marks (`VLine`,
`HLine`, `Text`, or `Segment`), those annotations MUST render in the Histogram's
derived count space rather than producing a separate panel. Literal annotation
coordinates resolve against the generated bin/count axes.

Histogram is a high-level geometry.

Histogram MUST have a specified primitive desugaring.

The following program:

```ag
Chart(data: "distribution.csv") {
    Space(value) {
        Histogram(bins: 25)
    }
}
```

MUST be visually equivalent to:

```ag
Chart(data: "distribution.csv") {
    Derive __histogram_0 = Bin(value, bins: 25)

    Space(bin_start * count, data: __histogram_0) {
        Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)
    }
}
```

The generated derived table name MUST be hygienic and MUST NOT collide with user-defined derived table names.

Visual equivalence means the same source data, bin settings, theme, viewport, and renderer version produce the same bin boundaries, scale domains, default guide labels, and SVG mark geometry.

Visual equivalence does not require diagnostics to expose synthetic `Derive` or `Rect` source locations.

Diagnostics produced by desugared nodes MUST map back to the original `Histogram` call unless a more precise user-authored span exists.

Scale and guide diagnostics for the generated `count` axis MUST map to the `Histogram` call.

Diagnostics for user-provided histogram settings such as `bins`, `binWidth`, `boundary`, or `closed` MUST map to the corresponding setting span.

The generated count-axis label defaults to `count`.

The generated `count` column is a synthetic stat output column.

The implementation MAY keep `Histogram` as a direct IR node internally, but its visual output MUST match the derived-table plus `Rect` model.

A grouped Histogram desugars to a grouped `Bin` (a two-column `(value, group)`
input) plus `Rect`s. The grouped `Bin` emits `bin_start`, `bin_end`,
`bin_center`, `count`, `density`, the group key column, the pre-stacked
`stack_lower`/`stack_upper` y-bounds, and the per-group `dodge_start`/`dodge_end`
sub-slot x-bounds. The stacked form

```ag
Space(body_mass) { Histogram(fill: species, bins: 16) }
```

MUST be visually equivalent to a `Bin` over `(body_mass, species)` feeding:

```ag
Rect(xmin: bin_start, xmax: bin_end, ymin: stack_lower, ymax: stack_upper, fill: species)
```

and the dodged form

```ag
Space(body_mass / species) { Histogram(bins: 16) }
```

MUST be visually equivalent to the same `Bin` feeding:

```ag
Rect(xmin: dodge_start, xmax: dodge_end, ymin: 0, ymax: count, fill: species)
```

The blended form

```ag
Space((selection_age + mission_age)) { Histogram(binWidth: 1) }
```

MUST be visually equivalent to a `Bin` over the union
`(selection_age + mission_age)` feeding:

```ag
Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count, fill: series)
```

### 14.8 Frequency Polygon

Syntax:

```ag
FreqPoly(bins: 25, stroke: "steelblue")
```

Supported spaces:

1D continuous vector

Frequency polygon shares binning with histogram.

It renders bin centers connected by lines.

Version 0.3.0 MUST advertise `FreqPoly` in the registry.

`FreqPoly` MUST use the same `Bin` stat and bin settings as `Histogram`.

Since version 0.46.0, `FreqPoly` accepts `orientation: "vertical" |
"horizontal"` with the same default and generated-axis semantics as
`Histogram`. Vertical orientation renders a `Line` over `bin_center * count`.
Horizontal orientation renders a `Line` over `count * bin_center`. `FreqPoly`
with `orientation` in a two-dimensional Cartesian frame MUST remain invalid
(`E1302`); `orientation` is not a synonym for swapping physical axes.

### 14.9 Density

> Promoted from a v0.1 `MAY` to a v0.2.0 requirement; see `docs/V0_2_PLAN.md`.

Syntax:

```ag
Density(fill: "#4c78a8", alpha: 0.4)
Density(bandwidth: 0.5, n: 256)
```

Supported spaces:

1D continuous (numeric) vector, or a union of 1D continuous (numeric) vectors (blended space)

Density computes a kernel density estimate of the input column and renders it
as a filled area from the curve down to a zero baseline.

Version 0.2.0 MUST advertise `Density` in the registry and implement the KDE
described in §15.11.

`Density` accepts `bandwidth` (positive number) and `n` (grid points, at least
2) settings, plus the `fill`, `stroke`, `strokeWidth`, and `alpha` visual
settings.

Version 0.31.0 MUST support overlaid densities by blending numeric columns with
the blend operator:

```ag
Space((selection_age + mission_age)) {
    Density(alpha: 0.6)
}
```

This form computes kernel density estimates for each blended column independently
and overlays them. The synthetic `series` column names the source column and
drives the categorical `fill` scale and legend.

The blended form

```ag
Space((selection_age + mission_age)) { Density() }
```

MUST be visually equivalent to a `Density` stat over the union
`(selection_age + mission_age)` feeding:

```ag
Area(fill: series)
```

over the `(density_x, density)` space of the derived table.

A `Density` over a non-numeric column MUST emit `E1404`; over any other non-vector/non-blended
space it MUST emit `E1302`.

### 14.10 Smooth

Syntax:

```ag
Smooth(method: "lm", stroke: "#333333", strokeWidth: 2)
```

Supported spaces:

2D Cartesian continuous x and y

Required inherited frame:

x

y

Supported methods:

`lm`

`loess` MUST be implemented in version 0.23

Default method:

`lm`

Smooth computes predicted values.

Smooth renders a line.

Version 0.23.0 MUST support `method: "loess"`, locally weighted degree-1
regression with tricube weights. The neighborhood fraction is set by
`span: number` in `(0, 1]` (default `0.75`); larger spans are smoother. `span`
applies only to `loess`; pairing it with `method: "lm"` MUST emit `E1404`. Loess
output MUST be deterministic and independent of platform locale or any
randomization.

Version 0.23.0 MUST support `se: true`, which draws a confidence band around the
fitted line (a filled polygon, drawn beneath the line, using the `fill` color
when given or the stroke color otherwise). The band half-width is `1.96` times
the standard error of the fit under a normal approximation (≈ 95%). The band is
sampled across the x-range; an `lm` band narrows toward the mean of x and widens
toward the extremes. `se` defaults to `false`.

Smooth grouping MUST follow the `group` aesthetic when present.

When `group` is absent, Smooth grouping SHOULD follow stroke or fill mappings.

Smooth MUST report diagnostic if x or y is non-continuous for `lm`.

Since version 0.36.0, Smooth MUST accept `dash: "solid" | "dotted" |
"dashed"` for the fitted line. The confidence band emitted by `se: true` is a
filled polygon and is not dashed. The default dash style is solid.

### 14.11 Boxplot

Syntax:

```ag
Boxplot(fill: gender)
```

Supported spaces:

categorical x by continuous y

nested categorical x by continuous y

continuous x by categorical y

continuous x by nested categorical y

Boxplot computes:

minimum whisker

first quartile

median

third quartile

maximum whisker

Whiskers extend to the most extreme observation within `1.5 · IQR` of the
quartiles.

Version 0.23.0: observations beyond the `1.5 · IQR` fences MUST render as small
open circles centered on the box ("outliers"). `outliers: true` is the default;
`outliers: false` suppresses them. Outlier order MUST follow the sorted group so
output stays deterministic.

Properties:

fill

stroke

alpha

width

outliers (boolean, default true)

Boxplot MUST group by the categorical position coordinate and nested coordinate.

### 14.12 Violin

Syntax:

```ag
Violin(fill: gender, quantiles: [0.25, 0.5, 0.75])
```

Supported spaces:

categorical x by continuous y

continuous x by categorical y

Violin computes density per group.

Version 0.3.0 MUST advertise `Violin` in the registry.

Violin MUST support categorical x by continuous y spaces and continuous x by
categorical y spaces.

Violin MUST compute one Gaussian KDE per category using the same deterministic
defaults as `Density`: Silverman bandwidth, 256 grid points, and a
three-bandwidth extension.

`bandwidth` and `n` MUST override those KDE defaults.

`quantiles` MUST accept an ordered numeric array. When omitted, no quantile
lines are drawn.

Violin MUST render a symmetric mirrored density area inside each category band.
For horizontal orientation, the density extent mirrors along y while the
distribution values map to x.

If implemented, quantile lines MUST be deterministic.

### 14.13 Ribbon

Syntax:

```ag
Ribbon(ymin: lower, ymax: upper, fill: "steelblue", alpha: 0.25)
```

Supported spaces:

2D Cartesian where y dimension may be union

Required properties:

`ymin`

`ymax`

Optional properties:

fill

stroke

alpha

group

Ribbon MUST sort by x within group.

Ribbon MUST render closed SVG paths.

Ribbon MUST skip rows with missing x, ymin, or ymax.

### 14.14 Area

Syntax:

```ag
Area(baseline: 0, fill: series, alpha: 0.25)
Area(fill: series, layout: "stack")
Area(fill: series, layout: "fill")
```

Supported spaces:

2D Cartesian

Area fills between y and baseline.

Properties:

baseline

fill

alpha

group

layout (`"identity"`, `"stack"`, or `"fill"`; default `"identity"`)

Area MUST sort by x within group.

`Area(layout: "identity")` fills between each group's y values and the
`baseline`.

`Area(layout: "stack")` groups rows by explicit `group` when present, otherwise
by categorical `fill` or categorical `stroke`, and renders one polygon per
group over cumulative y ranges. Positive and negative values stack separately
around the baseline. Scale-domain training MUST include stacked lower and upper
bounds.

Grouped stack and fill Area layouts MUST evaluate every non-empty group at every
valid physical x-position observed by any group. If a group has no row at an
observed x-position, that group/x cell contributes zero height; duplicate rows
for one group/x cell are aggregated before stacking.

`Area(layout: "fill")` uses the same grouping and positive/negative separation
as `"stack"`, but normalizes each physical x-position stack to share of total.
The normalized value axis MUST be locked to the observed normalized range
around the baseline (for all-positive stacks, baseline through baseline + 1).
Raw y values MUST NOT expand the normalized axis after fill layout is selected.

Grouped Area layouts require a numeric y axis and a usable grouping aesthetic;
otherwise semantic analysis MUST emit `E1302`.

Version 0.77.0: for `stack` and `fill` layouts, when the stack groups come
from a categorical `fill` or `stroke` mapping, that aesthetic's default legend
order MUST follow rendered visual stack order rather than raw scale/domain
order (§19.5). An explicit `group` mapping forms bands that need not align
with the color aesthetic's categories, so it keeps scale/domain legend order.

### 14.15 Tile

Syntax:

```ag
Tile(fill: value)
```

Supported spaces:

categorical x by categorical y

categorical x by temporal y

Tile maps fill to a fill scale.

Tile computes cell rectangles from x and y band scales.

### 14.16 Text

Syntax:

```ag
Text(label: name, fill: "black")
```

Supported spaces:

2D Cartesian

Required property:

label mapping or literal

Text renders SVG `text`.

Text MUST escape text content for SVG.

Text labels containing newline characters MUST render each line separately
inside one SVG `text` element. The renderer MUST preserve row order and MUST
escape each line before emission.

Text MAY specify `x` and `y` as numeric literals or column mappings. When either
coordinate is supplied, it resolves through the corresponding axis scale; mapped
categorical coordinates resolve to the band center. When omitted, Text inherits
the active space position for each row.

A Text mark with literal `x`, literal `y`, a literal `label`, and no data
mappings is a single annotation and MUST emit once, not once per row in the
active table.

Text supports the following alignment properties:

`anchor` — string literal `"start"`, `"middle"`, or `"end"`, selecting the
horizontal text anchor. The default is `"start"`.

`dx` — offsets the rendered text horizontally, in pixels. It MAY be a number
literal (one offset for the whole layer) or a column mapping (a per-row offset).
A mapped non-numeric cell contributes a zero offset.

`dy` — offsets the rendered text vertically, in pixels, with the same literal-or-
column semantics as `dx`.

`nudge` — since version 0.41, a numeric array `[dx, dy]` that offsets text in
pixels, after resolving its inherited or explicit coordinates. It composes with
`dx`/`dy`.

`nudgeData` — since version 0.41, a numeric array `[dx, dy]` in data units. The
renderer converts the values through the trained x/y axes before applying
pixel-space `nudge` and `dx`/`dy`. On band axes the value is a band-width
fraction. `nudgeData` is ignored for polar and spatial spaces.

`declutter` — boolean literal; default `false`. When `true`, labels that overlap
vertically or horizontally are spread apart before rendering. Decluttering
operates on the final positions (after `nudgeData`, `nudge`, `dx`, and `dy`).
Vertical decluttering is scoped to labels sharing an x column (rounded to the
nearest pixel) and keeps each adjusted group within the plot's vertical extent.
Horizontal decluttering is scoped to labels sharing a y baseline (rounded to the
nearest pixel), uses the renderer's deterministic estimated text widths, and
keeps each adjusted group within the plot's horizontal extent when the estimated
group fits. The result MUST be deterministic: within a column or row, labels are
laid out to maintain stable non-overlap while minimizing displacement from their
targets, with stable ordering. Connector lines and arbitrary two-dimensional
force layout are not provided in this version.

Text also accepts `fill`, `alpha`, and `size`.

`format` — string literal naming a deterministic numeric display format for a
numeric `label:` column. Supported formats are `.0f`, `.1f`, `.2f`, `$.2f`,
`.0%`, `.1%`, and `.2%`. Formatting MUST be locale-independent and identical
across SVG and draw-list output. `format` requires a numeric label column; using
it with a non-numeric label, a literal-only label, an unknown format string, or
with `timeFormat` MUST emit `E1908`.

`timeFormat` (since 0.31) — string literal naming a temporal format (the §19.4
named or custom chrono-style patterns). When `label:` maps a temporal column,
each label renders that column's UTC instant with the format instead of its
default text. Applied to a non-temporal `label:`, or with an unknown/invalid
format, it emits `E1907`.

Label boxes are expressed as `Rect` plus `Text` in version 0.36.0. The rectangle
bounds MUST be data columns or literals supplied by the author. Auto-sized
padded labels are deferred until the renderer has text measurement semantics.

#### 14.16.1 Terminal Label

Syntax:

```ag
Label(label: series, at: "end", group: series, dx: 8, fill: series)
```

Supported spaces:

2D Cartesian

`Label` renders one text mark per group at the start or end row in physical
x-axis order. `at` accepts `"start"` or `"end"` and defaults to `"end"`.

If `group` is present, it defines the terminal-label groups. If `group` is
absent and `label:` maps a column, rows are grouped by that label column. If
both are absent, one terminal label is emitted for the layer.

`Label` accepts `label`, `at`, `group`, `fill`, `alpha`, `size`, `anchor`,
`dx`, `dy`, and `format`. Styling, numeric formatting, and literal/mapped label
resolution follow `Text` where applicable. `Label` requires x and y axes;
unsupported `at` values MUST emit `E1204`, and a missing two-dimensional
Cartesian position space MUST emit `E1302`.

### 14.17 HLine

Syntax:

```ag
HLine(y: 12, stroke: "red", label: "Target")
```

Supported spaces:

2D Cartesian

HLine uses y scale to map literal y.

It spans the plot x range.

HLine accepts `dash: "solid" | "dotted" | "dashed"`. The default is solid.

Version 0.82.0 MUST support callout-badge controls on the `label:` of `HLine`
and `VLine` (§14.18 lists the shared rules):

- `labelPosition` — string literal selecting where the label sits along the
  rule. For `HLine`: `"start"` or `"end"` (default `"end"`). For `VLine`:
  `"top"` (default) or `"bottom"`. Invalid values MUST emit `E1204`.
- `labelShape` — string literal `"none"` (default, plain text), `"circle"`, or
  `"square"`, drawing a deterministically sized badge box behind the label.
  Badge size derives from the label text via the §17.3 estimated
  text-measurement model so output stays byte-stable.
- `labelFill` and `labelStroke` — color literals for the badge fill and border.
  `labelFill` defaults to the rule `stroke`; the badge text uses a readable
  contrast color derived from the fill; `labelStroke` defaults to no border.

When `labelShape` is `"none"` the label renders as plain centered text on the
rule at the selected position. A badge MUST render as `Rect`/circle plus `Text`
in the draw-list scene (no new primitive kind), so it participates in existing
scene metadata. An `HLine`/`VLine` `label:` with none of these arguments MUST be
byte-for-byte unchanged from prior releases (plain text label). Leader/connector
lines between a badge and an arbitrary data point remain deferred.

### 14.18 VLine

Syntax:

```ag
VLine(x: 3, stroke: "gray40", label: "Marker")
```

Supported spaces:

2D Cartesian

VLine uses x scale to map literal x.

It spans the plot y range.

VLine accepts `dash: "solid" | "dotted" | "dashed"`. The default is solid.

Version 0.82.0: `VLine` accepts the same `label:` callout-badge controls
described in §14.17 (`labelPosition`, `labelShape`, `labelFill`, `labelStroke`).
For `VLine` the badge rides at the `"top"` (default) or `"bottom"` of the rule,
centered on the line. Two `VLine`s with `label: "1"`/`label: "2"`, `labelShape:
"circle"`, `labelPosition: "top"` render circled digits at the plot top — the
keyed event-marker pattern.

### 14.19 Segment

Syntax:

```ag
Segment(x: 160, y: 55, xend: 185, yend: 85)
Segment(x: low, y: city, xend: high, yend: city, stroke: "#bbbbbb")
```

Supported spaces:

2D Cartesian

Segment maps literal endpoints through scales.

Version 0.23.0 MUST support column mappings for `x`, `y`, `xend`, and `yend` in
addition to literals. When any endpoint is a column mapping, Segment draws one
segment per data row from `(x, y)` to `(xend, yend)`; when all four are literals
it draws a single annotation segment. Mapped numeric and temporal endpoints map
through the continuous/temporal axis and extend its trained domain; a mapped
categorical endpoint resolves to the band center. The `stroke` aesthetic MAY be
a column mapping, colored per segment. Rows missing any endpoint value MUST be
skipped, with a single aggregated `R0002` warning reporting the skipped count.
This makes Segment suitable for slope and dumbbell charts where `Line` is not a
natural fit.

Since version 0.36.0, Segment MUST accept `dash: "solid" | "dotted" |
"dashed"`. The default is solid. Dash is a literal stroke style only; mapped
stroke-style scales and legends are deferred.

### 14.19.1 Interval Sugar

> Since version 0.37.0.

Syntax:

```ag
ErrorBar(ymin: lower, ymax: upper, capWidth: 0.4)
LineRange(xmin: lower, xmax: upper, orientation: "horizontal")
PointRange(ymin: lower, ymax: upper)
CrossBar(ymin: q25, ymax: q75, width: 0.6)
```

`ErrorBar`, `LineRange`, `PointRange`, and `CrossBar` are high-level sugar over
primitive-construction stats and primitive marks. They MUST lower before render:

- `ErrorBar` lowers to `IntervalSegments(...)` plus `Segment(...)`.
- `LineRange` lowers to `IntervalSegments(..., capWidth: null)` plus
  `Segment(...)`.
- `PointRange` lowers to the same interval segment layer, followed by a
  `Point(...)` layer in the original space.
- `CrossBar` lowers to `IntervalRects(...)` plus `Rect(...)`, followed by
  `IntervalMiddles(...)` plus `Segment(...)`.

Vertical orientation uses the first frame axis as the interval position and
requires `ymin` and `ymax` column mappings. Horizontal orientation uses the
second frame axis as the interval position and requires `xmin` and `xmax`
column mappings. If `orientation` is omitted, it MUST be inferred from which
bound pair is present; ambiguous or missing bounds are `E1205`. The sugar forms
require a two-dimensional Cartesian frame; incompatible spaces are `E1302`.

`capWidth` and `width` are non-negative finite numbers in position-axis data
units. They are applied by the derived stats when the position input is numeric.
For categorical positions, `IntervalRects` can still emit full-band categorical
rectangle bounds through `Rect`; band-relative partial widths and categorical
cap offsets are deferred.

Visual settings lower to the component primitives. `stroke`, `strokeWidth`,
`dash`, and `alpha` apply to the interval segment layer where accepted. `fill`,
`stroke`, `alpha`, `size`, and `shape` apply to the `PointRange` point layer
where accepted. `CrossBar` sends `fill`, `stroke`, `strokeWidth`, and `alpha`
to the rectangle body and `stroke`, `strokeWidth`, `dash`, and `alpha` to the
middle segment. Component primitive interaction behavior is used; composite
interaction metadata is not introduced in version 0.37.0.

The explicit derived-table primitive form and the promoted sugar form MUST
produce byte-for-byte identical SVG, draw-list JSON, raster output, and
interaction sidecar bytes when written with equivalent component layers.

### 14.20 Rug

Syntax:

```ag
Rug(sides: "bl", alpha: 0.55)
```

Supported spaces:

1D vector

2D Cartesian

Rug renders tick marks along axis edges.

### 14.21 2D Binning Geometries

Syntax:

```ag
Bin2D(bins: 30)
HexBin(bins: 30)
```

Supported spaces:

2D Cartesian continuous x and y

`Bin2D` MUST assign observations to deterministic rectangular bins and render
non-empty bins as rectangles filled by `count`.

`HexBin` MUST assign observations to deterministic hexagonal bins and render
non-empty bins as SVG polygons filled by `count`.

Both geometries accept `bins`, `fill`, `stroke`, `strokeWidth`, and `alpha`.

When `fill` is omitted, the fill channel MUST use a continuous gradient over
bin count.

### 14.22 Geometry Extensibility

The registry MUST be data-driven enough that LSP docs and completions can use the same metadata as semantic analysis.

Future plugin geometry support MUST be carefully sandboxed.

Version 0.1 SHOULD keep built-in geometries compiled into the binary.

### 14.23 Geo

Syntax:

```ag
Geo(fill: population, stroke: "#ffffff", strokeWidth: 0.25)
```

Supported spaces:

Spatial (a `Space` over a geometry column, spec §16.15)

`Geo` is a **polymorphic** mark: it dispatches on each row's geometry value and
projects every coordinate through the spatial scale before emitting SVG. Since
version 0.8 it MUST render:

- `Point` / `MultiPoint` → `<circle>` markers,
- `LineString` / `MultiLineString` → an unfilled `<path>` (stroked polyline),
- `Polygon` / `MultiPolygon` → a filled `<path>`, one `M…Z` subpath per ring
  (exterior then interiors), using `fill-rule="evenodd"` so holes are cut out.

`fill` MAY map to a column, producing a **choropleth**: the fill resolves per
feature through the gradient or categorical scale (spec §16.8, §16.13), and the
fill legend reuses the existing legend machinery. `stroke`, `strokeWidth`, and
`alpha` are constants.

Rendering MUST be deterministic: features are drawn in row order and rings in
source order. A `Geo` mark outside a spatial space is a semantic error — `E1801`
when the space frames a single non-geometry column, `E1804` when the space is a
planar Cartesian space.

Cartesian polygon recipes SHOULD use geometry-typed data: prebuild GeoJSON,
TopoJSON, Shapefile, or another geometry source, then render with
`Space(geom) { Geo(...) }`. A row-oriented x/y `Polygon` geometry is deferred in
version 0.36.0 because hole policy, missing-value breaks, subgroup ordering, and
backend parity are not specified for that surface.

### 14.24 Graticule

> Since version 0.22.

Syntax:

```ag
Graticule(stroke: "#cccccc", strokeWidth: 0.5, step: 10)
```

Supported spaces:

Spatial (a geometry-column space, or a `long * lat` space with a declared
`projection:`, spec §16.15)

`Graticule` is a **guide mark**: it draws the projected longitude/latitude grid
rather than data. It MUST sample each meridian and parallel in geographic space
and project every sample through the active spatial scale (§16.15), so curved
projections render as smooth curves. Lines MUST be emitted in a deterministic
order (meridians west→east, then parallels south→north).

Properties are constants: `stroke` (a color, defaulting to the theme's major
grid color), `strokeWidth` (defaulting to the theme's grid width), `alpha`, and
`step` (the grid spacing in degrees). When `step` is omitted, the renderer
chooses a deterministic "nice" spacing from the map's geographic extent. The
grid covers the rendered data's geographic bounding box.

A `Graticule` outside a spatial space — a planar Cartesian space with no
`projection:` — is `E1804`.

### 14.24.1 Image

> Since version 0.46.0.

Syntax:

```ag
Image(src: logo, size: 28)
Image(src: "logos/company.png", alpha: 0.9)
```

Supported spaces:

1D Cartesian/vector

2D Cartesian

nested 2D Cartesian where x or y has nested bands

faceted Cartesian

Required property:

`src`

Optional mappings:

src

size

Optional settings:

src string

alpha number

size number

jitter numeric array `[x, y]`

nudge numeric array `[dx, dy]`

nudgeData numeric array `[dx, dy]`

`Image` is a point-like geometry that draws one embedded raster/SVG image
centered at each resolved row position. In a 1D space, images use the same
plot-center baseline as `Point`. Rows with missing x, missing y in a 2D space,
or missing/non-string mapped `src` are skipped.

`src` MUST be either a string literal naming one local image path or a column
mapping whose values are local image paths. Paths resolve through the same
source/base-directory rules and `DriverIo` boundary as data files. URL-like
sources (`http:`, `https:`, `data:`, `javascript:`, etc.) MUST emit `E1204`;
source-authored URLs are not accepted for image marks. Supported local image
types are `.png`, `.jpg`, `.jpeg`, `.gif`, and `.svg`. A missing, unreadable,
unsupported, or dimensionless image source MUST emit `E1204`.

The renderer MUST embed each loaded image as a `data:image/...;base64,...` href
generated by Algraf, not by chart source. SVG emission uses an `<image>`
element. Draw-list emission uses an inert `image` op with the same embedded
href and coordinates. The render-model raster backend MAY emit `R0005` for
image ops it cannot draw directly; the canonical SVG-to-PNG path remains the
pixel-faithful rasterization route for image marks.

`size` is the maximum rendered side length in CSS pixels and defaults to the
theme point size. Images MUST preserve intrinsic aspect ratio. `alpha` controls
mark opacity and defaults to `1`. `jitter`, `nudge`, and `nudgeData` have the
same deterministic position-adjustment semantics as `Point` (§14.2).

A constant `src` setting does not create a legend. A mapped `src` creates a
discrete image legend with one swatch per source value in domain order; the
legend swatch uses the same embedded image as the mark and preserves aspect
ratio inside the legend swatch box.

`Image` accepts declarative interaction metadata (§14.25).

### 14.25 Declarative Interactions

> Since version 0.30.

Geometries that draw one filled mark per datum MAY carry declarative
*interaction* metadata. Interactions are **data attached to marks, never
executable source**: there is no event-handler syntax, expression language, or
script text. A chart declares *what* data participates and *how* marks group;
the renderer attaches that as inert metadata, and a viewer (a host runtime
reading the JSON sidecar of §24.6, the opt-in interactive runtime of §29.3, or a
Canvas/raster host reading the draw list of §24.6) interprets it.

Two interaction properties and one adjacent event-emitter call are recognized:

`tooltip` — a column, or an array of columns, whose per-row values describe a
mark. The renderer formats a deterministic, locale-independent sequence of
`label: value` lines, one per declared column, in declaration order.

`highlight` — a grouping column (written bare or as a quoted column name) whose
per-row value identifies the marks that emphasize together on hover (for
example, the categorical `fill` field also shown in the legend).

`On(event: "click", emit: column)` — since version 0.64.0, an inert event
emitter attached to the immediately preceding per-datum mark in the same
`Space`. `event` MUST be the string literal `"click"` in this version. `emit`
MUST name a column whose per-row value the host can read from the sidecar. `On`
MUST NOT create a drawable layer, affect scale training, or contain callbacks,
scripts, URLs, routing names, state references, or expressions. Algraf emits
only metadata; the host decides what, if anything, to do with the emitted value.

Interaction metadata MUST NOT affect scale training, layout, statistics, or the
geometry's drawn coordinates: it rides the geometry IR and the render scene
(§24.6) without changing what is drawn. Schema alone is enough to validate it —
no data rows are materialized during analysis.

Interaction properties are accepted only on the per-datum filled marks `Point`,
`Image`, `Bar`, `Rect`, and `Tile`. Using them on any other geometry is `E1206`. A
`tooltip` value that is neither a column nor an array of columns, or a
`highlight` value that does not name a column, is `E1207`. A referenced column
that does not exist is `E1101`. `On(...)` accepts the same per-datum filled mark
set. If `On(...)` is not placed immediately after an eligible mark, uses an
unsupported event name, omits `event` or `emit`, repeats an argument, targets an
unsupported geometry, or uses a non-column `emit` value, the analyzer MUST emit
`E1913`. An unknown `emit` column remains `E1101`.

Example:

```ag
Point(
    fill: species,
    tooltip: [species, flipper_length, body_mass],
    highlight: "species"
)
On(event: "click", emit: species)
```

The static SVG affordance is described in §18.10, the host-runtime sidecar and
draw-list representation in §24.6, and the opt-in interactive SVG runtime in
§29.3.

### 14.26 Z-Field Graphics

> Since version 0.38.0.

Algraf does not introduce source-level `Contour`, `ContourFilled`, `Raster`, or
`Summary2D` geometries in version 0.38.0. Z-field graphics are expressed as
derived tables plus existing primitive marks:

- regular raster-like fields use explicit `Rect` bounds, or `Tile` when the
  active axes are categorical/banded;
- contour lines use `ContourLines(...)` feeding `Path(group: contour_id)`;
- filled contour bands use `ContourBands(...)` feeding `Geo(fill: level_mid)`;
- 2D density contours use `Density2DContours(...)` feeding `Path`;
- rectangular z summaries use `Summary2D(...)` feeding `Rect`;
- hexagonal z summaries use `SummaryHex(...)` feeding `Geo`.

These forms MUST train `fill`, `stroke`, and legend scales exactly like any
other primitive layer over the produced columns. No interpolation keyword is
accepted in this version; raster recipes are nearest-cell / explicit-cell
renderings. A future convenience geometry MAY be added only if it lowers to the
corresponding derived-table form before rendering and produces identical SVG,
draw-list, raster, and interaction sidecar bytes.

### 14.27 Glyph Mark

> Since version 0.71.0. Supersedes the removed `Inset` block.

A **glyph** is a chart-valued mark: a chart-scoped `Glyph` declaration (§7.11)
invoked inside a `Space` body with ordinary geometry-call syntax. A glyph mark
renders its child `Space` blocks inside a bounded viewport anchored at the host
row, once per host row of the enclosing space. It is not a geometry registry
entry, a shape shortcut, or a raw SVG/HTML injection surface.

#### Invocation and aesthetics

```ag
pie(size: footfall, clip: "circle", padding: 1, at: "position")
```

- **`size`** is the ordinary size aesthetic. It MAY be a finite pixel number or a
  numeric host-table column. A mapped `size` trains the chart `Scale(size:)` and
  uses that scale's `range:` as the min/max viewport footprint; there is no
  `minSize`/`maxSize`.
- **`width`/`height`** give a fixed rectangular footprint and MUST NOT be
  combined with `size` (`E2206`). If only `width` is present, `height` defaults
  to the same value.
- **`clip`** MUST be `"rect"`, `"circle"`, or `false` and defaults to `"rect"`.
- **`padding`**, **`dx`**, and **`dy`** are finite pixel numbers and default to
  `2`, `0`, and `0`.
- **`at`** is the placement strategy and MUST be `"position"`, `"mark-center"`,
  or `"centroid"`, defaulting to `"position"`. `"position"` anchors to the host
  row's resolved x/y point. `"mark-center"` uses the rendered mark center when it
  can be computed (notably polar area marks such as pie slices) and otherwise
  falls back to the row anchor. `"centroid"` MAY be used in a spatial
  `Space(geom)` and anchors to the deterministic projected geometry centroid
  when one can be computed; otherwise render emits no child marks for that
  instance. An unsupported `at` value is `E2205`.
- **`legend: false`** suppresses chart-level legends contributed by this glyph
  call, reusing the ordinary mark legend control.
- Ordinary geometries ignore these glyph-only viewport properties.

#### Key resolution

Each declared `key` column is equi-matched against the host row context. For
each key column the analyzer resolves the host value by searching the
row-context chain outward — the immediate host row first, then each enclosing
glyph's host row — until a column of that name is found. An `outer.col`
qualifier forces the nearest enclosing glyph host when a name is shadowed. A key
that cannot be resolved in the host row-context chain is `E2204`; incompatible
match column types are `E2205`. Null match values never match, including
null-to-null. Matched child rows preserve child-table order, and duplicate child
rows render deterministically.

Because the key search is one-sided and resolves by name up the chain, nested
glyphs need no `parent.`-style qualifier:

```ag
Glyph trend(data: trends, key: [id, category], scales: "local") {
    Space(t * value) { Line(stroke: "#111827") }
}
Glyph nodepie(data: mix, key: id, scales: "shared") {
    Space(value, coords: polar, theta: y) {
        Bar(fill: category, layout: "fill")
        trend(width: 18, height: 8, at: "mark-center")
    }
}
```

#### Scale and legend participation

With the glyph `scales: "shared"` default (and per-`Scale` `train: "shared"`,
§16.18), each child `Space` bound to the glyph data table trains its position
scales across the union of all matched child rows for the glyph. Child spaces
bound to another named or derived table train against that table's own rows.
With `scales: "local"` (or `train: "local"`), each child `Space` trains from
only that instance's matched rows. Glyph internal scales flow into the chart
legend collection exactly like any other mark's scales, deduplicated by
`(aesthetic, domain)` (§17.7). A position or data-trained scale under
`train: "local"` produces no chart-level legend; aesthetic scales with a fixed
domain (e.g. a categorical color `range:` map) always merge regardless of
`train:`. Child position guides default off.

#### Glyph-body aesthetic scales

> Since version 0.72.

A `Glyph` body MAY contain a `Scale(size: col, range: [min, max], label: "…")`
declaration. Column resolution for such a scale uses the glyph's `data:` table
— the same row context the glyph body's inner `Space` sees — so a column that
lives in the glyph data (and not in the chart primary) resolves cleanly here.
When the call-site `size:` argument's column matches the glyph-body scale's
column, the scale's `range:` drives the call's viewport pixel range and the
scale produces a size legend through the normal pipeline (§16.13). A glyph-body
`Scale(size:)` takes precedence over a same-aesthetic, same-column chart-scope
`Scale(size:)`; the chart-scope form remains valid as a fallback and fires
only when its column genuinely resolves against the chart primary (§13.17
phase 6). Placing the size scale in the glyph body is RECOMMENDED, since the
chart-scope form only resolves when the column happens to exist in the chart
primary. Glyph-body `Scale(strokeWidth: …)` is deferred to a later version;
the glyph mark call surface today exposes only `size:` as a per-instance
numeric aesthetic.

#### Limits

Glyph marks MAY nest. A nested-glyph depth limit emits `E2209` and skips the
over-depth child scene; a recursive mark budget that would be exceeded emits
`E2210`. A glyph MUST NOT invoke itself directly or transitively. Glyph contents
MUST NOT contain user-authored JavaScript, CSS, HTML, external images, or raw
SVG fragments.

## 15. Statistics

### 15.1 Stat Model

Some geometries render raw data.

Some geometries compute derived data.

Statistics transform input dataframe and frame into a derived dataframe and derived frame.

Examples:

Histogram computes bins and counts.

Smooth computes predictions.

Boxplot computes quantiles.

Density computes density estimates.

Bar with `stat: "count"` computes counts.

`Derive` declarations expose statistical transforms as named tables.

`Derive` declarations are the preferred way to express high-level statistical graphics using primitive geometries.

Example:

```ag
Derive bins = Bin(value, bins: 25)
```

### 15.2 Stat Interface

Recommended trait:

```rust
pub trait Stat {
    fn compute(&self, input: &DataFrame, frame: &FrameIr) -> Result<StatResult, StatError>;
}
```

```rust
pub struct StatResult {
    pub data: DataFrame,
    pub frame: FrameIr,
    pub metadata: StatMetadata,
}
```

Stats SHOULD be pure functions.

Stats MUST not render SVG.

Stats MUST not read files.

Stats SHOULD preserve source row indices where practical.

### 15.3 Derived Stat Declarations

Derived stat declarations bind stat output to a chart-scoped table name.

Syntax:

```ag
Derive bins = Bin(value, bins: 25)
Derive trend from bins = Smooth(bin_center, count)
```

The left-hand identifier is the derived table name.

The optional `from` identifier is the active input table for the stat call. It
MUST name a visible `Table` or derived table. When omitted, the stat reads the
chart's primary table.

The right-hand side is a stat call.

The first positional expression is the stat input, resolved against the active
input table.

Named arguments configure the stat.

Derived stat declarations MUST be pure.

Derived stat declarations MUST NOT render marks.

Derived stat declarations MUST produce an output schema.

The output schema MUST be available before spaces using `data: bins` are analyzed.

Since version 0.47.0, derived stat declarations MAY depend on derived tables in
the same chart only through explicit `from` references, subject to the acyclic
dependency rules in §10.6.

When no `from` source exists, derived stats read from the primary data table.

Computed stat variables are ordinary named output columns on derived tables.
Algraf MUST NOT expose an `after_stat(...)` expression language in version
0.40. A high-level statistical geometry, when supported, MUST document the
equivalent `Derive` output columns and primitive geometry lowering. LSP hover
and completion MUST expose the output schema for every validated `Derive`.

### 15.4 Identity Stat

Identity behavior is table binding. Users render the original table by omitting
`data:` or by binding `Space(..., data: table_name)` to an existing table.
Version 0.40 does not expose a named `Identity(...)` derived stat because it
would duplicate ordinary table binding without creating useful columns.

Point, Line, Rect, Area, Ribbon, Tile, Text, HLine, VLine, and Segment usually use identity stat.

### 15.4.1 Distinct Stat

`Distinct(...)` retains the first source row for each distinct tuple of one or
more input columns and passes through the original schema unchanged.

Missing key values participate in equality: two null key values are the same
key. Geometry values MUST NOT be used as distinct keys. Output row order follows
the first retained row in source order. This makes the transform deterministic
for a given input order; it is intentionally not row-order-independent.

### 15.4.2 ECDF Stat

`Ecdf(value)` computes empirical cumulative distribution vertices from one
numeric input column. Missing, null, and non-finite values are skipped.

Output columns are:

`x`

`y`

Rows are sorted by `x`. The output is right-continuous and starts at
`(min(x), 0)`: for each unique input value it emits the previous cumulative
share and the new cumulative share at that same `x`. Duplicate values therefore
produce one vertical jump whose height reflects their multiplicity.

### 15.4.3 QQ Stat

`Qq(value, distribution: "normal", reference: true)` computes quantile-quantile
rows for one numeric input column. Version 0.40 MUST support only
`distribution: "normal"`; other distribution families are deferred. Missing,
null, and non-finite values are skipped. Sample values are sorted ascending.

Output columns are:

`theoretical`

`sample`

`line_x`

`line_y`

`line_xend`

`line_yend`

`role`

Point rows contain `theoretical`, `sample`, and `role: "point"`. When
`reference` is true and at least two finite samples exist, one additional row
contains a deterministic QQ reference segment in the `line_*` columns and
`role: "reference"`. The reference line uses the first and third sample
quartiles and the corresponding normal quartiles.

### 15.4.4 Summary Stats

`Summary(value, by: [group...], reducer: "...")` aggregates one value column,
optionally grouped by one or more non-geometry grouping columns. Reducers are
the deterministic enum values `"count"`, `"mean"`, `"min"`, `"max"`, `"sum"`,
`"median"`, and `"mean_se"`.

Output columns are the grouping key columns, followed by:

`value`

`count`

For `reducer: "mean_se"`, output also includes:

`lower`

`upper`

`se`

`count` counts finite values for numeric reducers and non-null values for the
`count` reducer. `median` uses the same Hyndman and Fan Type 7 quantile helper
as boxplot summaries. Group output order MUST be deterministic by grouping-key
value, not by first appearance.

`SummaryBin(x, value, by: [group...], ...)` bins a numeric `x` column using the
same `bins`, `binWidth`, `boundary`, and `closed` policy as `Bin`, then applies
the same reducers to `value` inside each `(bin, group)` cell. Output starts with
`bin_start`, `bin_end`, and `bin_center`, then grouping key columns, then the
summary measure columns above. Row order is bin-major, group-minor with
deterministic grouping-key order.

### 15.4.5 Cut Stat

`Cut(value, breaks: [...], labels: [...], output: "class")` appends a reusable
categorical class column to the original table. `breaks` MUST be a strictly
increasing non-empty numeric array. `labels`, when provided, MUST have the same
length as `breaks`; the final label represents values greater than or equal to
the final break. The default output column name is `<input>_class`.

Intervals are left-closed and right-open: `[break[i], break[i+1])`, with the
last interval open-ended. Values below the first break, missing values, and
non-finite values produce null class cells. Output row order matches source row
order because the transform appends a column rather than summarizing rows.

Quantile regression is deferred past version 0.40. Boxplot, violin quantile
lines, `Summary`, and `SummaryBin` cover deterministic distribution-summary
use cases without adding a model dependency or WASM footprint in this release.

### 15.5 Count Stat

Count stat counts rows by group.

Bar may use count stat when y is absent.

Example:

```ag
Chart(data: "demographics.csv") {
    Space(gender) {
        Bar(stat: "count", fill: gender)
    }
}
```

This produces a derived frame `gender * count`. The group columns in a count
derived frame MUST preserve the source columns' logical data types; count
aggregation MUST NOT stringify numeric, boolean, temporal, or string keys as an
implementation shortcut.

The y label defaults to `count`.

### 15.6 Bin Stat

Bin stat groups continuous values into bins.

Histogram uses bin stat.

`Derive name = Bin(...)` uses bin stat.

Required inputs:

continuous x vector

Settings:

bins

binWidth

boundary

closed

`bins` sets the requested number of bins.

`binWidth` sets exact bin width.

`bins` and `binWidth` MUST NOT both be provided.

`boundary` sets an anchor value that bin boundaries align to.

When `binWidth` is provided without `boundary`, numeric bins MUST default to a
boundary of `binWidth / 2`. This centers width-1 bins for integer-valued data on
integer tick marks: value `34` belongs to `[33.5, 34.5)`.

Default `closed` is `"left"`.

`closed: "left"` means bins are `[start, end)`.

`closed: "right"` means bins are `(start, end]`.

The final bin SHOULD include the maximum value even when `closed: "left"` would otherwise exclude it.

Values exactly on a boundary MUST be assigned according to `closed`.

Example with `binWidth: 10`, `boundary: 0`, and `closed: "left"`:

`0` belongs to `[0, 10)`.

`10` belongs to `[10, 20)`.

Example with `binWidth: 10`, `boundary: 0`, and `closed: "right"`:

`0` belongs to `(-10, 0]`.

`10` belongs to `(0, 10]`.

Output columns:

bin_start

bin_end

bin_center

count

density

Bin stat MUST output `bin_start`, `bin_end`, `bin_center`, and `count`.

Bin stat SHOULD output `density`.

Version 0.23.0: when the Bin stat receives a second, categorical group column
(the grouped-`Histogram` desugaring, spec §14.5), it MUST bin every group over
the same shared edges and emit one row per `(bin, group)`, adding the group key
column, the pre-stacked `stack_lower`/`stack_upper` y-bounds, and the per-group
`dodge_start`/`dodge_end` sub-slot x-bounds (the bin divided into equal slots in
group order). Stacking and dodge order follow group first-appearance, and row
order is bin-major, group-minor, for determinism.

Version 0.24.0: when the Bin stat receives a union of numeric columns (the
blended-`Histogram` desugaring, spec §14.7), it MUST bin all members over the
same shared edges computed from the combined numeric domain and emit one row per
`(bin, series)`. The output MUST add a synthetic string `series` column whose
value is the source column name. Row order is bin-major, series-minor following
the source union member order. Null and non-finite cells are skipped per series.

`bin_start`, `bin_end`, and `bin_center` have the same domain type as the input column.

For numeric inputs, bin boundary columns are numeric.

For temporal inputs, bin boundary columns are temporal if temporal binning is implemented.

Version 0.1 MUST support numeric binning.

Version 0.2.0 MUST support temporal binning for `Bin` and `Histogram`.

For temporal inputs, `bins`, `boundary`, and `closed` use the same interval assignment semantics as numeric binning over UTC-equivalent microsecond instants.

Version 0.20.0 MUST support calendar-aware temporal bins with
`interval: "<unit>"` on `Bin`, `Histogram`, and `FreqPoly`. Supported units are
`minute`, `hour`, `day`, `week`, `month`, `quarter`, and `year`. `interval` is
valid only for temporal inputs and MUST NOT be combined with `bins`,
`binWidth`, or `boundary`. Weeks start on Monday, and quarters start on January
1, April 1, July 1, and October 1.

`Histogram` over a temporal vector MUST trigger the same diagnostic when temporal binning is unavailable.

Nesting a high-cardinality temporal vector directly with `/` SHOULD produce a warning when it would create one panel or band per timestamp.

The warning SHOULD suggest deriving or precomputing a coarser period column such as day, week, month, or year.

### 15.7 Smooth Stat

Smooth stat computes fitted y values.

Method `lm` fits linear regression.

Version 0.23.0: method `loess` fits locally weighted degree-1 regression with
tricube weights and a neighborhood fraction set by `span` in `(0, 1]` (default
`0.75`). The result MUST be deterministic. `span` paired with `lm` MUST emit
`E1404`.

Output columns:

x

y

group

Version 0.23.0: when `se: true`, the Smooth stat MUST append `ymin`, `ymax`, and
`se` columns, where `se` is the standard error of the fitted `y` and
`ymin`/`ymax` are `y ∓ 1.96 · se` (a ≈95% band under a normal approximation).
Without `se`, only `x` and `y` (and `group` when grouped) are emitted. A
downstream `Ribbon(ymin: ymin, ymax: ymax)` over the derived table draws the
band explicitly.

Smooth stat MUST handle insufficient data with diagnostics.

### 15.8 Boxplot Stat

Boxplot stat computes group summaries.

Output columns:

group key columns

ymin

lower

middle

upper

ymax

outlier values MAY be separate.

Version 0.1 MUST use Hyndman and Fan Type 7 quantiles, matching the default quantile method used by R.

Boxplot whiskers MUST extend to the most extreme data points within `1.5 * IQR` of the first and third quartiles.

Values outside the whiskers are outliers.

Outliers MAY be rendered by default.

The quantile implementation MUST be deterministic and covered by snapshot or unit tests.

### 15.9 Stack Stat or Position Transform

Stacking may be represented as a position transform rather than a stat.

The implementation MUST choose one architecture and document it.

Recommended:

Use a position transform after scale training requirements are known but before render primitives are emitted.

Stacking computes y0 and y1.

Fill normalization computes proportions.

### 15.10 Missing Values in Stats

Stats MUST define missing-value behavior.

Default behavior:

drop rows with missing required columns.

emit warning count in render metadata.

do not warn for every row.

### 15.11 Density Stat

> Promoted to a v0.2.0 requirement; see `docs/V0_2_PLAN.md`.

The Density stat computes a Gaussian kernel density estimate over a numeric
input column.

Output columns:

`density_x` — evaluation grid position.

`density` — estimated density at that position.

The estimate MUST be deterministic. The default kernel is Gaussian. The default
bandwidth MUST use Silverman's rule of thumb,
`0.9 * min(stddev, IQR / 1.349) * n^(-1/5)`, where `n` is the count of finite
input values. The estimate MUST be evaluated on a uniform grid of `n` points
(default 256) spanning the data range extended by three bandwidths on each side.

The `bandwidth` setting overrides the computed bandwidth and MUST be positive.

The `n` setting overrides the grid-point count and MUST be at least 2.

The resulting density MUST integrate to approximately 1.

Fewer than two finite input values produces an empty result.

`Density()` (§14.9) desugars to this stat plus an `Area` geometry over the
`(density_x, density)` derived table.

### 15.12 2D Binning Stats

`Bin2D` groups two continuous input columns into deterministic rectangular
bins.

Output columns:

x_start

x_end

x_center

y_start

y_end

y_center

count

density

`HexBin` groups two continuous input columns into deterministic hexagonal bins.

Output columns:

x

y

radius

count

density

Both stats accept `bins`, which MUST be at least 1 and defaults to 30.

### 15.13 Geometry-Producing Stats

> Since version 0.22.

Two derived stats consume a geometry column and produce a geometry column,
bridging spatial data into the derived-table model (§15.3):

- **`Centroid(geom)`** replaces each row's geometry with its centroid `Point`:
  the area-weighted centroid for areal geometries (the shoelace centroid of the
  exterior ring, weighted across a `MultiPolygon`'s parts), and the mean vertex
  otherwise. An empty geometry produces a missing cell.
- **`Simplify(geom, tolerance: t)`** applies Douglas–Peucker simplification to
  each line and polygon ring at `tolerance` `t`, expressed in the geometry's own
  coordinate units (degrees for WGS84 sources). `tolerance` MUST be a
  non-negative number and defaults to a small value. A ring that would simplify
  below a valid four-point ring is kept unchanged. Points are returned
  unchanged.

Both stats are pure and deterministic and read no external resources. Their
output schema passes every scalar column through unchanged and carries the
computed geometry in the original geometry column, so the result renders through
the `Geo` mark (§14.23) like any other geometry source. The stat input MUST be a
single geometry column; a non-geometry input is `E1404`. `Centroid` takes no
settings; an unknown `Simplify` setting or a negative `tolerance` is `E1404`.

### 15.14 Spatial Join

> Since version 0.22.

`SpatialJoin(geom, table: regions, predicate: "within")` joins a point geometry
column against a chart-scoped polygon `Table` by spatial predicate, appending the
polygon table's attributes to each point row:

```ag
Chart(data: GeoJson("stations.geojson")) {
    Table regions = GeoJson("regions.geojson")
    Derive tagged = SpatialJoin(geom, table: regions, predicate: "within")
    Space(geom, data: tagged) { Geo(fill: region_name) }
}
```

- The positional input MUST be a single geometry column (the point side); a
  point is represented by its centroid (§15.13) for the predicate test.
- `table:` MUST name a chart-scoped `Table` declaration (a named table, not a
  derived table) that has a geometry column. A missing `table:`, an unknown
  table, or a table without a geometry column is `E1404`.
- `predicate:` defaults to `"within"`, the only predicate supported in this
  release; any other value is `E1404`.
- The output is a named derived table behind the dataframe boundary (§10.5): the
  point side passes through unchanged, then every non-geometry polygon column
  whose name does not collide with a point-side column is appended. A point that
  matches several polygons takes the **first** in polygon-row order; a point with
  no geometry or no match gets missing cells for the appended columns. Behavior
  is deterministic.

### 15.15 Primitive-Construction Stats

> Since version 0.36.0.

Primitive-construction stats generate ordinary rows for existing primitive marks
instead of introducing ggplot-compatible mark aliases.

`StepVertices(x, y, direction: "hv")` expands source-ordered coordinate rows
into orthogonal path vertices. `direction` defaults to `"hv"` and MUST be
either `"hv"` (horizontal to the new x, then vertical to the new y) or `"vh"`
(vertical at the previous x, then horizontal to the new x); any other value is
`E1404`. The output columns are the input x column name, the input y column
name, and integer `step_group`. The x/y output dtypes match the input column
dtypes. For each contiguous valid run, output order is source order: the first
valid row emits one vertex; every following valid row emits the intermediate
orthogonal vertex followed by the source vertex. A row missing x or y emits one
null sentinel between valid runs, increments `step_group`, and causes `Line`/
`Path` renderers to break the path instead of connecting across the gap.

`JitterPoints(x, y, width: w, height: h)` emits primitive float `x` and `y`
columns with deterministic source-row jitter added to each coordinate, followed
by non-conflicting source columns. `width` and `height` default to `0`, MUST be
non-negative finite numbers, and are measured in the input coordinate's data
units. Required inputs MUST be numeric, temporal, or unknown at analysis time;
runtime rows missing either coordinate are dropped. The jitter function MUST be
stable across platforms and seed-free. A point-layer sugar form
`Point(jitter: [w, h])` over fixed continuous scale domains MUST render
byte-for-byte identically to `JitterPoints(x, y, width: w, height: h)` followed
by primitive `Point()`.

`VectorEndpoints(x, y, angle, length, lengthScale: n)` emits primitive Segment
columns `x`, `y`, `xend`, and `yend`, all floats. The angle is in radians.
`lengthScale` defaults to `1.0` and MUST be a non-negative finite number.
Required inputs MUST be numeric or unknown at analysis time; non-numeric inputs
are `E1404`. Runtime rows missing any required numeric cell are dropped. The
endpoint formula is `xend = x + cos(angle) * length * lengthScale` and
`yend = y + sin(angle) * length * lengthScale`. Non-conflicting source columns
MUST be passed through unchanged so downstream Segment aesthetics can reference
columns such as speed or cohort.

`CurveSample(x0, y0, x1, y1, curvature: c, points: k)` emits sampled path
vertices for one quadratic curve per source row. Output columns are float `x`,
float `y`, integer `link_id`, followed by non-conflicting source columns
repeated on every sampled vertex. The `link_id` is the source row index, so
`Path(group: link_id)` draws one curve per input row. Required inputs MUST be
numeric or unknown at analysis time; non-numeric inputs are `E1404`. Runtime
rows missing any endpoint are dropped. `curvature` defaults to `0.35` and MUST
be finite; negative values bend the opposite way. `points` defaults to `16` and
MUST be an integer in `[2, 1024]`. The control point is the source segment
midpoint plus `curvature * segment_length` along the left-hand perpendicular;
sampling includes both endpoints and is deterministic.

> Since version 0.37.0.

`IntervalSegments(position, lower, upper, orientation: "vertical" |
"horizontal", capWidth: n)` emits primitive `Segment` endpoint rows. Vertical
orientation maps `position` to `x`/`xend` and `lower`/`upper` to `y`/`yend`;
horizontal orientation maps `lower`/`upper` to `x`/`xend` and `position` to
`y`/`yend`. The output columns are `x`, `y`, `xend`, `yend`, string
`interval_role`, integer `interval_id`, followed by non-conflicting source
columns. One stem row is emitted per valid source row. If `capWidth` is present
and positive and the position input is numeric, lower and upper cap rows are
also emitted in source order. Rows missing any required input are dropped.
`orientation` defaults to `"vertical"` for explicit `Derive`; invalid values
are `E1404`. `capWidth` MUST be non-negative and finite.

`IntervalRects(position, lower, upper, orientation: ..., width: n)` emits
primitive `Rect` bounds for interval bodies. Output columns are `xmin`, `xmax`,
`ymin`, `ymax`, `interval_role`, `interval_id`, followed by non-conflicting
source columns. Numeric positions use `width` in data units (default `0.8`) to
compute symmetric position-axis bounds. Categorical positions are passed
through for both position bounds, allowing `Rect` to resolve the full category
band. Rows missing any required input are dropped.

`IntervalMiddles(position, middle, orientation: ..., width: n)` emits primitive
`Segment` endpoint rows for crossbar middle lines. It uses the same orientation
and numeric `width` semantics as `IntervalRects`, and emits `x`, `y`, `xend`,
`yend`, `interval_role`, `interval_id`, followed by non-conflicting source
columns. Rows missing any required input are dropped.

These stats are pure, read no external resources, and MUST preserve deterministic
output ordering. They do not create stroke-style legends, position adjustments,
or source-level aliases such as `geom_step`, `geom_curve`, or `linetype`.

### 15.16 Z-Field Stats

> Since version 0.38.0.

Z-field stats consume numeric x/y coordinates plus either a third positional
numeric input or a named `z:` numeric column. Missing z input is `E1406`,
non-numeric z input is `E1407`, and non-numeric or malformed x/y input is
`E1408`. Unknown-type columns MAY pass analysis and be treated as missing at
render time if their cells are not numeric.

`ContourLines(x, y, z: value, levels: ...)` computes isoline vertices over a
regular x/y/z grid. Duplicate `(x, y)` cells are averaged deterministically.
Missing grid cells cause adjacent cells to be skipped. `levels` MAY be a number
or a strictly increasing numeric array; when omitted, ten interior levels are
used. A numeric `levels: n` emits `n` evenly spaced interior levels between the
finite z minimum and maximum. Output columns are:

`x`, `y`, `level`, `level_index`, `contour_id`.

Rows are ordered by level, y grid index, x grid index, then triangle order.
`Path(group: contour_id, stroke: level)` renders the primitive line form.

`ContourBands(x, y, z, levels: ...)` computes filled contour bands over the same
regular grid model. A numeric `levels: n` means `n` bands; an array supplies the
band breaks and MUST contain at least two strictly increasing finite numbers.
Each output row is a clipped band polygon. Output columns are:

`geom`, `level_low`, `level_high`, `level_mid`, `band_index`.

`Density2D(x, y, bandwidth: n, grid: [nx, ny])` computes a deterministic
bivariate Gaussian KDE. The default bandwidth is Silverman's rule independently
per axis; a positive numeric `bandwidth` applies to both axes. `grid` defaults
to `[64, 64]`, accepts a number or `[nx, ny]`, requires each dimension to be at
least 2, and is capped by the renderer to keep defaults bounded. Output columns
are:

`x`, `y`, `density`.

`Density2DContours` and `Density2DBands` first compute the `Density2D` grid and
then apply the same contour-line or contour-band generation rules to the
`density` field. Their schemas match `ContourLines` and `ContourBands`.

`Summary2D(x, y, z: value, bins: [nx, ny], reducer: "mean")` aggregates finite z
cells into rectangular x/y bins. `bins` accepts a number or two-number array,
defaults to 30, and each dimension MUST be at least 1. Empty bins are omitted.
Reducers are `"count"`, `"mean"`, `"min"`, `"max"`, `"sum"`, and `"median"`.
Median sorts finite values with total floating-point ordering and averages the
two middle values for even counts. Output columns are:

`x_start`, `x_end`, `x_center`, `y_start`, `y_end`, `y_center`, `count`,
`density`, `value`.

`SummaryHex(x, y, z, bins: n, reducer: ...)` aggregates finite z cells on the
same deterministic hex lattice as `HexBin`. Empty hexes are omitted. Output
columns are:

`geom`, `x`, `y`, `radius`, `y_radius`, `count`, `density`, `value`.

All z-field stats are pure, read no external resources, and MUST return stable
row ordering independent of input row order. Their output schemas MUST be
available to analysis, completion, hover, and inlay-hint code before render-time
materialization. Contour labels are not automatic in version 0.38.0; authors MAY
overlay `Text` using separately derived or authored label positions.

## 16. Scale Training

### 16.1 Scale Training Overview

Scale training converts semantic frames and data into functions.

Input:

frame IR

dataframe

viewport

theme defaults

scale declarations

Output:

trained scaled space

trained aesthetic scales

guide models

### 16.2 Position Scale Types

Supported position scales:

continuous linear

categorical band

temporal linear MUST be implemented in version 0.1

log10 MUST be implemented in version 0.2

sqrt MUST be implemented in version 0.23

`reverse: true` for position axes MUST be supported since version 0.2.0
(§16.13).

### 16.3 Continuous Scale

A continuous scale maps numeric domain to pixel range.

Domain:

minimum finite value

maximum finite value

Range:

pixel start

pixel end

Continuous scales SHOULD support expansion padding.

Continuous scales SHOULD generate nice ticks.

Continuous scales MUST handle zero-width domains.

Zero-width domain behavior:

expand symmetrically by a small amount.

If value is zero, use `[-0.5, 0.5]` or implementation-defined default.

### 16.4 Temporal Scale

A temporal scale maps date or datetime values to pixel range.

Temporal scales are required in version 0.1.

Temporal domains are represented internally as normalized temporal values.

Date-only values SHOULD be represented as days since the Unix epoch.

Datetime values SHOULD be represented as microseconds since the Unix epoch.

RFC3339 values with offsets MUST be normalized to UTC instants.

Naive datetime values MUST be treated as UTC-equivalent instants for scale mapping, without applying local timezone or daylight-saving rules.

Temporal scales MUST preserve the distinction between date-only values and datetime values for formatting defaults.

If a column mixes date-only and datetime values, the scale treats the column as datetime and formats labels as datetimes.

Version 0.1 temporal scale mapping MUST NOT perform timezone-aware calendar arithmetic.

Daylight-saving transitions MUST NOT affect temporal scale positions in version 0.1.

Accepted version 0.1 input formats are defined in section 10.3.

Temporal ticks SHOULD use nice calendar intervals.

Version 0.28.0 temporal ticks SHOULD use a deterministic calendar and clock
interval ladder before falling back to numeric interpolation. The ladder SHOULD
include millisecond, second, minute, hour, day, week, month, and year boundaries
where the microsecond representation supports them. Tick counts MUST remain
bounded and deterministic.

Version 0.75.0 extends the automatic ladder between its daily and yearly
rungs. Spans between roughly two weeks and four months MUST offer
Monday-anchored week boundaries (1- and 2-week strides). Month boundaries MUST
offer 1-, 2-, 3-, and 6-month strides counted on the epoch month grid
(January 1970), so multi-month strides keep the same calendar phase every year
(a 3-month stride reads January/April/July/October). Year boundaries MUST
extend their stride ladder far enough (through at least 1000-year strides)
that century-scale domains stay on calendar boundaries. After this extension
the equal-spaced numeric fallback MUST be unreachable for date-precision
domains spanning at least two calendar days; it remains only as a final
safety net.

Version 0.75.0 default temporal tick labels MUST adapt to tick granularity
when no explicit `timeFormat` or `labels:` apply: ticks that all fall on
year starts read `%Y` (`2024`), ticks that all fall on month starts read
`%Y-%m` (`2024-04`), and other day-aligned ticks read `%Y-%m-%d`. Ticks with
sub-day components keep the precision-based defaults (date `%Y-%m-%d`,
datetime `%Y-%m-%d %H:%M`). Label adaptation inspects only the tick instants,
never the host locale, timezone, or wall clock.

When a temporal axis is trained from explicit interval bounds, such as `Rect`
marks generated by `Histogram(interval: ...)` or `Derive ... = Bin(...,
interval: ...)`, axis ticks SHOULD prefer interval centers before falling back
to generic calendar ticks. The axis domain still spans the interval bounds.

Temporal tick labels SHOULD adapt to domain span.

Default examples:

date-only labels may use `Jan 02` or `2026-01-02`.

hourly labels may use `Jan 02 15:04`.

sub-second labels are not required in version 0.1.

### 16.5 Categorical Band Scale

A band scale maps categories to bands.

Domain:

ordered unique categories

Range:

pixel start

pixel end

Settings:

paddingInner

paddingOuter

align

Band scale returns:

band start

band center

band width

### 16.6 Nested Band Scale

Nested band scale composes band scales.

Outer scale maps primary categories to macro bands.

Inner scale maps secondary categories to micro bands inside each macro band.

For expression:

```ag
quarter / type
```

Outer domain:

unique quarters

Inner domain:

unique types

Mapping algorithm:

1. Map row quarter to outer band start.
2. Get outer bandwidth.
3. Map row type inside `[0, outer bandwidth]`.
4. Add inner offset to outer start.
5. Use inner bandwidth as mark width.

### 16.7 Union Scale

Union scale trains on multiple columns.

For continuous domains:

minimum of all members

maximum of all members

For categorical domains:

stable unique categories across members

For temporal domains:

minimum timestamp

maximum timestamp

### 16.8 Aesthetic Scales

Aesthetic scales are trained from mappings.

Fill and stroke:

categorical palette or continuous gradient.

Continuous fill and stroke gradients are required in version 0.1.

Continuous fill and stroke domains MUST support numeric and temporal columns.

Continuous fill and stroke domains SHOULD reject arbitrary string columns with a diagnostic and suggest categorical palette behavior.

Default continuous gradient MUST be deterministic and perceptually ordered.

Recommended default continuous gradient:

`#440154`

`#31688E`

`#35B779`

`#FDE725`

Alpha:

numeric range default `[0.1, 1.0]` or categorical mapping.

Size:

numeric range default `[2, 8]` (radius px). A mapped `size` trains a continuous
scale from the column's domain into this range; `Scale(size:, domain:, range:)`
overrides either end (spec §16.11).

strokeWidth:

numeric range default `[0.5, 4]` (line-width px). A column-mapped `strokeWidth`
on `Line`/`Path` trains a continuous scale into this range and is drawn per
segment from its endpoints' scaled values (spec §13.8, §14.3.1).
`Scale(strokeWidth:, domain:, range:)` overrides either end. A `size` or
`strokeWidth` scale mapped to a non-numeric column is `E1607`.

Shape:

categorical symbol sequence.

### 16.9 Color Palettes

Version 0.1 MUST define default categorical palette.

Palette MUST be deterministic.

Palette SHOULD be colorblind-aware.

Recommended default:

`#4E79A7`

`#F28E2B`

`#E15759`

`#76B7B2`

`#59A14F`

`#EDC948`

`#B07AA1`

`#FF9DA7`

`#9C755F`

`#BAB0AC`

### 16.10 Nice Ticks

Continuous axes SHOULD use nice ticks.

Tick algorithm requirements:

deterministic

bounded tick count

stable for small domain changes

handles negative ranges

handles zero crossing

handles zero-width domain

When a continuous axis scale declares `integer: true`, ticks MUST be whole
integers. The tick stride MUST be the nice step rounded up to at least 1, so a
small domain lands on consecutive integers (1, 2, 3, …) and a large domain
keeps a human-friendly integer stride (2, 5, 10, …). Expansion padding is
unaffected; only the tick values are constrained.

### 16.11 Scale Declarations

Syntax example:

```ag
Scale(axis: x, type: "log10")
Scale(axis: x, type: "categorical")
Scale(axis: x, domain: [0, 100])
Scale(axis: y, reverse: true)
Scale(axis: y, integer: true)
Scale(fill: species, palette: "accent")
Scale(fill: value, gradient: ["#3366cc", "#cc3333"])
Scale(strokeWidth: survivors, domain: [0, null], range: [0, 30])
Scale(stroke: direction, range: ["A" => "burlywood", "R" => "black"])
```

Version 0.2.0 MUST implement source-level `Scale` declarations.

`Scale` declarations MAY appear at chart scope or space scope.

Space-local scale declarations override chart-level declarations for the same target.

Scale targets are `axis`, `fill`, `stroke`, `size`, and `strokeWidth`. A `Scale`
with no target MUST emit `E1204`.

Axis scale targets use `Scale(axis: x, ...)` or `Scale(axis: y, ...)`; axis selectors MUST be bare `x` and `y`, not string literals.

Version 0.2.0 MUST support continuous position scale types `"linear"` and `"log10"`.

Version 0.23.0 MUST support the continuous position scale type `"sqrt"`, a
square-root transform for non-negative continuous axes. Tick values are nice
data values (as for `"linear"`), positioned along the axis by their square root.
A `"sqrt"` scale on a non-numeric axis MUST emit `R0004`, as MUST a `"sqrt"`
scale whose declared `domain` contains a negative bound. When the trained domain
or declared bound is negative the renderer falls back to a linear axis rather
than producing `NaN` positions.

Version 0.65.0 MUST support the position scale type `"categorical"`, which
forces the targeted scalar position axis to train as a categorical band axis
without changing the backing column's inferred data type. The override applies
to non-geometry scalar axes, including numeric and temporal columns. Category
keys MUST use the same deterministic formatting as ordinary categorical domain
training: integers as decimal strings, floats through the float-category
formatter, temporals as UTC RFC3339 strings, and strings/booleans/mixed values
as their existing category text. Missing values produce no category and are
skipped by marks as on ordinary band axes.

`Scale(axis: x, type: "categorical")` and `Scale(axis: y, type:
"categorical")` are axis-only declarations. Aesthetic scales MUST reject
`type: "categorical"` with `E1204`. Numeric domain bounds, `breaks:`, and
`integer:` are continuous-axis controls and MUST produce diagnostics when
combined with `type: "categorical"`. String-array `domain:` values remain valid
and order the formatted category keys. Geometry columns and blended/union axes
MUST NOT be silently coerced to categorical axes; implementations MUST diagnose
those unsupported combinations.

Version 0.75.0 MUST support the explicit position scale type `"temporal"`:

```ag
Scale(axis: x, type: "temporal")
Scale(axis: y, type: "temporal")
```

`type: "temporal"` is axis-only; aesthetic scales reject it with `E1204` like
every other scale type. Temporal columns already train temporal axes
automatically, so the explicit declaration is an assertion: it documents the
author's intent and guards against silent categorical fallback. It MUST NOT
coerce non-temporal columns into dates at render time; a `type: "temporal"`
declaration on a known non-temporal axis column MUST emit `R0004` and fall
back to the column's natural axis. `integer: true` combined with
`type: "temporal"` MUST emit `E1204`. `type: "categorical"` remains the
explicit opt-out for authors who want temporal values treated as ordered
category bands.

Version 0.2.0 MUST support numeric position domains with `domain: [min, max]`.

Version 0.6.0 MUST allow either element of a numeric `domain` (and `range`)
array to be `null`, meaning "infer this bound from the data" (e.g.
`domain: [0, null]`). A `domain`/`range` array that is not two elements, or whose
non-null elements are not finite numbers, MUST emit a diagnostic (`E1204` for
`domain`, `E1603` for `range`). `domain` applies to axis, `size`, and
`strokeWidth` scales; `range` (numeric) applies to `size` and `strokeWidth`
scales. A numeric `range` on an axis or color scale MUST emit `E1603`. (`E1605`
is reserved for a `null` bound used where data inference is not meaningful.)

Version 0.6.0 MUST support a numeric output `range: [lo, hi]` on `size` and
`strokeWidth` scales, mapping the trained domain into that pixel range
(spec §16.8). Either bound MAY be `null` to use the default range end.

Version 0.61.0 MUST support string-array `domain:` values on position-axis
scales for categorical and nested-band axes:

```ag
Scale(axis: x, domain: ["Trips", "Revenue", "Stations"])
```

Declared categories MUST lead the trained categorical domain in source order.
Declared categories with no matching rows MUST still reserve bands. Observed
data categories not listed in the declaration MUST be appended in first-
appearance order and SHOULD emit `R0004` so authors know the declared order was
incomplete. Empty string domains and duplicate declared categories MUST emit
`E1604`. String-array domains on aesthetic scales MUST emit `E1606`; string-
array domains that cannot apply to a continuous position axis MUST emit `R0004`
at render time.

Algraf does not expose a visible no-op `Blank` mark in version 0.36.0. The
limit-anchor use case MUST be expressed with explicit scale domains, for
example `Scale(axis: x, domain: [0, 100])` or `Scale(axis: y, domain: [0,
null])`.

Version 0.40 MUST support exact `breaks:` arrays for position axes and
continuous/binned color, `size`, and `strokeWidth` legends. Numeric break arrays
MUST contain finite strictly increasing values. Temporal axis breaks MAY be
written as temporal literals such as `date("2026-01-01")` or
`datetime("2026-01-01T00:00:00Z")`; they are normalized to the same UTC-equivalent
internal representation as temporal domains. A `labels:` array paired with
`breaks:` overrides tick or legend text positionally and MUST have the same
length as `breaks`, otherwise `E1604`. A labels array without breaks MUST emit
`E1604`.

Version 0.75.0 MUST support generated calendar tick cadences on temporal axes
through `tickInterval`:

```ag
Scale(axis: x, tickInterval: "3 months")
Scale(axis: x, tickInterval: "1 week")
Scale(axis: y, tickInterval: "6 hours")
```

`tickInterval` accepts a string of the form `"<unit>"` or `"<count> <unit>"`
with a positive integer count and a singular or plural unit from:
`millisecond`, `second`, `minute`, `hour`, `day`, `week`, `month`, `quarter`,
and `year`. Malformed values — zero, negative, or fractional counts, unknown
units, extra tokens, or non-string values — MUST emit `E1204`. `tickInterval`
on an aesthetic scale MUST emit `E1204`; combined with `type: "categorical"`,
`"log10"`, or `"sqrt"` it MUST emit `E1608`, and on an axis whose column is
known non-temporal at render time it MUST emit `E1608` as a warning and be
ignored. When exact `breaks:` are declared on the same axis, `breaks:` win and
`tickInterval` MUST emit the warning `E1609`.

Generated ticks anchor to fixed unit grids, never to the trained domain start,
so a cadence keeps the same calendar phase on any domain:

- month and quarter steps count on the epoch month grid (January 1970); step
  counts that divide 12 — including `"3 months"`/`"1 quarter"` and
  `"6 months"` — land on the same months every year (January/April/July/
  October and January/July respectively);
- year steps land on years divisible by the step count;
- week steps count from the ISO Monday grid;
- day steps count from the Unix epoch day grid;
- hour, minute, second, and millisecond steps restart at every UTC-equivalent
  midnight, so `"6 hours"` reads 00:00/06:00/12:00/18:00.

Version 0.75.0 also makes explicit temporal `breaks:` exact in rendering:
declared break instants inside the trained domain all become ticks, with no
index thinning (before 0.75.0, arrays longer than the automatic budget were
deterministically thinned). Label overlap remains the responsibility of
guide-planning label thinning (§19.4).

Ticks outside the trained domain are not emitted; ticks on exact domain
boundaries are. Generated ticks MUST remain bounded: when the requested
cadence exceeds the interval tick budget, the renderer MUST promote the step
count by the smallest integer multiple that fits the budget — preserving the
unit grid phase — rather than index-thinning or falling back to numeric
interpolation, and SHOULD emit `R0004` describing the promotion. Interval
ticks are exempt from the automatic-tick label budget; visual label overlap
remains the responsibility of guide-planning label thinning (§19.4), which
keeps the ticks and grid lines.

`timeFormat` (§19.4), `tickLabelAngle`, and `tickLabelRows` compose with
`tickInterval` unchanged: the scale controls tick positions, the guide
controls their presentation.

Version 0.40 MUST support scale expansion through `expand:` or `expansion:`.
A single number is interpreted as multiplicative expansion with additive
expansion `0`. A two-number array is `[mult, add]`. Continuous axes expand by
`span * mult + add` on both sides before explicit domain bounds are applied.
Temporal axes use the same rule in microsecond units. Categorical axes use
`mult` as deterministic outer band padding. Explicit `domain:` bounds remain
data-domain training constraints and are not row filters or visual coordinate
zoom controls; Cartesian data marks are still clipped to the final plot
rectangle by default after scale mapping (§18.5). Visual coordinate zoom is a
separate `Space` coordinate-view control (§16.17).

Version 0.2.0 MUST support `reverse: true` for position axes.

Version 0.3.0 MUST support `integer: true` for continuous position axes, which
constrains axis ticks to whole integers (see §16.10). `integer` applies only to
axis scales; using it on a `fill`/`stroke` scale MUST emit `E1204`, as must a
non-boolean value.

Version 0.2.0 MUST support categorical `fill` and `stroke` palette selection with `palette: "default"` and `palette: "accent"`.

Version 0.3.0 MUST support continuous `fill` and `stroke` gradient selection
with `gradient: [...]`.

`gradient` MUST accept an ordered array of two or more color string literals.
Version 0.68.5 MUST accept gradient color literals written as hex colors
(`#rgb` or `#rrggbb`), alpha hex colors (`#rgba` or `#rrggbbaa`), safe
enumerated color names, `rgb(r, g, b)`, or `rgba(r, g, b, a)`. `rgb`/`rgba`
channels MUST be numeric bytes in `[0, 255]`; `rgba` alpha MUST be a finite
number in `[0, 1]`. Renderers MUST respect color alpha when interpolating
gradient stops and MUST continue to emit `E1601` for invalid gradient color
literals.

Gradient stops MUST be interpolated evenly across the trained continuous
domain.

Version 0.20.0 MUST also support positioned gradient stops:

```ag
Scale(fill: value, gradient: [
    Stop(value: 0, color: "#3366cc"),
    Stop(value: 100, color: "#cc3333"),
])
```

String stops and `Stop(...)` values MUST NOT be mixed in one gradient. Stop
values are domain values and MUST be strictly increasing. Colors use the
gradient color validation rules above.

`gradient` MUST be valid only for continuous color mappings. Invalid gradient
arrays MUST emit `E1601`; using `gradient` with a categorical mapping MUST emit
`E1602`.

Invalid scale/domain combinations MUST emit targeted diagnostics.

Version 0.40 MUST support `mode: "binned"` for numeric `fill` and `stroke`
scales. Binned color scales classify the original numeric column during scale
training and rendering; authors do not need a prepared categorical helper
column. Explicit `breaks:` define left-closed/right-open bins, with the final
bin open-ended. If no breaks are provided, the implementation creates
deterministic equal-width default bins over the trained domain. `range:` for a
binned color scale is an ordered color-string array; when both `range:` and
`breaks:` are present their lengths MUST match, otherwise `E1604`. Legend
entries are discrete, ordered by bin, and use generated labels like
`0-10`/`10+` unless overridden by a `labels:` array.

Version 0.40 MUST support `mode: "identity"` for string-like `fill` and
`stroke` mappings. Identity color scales use the data cell directly as the SVG
color only after deterministic safety validation. Accepted values are hex
colors (`#rgb` or `#rrggbb`) or the implementation's enumerated safe SVG color
names. Arbitrary CSS, URLs, functions, variables, and style strings MUST NOT be
accepted. Identity color scales produce no legend by default and MUST reject
other mapping controls such as `palette`, `gradient`, `range`, `breaks`,
`labels`, and `domain`.

Version 0.2.0 MUST support a `label` argument on `fill`/`stroke` scales that
overrides the column-derived legend title (see §16.13).

Z-field derived columns such as `level`, `level_mid`, `density`, and `value`
are ordinary numeric columns for scale training. A `fill` or `stroke` mapping to
one of these columns MUST create the same continuous color scale and legend as a
mapping to any user-authored numeric column.

### 16.12 Scale Resolution

When a geometry requests a coordinate, it should not know whether the space is nested.

Recommended API:

```rust
impl ScaledSpace {
    pub fn resolve_x(&self, row: &Row) -> Option<f64>;
    pub fn resolve_y(&self, row: &Row) -> Option<f64>;
    pub fn x_bandwidth(&self, row: &Row) -> Option<f64>;
    pub fn y_bandwidth(&self, row: &Row) -> Option<f64>;
}
```

The geometry uses this API.

Nested behavior is encapsulated in `ScaledSpace`.

### 16.13 Scale-Driven Legend Labels

> Promoted to a v0.2.0 requirement; see `docs/V0_2_PLAN.md`.

A `fill` or `stroke` scale MAY carry a `label` string:

```ag
Scale(fill: species, label: "Penguin Species")
```

When present, the label MUST be used as the legend title for that aesthetic
instead of the column-derived default. `label` MUST be a string literal; a
non-string value MUST emit `E1204`.

The named categorical palette registry recognizes `"default"` and `"accent"`.
Unknown palette names MUST emit `E1204`.

#### 16.13.1 Manual Categorical Colors and Renamed Entries

A categorical `fill`/`stroke` scale MAY take a **map** `range:` assigning each
category an explicit color, and a map `labels:` renaming legend entries:

```ag
Scale(stroke: direction,
      range:  ["A" => "burlywood", "R" => "black"],
      labels: ["A" => "Advance",   "R" => "Retreat"],
      label:  "Direction")
```

Map keys define category and legend-entry order, so a manual `range:` map needs
no separate `domain`. Explicit colors override the palette; renamed labels flow
into the scale-driven legend (each entry's swatch keeps its mapped color, its
text uses the renamed label). When both `range:` and `labels:` maps are present
their key sets MUST match, otherwise `E1604`. Map keys and values MUST be string
literals (`E1604` otherwise). A map `range:`/`labels:` on a non-categorical or
non-color scale MUST emit `E1606`; a numeric `range:` on a categorical color
scale MUST emit `E1603`.

A `size` scale (`Scale(size: col, range:, label:)`) whose only downstream
consumer is a `Glyph` call's `size:` argument MUST produce a size-legend
candidate, with the scale's `label:` — or the default column display name when
`label:` is omitted — as the swatch title (since version 0.72). The candidate
dedupes against any same-aesthetic, same-title chart-scope scale via the
normal legend-merge rules. Glyph-body scales take precedence over chart-scope
scales for the same `(aesthetic, column)` pair (§14.27).

### 16.14 Projection

> Since version 0.8.

A spatial space MAY declare a cartographic **projection** as a `Space` argument:

```ag
Space(geom, projection: "albers_usa") { Geo(fill: population) }
```

`projection:` MUST be a string literal — a friendly alias or a raw PROJ string
(`+proj=…`). The source CRS is WGS84 (`EPSG:4326`) lon/lat. The alias registry
maps over [`proj4rs`]:

| alias | meaning |
| --- | --- |
| `equirectangular` | plate carrée (planar lon/lat); the default when `projection:` is omitted |
| `mercator` | Web-style Mercator |
| `robinson` | Robinson |
| `albers` | continental-US Albers equal-area (lower-48) |
| `albers_usa` | `albersUsa`-style composite: lower-48 Albers plus conventional Alaska and Hawaii insets |

The `albers_usa` composite routes each coordinate to one of three conic
equal-area sub-projections by geographic region — Hawaii (latitude 16°–26°N,
longitude 165°–150°W), Alaska (latitude ≥ 50°N, including the Aleutians), and
the lower-48 (everything else) — then scales and translates the Alaska
(≈0.35×) and Hawaii insets into the lower-48 frame following d3-geo's
`albersUsa` offsets. Region routing and inset placement are deterministic and
data-independent (the offsets are fixed fractions of the projection scale), so
lower-48-only maps keep the same appearance as the `albers` alias. Coordinates a
sub-projection cannot represent are dropped, as for any projection.

A raw `+proj=…` string passes through to `proj4rs` unchanged. A non-string
`projection:` is `E1802` (the analyzer); an unknown alias or malformed PROJ
string is `E1802` (the renderer, where the registry lives). When `projection:`
is omitted but the frame is a geometry column, the default equirectangular
projection applies so raw lon/lat maps degrade gracefully.

Overlaid spatial spaces MUST declare the same projection; a conflict is `E1803`
(mirroring shared position scales, §17.5).

### 16.15 Spatial Scale

> Since version 0.8.

A spatial frame (§8.8) trains a **spatial scale** in place of independent x/y
position scales. The renderer MUST:

1. iterate the geometry (or `long * lat`) coordinates and compute the geographic
   bounding box,
2. project the box through the declared projection (sampling vertices for
   non-affine projections),
3. fit the projected box into the plot rectangle **preserving aspect ratio**
   (letterbox), so equal-area maps are never stretched,
4. map geographic → projected → pixel for every rendered coordinate.

A spatial space MUST draw no automatic latitude/longitude axes or grid lines; a
longitude/latitude grid is drawn only by an explicit `Graticule` mark (§14.24).
Output MUST stay deterministic (§18.12): `proj4rs` floating-point trig is
absorbed by the 3-decimal SVG coordinate formatter.

When projecting `Geo` line and polygon rings, the renderer MUST:

- **Resample long segments.** A segment whose longitude or latitude span exceeds
  a fixed threshold (5°) is subdivided into equal sub-segments no larger than the
  threshold before projection, so a long edge follows the projection's curvature
  instead of a straight pixel chord. A shorter segment is projected
  vertex-for-vertex, so existing detailed-boundary maps render unchanged. The
  threshold is fixed and deterministic.
- **Break antimeridian crossings.** When the longitude step between consecutive
  vertices exceeds 180°, the connecting chord is broken (a new subpath begins)
  rather than drawn across the whole map.

A spatial frame whose column is not a geometry column is `E1801`.

### 16.16 Polar Coordinate Transform

> Since version 0.26.

A space with `coords: "polar"` (§4.2) trains its position scales exactly as a
Cartesian space — continuous, temporal, and band domains are unchanged — and
remaps only the pixel **range** each axis occupies:

- The **theta axis** (selected by `theta`) maps its domain to an angular range
  whose origin and sweep are configurable (since version 0.31). The default
  origin is the 12-o'clock position (`-π/2`) and the default sweep is clockwise,
  giving the range `[-π/2, 3π/2]` and reproducing the fixed behavior of versions
  0.26–0.30. A space MAY rotate the origin with `startAngle` (degrees clockwise
  from 12 o'clock, in `[-360, 360]`; otherwise `E1909`) and reverse the sweep
  with `direction` (`"clockwise"` default | `"counterclockwise"`; otherwise
  `E1910`). The theta-domain minimum maps to `startAngle` and the maximum to one
  full turn away, clockwise or counterclockwise. These arguments do not change
  domain training, only the angular range each datum maps into; existing polar
  output is unchanged when both are absent.
- The **radius axis** (the other frame axis) maps its domain to
  `[innerRadius · R, R]`, where `R = min(plot.width, plot.height) / 2` and the
  polar center is the plot rectangle's midpoint. When the theta axis is
  categorical, `R` is reduced to reserve room for the perimeter category labels
  (§19.8) so they stay within the plot rectangle (e.g. clear of the legend); a
  continuous angle (pie/donut) draws no perimeter labels and keeps the full `R`.

Final pixel positions are `x = cx + r·cos(θ)`, `y = cy + r·sin(θ)`, so
point-like geometries need no polar awareness: the space resolves each datum to a
Cartesian pixel. Area-filling geometries additionally read raw `(θ, r)` band and
value extents to draw wedges and annular segments (§15, §18). A categorical theta
axis tiles the full circle without band padding so adjacent wedges abut.

A 1D polar frame (e.g. `Space(amount)`) has no radius axis: the single value
wraps around the angle and the radius spans the full `[innerRadius · R, R]`
annulus — this is the pie/donut form. Polar is opt-in; a space without `coords`
is Cartesian and its output is unchanged.

**Radial bar chart (since version 0.31).** A `Bar` in a polar space MAY carry a
categorical `radius:` mapping (idiomatically with `theta: "y"`). This selects the
*radial bar* mode: each distinct category of the mapped column occupies its own
concentric ring (the annulus is divided into equal-width rings, outermost first),
and the theta axis — the frame's value — drives each bar's independent angular
sweep from `startAngle` to the value's angle. This is distinct from the
cumulative pie path (continuous angle, no `radius:` mapping, wedges accumulate
around the circle) and the coxcomb path (categorical angle, value radius). The
`radius:` mapping MUST resolve to a categorical column and MUST appear on a polar
`Bar`; otherwise `E1910`. The mode reuses the same wedge/annular-segment emission
as other polar bars and adds no new geometry.

### 16.17 Cartesian Coordinate View

> Since version 0.41.

A Cartesian `Space` MAY declare visual coordinate controls:

```ag
Space(x * y, zoomX: [0, 10], zoomY: [null, 100], aspect: 1) { ... }
```

`zoomX` and `zoomY` are two-element arrays of numeric, temporal, or `null`
bounds. A `null` bound preserves the trained scale bound on that side. Temporal
bounds MAY use `date(...)` or `datetime(...)` literals and are normalized to UTC
microseconds. Non-finite numbers, arrays of any other length, and non-temporal
call values MUST emit `E1204`.

Coordinate zoom is visual-only. It MUST be applied after source data loading,
derived stat materialization, stat-generated geometry rows, scale-domain
training, scale expansion, and explicit `Scale(domain:)` bounds. It changes the
axis view domain and guide ticks, but MUST NOT filter rows before stats or
before draw-list/sidecar mark enumeration.

Zoomed Cartesian panels MUST clip data marks to the final plot rectangle. SVG
uses a deterministic `clipPath`; the draw-list backend emits matching
`clipStart`/`clipEnd` scope ops; the render-model raster backend applies the
same rectangular mask. The interaction sidecar keeps per-row mark coordinates;
marks whose point coordinate lies outside a clipped panel carry
`"clipped": true`, and each clipped plot carries `clip_rect` equal to the plot
rectangle. Host runtimes MAY use `clip_rect` to hide or ignore clipped marks.

`aspect` is a positive finite number specifying the ratio of x pixels per data
unit to y pixels per data unit. The renderer MUST preserve the chart viewport,
margins, legends, facet strips, and facet grid allocation, then shrink and
center the Cartesian plot rectangle inside its allocated panel when needed. It
MUST apply only when both axes have continuous or temporal data-unit spans; it
is ignored for categorical, polar, and spatial spaces. `aspect: 1` makes equal
x/y data-unit distances visually equal after the final layout.

### 16.18 Scale Training Scope

> Since version 0.71.0.

`Scale(...)` gains an optional `train:` property controlling how the scale is
trained across repeated instances of an enclosing glyph mark (§14.27):

```ag
Scale(y: gdp, train: "local")     // each glyph instance auto-scales y
Scale(x: year, train: "shared")   // all glyph instances share the x domain
```

- `train:` MUST be `"shared"` or `"local"`. An invalid value is `E1606`.
- `train: "shared"` trains the scale across the union of all instances of the
  enclosing glyph within its host space.
- `train: "local"` trains the scale per glyph instance.
- When `train:` is absent, the glyph's `scales:` default (§7.11) applies; outside
  a glyph, `train:` has no effect.

This subsumes two older vocabularies. The facet `Layout(facetScales: "free-x")`
control (§17.4) is equivalent to `train: "local"` on the x position scale and is
retained as facet sugar. The removed inset `scales: "shared" | "local"`
becomes the glyph-level default that `train:` overrides per scale. A position or
data-trained scale under `train: "local"` produces no chart-level legend (no
shared domain); aesthetic scales with a fixed domain always merge (§17.7).

## 17. Layout

### 17.1 Viewport Model

SVG root has width and height.

Default width:

800

Default height:

520

Chart margins reserve space for guides and labels.

Plot area is the inner rectangle where data marks render.

### 17.2 Layout Rectangles

Recommended structures:

```rust
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
```

```rust
pub struct Layout {
    pub svg: Rect,
    pub plot: Rect,
    pub title: Option<Rect>,
    pub subtitle: Option<Rect>,
    pub caption: Option<Rect>,
    pub x_axis: Option<Rect>,
    pub y_axis: Option<Rect>,
    pub legend: Option<Rect>,
}
```

### 17.3 Margin Calculation

Version 0.1 MAY use fixed margins.

Recommended defaults:

top 40

right 30

bottom 50

left 60

If a legend is present, the side named by the resolved theme's
`legendPosition` increases to reserve the measured legend rectangle. Valid
positions are `"right"` (default), `"bottom"`, `"top"`, and `"left"`.

Version 0.79.0: the measured legend rectangle MUST be derived from the
collected legend titles and entry labels before the final layout pass. Right
and left legends reserve measured width plus the plot/legend gap; top and
bottom legends reserve measured wrapped height plus the plot/legend gap.

If title present, top margin increases.

If x tick labels rotated, bottom margin increases.

Dynamic text measurement is hard in pure SVG.

Version 0.1 MAY approximate text dimensions.

`Chart` MAY set a per-side margin via `marginTop`, `marginRight`,
`marginBottom`, and `marginLeft` (each a non-negative integer in pixels). When
an argument is absent, the computed default for that side is unchanged.

When the chart has axes, a configured value acts as a floor: the margin for that
side is widened to at least the configured value, composed with the computed
margin (`max(computed, configured)`). Because it is a floor, it never shrinks a
side below what the content requires (axis line, tick labels, title, legend
reserve).

When the chart has no axes (its resolved theme disables them — e.g. the `void`
theme), the base margin is pure padding rather than content reserve. A
configured value then sets that side exactly, and MAY be `0` to bleed marks to
the viewport edge (useful for embedded sparklines). Chart title, subtitle, and
caption reserve still act as a floor on the sides that carry them, so explicit
text is never clipped.

Version 0.82.0: layout reserves the axis rectangle on the side chosen by the
resolved axis position (§19.2, §19.3). The larger axis-bearing margin and the
smaller opposite margin swap with the side: a right y axis reserves the wide
margin on the right (the left becomes light padding), and a top x axis reserves
the wide margin on the top. The y tick-label/title width and x tick-label/title
height are reserved on whichever side now carries the axis. The
`marginTop`/`marginRight`/`marginBottom`/`marginLeft` floors continue to compose
as `max(computed, configured)` on whichever side carries the axis.

Version 0.82.0: `Chart(caption: "...")` MUST honor newline (`\n`) characters,
rendering each line as a separate stacked text line below the plot in source
order, reusing the per-line escaping rule from `Text` (§14.16). `Chart` MAY
include a `source: "..."` string that renders as a final, de-emphasized line (or
lines, also honoring `\n`) below the caption, styled by the `plotSource` theme
token (§20.1). Layout MUST reserve the measured multi-line height for the
caption-plus-source block so no line is clipped or overlaps the x axis. A
non-string `caption:` or `source:` MUST emit the chart-argument type diagnostic.
Badge sizing and these reserves use the deterministic approximate
text-measurement model so output stays byte-stable.

### 17.4 Facet Layout

Faceting uses nested spaces over a Cartesian plane.

Expression:

```ag
(x * y) / species
```

Facet layout maps each species to a panel.

Facet settings are supported in version 0.1 through `Layout`.

```ag
Layout(facetColumns: 3)
```

Version 0.1 MUST implement facet wrap.

Facets share x and y scales by default.

The renderer MUST choose a default facet column count automatically when `facetColumns` is absent.

Default column count SHOULD produce a compact grid based on panel count and viewport aspect ratio.

Facet rows are derived from panel count and column count.

Facet labels are guides.

Version 0.41 MUST also support facet grids through `Layout(facetRows: col,
facetCols: col)`. Row and column facet domains use first-appearance categorical
order, and panels are assigned row-major. Empty row/column combinations MUST
still reserve plot rectangles and strips. When either `facetRows` or `facetCols`
is omitted, that dimension has one implicit level.

`Layout(facetScales: mode)` controls per-facet position-scale training. The
accepted string modes are `"fixed"` (default), `"free-x"`/`"free_x"`,
`"free-y"`/`"free_y"`, and `"free"`. Free scales are panel-local for axes named
by the mode; the other axes continue to share the full faceted data domain.
Coordinate zoom, when present, is applied after each panel's fixed or free
domain has been trained. Since version 0.71.0 `facetScales` is facet sugar over
the per-`Scale` `train:` mechanism (§16.18): a free axis is equivalent to
`train: "local"` on that position scale.

`Layout(facetLabel: "value" | "name-value")` controls strip text. `"value"` is
the default. `"name-value"` prefixes each value with its facet column name.
`Layout(facetLabels: ["raw" => "Label"])` MAY map raw facet values to custom
strip labels. Map keys and values MUST be string literals (`E1204` otherwise).

`Layout(panelSpacing: n)` sets both horizontal and vertical facet gaps in pixels.
`Layout(panelSpacing: [x, y])` sets them independently. Values MUST be
non-negative finite numbers (`E1204` otherwise).

### 17.5 Multiple Space Blocks

Multiple `Space` blocks in the same chart may overlay in the same plot area if compatible.

Example:

```ag
Space(time * (lower + upper)) {
    Ribbon(ymin: lower, ymax: upper)
}

Space(time * estimate) {
    Line()
}
```

The analyzer SHOULD detect compatible spaces.

Compatible spaces MAY share position scales.

Incompatible spaces MAY require separate panels or produce diagnostics.

Version 0.1 SHOULD allow overlays when x and y domains are compatible.

Version 0.6.0: when compatible spaces overlay but back onto different tables
(e.g. one bound to `Chart(data:)` and another to a named `Table`, spec §10.10),
their continuous and temporal position-scale domains MUST be unioned across all
contributing tables, so the secondary layer aligns with the primary in the
shared plot area. A bound that a space has locked (a `fill`-layout bar, a `Rect`)
is not widened by this union.

Version 0.78.0: the zero-baseline requirement participates in that union. When
any overlaid space requires a numeric axis domain to include zero (a `Bar`, or
an `Area` with a zero baseline), every compatible overlaid space sharing that
axis MUST adopt the requirement, so all spaces resolve the same domain padding
and train identical domains. Without this, a secondary layer whose own data
does not reach zero pads below zero while the zero-pinned primary does not, and
the secondary layer's marks render shifted by the padding amount even though
the unioned min/max extents agree. Spaces with locked bounds keep their exact
values and do not adopt the requirement. An explicit chart-level
`Scale(axis: …, domain: […])` continues to apply to every overlaid space's
config and therefore overrides the shared trained domain in all of them at
once.

Version 0.8: overlaid **spatial** spaces share one spatial scale (§16.15). The
renderer MUST union the projected bounding boxes of all spatial spaces and fit
them once, so a projected `long * lat` point overlay aligns with a `Space(geom)`
basemap under the same projection. All overlaid spatial spaces MUST declare the
same projection (`E1803` on conflict).

Multiple `Space` blocks are distinct from multiple `Chart` blocks (spec §7.1).
Spaces within one chart may overlay in a shared plot area and share position
scales, guides, theme, and layout. Separate charts share none of these and
render to separate outputs.

### 17.6 Layer Order

Rendering order follows source order.

Earlier geometries render below later geometries.

Earlier spaces render below later spaces.

Guides render above or outside plot depending on guide type.

Background renders first.

Grid renders before data marks.

### 17.7 Legend Merging for Glyphs

> Since version 0.71.0.

Glyph internal scales flow into the chart legend collection exactly like any
mark's scales, deduplicated by `(aesthetic, domain)`. N glyph instances that
share one `fill: category` scale yield one legend. Shared-scale glyph legends
MUST use the same planned row subset used for child scale training. Per-call
suppression reuses the ordinary mark legend control (`legend: false`); a
position or data-trained scale under `train: "local"` (§16.18) contributes no
chart-level legend because it has no single shared domain.

## 18. SVG Rendering

### 18.1 Rendering Overview

Rendering is a deterministic transformation:

AST plus data becomes IR.

IR plus data becomes trained render model.

Render model becomes SVG string.

The renderer MUST escape all user-provided text.

The renderer MUST escape attribute values.

The renderer MUST produce valid XML-compatible SVG.

The renderer SHOULD include `xmlns`.

Version 0.1 SVG output MUST use inline attributes for mark, guide, and theme styling.

Polar geometries (§16.16) emit `<path>` elements using the SVG arc (`A`) command:
a solid wedge (`M … A … L center Z`) or an annular segment (outer arc forward,
inner arc back). Polar `Line`/`Area` emit closed `<path>` polygons. Arc and
polygon coordinates use the same 3-decimal deterministic formatting as all other
SVG numbers (§18.8).

Version 0.1 SVG output MUST NOT depend on embedded CSS for core visual appearance.

Class attributes MAY still be emitted for debugging, testing, and downstream selection, but the SVG MUST render correctly if class selectors are ignored.

### 18.2 SVG Root

Example:

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="800" height="520" viewBox="0 0 800 520" role="img">
</svg>
```

Root SHOULD include:

`xmlns`

`width`

`height`

`viewBox`

`role`

`aria-label` or `aria-labelledby` when title exists.

### 18.3 SVG Groups

Recommended group order:

background

title

subtitle

plot background

grid

geometries

axes

legends

caption

debug metadata if requested

Group classes SHOULD be stable.

Example:

```svg
<g class="algraf-plot-area">
<g class="algraf-grid">
<g class="algraf-layer algraf-geom-point">
```

### 18.4 Coordinate System

SVG y increases downward.

Data y usually increases upward.

Y scales MUST invert range for Cartesian plots.

For plot rect:

x range is `[plot.x, plot.x + plot.width]`.

y range is `[plot.y + plot.height, plot.y]`.

### 18.5 Clipping

Data marks SHOULD be clipped to plot area by default.

Version 0.80.0: Cartesian panels MUST open a deterministic rectangular clip
scope around data-mark layers by default, regardless of whether the axis view
comes from data-trained domains, explicit `Scale(axis: ..., domain: ...)`
bounds, or `Space(zoomX:/zoomY:)`. The clip scope is applied after source data
loading, derived stat materialization, scale-domain training, scale expansion,
explicit scale bounds, and coordinate zoom. It MUST NOT filter source rows or
stat rows before geometry emission.

Glyph clips MAY be rectangular or circular, according to a glyph mark's `clip:`
(§14.27).

SVG clip path IDs MUST be deterministic.

If multiple charts appear in one document later, IDs MUST be unique.

Version 0.1 can derive IDs from stable counters.

### 18.6 Rendering Trait

Recommended trait:

```rust
pub trait RenderableGeometry {
    fn render_svg(&self, ctx: &RenderContext, layer: &LayerModel) -> Result<String, RenderError>;
}
```

Render context includes:

layout

theme

trained scales

palette

formatter settings

### 18.7 Render Model

Recommended render model:

```rust
pub struct RenderModel {
    pub layout: Layout,
    pub layers: Vec<LayerModel>,
    pub guides: Vec<GuideModel>,
    pub theme: Theme,
}
```

Layer model:

```rust
pub struct LayerModel {
    pub geometry: GeometryKind,
    pub data: DataFrame,
    pub space: ScaledSpace,
    pub aesthetics: ResolvedAesthetics,
    pub span: Span,
}
```

The realized renderer resolves an equivalent per-panel scene during planning and
hands it to the SVG backend during emission; see §24.6 for the planning/emission
boundary these structures sit on.

Geometry and guide emission do not write SVG directly. They describe each
primitive — rectangle, circle, path, polygon, line, or text — to a shared,
backend-neutral *mark sink*. The SVG backend's sink serializes each primitive to
the deterministic SVG of this section; the draw-list backend's sink records an
equivalent op (§24.6). Because both backends observe the same primitive calls,
they agree on coordinates and colors by construction, and a new geometry or
guide primitive reaches every backend at once.

Glyph-mark planning is recursive over the same planned scene. For each glyph
instance, planning MUST resolve the host anchor in pixel coordinates, match
child rows explicitly, allocate the child viewport, train each child space using
the declared shared/local policy (§16.18), resolve child themes/guides/scales,
and store the resulting child panels before any backend emits output. Emission
MUST consume those planned child panels and emit child layers through the same
mark sink using absolute coordinates; backends MUST NOT recompute matches,
anchors, scale domains, guide visibility, or row subsets. Nested glyphs MUST be
supported with a deterministic maximum depth of at least 8; exceeding that
limit emits `E2209` and skips the over-depth child scene. Recursive mark
budgets MUST estimate matched child output before emission and emit `E2210`
when the configured budget would be exceeded. Glyph clipping MUST be
represented as rectangular or circular clip scopes in SVG, draw-list, and
raster output, or omitted when `clip: false`.

### 18.8 Path Formatting

Numeric SVG values SHOULD be rounded deterministically.

Default precision:

3 decimal places or fewer where possible.

The renderer SHOULD avoid excessive trailing zeros.

The renderer MUST avoid locale-dependent formatting.

Decimal separator MUST be `.`.

### 18.9 Text Escaping

Text nodes MUST escape:

`&`

`<`

`>`

Attribute values MUST escape:

`&`

`<`

`>`

`"`

The renderer MUST NOT inject raw user strings into SVG.

### 18.10 Accessibility

SVG MUST include `<title>` when `Chart(title:)` exists. The SVG root MUST carry
an `aria-label` using `Chart(alt:)` when present, otherwise the chart title when
present. SVG MUST include `<desc>` when `Chart(description:)` exists; otherwise
it SHOULD fall back to chart subtitle/caption text when either exists.

Example:

```svg
<title>How old are astronauts?</title>
<desc>Histogram of astronaut age at selection and mission.</desc>
```

The renderer SHOULD preserve meaningful text labels. The interaction sidecar and
draw-list metadata MUST expose the same chart-level `title`, `subtitle`,
`caption`, `alt`, and resolved `description` values.

Purely decorative groups MAY use `aria-hidden="true"`.

When a mark declares a `tooltip` (spec §14.25), the SVG backend MUST emit the
tooltip as an accessible `<title>` child of that mark's shape element, with no
script. When a mark declares a `highlight` grouping key, the backend MUST emit a
stable `data-algraf-highlight="<group>"` attribute on the shape so a viewer can
relate marks of the same group. When a mark declares `On(event: "click",
emit: column)`, the backend MUST emit inert `data-algraf-event="click"` and
`data-algraf-emit-field="<column>"` attributes, and SHOULD emit
`data-algraf-emit-value="<value>"` when the row value exists. The `<title>` text
and every interaction attribute value are escaped per §29.3. A chart that
declares no interaction metadata MUST
produce byte-for-byte unchanged SVG: shapes stay self-closing and carry no
interaction attributes. These static affordances are present without any opt-in;
the JSON sidecar (§24.6), draw-list backend (§24.6), and opt-in interactive
runtime (§29.3) read the same inert metadata. Requesting a sidecar MUST NOT
change the SVG bytes.

### 18.11 Debug Rendering

CLI MAY support:

```bash
algraf render chart.ag --debug-layout
```

Debug layout MAY draw rectangles for plot area, margins, and guide boxes.

Debug metadata MAY include comments.

Debug comments MUST be optional.

### 18.12 Determinism

Given same source, data, binary version, options, target platform, and configured font metrics, SVG output MUST be byte-stable except for explicitly documented metadata.

Cross-platform byte stability is a goal, not a version 0.1 guarantee.

Version 0.1 MUST document the platforms used for snapshot baselines.

No timestamps should appear by default.

No random IDs should appear by default.

Palette assignment MUST be deterministic.

Category ordering MUST be deterministic.

The implementation MUST use deterministic map iteration for any state that can affect output ordering.

`IndexMap` or `BTreeMap` SHOULD be used where output order can leak into SVG.

Rust `HashMap` iteration MUST NOT be used to determine SVG element order, category order, guide order, or attribute order.

Text measurement MUST use deterministic approximations or bundled/static metrics.

The renderer MUST NOT depend on host font discovery for layout decisions in version 0.1.

Floating point formatting MUST use locale-independent formatting.

Floating point output precision MUST be fixed by renderer configuration.

### 18.13 Render Mark Budgets

Static SVG and draw-list output MUST have a deterministic raw-mark budget.
Version 0.43 uses a default budget of 100,000 raw marks per geometry layer for
row-to-mark geometries such as `Point`, `Bar`, `Rect`, `Tile`, `Text`, `Rug`,
`Segment`, `HexBin`, and `Geo`.

When the renderer can determine before emission that a raw layer would exceed the
active budget, it MUST emit `E2001` at the geometry span, skip that layer, and let
the CLI's ordinary diagnostic blocking rules decide whether output is written.
The diagnostic help SHOULD recommend binning, aggregation, sampling, SQLite or
Parquet preprocessing, or an explicit higher budget.

Derived aggregate geometries such as `Histogram`, `Bin2D`, and stats that
materialize bounded derived tables SHOULD be preferred for large sources. Raising
or disabling the budget is an explicit user choice; large-data support does not
mean generating pathological SVG nodes by default.

## 19. Guides

Guide handling is split into planning (label measurement and axis-margin
reservation, which runs before final layout) and emission (writing axes, grids,
facet strips, and legends to SVG during document assembly); see §24.6.

### 19.1 Axis Generation

Axes are generated for position scales by default.

1D spaces may generate one axis.

2D spaces generate x and y axes.

Axes include:

axis line

ticks

tick labels

axis title

optional grid lines

### 19.2 X Axis

X axis usually appears at bottom.

Categorical x axis uses category labels.

Continuous x axis uses nice ticks.

Temporal x axis uses formatted temporal ticks.

Version 0.82.0 MUST support `Guide(axis: x, position: "top")` to render the x
axis (ticks, tick labels, and title) on the top edge of the plot rectangle.
`position` for the x axis accepts only `"top"` or `"bottom"` (default
`"bottom"`); any other value, or a value not valid for the x axis (e.g.
`"left"`), MUST emit `E1204` and the axis MUST fall back to its default side.
The layout MUST reserve the axis rectangle on the chosen side (§17.3): a top x
axis reserves top margin instead of bottom. Grid lines, plot clipping (§18.5),
and data-mark placement MUST be unaffected by axis side — only guide placement
and margin reservation move. A theme-level default is set by `axisXPosition`
(§20.1), which a per-chart `Guide(axis: x, position:)` overrides.

### 19.3 Y Axis

Y axis usually appears at left.

Continuous y axis uses nice ticks.

Categorical y axis uses category labels.

Version 0.82.0 MUST support `Guide(axis: y, position: "right")` to render the y
axis (ticks, tick labels, and title) on the right edge of the plot rectangle.
`position` for the y axis accepts only `"left"` or `"right"` (default
`"left"`); any other value, or a value not valid for the y axis (e.g. `"top"`),
MUST emit `E1204` and the axis MUST fall back to its default side. The layout
MUST reserve the axis rectangle on the chosen side (§17.3): a right y axis
reserves right margin instead of left. A theme-level default is set by
`axisYPosition` (§20.1), which a per-chart `Guide(axis: y, position:)`
overrides. `position` requires `axis: x` or `axis: y`; without it, `E1204`.
This release moves a single trained axis to a chosen side; per-side independent
dual axes (a second value scale) remain deferred.

### 19.4 Axis Labels

Default axis label is column name.

For transformed stats, default label may be computed variable name.

Examples:

histogram y label: `count`

density y label: `density`

fill-normalized stack y label: `proportion`

Version 0.2.0 MUST support axis label overrides:

```ag
Guide(axis: x, label: "Flipper Length (mm)")
```

Axis references MUST use bare `x` and `y` selector values.

Axis references MUST NOT use string literals such as `"x"` in version 0.1.

Axis references MUST NOT use string literals such as `"x"` in version 0.2.0.

Version 0.6.0 MUST support `Guide(axis: x, label: null)` (and `axis: y`) to
suppress that axis's title, reusing the `null` = "suppress" convention. Axis
ticks and grid lines are unaffected. `label` accepts a string literal or `null`;
any other value MUST emit `E1204`.

Version 0.20.0 MUST support `Guide(axis: x, timeFormat: "iso-minute")` and
`Guide(axis: y, timeFormat: "iso-minute")` for temporal axes. `iso-minute`
renders datetime labels as `YYYY-MM-DD HH:MM`; `iso-date` renders the UTC date
portion as `YYYY-MM-DD`. `timeFormat` without `axis: x` or `axis: y`, unknown
format names, and non-temporal application contexts MUST produce targeted
diagnostics or be ignored with diagnostics during semantic analysis.

Version 0.28.0 MUST also support named temporal formats `iso-second`,
`iso-millis`, `rfc3339`, `year`, `month`, `month-day`, `time-minute`, and
`time-second`. It MUST accept custom chrono/strftime-style format strings such
as `"%b %-d, %Y"` and `"%Y-%m-%d %H:%M:%S"` after semantic validation. Temporal
formatting MUST be independent of the host locale, local timezone, and wall
clock.

Version 0.31.0 MUST support off-axis temporal formatting through a `timeFormat:`
argument on the `Text` geometry, reusing the same named and custom formats as
`Guide(timeFormat: …)`. When a `Text` maps `label:` to a temporal column and
declares `timeFormat:`, each label MUST render the column's UTC instant with that
format rather than the default label text. A `timeFormat:` whose name/pattern is
unknown or invalid, or applied where `label:` is not a temporal column, MUST emit
`E1907`. Output stays locale-, timezone-, and wall-clock-independent.

Version 0.23.0 MUST support `Guide(axis: x, tickLabelAngle: -45)` and
`Guide(axis: y, tickLabelAngle: 30)` to rotate tick labels by an explicit angle
in degrees. `tickLabelAngle` MUST accept only finite numeric literals in the
inclusive range `[-90, 90]`; non-numeric, non-finite, or out-of-range values
MUST emit `E1204`. `tickLabelAngle` without `axis: x` or `axis: y` MUST emit
`E1204`. The default angle is `0`, preserving existing horizontal tick labels.
The renderer MUST rotate each tick label around its own anchor point and reserve
enough guide margin using deterministic approximate text measurements. When
thinning overlapping labels, the renderer MUST account for the angle: rotated
labels are parallel strands whose adjacency depends on the perpendicular gap
between baselines (the tick spacing scaled by `sin|angle|`) rather than their
length, so a rotated axis keeps more category labels than a horizontal one at the
same spacing. X-axis label thinning MUST evaluate candidates in visual pixel
order, not source/domain order, so reversed axes preserve edge labels and use
the same non-overlap rule as non-reversed axes.

Version 0.40 MUST support `Guide(axis: x, tickLabelRows: n)` and
`Guide(axis: y, tickLabelRows: n)` for deterministic tick-label dodging.
`tickLabelRows` accepts integer literals from 1 through 8; other values MUST
emit `E1204`. The default is 1. For the x axis, labels are assigned to rows by
tick index modulo `n`, adding deterministic vertical offsets. For the y axis,
labels are assigned to offset columns by tick index modulo `n`. Guide planning
MUST reserve the additional margin implied by the configured rows using the
same deterministic text measurement model as rotated labels.

Version 0.82.0 MUST support `Guide(axis: x, format: "...")` and `Guide(axis: y,
format: "...")` to format numeric (continuous, non-temporal) axis tick labels
using the deterministic numeric format vocabulary defined for `Text` (§14.16):
`.0f`, `.1f`, `.2f`, `$.2f`, `.0%`, `.1%`, `.2%`. This gives editorial control
over value-axis labels (e.g. `"800"` rather than `"800.0"`). An unknown format
string, `format` combined with `timeFormat`, or `format` without `axis: x` or
`axis: y`, MUST emit `E1909`. A numeric `format` applied to a temporal or
categorical axis has no effect (those axes use `timeFormat` and category labels
respectively). Output is identical across SVG, raster, and draw-list backends.

Version 0.82.0 MUST support per-axis grid-line visibility. `Guide(axis: x,
grid: false)` suppresses the vertical grid lines at x ticks; `Guide(axis: y,
grid: false)` suppresses the horizontal grid lines at y ticks. A bare
`Guide(grid: false)` continues to toggle all grid lines. A theme MAY set the
default per-axis visibility with `gridX`/`gridY` (§20.1), which a per-chart
`Guide(axis:, grid:)` overrides. This lets a house style keep only horizontal
rules. Per-axis grid control affects only grid lines, not axis lines, ticks, or
tick labels.

### 19.5 Legend Generation

Legends are generated for mapped aesthetics by default.

No legend is generated for literal settings.

Fill mapping creates fill legend.

Stroke mapping creates stroke legend.

Discrete color legends default to scale/domain order, except for visibly
stacked layouts. Version 0.77.0: when a discrete `fill` or `stroke` legend's
aesthetic forms the stack groups of `Bar(layout: "stack" | "fill")`,
`Area(layout: "stack" | "fill")`, or the grouped stacked Histogram's
pre-stacked `Rect` desugaring (§14.7), the default legend entry order MUST
follow the rendered visual stack order:

- Categories that visibly stack together at any position form a cohort.
- Within a cohort, the positive side lists segments outward-to-baseline — a
  vertical positive stack's top band first, a rightward-growing horizontal
  stack's rightmost band first — followed by the negative side
  baseline-outward. A category belongs to the side of its first visible
  contribution.
- Cohorts are ordered by their earliest member in scale/domain order. A
  domain containing multiple disjoint visible stack cohorts MUST NOT be
  reversed wholesale; each cohort reorders independently.
- Only visible (nonzero-extent) stack contributions participate. The
  zero-height placeholder cells sparse stacked Areas evaluate (§14.14) MUST
  NOT link cohorts or affect ordering.
- Accumulation order within a cohort is reconstructed from the per-position
  accumulation sequences; conflicting pair directions resolve first-seen, and
  remaining ties or cycle breaks fall back to domain order, so the result is
  deterministic across repeated runs.
- The reorder is presentation-only: it MUST NOT change trained scale domains,
  category color assignment (manual `range:` maps keep binding colors by
  category), interaction group domains, or geometry placement.

Non-stacked discrete legends, and continuous, binned, size, image, and
identity-color legends, keep their existing order. Polar stacked marks
(§16.16) keep scale/domain legend order. Legend suppression (`Guide(fill:
null)`, `Guide(stroke: null)`, `Guide(legend: false)`) is unaffected.

A `size` or `strokeWidth` mapping MUST create a size legend. The legend title
follows the same rules as color legends — the scale's `label:` when declared,
otherwise the mapped column's display name (see §16.13). Its entries are five
evenly spaced ticks across the trained domain, each drawn as a swatch sized by
the scale's output: a `strokeWidth` legend draws a line of the mapped thickness,
and a `size` legend draws a circle of the mapped radius. A swatch whose mapped
magnitude is zero draws no mark, only its tick label. If the corresponding
scale declares `breaks:`, those exact legend values replace the default five
evenly spaced entries. A paired `labels:` array replaces the displayed text
positionally.

A `shape` mapping MUST create a discrete shape legend with one swatch per
category in domain order, each drawn as the marker glyph that category's points
use (§16.10). The title follows the same rules as color legends — the scale's
`label:` when declared, otherwise the mapped column's display name (§16.13). A
standalone shape legend draws its swatches in the default mark fill; when the
same column is also mapped to `fill` or `stroke`, the shape legend is merged
into that color legend instead of duplicated (§19.7).

Marks inside a glyph (§14.27) contribute chart-level legends only for scales
trained with shared scope (`train: "shared"` or the glyph `scales: "shared"`
default, §16.18). Those legend domains use the same shared row subset used for
child scale training. A position or data-trained scale under `train: "local"`
MUST NOT emit a chart-level legend (§17.7).

Alpha mappings are accepted where geometry registries allow them, but alpha
scale targets and alpha legends are deferred past version 0.40. Dash/stroke
style scale targets and dash legends are also deferred; dash remains an
enumerated literal setting where supported.

Legend merging MAY combine aesthetics mapped to same column.

Version 0.1 MAY keep legends separate.

### 19.6 Guide Suppression

Guide suppression example:

```ag
Guide(fill: null)
Guide(stroke: null)
```

Or:

```ag
Guide(legend: false)
Guide(grid: false)
```

Version 0.2.0 MUST support `Guide(legend: false)`.

Version 0.2.0 MUST support aesthetic-specific legend suppression with `Guide(fill: null)` and `Guide(stroke: null)`.

Version 0.2.0 MUST support grid suppression with `Guide(grid: false)`.

`Guide` declarations MAY appear at chart scope or space scope.

Space-local guide declarations override chart-level guide declarations for that space.

### 19.7 Legend Merging

> Promoted from a v0.1 `MAY` to a v0.2.0 requirement for `fill`/`stroke`;
> see `docs/V0_2_PLAN.md`.

When `fill` and `stroke` map to the same categorical column with compatible
domains, version 0.2.0 MUST merge them into a single legend rather than
emitting two legends with the same title.

Two discrete legends have compatible domains when their entry labels are
equal and in the same order, as finally displayed — i.e. after any stacked
visual reordering (§19.5) has been applied to each candidate.

A merged legend MUST render each swatch with the fill color as the swatch face
and the stroke color as the swatch outline.

When `shape` maps to the same categorical column with a compatible domain as a
`fill` or `stroke` legend, the implementation MUST fold the shape legend into
that color legend — drawing each swatch as the category's marker glyph filled
with the color legend's color — rather than emitting a separate shape legend
with the same title. Version 0.77.0: the fold matches shapes to color entries
by label rather than by position, so a color legend displayed in stacked
visual order (§19.5) keeps each category's marker glyph aligned.

Aesthetics mapped to different columns MUST keep separate legends.

Continuous (gradient) legends are not merged in version 0.2.0.

### 19.8 Polar Guides

> Since version 0.26.

A polar space (§16.16) replaces the Cartesian grid and axes with:

- **Radius rings** at each radius-axis tick (plus the outer boundary). With
  `gridShape: "circle"` (default) each ring is an SVG `<circle>`; with
  `gridShape: "polygon"` each ring is an SVG `<polygon>` through the spoke
  vertices (the radar pentagon/hexagon grid). An invalid `gridShape` is `E1906`.
- **Spokes** from the inner radius to the perimeter at each theta tick, drawn
  only for a categorical theta axis.
- **Perimeter labels** placed around the outside at each theta category, and
  **radius labels** along the top spoke.

`Guide(grid: false)` suppresses polar guides as it does Cartesian grids.

### 20.1 Theme Object

The render theme structure (colors are stored as SVG color strings):

```rust
pub struct Theme {
    pub name: &'static str,
    pub font_family: String,
    pub font_size: f64,
    pub background: String,
    pub plot_background: String,
    pub axis_color: String,
    pub grid_major_color: String,
    pub grid_major_width: f64,
    pub text_color: String,
    pub title_size: f64,
    pub point_size: f64,
    pub line_width: f64,
    pub grid: bool,
    pub grid_x: bool,
    pub grid_y: bool,
    pub axes: bool,
    pub plot_title: TextStyle,
    pub plot_subtitle: TextStyle,
    pub plot_caption: TextStyle,
    pub plot_source: TextStyle,
    pub axis_title: TextStyle,
    pub axis_text: TextStyle,
    pub strip_text: TextStyle,
    pub legend_title: TextStyle,
    pub legend_text: TextStyle,
    pub panel_background: RectStyle,
    pub grid_major: LineStyle,
    pub grid_minor: LineStyle,
    pub legend_position: LegendPosition,
    pub legend_spacing: f64,
    pub axis_x_position: AxisPosition,
    pub axis_y_position: AxisPosition,
}

pub struct TextStyle {
    pub font_family: String,
    pub size: f64,
    pub fill: String,
}

pub struct LineStyle {
    pub stroke: String,
    pub stroke_width: f64,
}

pub struct RectStyle {
    pub fill: String,
    pub stroke: Option<String>,
    pub stroke_width: f64,
}

pub enum LegendPosition {
    Right,
    Bottom,
    Top,
    Left,
}
```

These fields are the override targets for custom themes (spec §20.8).

Version 0.82.0 adds editorial-chrome fields:

- `plot_source: TextStyle` — styles the `source:` attribution line (§17.3),
  defaulting to a smaller, lighter variant of `plot_caption`.
- `grid_x: bool` / `grid_y: bool` — per-axis grid-line visibility defaults
  (both `true`), letting a house style keep only horizontal rules (§19.4).
- `axis_x_position` / `axis_y_position` — theme-level default axis side
  (`AxisPosition` is `Left`/`Right`/`Top`/`Bottom`), overridden per chart by
  `Guide(axis:, position:)` (§19.2, §19.3).

### 20.2 Minimal Theme

`minimal` theme:

white background

light gray grid

no plot border

dark text

subtle axis labels

### 20.3 Classic Theme

`classic` theme:

white background

axis lines

no grid or minimal grid

dark text

### 20.4 Dark Theme

`dark` theme:

dark background

light text

muted grid

color palette adjusted for contrast

### 20.5 Void Theme

`void` theme:

no axes

no grid

transparent or white background

data marks only

### 20.6 Light Theme

`light` theme:

white background

light gray grid

dark text

The `light` theme is currently an alias of `minimal`: it selects the same base
theme values and differs only in reported name. It exists so source and CLI can
request a neutral light theme by an explicit name. Later versions MAY give
`light` distinct values; until then renderers MUST treat it as equivalent to
`minimal`.

Version 0.42.0 also ships these neutral presentation presets:

- `gray`: white chart background, light gray panel background, white major grid,
  very light minor grid, dark axis/text defaults.
- `bw`: white chart and panel background, dark panel border and axis defaults,
  light gray major/minor grid lines.
- `linedraw`: white chart and panel background, black axis/text defaults, thin
  black panel border, thin gray major/minor grid lines, and a thinner default
  line width.

### 20.7 Theme Syntax

Theme declaration syntax:

```ag
Theme(name: "minimal")
```

Version 0.1 MUST use `Theme(name: "minimal")` for source-level theme selection.

`Chart(theme: "minimal")` MUST NOT be accepted in version 0.1.

Example:

```ag
Chart(data: "penguins.csv") {
    Theme(name: "minimal")

    Space(flipper_length * body_mass) {
        Point()
    }
}
```

### 20.8 Custom Theme

`Theme(...)` MAY carry override properties in addition to (or instead of) the
named base `name`. Overrides are layered on top of the named base theme (or, when
no `name` is given, on top of the inherited base) to produce the resolved theme
(spec §20.1). A space-local `Theme(...)` resolves the same way, inheriting the
chart base when it omits `name` (spec §7.3).

Example:

```ag
Theme(
    name: "minimal",
    axisText: Text(size: 12, fill: "#333333"),
    gridMajor: Line(stroke: "#dddddd", strokeWidth: 1),
    plotBackground: "#fafafa"
)
```

Grouped, geometry-style overrides reuse existing property value forms:

- text styles: `plotTitle`, `plotSubtitle`, `plotCaption`, `plotSource`,
  `axisTitle`, `axisText`, `stripText`, `legendTitle`, and `legendText` accept
  `Text(fontFamily?, size?, fill?)`. `plotSource` (since 0.82.0) styles the
  `source:` line (§17.3).
- line styles: `gridMajor` and `gridMinor` accept
  `Line(stroke?, strokeWidth?)`.
- rectangle styles: `panelBackground` accepts
  `Rect(fill?, stroke?, strokeWidth?)`.

The remaining overrides are direct scalar values mapping to the theme fields
(spec §20.1):

- color strings: `background`, `plotBackground`, `axisColor`, `gridColor`,
  `textColor`
- numbers: `fontSize`, `titleSize`, `pointSize`, `lineWidth`
- string: `fontFamily`
- booleans: `grid`, `axes`, and (since 0.82.0) `gridX`/`gridY` for per-axis
  grid-line defaults (§19.4)
- legend controls: `legendPosition` string (`"right"`, `"bottom"`, `"top"`,
  or `"left"`) and numeric `legendSpacing`
- axis sides (since 0.82.0): `axisYPosition` string (`"left"`/`"right"`) and
  `axisXPosition` string (`"top"`/`"bottom"`), each scoped to its axis; a
  wrong-axis or unknown value MUST emit `E1705` (§19.2, §19.3)

Legacy scalar fields remain compatibility shorthands. For example,
`plotBackground: "#fafafa"` sets the panel fill, while the structured
`panelBackground: Rect(fill: "#fafafa", stroke: "#333333", strokeWidth: 1)`
also controls the panel border.

Override values reuse the standard property value forms and MAY reference `let`
variables (spec §9.6).

An unknown override key MUST produce diagnostic `E1704`. An override value of the
wrong type or shape (for example a non-`Line(...)` value for `gridMajor`, or a
string for `fontSize`) MUST produce diagnostic `E1705`.

## 21. LSP Architecture

### 21.1 LSP Goals

The LSP MUST use the same parser as CLI.

The LSP MUST use the same analyzer as CLI.

The LSP MUST recover from incomplete source.

The LSP MUST provide diagnostics.

The LSP SHOULD provide completions.

The LSP SHOULD provide hover.

The LSP SHOULD provide document symbols.

The LSP SHOULD provide formatting.

The LSP MAY provide semantic tokens.

The LSP MAY provide code actions.

The authoritative preview/render path is the `algraf render` command.

The LSP provides inline previews through the `algraf/preview` custom request
(spec §21.18), and it MUST call the same internal render pipeline as
`algraf render`.

LSP preview rendering MUST run asynchronously and MUST be cancellable.

LSP preview rendering MUST NOT block diagnostics, completion, or hover responses.

### 21.2 LSP Runtime

Recommended dependencies:

```toml
tower-lsp = "0.20"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "io-std", "sync", "fs"] }
dashmap = "6"
```

The binary mode:

```bash
algraf lsp
```

reads JSON-RPC from stdin.

It writes JSON-RPC to stdout.

It writes logs to stderr or LSP log messages.

### 21.3 LSP State

Recommended state:

```rust
pub struct Backend {
    client: Client,
    documents: DashMap<Url, DocumentState>,
    schema_cache: Arc<InMemorySchemaCache>,
}
```

Since version 0.16 the schema cache is the driver-owned, fingerprint-validated
service of §10.9 rather than an LSP-local map; `DataSourceKey` and the cached
schema/error types live in `driver`. Primary and named-table schema resolution
both go through this one cache, so they share keying and invalidation. The LSP
MUST resolve named-table schemas for every chart in the document, not just the
first chart.
For `Sqlite(...)`, the LSP MUST include the SQL query in the schema-cache key,
MUST use the bounded SQL schema-sampling policy from §10.12, and MUST surface
the `E0025` gated-off diagnostic instead of loading SQL when the `sql` feature
gate is absent.
For `Parquet(...)`, the LSP MAY defer full column decoding and SHOULD use
metadata-only schemas when the native Parquet feature is available.

Document analysis SHOULD be a pure blocking helper that parses, resolves cached
schemas, analyzes, and returns `DocumentState` plus diagnostics. Document
management owns insertion, versioning, and diagnostic publication. No-op text
edits MAY reuse cached diagnostics only when the document has no external schema
sources; documents that depend on files or SQLite queries MUST re-run schema
resolution so fingerprint invalidation can observe external changes.

Because the LSP transport handles messages concurrently, document management
MUST make the latest opened/changed text visible to text-derived requests
(semantic tokens, formatting, signature help) before blocking analysis for that
text completes; analysis-derived state MAY lag behind the text until analysis
lands. Answering a `semanticTokens/full` request from superseded text makes the
editor paint misaligned tokens against the current buffer. Correspondingly, an
analysis result for a superseded document version MUST NOT overwrite newer
text or publish its diagnostics, MUST NOT resurrect a closed document, and a
`didChange` carrying a version lower than the cached one MUST be ignored.

When an editor/LSP document uses `Chart(data: input)` or `Chart(data: stdin)`
and the host has not supplied caller-provided data bytes or an injected primary
schema, analysis MUST treat the primary table schema as unknown rather than
empty. In that state, primary-table column references MUST resolve with
`Unknown` type and MUST NOT emit `E1101` solely because the caller-input schema
is unavailable. The editor/LSP MUST still report syntax diagnostics,
non-column semantic diagnostics, and diagnostics for named tables whose schemas
are available or whose source-specific errors are known. CLI and embedded
render/check paths that receive caller-provided bytes still infer the schema and
validate primary columns normally.

Document state:

```rust
pub struct DocumentState {
    pub text: String,
    pub version: i32,
    pub parse: Option<ParseState>,
    pub analysis: Option<AnalysisState>,
    pub primary_schema: Option<Vec<ColumnDef>>,
    pub table_schemas: HashMap<String, Vec<ColumnDef>>,
    pub data_path: Option<PathBuf>,
    pub has_external_schema_sources: bool,
    pub diagnostics: Vec<Diagnostic>,
}
```

Parse state:

```rust
pub struct ParseState {
    pub ast: Program,
    pub diagnostics: Vec<Diagnostic>,
}
```

Analysis state:

```rust
pub struct AnalysisState {
    pub ir: Option<ChartIr>,
    pub diagnostics: Vec<Diagnostic>,
}
```

### 21.4 Document Synchronization

Version 0.1 MAY use full text synchronization.

Full sync is simpler and acceptable for small DSL files.

Incremental sync SHOULD be considered later.

On `didOpen`:

store document text

parse

start schema resolution on a blocking task when filesystem or SQLite metadata
may be touched

analyze when schema available

publish diagnostics

On `didChange`:

update text

parse

skip analysis for no-op text changes only when no external schema sources are
present

debounce schema resolution if data source changed or external schema sources are
present

analyze with cached schema if available

publish diagnostics

On `didClose`:

remove document state

schema cache MAY remain.

### 21.5 Diagnostics

Diagnostics include parser and semantic diagnostics.

Parser diagnostics are published even when semantic analysis cannot run.

Semantic diagnostics are published when enough AST exists.

Diagnostic source SHOULD be `algraf`.

Diagnostic codes SHOULD be stable.

### 21.6 Completion

Completion contexts:

top-level expects `Chart`

inside chart body expects `Space`, `Scale`, `Guide`, `Theme`, `Layout`

inside `Chart(...)` expects chart argument names

inside `Space(...)` expects column names and algebra operators

after algebra operator expects column names or `(`

inside space body expects geometry names, `Scale`, `Guide`

inside geometry args expects property names

after property colon expects columns, literals, string-valued options, selectors, sentinels, or symbols depending on property

Completion MUST be context-sensitive.

Completion SHOULD include documentation.

Completion SHOULD include insert text.

Completion SHOULD include kind.

Column completions SHOULD use schema cache.

`Sqlite(...)` completion and hover documentation MUST be offered only when the
document declares `Algraf(version: "0.21", features: ["sql"])`; otherwise the
constructor remains gated and diagnostics explain the required header.
`Parquet(...)` completion and hover documentation MUST be offered by native
builds that include Parquet support.

If schema unavailable, completion SHOULD return syntax keywords and optionally a loading message.

### 21.7 Hover

Hover contexts:

operator hover explains algebra operator

column hover shows type and source

When a sampled schema is available, column hover SHOULD show the inferred type,
the primary or named-table source, and a small set of sample values. Sampled
types MUST be labeled provisional in the hover text or associated analysis
state.

Derived-table hover shows the table name, producer stat, and output schema for
`Derive` names, `Derive name from derived_name` references, and
`data: derived_name` references. Column hover inside
`Space(..., data: derived_name)` MUST resolve against that derived table's
schema, not the primary source schema.

Source-path hover for `Chart(data: "file.csv")`, `Table name = "file.csv"`,
`Chart(data: Parquet("file.parquet"))`, and constructor-backed named table
sources SHOULD show the resolved source label, sampled column types, sample
values from the schema, and a bounded raw row preview when available. Source
previews MUST be labeled provisional and MUST degrade gracefully when data is
missing, unreadable, unsupported for row preview, or too large to sample on the
editor path.

Named-table hover for `Table name = ...` declarations, `Chart(data: name)`,
`Space(..., data: name)`, and `Derive output from name = ...` references MUST
show the table name and sampled schema when available. Column hover inside
`Space(..., data: name)` MUST continue to resolve against that named table's
schema, not the primary source schema.

Declaration hover for `Chart`, `Space`, `Theme`, `Scale`, `Guide`, `Layout`, and
`Table` shows a short description, accepted attributes with value forms/defaults
where known, and a concise valid example.

Geometry and stat hover shows shared registry docs, accepted properties or
arguments, required properties where known, and a concise valid example.

property hover shows property docs

string-valued option hover shows option docs

Hover over `/`:

Explains nest operator.

Hover over `*`:

Explains cross operator.

Hover over `+`:

Explains blend operator.

### 21.8 Go To Definition

The LSP SHOULD provide go to definition (`textDocument/definition`) using the
same name resolution as completion and analysis (spec §9.4). It MUST NOT require
rendering.

Definition resolution:

- A column reference produced by a `Derive` (a derived column such as
  `bin_start`) resolves to that `Derive` declaration's table-name identifier.
- A `data:` reference to a derived table resolves to that `Derive` declaration.
- A column reference that resolves to a CSV header resolves to that header's
  position in the data file, when the data path resolves (best effort).
- The chart-level `data:` string value resolves to the start of the resolved
  CSV file.

When a reference is ambiguous (for example a derived column produced by more
than one `Derive`) or does not resolve, the LSP MUST return no definition
rather than guess.

### 21.9 Document Symbols

Document symbols SHOULD include:

Chart

Space blocks

Geometry calls

Guide declarations

Scale declarations

Symbols help outline navigation.

### 21.10 Formatting

Formatter SHOULD produce:

4-space indentation

one block item per line

one call argument per line when call exceeds line width

spaces around algebra operators

parentheses around mixed algebra operators where clarity requires

Formatter MUST preserve comments where practical.

Formatter MAY be deferred in version 0.1.

The LSP SHOULD provide range formatting (`textDocument/rangeFormatting`). Because
the Algraf formatter is holistic and deterministic, a range request reformats
the whole document and returns a single edit rather than implementing a partial
formatter. On-type formatting is deferred: reformatting on each keystroke would
surprise authors, which §21.1-style high-confidence behavior avoids.

On-type formatting (`textDocument/onTypeFormatting`) remains deferred and MUST
NOT be advertised in this version. The reasons are intrinsic to the holistic
formatter: (1) it only reflows documents that parse without errors (spec
§21.10), and source is most often mid-edit — and thus invalid — at the moment a
trigger character is typed, so the formatter would no-op or, worse, reflow a
stale-but-valid parse; (2) the formatter rewrites the whole document, so a
keystroke trigger would produce large, cursor-displacing edits rather than the
local touch-ups on-type formatting implies; and (3) deterministic whole-document
formatting is already available on demand via document and range formatting. A
future version MAY enable a deliberately narrow subset — e.g. closing-brace (`}`)
and newline triggers that only re-indent the just-closed block when the document
parses cleanly — but only if it can guarantee edits stay local and never fire on
invalid input. Until then, no on-type formatting capability is registered.

### 21.11 Semantic Tokens

Semantic token categories:

keyword

function

property

variable

operator

string

number

comment

Version 0.2.0 MUST implement semantic tokens for these categories.

### 21.12 Code Actions

Potential code actions:

create missing data file is not recommended.

rename misspelled geometry to suggested geometry.

quote literal color.

replace `Bar(layout: "dodge")` with nested algebra suggestion.

replace `x * y * group` with `(x * y) / group` when `group` is categorical.

replace unparenthesized `time * lower + upper` with `time * (lower + upper)`.

For example:

```ag
Space(quarter * amount) {
    Bar(fill: type, layout: "dodge")
}
```

Code action could produce:

```ag
Space((quarter / type) * amount) {
    Bar(fill: type)
}
```

Version 0.2.0 MUST implement code actions for high-confidence existing diagnostics, including quoted enum/string fixes, quoted color literals, misspelled geometry suggestions, unsupported 3D Cartesian nesting suggestions, and blend-parenthesization fixes.

Version 0.4.0 adds:

- A `quickfix` for `E1101` (unknown column) that applies the suggested column
  name, quoting it when it is not a plain identifier.
- A `quickfix` for `E1202` (unknown property) that applies the suggested
  property name.
- A `refactor.rewrite` action that desugars a single-`Histogram` space into the
  explicit `Derive ... = Bin(...)` plus `Rect` form the analyzer produces. The
  action fires only when the space holds exactly one `Histogram` over a
  single-column frame and is a direct chart-body item, so the rewrite is
  unambiguous. The LSP MUST advertise the `refactor` kind only while this action
  exists. None of these actions require rendering, and they preserve unrelated
  formatting.

### 21.13 Cancellation and Shutdown

The LSP MUST honor client cancellation for long-running custom requests.

Preview rendering MUST be cancellable through the LSP request cancellation mechanism.

Implementation SHOULD associate each long-running preview task with a cancellation token.

When a newer preview request supersedes an older preview request for the same document, the older task SHOULD be cancelled.

Cancelled preview tasks MUST NOT publish stale preview output.

The LSP MUST handle `shutdown` by stopping new work and allowing in-flight lightweight requests to finish promptly.

The LSP MUST handle `exit` by terminating the process after shutdown according to LSP conventions.

### 21.14 Find References and Document Highlight

The LSP SHOULD provide find references (`textDocument/references`) and document
highlight (`textDocument/documentHighlight`) for column names and derived-table
names, using the same name resolution as completion and go to definition.

For a column name, references are every column occurrence with that name in the
document (frames, aesthetic mappings, and stat inputs).

For a derived-table name, references are the `Derive` declaration plus every
`data:` use of that table. Document highlight MUST mark the declaration as a
write and uses as reads. References honor `context.includeDeclaration`.

Spans MUST be byte-accurate, including non-ASCII identifiers (spec §6.12).

### 21.15 Signature Help

The LSP SHOULD provide signature help (`textDocument/signatureHelp`) while the
cursor is inside a geometry call or a `Scale`/`Guide`/`Theme`/`Layout`/`Chart`
call. The signature lists the accepted properties from the geometry/property
registry (spec §13.8–13.9) — the same metadata completion uses.

The active parameter MUST follow the cursor across top-level argument commas,
ignoring commas nested in array values.

### 21.16 Rename

The LSP SHOULD provide rename (`textDocument/rename`) and prepare-rename
(`textDocument/prepareRename`) for derived-table names, updating the `Derive`
declaration and every `data:` use. Source CSV columns are not user-introduced
and MUST NOT be renameable; prepare-rename returns nothing for them.

### 21.17 Inlay Hints

The LSP MAY provide inlay hints (`textDocument/inlayHint`) when there is an
active hint family worth advertising.

As of version 0.39.5, Algraf does not advertise an active inlay-hint provider.
Derived-table schemas are inspected through hover on `Derive` names and
`data:` references (spec §21.7), not through grey inline text after each
declaration. Legacy clients that still issue `textDocument/inlayHint` MAY
receive an empty list.

### 21.18 Preview Rendering

The LSP provides an SVG preview through the `algraf/preview` custom request.

Request params: `{ "uri": <document URI>, "interactive"?: false }`.

Result:

```json
{
  "svg": "<svg …>…</svg>" | null,
  "message": "human-facing explanation" | null,
  "superseded": false,
  "generation": 3,
  "dataPaths": ["/abs/path/to/data.csv"]
}
```

The server renders by calling the same pipeline as `algraf render`: it parses
the cached document, loads the full CSV identified by the chart's `data:` path,
analyzes against that schema, and renders to SVG.

`dataPaths` reports the resolved data dependencies from the driver's chart
dependency inventory so the client can watch them and re-request when the
underlying data changes without a source edit. Path resolution stays on the
server; the client watches (so remote workspaces work) and SHOULD also offer a
manual refresh, which is the fallback for data sources that cannot signal
change. `data: stdin` and a missing
data source return a `message` rather than an SVG. A document with blocking
parse or semantic errors returns a `message` so the previous preview is not
replaced with a broken render.

Rendering MUST run off the request reactor (e.g. on a blocking task) so it does
not delay diagnostics, completion, or hover. Each document carries a request
generation counter; when a newer preview request supersedes an older one, the
older result MUST be reported with `superseded: true` and MUST NOT carry stale
SVG (spec §21.13). The client SHOULD debounce edits and ignore superseded or
out-of-order replies using `generation`.

Document analysis that may touch data-source metadata or schema bytes SHOULD
also run off the request reactor, using the driver schema cache on a blocking
task around the synchronous provider. This MUST NOT change LSP protocol behavior
or diagnostic content.

The preview is read-only and script-safe by default: when `interactive` is
omitted or `false`, the server returns script-free SVG and the client MUST NOT
execute scripts in the preview surface. When the request sets `interactive:
true`, the server renders with the opt-in interactive runtime (spec §29.3) and
the returned SVG carries the single, fixed, Algraf-shipped script — and only
that script. The preview never executes user-authored script: interaction comes
solely from the audited runtime reading inert per-mark metadata (§14.25,
§18.10) and the already-rendered plot rectangles/axis tick labels used for
crosshair value readouts. A document with no interaction metadata has the same
static chart body whether or not `interactive` is set; the interactive form may
still provide plot crosshairs where Cartesian axis ticks are available. The
read-only, superseded, and generation semantics above are unchanged by this
flag.

The preview result MUST also include the same interaction sidecar JSON described
in §24.6, in a `metadata` field, whenever rendering succeeds. Clients MAY ignore
it for static `<img>` previews, but interactive clients SHOULD consume it rather
than scraping SVG geometry. The sidecar is inert data and is returned for both
static and interactive preview requests.

## 22. CLI Specification

### 22.1 Binary Name

The binary name is `algraf`.

### 22.2 Commands

Required commands:

`algraf render`

`algraf lsp`

Recommended commands:

`algraf check`

`algraf format`

`algraf schema`

`algraf ast`

`algraf ir`

`algraf init`

### 22.3 Render Command

Usage:

```bash
algraf render chart.ag --output chart.svg
```

CSV data from standard input:

```bash
cat data.csv | algraf render chart.ag --data -
```

Algraf source from standard input:

```bash
cat chart.ag | algraf render - --data data.csv
```

If output omitted, output writes to stdout.

If input omitted or `-`, source reads from stdin.

If `--eval <source>` or `-e <source>` is supplied, source reads from that inline
string and diagnostics label it as `<eval>`. `--eval` MUST be mutually exclusive
with positional source input. Inline source resolves relative data paths against
`--base-dir` when present, otherwise the current working directory.

If `--data -` is supplied, caller-provided data reads from stdin.

The command MUST reject using `-` for both source and caller-provided data.

If the source contains `Chart(data: input)` or `Chart(data: stdin)`, caller data
reads from stdin unless `--data <path>` overrides it.

`--data-format <csv|tsv|json|ndjson|geojson|topojson|parquet|arrow-stream>`
MUST select the format for caller-provided bytes and for a primary
`--data <path>` override. Without this flag, caller-provided bytes use the
sniffing and CSV-fallback policy from §10.2.1 and `--data <path>` uses extension
inference.

`--var key=value` MAY be repeated on source-consuming commands. Expansion
happens before parsing against the expanded source. `${name}` and `$name`
placeholders are replaced with raw Algraf source fragments after shell parsing.
Undefined variables and duplicate keys MUST produce deterministic usage errors.
The expansion layer MUST NOT evaluate expressions, read environment variables,
include files, or provide conditionals or loops.

If the source contains gated `Sqlite(...)`, the CLI MUST require
`Algraf(version: "0.21", features: ["sql"])`. No CLI flag enables network,
environment-variable, or command sources in version 0.21.

The render command MUST enforce the render mark budget from §18.13. The default
budget is 100,000 raw marks. `--mark-budget <n>` sets the budget for one render
command. `--allow-large-render` disables the budget for users who explicitly want
large raw SVG or draw-list output.

Render options:

`--output <path>`

`--format <svg|svg+json|draw-list|raster>`

`--metadata <path>`

`--width <px>`

`--height <px>`

`--png-scale <factor>`

`--png-dpi <dpi>`

`--base-dir <path>`

`--data <path|->`

`--data-format <csv|tsv|json|ndjson|geojson|topojson|parquet|arrow-stream>`

`--mark-budget <n>`

`--allow-large-render`

`--eval <source>` / `-e <source>`

`--var <key=value>`

`--theme <name>`

`--debug-layout`

`--emit-metadata`

`--strict`

`--interactive`

`--interactive` embeds the fixed, audited interactive runtime (spec §29.3) in
SVG-producing output, enabling tooltip-on-hover and highlight-on-hover from the inert
per-mark metadata of §14.25/§18.10 and Cartesian plot crosshairs/value readouts
from the emitted plot rectangles and axis tick labels. It applies only to
`--format svg` and `--format svg+json`; the rendered chart body is byte-for-byte
identical to the static render, with the single Algraf-shipped `<script>`
appended before `</svg>`.
Without the flag the SVG is script-free. The static
`<title>`/`data-algraf-highlight` affordances are present either way;
`--interactive` only adds the script that animates them and the existing
plot/axis geometry.

`--theme <name>` is a render-time override.

It does not change source syntax.

`--metadata <path>` writes the JSON interaction sidecar described in §24.6
alongside the primary render output. It is valid with every backend. The sidecar
MUST be deterministic, MUST use the same locale-independent number formatting as
SVG output, and MUST NOT change the primary output bytes.

`--format <svg|svg+json|draw-list|raster>` selects the output backend (§24.6);
it defaults to `svg`. With `svg`, a `.png` `--output` path rasterizes the SVG
through the canonical PNG wrapper (which uses system fonts); this is the default
PNG path and is unchanged. With `svg+json`, `--output <base>` writes both SVG
and a `<base>.meta.json` sidecar; if `<base>` has no extension, the SVG path is
`<base>.svg`. With `draw-list`, the command emits the serializable draw-list
JSON described in §24.6, and PNG rasterization does not apply. With `raster`,
the command emits a PNG drawn directly from the scene's draw list by the
render-model raster backend (no SVG parser, no system fonts); it honors
`--png-scale`/`--png-dpi` and writes binary PNG to `--output` or stdout. The
render-model raster renders shape primitives; text glyphs are a documented
equivalence limit (§24.6). The draw list, sidecar, and SVG outputs are
byte-for-byte deterministic; raster output is deterministic for a given
platform.

Source files MUST use `Theme(name: "...")` for persistent theme selection.

Theme precedence from weakest to strongest is:

1. Built-in default theme.
2. Chart-level `Theme(...)`.
3. Space-local `Theme(...)`.
4. CLI `--theme <name>`.

In version 0.1, CLI `--theme <name>` replaces the base theme name while preserving the same override order.

When custom theme fields are added later, CLI `--theme <name>` MUST replace the base theme but MUST NOT discard explicit source-level field overrides unless an explicit reset option is added.

### 22.4 Check Command

Usage:

```bash
algraf check chart.ag
```

Check parses and analyzes without rendering.

It exits nonzero on errors.

It prints diagnostics.

### 22.5 Format Command

Usage:

```bash
algraf format chart.ag
```

Default writes formatted source to stdout.

`--write` overwrites file.

Formatter must not require data file availability.

### 22.6 Schema Command

Usage:

```bash
algraf schema chart.ag
```

Schema command prints resolved data schema as JSON or table.

For `Sqlite(...)`, `--sample-size <n>` bounds the number of result rows stepped
for type inference; omitted sample size loads the full query result schema.
For Parquet sources, schema output SHOULD come from metadata and MAY omit sample
examples.

Options:

`--json`

`--sample-size <n>`

### 22.7 AST Command

Usage:

```bash
algraf ast chart.ag --json
```

Prints parse AST.

Useful for parser debugging and tests.

### 22.8 IR Command

Usage:

```bash
algraf ir chart.ag --json
```

Prints semantic IR.

Requires schema resolution.

### 22.9 Init Command

`algraf init --codex`, `algraf init --claude`, and `algraf init --agy` create
project root guidance for LLM coding agents.

The command MUST write an `ALGRAF_LANG.md` language reference in the target
directory. It MUST NOT overwrite an existing `ALGRAF_LANG.md` with different
content. If the file already matches the built-in template, the command is a
no-op for that file.

`--codex` and `--agy` target `AGENTS.md`. `--claude` targets `CLAUDE.md`.
Existing agent files MUST NOT be overwritten. If an existing target file already
mentions `ALGRAF_LANG.md`, the command leaves it unchanged. Otherwise it appends
a short section that points agents at `ALGRAF_LANG.md`.

The command MAY accept a positional target directory. If no directory is
provided, it uses the current directory. At least one of `--codex`, `--claude`,
or `--agy` is required.

### 22.10 Exit Codes

Exit code 0:

success

Exit code 1:

diagnostic errors

Exit code 2:

CLI usage error

Exit code 3:

I/O error

Exit code 4:

internal error

Internal errors SHOULD print bug-report guidance.

### 22.11 Diagnostic Output

Human output SHOULD include:

file path

line and column

severity

code

message

source excerpt

help text

JSON output SHOULD be available:

```bash
algraf check chart.ag --json
```

JSON diagnostic output MUST use a stable object shape:

```json
{
  "source": "algraf",
  "code": "E1101",
  "severity": "error",
  "message": "unknown column",
  "file": "chart.ag",
  "span": { "start": 42, "end": 47 },
  "range": {
    "start": { "line": 3, "character": 10 },
    "end": { "line": 3, "character": 15 }
  },
  "related": [],
  "help": "Check the CSV header or use a backtick-quoted column identifier."
}
```

`span` uses UTF-8 byte offsets.

`range` uses zero-based line and UTF-16 character offsets to match LSP conventions.

The human diagnostic renderer SHOULD use the same diagnostic data model as JSON output and LSP diagnostics.

## 23. Rust Crate Architecture

### 23.1 Workspace Layout

Recommended layout:

```text
algraf/
  Cargo.toml
  crates/
    algraf-cli/
    algraf-core/
    algraf-data/
    algraf-driver/
    algraf-editor-services/
    algraf-lsp/
    algraf-render/
    algraf-semantics/
    algraf-syntax/
    algraf-wasm/
  docs/
  examples/
  tests/
```

For early implementation, a single crate is acceptable.

The design SHOULD keep module boundaries aligned with future crates.

### 23.2 Module Boundaries

`syntax`:

lexer

parser

AST/CST

parse diagnostics

formatter

The parser is internally split into cursor, tree-building, block/declaration,
value, algebra, and post-parse validation modules while preserving the public
`parse`, `parse_algebra`, and `Parse` API.

shared source-constructor metadata table (recognized constructor names, format
policy, path-argument rules, documentation, completion text; spec §10.11),
expressed without depending on the data crate's runtime format type

`semantics`:

name resolution

schema-aware validation

IR

geometry registry

semantic diagnostics

`driver`:

source-expression extraction

source-relative path resolution

a load-free chart data plan (`ChartDataPlan`) that records primary location,
named table locations, explicit formats, source spans, and the dependency
inventory before any byte load; loading and schema resolution execute from it

data and schema loading orchestration

injectable synchronous data I/O provider and OS-backed compatibility adapter

a shared, injectable schema cache service keyed by `DataSourceKey` and validated
by `SourceFingerprint` (spec §10.9), storing schemas and load errors rather than
full frames

runtime cache policy documentation distinguishing schema, full-frame,
render-result, and persistent caches (spec §10.15)

chart data dependency inventory

chart analysis preparation for CLI, LSP, and render callers

centralized driver/data error-to-diagnostic mapping shared by CLI and LSP

small invocation variable expansion helpers for CLI and embedded callers;
expansion is raw source-fragment substitution before parsing and has no access
to the environment, filesystem, network, or process state

a preparation report model that collects parse, load, semantic, data-warning,
and render entries in deterministic phase order, plus a partial preparation path
that does not short-circuit at the first recoverable phase boundary

`data`:

CSV loading

schema inference

dataframe

type inference

`render`:

scale training

layout

stats, internally split into `stats/bin.rs`, `stats/density.rs`,
`stats/smooth.rs`, `stats/summary.rs`, and shared deterministic-output helpers

geometries

SVG emission

space training, internally split so temporal tick/format helpers and polar
frame helpers live outside the axis-training core

an embedded rendering facade that accepts inline source, caller-provided bytes
or `serde_json::Value`, explicit data format, optional variables, render options,
an opt-in interactive SVG flag, and injected driver I/O; the facade MUST use the
same driver preparation and render execution paths as CLI render and MUST NOT
depend on `algraf-cli`

The render crate is internally split along the planning/emission boundary of
§24.6: planning modules resolve a render scene (derived tables, scales, layout,
guide measurements, legends) and a closed set of private backends emit output
from it — the canonical SVG backend and a serializable draw-list backend.

`lsp`:

tower-lsp backend

document cache

LSP transport and request routing

preview rendering orchestration

`editor-services`:

shared completion

shared hover

signature help

semantic tokens

code actions

navigation, references, rename, and document symbols

diagnostics publication

`cli`:

argument parsing

command dispatch

I/O

`wasm`:

browser-embeddable runtime entry points

in-memory driver I/O integration

editor-service and render facades for host applications

### 23.3 Dependency Guidelines

Recommended dependencies:

`clap` for CLI

`logos` for lexing

`rowan` for lossless concrete syntax trees

`serde` for debug JSON

`serde_json` for AST/IR output

`csv` for CSV parsing

`chrono` or `time` for temporal parsing and formatting

`indexmap` for stable ordering

`geo-types`, `geojson`, and `shapefile` for geospatial sources

`libsqlite3-sys` for local SQLite sources

`arrow-ipc` for Arrow IPC stream caller data

`thiserror` for errors

`tower-lsp` for LSP

`tokio` for async LSP runtime

Async driver helper traits SHOULD use `std::future`/boxed futures unless a
runtime-specific implementation is introduced by a caller. The driver crate MUST
NOT depend on Tokio.

The `driver` crate SHOULD depend only on `core`, `syntax`, `data`, and
`semantics`. CLI and LSP MAY depend on the driver, but the driver MUST NOT depend
on CLI or LSP crates.

The `driver` crate MUST keep data I/O behind its provider trait. Existing public
driver helpers MAY keep OS-backed defaults for compatibility, but internal
preparation and loading paths MUST be able to use an injected provider.

`dashmap` for concurrent LSP caches

`insta` for snapshots

`similar` or `pretty_assertions` for test diffs

Polars is not a version 0.1 dependency.

Future Polars support MAY be added behind an optional feature if it implements the internal table abstraction without changing language semantics.

### 23.4 Error Handling

Internal errors use Rust `Result`.

User diagnostics are not exceptional.

Parser returns AST plus diagnostics.

Analyzer returns optional IR plus diagnostics.

Renderer returns result plus render diagnostics or render error.

Panic is reserved for programmer bugs.

CLI catches top-level errors and prints concise messages.

LSP logs internal errors and avoids crashing where possible.

The driver SHOULD provide a single mapping from driver/data loading errors to
stable diagnostic `(code, message)` pairs, so the CLI and LSP report
missing-file, unreadable-file, malformed CSV/JSON, SQLite query/safety/type
errors, and geospatial parse conditions identically (codes `E1005`–`E1013`,
`E1805`).

The driver SHOULD offer a preparation report that holds parse, load, semantic,
data-warning, and render entries in deterministic phase order. Diagnostics that
carry a meaningful source span live in the diagnostic stream; data inference
warnings, which generally know only a data-column name, are kept as structured
entries with table/source/column context rather than given a synthetic span
(spec §10.3). A partial preparation path MAY return whatever loaded successfully
alongside this report instead of failing at the first recoverable phase
boundary, while a strict preparation path remains available for render callers
that must block on parse, load, or semantic errors.

### 23.5 Immutability

Core pipeline SHOULD use immutable data structures where practical.

Parser state is mutable internally.

Analyzer may build IR mutably internally.

Runtime should pass immutable references to data and models.

Renderer should emit strings without mutating shared global state.

### 23.6 Parallelism

Version 0.1 does not need parallel rendering.

Future rendering MAY parallelize:

independent spaces

facets

stat computations

guide layout

Parallel output must remain deterministic.

## 24. Rendering Pipeline

### 24.1 Full Pipeline

1. Read source.
2. Lex source.
3. Parse source into AST.
4. Collect parse diagnostics.
5. Extract chart data source.
6. Resolve data path.
7. Load schema.
8. Analyze derive declarations and derived schemas.
9. Analyze spaces against their active primary or derived table schemas.
10. Build chart IR.
11. Load data.
12. Compute derived tables.
13. Desugar high-level statistical geometries where required.
14. Compute geometry-local stats.
15. Train scales.
16. Compute layout.
17. Build render model.
18. Emit SVG.
19. Write output.

### 24.2 LSP Pipeline

1. Receive document text.
2. Lex source.
3. Parse source into AST.
4. Publish parse diagnostics.
5. Extract data source if possible.
6. Resolve schema on a blocking task when filesystem or SQLite metadata may be
   touched.
7. Infer derived table schemas with schema-only stat planning where possible.
8. Analyze source when schemas are available.
9. Publish semantic diagnostics.
10. Serve completions and hover from cached AST and schemas.

### 24.3 Shared Core

Parser MUST be shared.

Analyzer MUST be shared.

Geometry registry MUST be shared.

Schema model MUST be shared.

CLI and LSP MUST NOT define separate syntax rules.

### 24.4 Render Model Construction

For each space:

evaluate frame IR

run geometry stat if any

train position scales

train aesthetic scales

construct layer model

After all spaces:

merge compatible scales if configured

construct guides

compute layout

finalize layer viewports

### 24.5 SVG Emission Order

1. XML/SVG opening.
2. Definitions.
3. Background.
4. Title/subtitle.
5. Plot panel background.
6. Grid.
7. Data layers in source order.
8. Axes.
9. Legends.
10. Caption.
11. Closing SVG.

### 24.6 Render Execution Boundary

The renderer is organized around one boundary, between **planning** and
**emission**:

- Planning (pipeline steps 12–17) consumes the IR and loaded data eagerly and
  resolves a fully described render scene: derived tables, geometry-local stats,
  trained scales, layout rectangles, guide measurements, glyph instances,
  planned child panels, and legends. Planning reads data only through the
  data-table abstraction and MUST NOT write output bytes.
- Emission (pipeline step 18) takes that scene and serializes it through one
  output backend. The backend MUST NOT make layout or scale decisions, and it
  MUST consume the planned render scene rather than the source AST.

Data materialization MUST be eager: stats and scale training run during
planning against in-memory tables. The output backend set is a closed,
compiled-in implementation detail: the renderer MAY expose a private trait,
enum, or facade to name this seam, but it MUST NOT expose a plugin API or accept
externally supplied backends. Guide planning (label measurement, axis-margin
reservation) and guide emission (writing axes, grids, strips, and legends to
SVG) are likewise separated, planning before final layout and emission during
document assembly.

Z-field stats (§15.16) materialize during planning like every other derived
stat. They MUST lower to ordinary derived rows consumed by existing primitive
emission paths (`Rect`, `Path`, and `Geo`), so the SVG, draw-list, raster, and
interaction-sidecar outputs observe the same planned scene. No backend may
invent a different contour, density, raster, or summary algorithm.

The renderer ships three backends over this seam:

- The **SVG backend** (§18) is canonical and emits the deterministic SVG
  document. It MUST remain unchanged in escaping, number formatting, ordering,
  and accessibility behavior regardless of any other backend.
- The **interaction sidecar** is deterministic JSON emitted on request
  (`--metadata` or `--format svg+json`, §22.3). It is not a backend that draws
  pixels; it serializes host-runtime data from the same planned scene. The
  sidecar MUST carry `version: 1`, `plot_rect`, `axes`, `chart`, `legend`,
  `marks`, `groups`, and `plots` fields in stable key order. `plot_rect` is the
  first plot area's SVG pixel rectangle. `chart` carries `title`, `subtitle`,
  `caption`, `source` (since 0.82.0), `alt`, and resolved `description` values,
  using `null` for absent values; `caption` and `source` preserve embedded
  newlines verbatim (§17.3). `legend` is `null` when no legend is present; otherwise it carries the
  resolved `position` (`"right"`, `"bottom"`, `"top"`, or `"left"`) and SVG
  pixel `rect`. `plots[]` carries every top-level, faceted, and glyph plot
  area's `id`, `plot_rect`, and `axes` so nested charts are addressable without
  re-running layout. Top-level plot IDs are `plot0`, `plot1`, etc. Glyph plot
  IDs are hierarchical and include the current mark prefix, glyph mark index,
  host source-row index, and child-space index, e.g.
  `p0:i0[3]:s0`. The bracketed row value is the host table's source row
  number, not the ordinal among rendered glyph instances, so IDs stay stable
  when earlier host rows fail to match or cannot resolve an anchor. Nested
  glyph IDs extend this prefix recursively. A plot MAY include `clip_rect` when
  Cartesian plot clipping or a glyph clip bounds data marks (§18.5). Circular
  glyph clips report their bounding rectangle in metadata; the draw scene
  carries the exact circle clip. `axes.x`
  and `axes.y`, when present, describe host-invertible scales with `scale`,
  `domain`, `range`, `format`, `label`, and (since 0.82.0) `position` — the
  resolved plot edge `"left"`/`"right"`/`"top"`/`"bottom"` (§19.2, §19.3); band
  scales also carry padding and bandwidth metadata. Continuous scale names are `linear`, `log10`, and `sqrt`;
  temporal scales use `time` with UTC microsecond domains; categorical scales
  use `band` or `nested-band` with string domains. A host inverts a continuous
  axis by applying the inverse of the named transform over `domain` and `range`;
  it inverts `time` the same way as `linear` and formats the resulting UTC
  microseconds; it inverts a band axis by selecting the nearest domain band.
  `marks[]` carries stable `id`, `plot`, `x_px`, `y_px`, optional `clipped:
  true`, `groups`, optional `interaction`, and display-ready `tooltip` rows for
  each pickable per-datum mark that survives layout. When present,
  `interaction` carries `{ "event": "click", "emit_field": "<column>" }` for an
  `On(...)` emitter. The emitted row value is resolved through
  `mark.groups[mark.interaction.emit_field]`; hosts MUST NOT evaluate Algraf
  source or scrape SVG attributes to recover it. Top-level mark IDs use
  `p{panel}:g{geometry}:r{row}`;
  glyph mark IDs prefix the hierarchical glyph plot ID, e.g.
  `p0:i0[3]:s0:g0:r2`. `groups` maps each highlight key to its
  first-appearance-ordered values and also includes event-emitter fields when a
  mark declares `On(...)`. Host runtimes MAY use the mark coordinates and group
  values to implement host-owned legend selection, plot brushing, or click
  routing; Algraf MUST NOT serialize mutable selection state. The sidecar is
  inert data: it MUST NOT contain scripts, callbacks, URLs, or host policy.
- The **draw-list backend** records a serializable, Canvas-drawable draw list of
  drawable primitives from the same scene. It is the proof that the seam supports
  more than one output format. The draw list MUST use the same locale-independent
  number formatting as §18.8 and MUST be deterministic. It is a *complete* scene
  description: every SVG element the renderer emits for the chart body and guides
  has a corresponding draw-list op with identical coordinates and colors. The op
  set is `clipStart`, `circleClipStart`, `clipEnd`, `rect`, `circle`, `path`,
  `polygon`, `image`, `line`, and `text`; geometry, guide, and glyph emission all produce
  these primitives through one shared mark sink, so
  the two backends cannot diverge below the panel level. The draw list covers the
  canvas, background, plot panels (with facet strips and labels), chart
  title/subtitle/caption, per-datum geometry marks (including polar arc/wedge
  paths), gridlines, axis lines/ticks/tick-labels/titles, and legends. Each op
  carries a `role` naming the chart region it belongs to. An `image` op carries
  an Algraf-generated embedded data href plus x/y/width/height and optional
  opacity. A shape or image op for a mark that declares interaction metadata
  (spec §14.25) also carries an inert `interaction` object with optional
  `tooltip`, `highlight`, and `event` fields, recorded through the same shared
  mark sink that the SVG backend uses for its `<title>`,
  `data-algraf-highlight`, `data-algraf-event`, `data-algraf-emit-field`, and
  `data-algraf-emit-value` affordances (§18.10) — so the two backends carry the
  same interaction metadata by construction. The top-level draw-list JSON also
  carries an `interactions` object with the exact same sidecar shape described
  above, versioned together with the sidecar. The draw list MUST NOT execute
  scripts or embed behavior; it is inert data.
- The **render-model raster backend** draws the draw list to a raster image with
  a CPU rasterizer (`tiny-skia`), not by rasterizing SVG bytes. It pulls in no
  browser runtime and no system fonts, and is deterministic for a given
  platform; anti-aliasing MAY differ across platforms. It renders the scene's
  shape primitives (rectangles, circles, paths, polygons, lines); rendering text
  glyphs is out of scope and is a documented equivalence limit — text positions
  and content are present in the draw list, and the SVG backend defines the
  intended text appearance. A backend that cannot represent a scene element emits
  `R0005` (§26) and continues.

The closed backend set is SVG, draw-list, and render-model raster. The canonical
CLI rasterizer (`--output *.png` with `--format svg`) rasterizes the *SVG*
backend's output through a system-font wrapper and remains the default,
pixel-faithful PNG path; it is distinct from the render-model raster backend
(`--format raster`, §22.3). Retained-DOM and WebGL backends and lazy or streaming
data materialization are deferred to a later release.

Schema-only planning is outside this render execution boundary: semantic
analysis may compute built-in derived schemas from typed frames, but it MUST NOT
materialize derived frames or inspect data rows. The renderer remains the owner
of full stat execution.

### 24.7 Browser / WASM Runtime

The `algraf-wasm` crate is an optional browser-embeddable runtime over the same
parse -> analyze -> render pipeline used by the CLI. It is an adapter around the
driver and renderer, not a second renderer. For charts that use capabilities
available in the WASM build, SVG output MUST be deterministic and MUST use the
same render scene, escaping, number formatting, sidecar schema, and diagnostic
shape as the native pipeline.

The Rust entry point `render_to_svg(source, files)` accepts `.ag` source text
and an in-memory map from data-source name to bytes. The runtime MUST satisfy
the driver's `DriverIo` boundary from that map and MUST NOT read host files,
network resources, process state, environment variables, or clocks. Hosts MAY
fetch data before calling the runtime, but those fetches are outside Algraf.
Missing in-memory data sources MUST surface through the existing driver/data
diagnostic path, not as panics.

The render result shape is:

```json
{
  "svg": "string or null",
  "sidecar": "string or null",
  "diagnostics": [],
  "error": "string or null"
}
```

When SVG is produced, `sidecar` MUST contain the same versioned interaction
sidecar JSON described in §24.6. `diagnostics` MUST use the existing structured
diagnostic shape (`code`, `severity`, byte-offset `span`, `message`, optional
`related`, optional `help`). `error` is reserved for catastrophic, span-less
renderer failures that cannot be expressed as a source diagnostic.

For the browser `wasm32-unknown-unknown` build, the shipped ABI is a manual
pointer/length JSON ABI, not generated `wasm-bindgen` bindings:

- `algraf_alloc(len) -> ptr` allocates a UTF-8 request buffer.
- `algraf_dealloc(ptr, len)` releases a buffer allocated by the module.
- `algraf_render_json(ptr, len) -> packed_ptr_len` reads request JSON of the
  form
  `{ "source": "...", "files": { "data.json": "..." }, "variables": { "color": "#3366cc" } }`
  and returns a pointer/length pair packed into a `u64` with the pointer in the
  low 32 bits and the byte length in the high 32 bits. The `variables` field is
  optional and defaults to an empty map.

The browser JSON ABI supports text data sources supplied as UTF-8 strings. The
runtime MUST expand `${name}` and `$name` placeholders with
`algraf_driver::expand_variables` before parsing, using the same raw
source-fragment substitution semantics as CLI `--var` (spec §22.3). Variable
substitution failures MUST return the standard render response shape with `svg`
and `sidecar` set to null, an empty diagnostics array, and a span-less `error`
string; they MUST NOT panic.
Convenience `check`, `parse`, and `format` exports are not part of the v0.34
runtime contract; hosts that need diagnostics call `render` and consume the
returned diagnostics. A future release MAY add convenience exports if their
diagnostic and span behavior is specified.

Since version 0.35.5, the WASM runtime also exposes a browser editor-service
JSON ABI for Monaco-style clients. The ABI is an adapter over the same editor
feature helpers used by the native LSP server, not a TypeScript language
implementation and not a JSON-RPC transport. A request supplies current source
text, an optional document URI, the same in-memory text data-source map used by
browser rendering, and one feature request:

```json
{
  "source": "Chart(...) { ... }",
  "uri": "inmemory://algraf/demo.ag",
  "files": { "data.csv": "x,y\n1,2\n" },
  "request": { "kind": "hover", "position": { "line": 1, "character": 12 } }
}
```

The response is:

```json
{
  "diagnostics": [],
  "result": "LSP-shaped feature result or null",
  "error": "string or null"
}
```

`diagnostics` are LSP-shaped diagnostics derived from the same parse/analyze
state used by the requested feature. `result` MUST remain close to `lsp-types`
serialization for hover, completion, signature help, formatting edits, semantic
tokens, code actions, definition, references, document highlights, prepare
rename, rename, and document symbols. Hosts MAY map those values
into editor-native provider APIs, but MUST NOT reimplement Algraf parsing,
semantic analysis, registry documentation, formatting, code actions, or hover
decision logic in the browser client.

The v0.39.5 browser demo does not advertise an inlay-hint provider. Legacy
editor-service `inlayHints` requests MAY return an empty LSP-shaped list, but
derived-table schema inspection belongs to hover.

The editor-service ABI uses UTF-16 LSP positions and ranges at the boundary.
Internal Algraf spans remain byte offsets. Implementations MUST test
byte-offset ↔ UTF-16 conversion with non-ASCII text because browser editors
typically expose UTF-16 columns.

The browser editor service sees only host-supplied in-memory files. Completion
and hover MUST use those files for primary and named-table schema samples when
available. Hover over source strings MAY also show bounded raw CSV/TSV row
previews from those in-memory files. Navigation to host-supplied data MAY return
synthetic `inmemory://algraf/...` locations. Navigation that would require
arbitrary host filesystem access MUST fail gracefully in the browser rather than
reading files.

The WASM runtime does not enable the native `sql` Cargo feature, so SQLite
sources are unavailable in that build. A SQLite source in a no-`sql` build MUST
fail through the same data/driver diagnostic path used for SQLite data errors.
The WASM runtime MAY also omit Arrow IPC stream decoding. An explicit
`arrow-stream` load in a build without that feature MUST fail through `E1021`
rather than panic or attempt process stdin access.
PNG/raster output, filesystem-backed source discovery, and host-owned UI state
are outside the v0.34 browser runtime.

The root-level browser demo is a static host for this ABI, not a separate
runtime contract. When the demo is served from a subpath, such as GitHub Pages
project sites, it MUST resolve its own public `wasm/` and `data/` assets through
the host's configured public base path. Root-absolute demo asset URLs are not
part of the browser ABI.

Version 0.53.0 organizes the static browser demo host as a light-themed site
with `/`, `/docs`, and `/demos` routes. The landing page, guided docs, and demos
route use the browser WASM runtime; Monaco-backed editor feedback in those hosts
remains an adapter over the browser editor-service ABI described above, not a
separate TypeScript language implementation. Static deployment hosts MAY serve
the same app through an HTML fallback for clean-path routes.

Version 0.54.0 configures the static browser demo host's Monaco editors so hover
and overflow widgets can paint outside the editor viewport. This is host UI
behavior only; the browser editor-service ABI and Rust hover decision logic are
unchanged.

Version 0.57.0 reorganizes the static demo host's `/docs` route into a
multi-page documentation section (an overview plus topic pages for the algebra,
bar layouts, facets, insets, statistics, theming, and tooling), each embedding
live editors over the browser ABIs above. This is host UI and content only; the
browser render and editor-service ABIs are unchanged.

Version 0.60.0 publishes the VS Code extension `.vsix` and standalone browser
`algraf.wasm` runtime as GitHub Release assets with both versioned filenames
and `latest` aliases. This is repository packaging only; the language, renderer,
editor services, browser demo, and WASM ABI are unchanged.

Version 0.63.0 adds package-shaped browser integrations without changing `.ag`
syntax, rendering, scale, guide, data, CLI, LSP, or WASM ABI behavior. The
canonical static editor assets live under `editors/assets/`. The VS Code
extension remains self-contained by syncing those assets into package-local
grammar and language-configuration paths before extension compile, check,
package, or prepublish workflows. The `packages/wasm/` package publishes the
org-free package name `algraf-wasm` and owns runtime loading, browser ABI types,
and helpers for caller-provided WASM URLs or a generated package-local
`algraf.wasm` artifact. The `editors/monaco/` package publishes the org-free
package name `algraf-editor` and owns Monaco language registration, TextMate
grammar wiring, language configuration, theme defaults, marker conversion,
provider registration, structural runtime/editor-service types, and a thin
React editor component.

`algraf-wasm` MUST NOT include Monaco, React, preview UI, editor chrome, or
product-specific controls. `algraf-editor` MUST NOT implement Algraf parsing,
analysis, rendering, diagnostics, completion, hover, signature help,
formatting, semantic tokens, code actions, symbols, definition/reference, or
rename in TypeScript; it MUST adapt the upstream WASM/editor-service ABI into
Monaco providers. In source mode, hosts MAY consume sibling package directories
from `../algraf` and pass an explicit local `wasmUrl` for a generated artifact
copied into public assets. In packed mode, release validation MAY build local
tarballs into ignored package `dist/` or workspace `artifacts/` directories and
install them with `file:` paths before npm publication. Generated
`algraf.wasm` binaries and local package tarballs MUST NOT be checked into
source.

Version 0.67.0 prepares `algraf-wasm` and `algraf-editor` for manual npm
publication without changing `.ag` syntax, rendering, editor-service behavior,
or the browser JSON ABI. Published package manifests MUST expose generated
`dist/` entrypoints for `main`, `module`, `types`, and `exports`, with
CommonJS `dist/index.cjs`, ESM `dist/index.mjs`, and TypeScript declarations.
The WASM package tarball MUST include `dist/algraf.wasm`; the editor package
tarball MUST include generated `dist/` files plus package-local static editor
assets. `prepack` MUST build the publishable surface, and release validation
MUST inspect `npm pack --dry-run` output to prove ignored generated `dist/`
files are included by the npm `files` whitelist.

Version 0.68.0 establishes the benchmark process for Arrow-stream and
large-data aggregate performance work. The implemented benchmark-infrastructure
scope is recorded in [`V0_68_PLAN.md`](V0_68_PLAN.md).

Version 0.68.1 patches the `algraf-editor` browser package asset contract
without changing `.ag` syntax, rendering, editor-service behavior,
`algraf-wasm`, or the browser JSON ABI. Published `algraf-editor` `dist/`
entrypoints MUST NOT emit Vite-specific `?worker` or `?url` imports for Monaco
workers or Onigasm WASM assets. Browser hosts that want package-provided Monaco
worker setup MUST pass a `createEditorWorker` setup option, and hosts MUST pass
an `onigasmWasmUrl` setup option when using TextMate grammar loading.

Version 0.68.5 expands continuous gradient color literals to accept alpha hex,
`rgb(...)`, and `rgba(...)` strings. The implemented patch scope is recorded in
[`V0_68_5_PLAN.md`](V0_68_5_PLAN.md).

Version 0.69.0 is the active implementation target for Arrow-stream and
large-data aggregate performance. Planned work is recorded in
[`V0_69_PLAN.md`](V0_69_PLAN.md). The implementation keeps the existing `Table`
boundary and moves `Summary`, `SummaryBin`, `Ecdf`, `Qq`, `Cut`, categorical
domain collection, and single-column `Count` execution onto borrowed typed
column views where available, avoiding repeated scalar column-name lookup and
unnecessary category stringification in row-heavy stat loops. Native caller
stdin also has a reader-oriented path for explicit and sniffed data formats.
The broader release direction is to improve Arrow IPC stream ingest, typed
column scans, stat/domain execution, aggregate-first large-data rendering UX,
and the PDL-to-Algraf Arrow stream handoff without adding PDL syntax or
exposing concrete dataframe engines above `algraf-data`.

Version 0.64.0 adds declarative `On(event: "click", emit: column)` event
emitters for host applications. Event emitters are inert metadata attached to
the preceding per-datum mark, are serialized through the existing version-1
interaction sidecar and draw-list interaction objects, and do not introduce
selection state, callbacks, scripts, routing rules, or PDL references inside
Algraf.

The manual pointer/length ABI does not mean the shipped `.wasm` is free of
`wasm-bindgen`-style imports. Dependencies compiled for
`wasm32-unknown-unknown` MAY emit their own `wasm-bindgen` import calls; in
particular `proj4rs` (the projection backend, §16.14) parses every numeric
proj-string parameter through `js_sys` `parseFloat`/`parseInt` on that target.
A host that supplies the manual ABI therefore MUST also satisfy those imports
with a correct marshaling: the `js_sys` number parsers receive their `&str`
argument as a `(ptr, len)` pair addressing UTF-8 bytes in the module's linear
memory, and the host MUST decode that slice rather than coerce the pointer
integer. An incorrect shim does not fail loudly — it silently corrupts the
projection parameters, so projected maps render distorted while non-projected
charts are unaffected. Because §24.7 already requires WASM output to match the
native render scene for capabilities available in the build, and projection is
one such capability, projected SVG from the browser runtime MUST be coordinate-
identical to the native renderer for the same source and data.

## 25. Examples Compared With GramGraph

### 25.1 Grouped Line Chart

GramGraph:

```bash
cat examples/timeseries.csv | gramgraph 'aes(x: time, y: value, color: series) | line()'
```

Algraf:

```ag
Chart(data: "examples/timeseries.csv") {
    Space(time * value) {
        Line(stroke: series)
    }
}
```

Semantic difference:

GramGraph maps grouping through color in a pipeline.

Algraf defines `time * value` as space and maps `series` to stroke.

### 25.2 Dodged Bar Chart

GramGraph:

```bash
cat examples/financials.csv | gramgraph 'aes(x: quarter, y: amount, color: type) | bar(position: "dodge")'
```

Algraf:

```ag
Chart(data: "examples/financials.csv") {
    Space((quarter / type) * amount) {
        Bar(fill: type)
    }
}
```

Semantic difference:

GramGraph uses a geometry position argument.

Algraf represents dodge as nested space.

### 25.3 Stacked Bar Chart

GramGraph:

```bash
cat examples/financials.csv | gramgraph 'aes(x: quarter, y: amount, color: type) | bar(position: "stack")'
```

Algraf:

```ag
Chart(data: "examples/financials.csv") {
    Space(quarter * amount) {
        Bar(fill: type, layout: "stack")
    }
}
```

Semantic difference:

Stacking is a collision policy inside a normal space.

Dodging is not.

### 25.4 Faceted Plot

GramGraph:

```bash
cat examples/regional_sales.csv | gramgraph 'aes(x: time, y: sales, color: product) | line() | facet_wrap(by: region)'
```

Algraf:

```ag
Chart(data: "examples/regional_sales.csv") {
    Space((time * sales) / region) {
        Line(stroke: product)
    }
}
```

Semantic difference:

Algraf treats faceting as nesting the whole 2D plane by `region`.

### 25.5 Ribbon

GramGraph:

```bash
cat examples/ribbon_data.csv | gramgraph 'aes(x: x, y: y, ymin: lower, ymax: upper) | ribbon() | line()'
```

Algraf:

```ag
Chart(data: "examples/ribbon_data.csv") {
    Space(x * (lower + upper)) {
        Ribbon(ymin: lower, ymax: upper, fill: "steelblue", alpha: 0.3)
    }

    Space(x * y) {
        Line(stroke: "steelblue")
    }
}
```

Semantic difference:

Algraf uses blend to train a y domain that contains both interval bounds.

### 25.6 Histogram

GramGraph:

```bash
cat examples/distribution.csv | gramgraph 'aes(x: value) | histogram(bins: 25)'
```

Algraf:

```ag
Chart(data: "examples/distribution.csv") {
    Space(value) {
        Histogram(bins: 25)
    }
}
```

Algraf primitive form:

```ag
Chart(data: "examples/distribution.csv") {
    Derive bins = Bin(value, bins: 25)

    Space(bin_start * count, data: bins) {
        Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)
    }
}
```

Semantic difference:

Both express a 1D input.

Algraf can express the chart either as a high-level stat geometry or as an explicit derived table plus primitive `Rect` marks.

### 25.7 Boxplot

GramGraph:

```bash
cat examples/demographics.csv | gramgraph 'aes(x: gender, y: height, color: gender) | boxplot()'
```

Algraf:

```ag
Chart(data: "examples/demographics.csv") {
    Space(gender * height) {
        Boxplot(fill: gender)
    }
}
```

### 25.8 Heatmap

GramGraph:

```bash
cat examples/heatmap_data.csv | gramgraph 'aes(x: x, y: y, fill: value) | heatmap()'
```

Algraf:

```ag
Chart(data: "examples/heatmap_data.csv") {
    Space(x * y) {
        Tile(fill: value)
    }
}
```

## 26. Diagnostics Catalog

Diagnostic codes are registered centrally in `algraf-core`. Implementations
MUST emit registered codes, and JSON/LSP diagnostics MUST serialize the code as
its stable string form.

### 26.1 Parse Diagnostics

`E0001 expected Chart block`

`E0002 expected '(' after Chart`

`E0003 expected chart argument`

`E0004 expected ':' after argument name`

`E0005 expected argument value`

`E0006 expected ')'`

`E0007 expected '{'`

`E0008 expected '}'`

`E0009 expected algebra expression`

`E0010 expected identifier`

`E0011 unexpected token`

`E0012 unterminated string`

`E0013 invalid number literal`

`E0014 expected ',' or ')'`

`E0015 expected ',' or ']'`

`E0016 expected '=' after derived table name`

`E0017 expected stat call after '='`

`E0018 invalid escape sequence`

`E0019 unterminated quoted identifier`

`E0020 unterminated block comment`

`E0021 malformed binding or map-literal entry` (a missing `=` in a `let`, or a
missing `=>`/stray separator in a map literal)

`E0022 malformed source header`

`E0023 unsupported source language version`

`E0024 unknown or duplicate feature gate`

`E0025 required feature gate is not enabled`

### 26.2 Semantic Diagnostics

`E1001 Chart requires data argument or Table main`

`E1002 duplicate Chart argument`

`E1003 unsupported Chart argument`

`E1004 data source must be string literal, source constructor, table reference, or caller-input sentinel`

`E1005 data file not found`

`E1006 data file could not be read`

`E1007 CSV header missing`

`E1008 duplicate data column`

`E1009 malformed JSON input`

`E1010 JSON data must be an array of row objects`

`E1011 SQLite query error`

`E1012 unsafe or nondeterministic SQLite source`

`E1013 unsupported SQLite result type`

`E1014 invalid temporal parse declaration`

`E1015 duplicate temporal parse declaration`

`E1016 unknown temporal parse target`

`E1017 invalid temporal literal`

`E1018 temporal literal in an unsupported position`

`E1019 explicit temporal parse failure (onError: "error")`

`E1020 Parquet parse error or unsupported Parquet column type`

`E1021 Arrow IPC stream parse error or unsupported Arrow stream column type`

`E1022 unsupported caller-provided stream format`

`E1101 unknown column`

`E1102 ambiguous column`

`E1103 unknown derived or named table`

`E1104 duplicate derived table`

`E1105 duplicate Table declaration`

`E1106 Table data file not found`

`E1107 Table data file could not be read`

`E1108 Table name conflicts with derived table`

`E1201 unknown geometry`

`E1202 unknown property`

`E1203 duplicate property`

`E1204 invalid property type`

`E1205 missing required property`

`E1206 interaction property not supported on this geometry`

`E1207 invalid interaction property value`

`E1301 unsupported algebraic space`

`E1302 incompatible geometry and space`

`E1303 unsupported data type for scale`

`E1304 unsupported blend domains`

`E1305 blend operator must be parenthesized`

`E1306 3D Cartesian spaces are unsupported; use nesting for facets`

`E1401 statistic failed`

`E1402 insufficient data for statistic`

`E1403 unknown stat`

`E1404 invalid stat input`

`E1405 temporal binning is not supported in this version`

`E1406 missing z-channel stat input`

`E1407 z-channel stat input is not numeric`

`E1408 z-field stat x/y input is malformed or non-numeric`

`E1501 cycle between derived table declarations`

`E1701 let binding value must be a constant`

`E1702 duplicate let binding`

`E1703 invalid feature-gate declaration`

`E1704 unknown Theme property`

`E1705 invalid Theme property value`

`E1706 invalid Style fragment`

`E1601 invalid gradient declaration`

`E1602 scale mode or gradient requires a compatible color mapping`

`E1603 invalid range declaration`

`E1604 map key / category mismatch, non-increasing breaks, or disagreeing range/labels lengths`

`E1605 null bound not permitted here` (reserved)

`E1606 map/array form wrong for this scale kind`

`E1607 strokeWidth/size scale requires a numeric column`

`E1608 tickInterval requires a temporal axis`

`E1609 tickInterval ignored because exact breaks are declared` (warning)

`E1801 spatial space requires a geometry column`

`E1802 invalid or unknown projection`

`E1803 overlaid spaces declare conflicting projections`

`E1804 Geo mark requires a spatial space`

`E1805 GeoJSON / shapefile parse error or unsupported geometry type`

`E1901 invalid coordinate system (expected "cartesian" or "polar")`

`E1902 invalid polar theta axis (expected "x" or "y")`

`E1903 innerRadius out of range (expected a number in [0, 1))`

`E1904 polar coordinates require a 1D or 2D (a * b) frame`

`E1905 3D+ polar frames are unsupported`

`E1906 invalid polar gridShape (expected "circle" or "polygon")`

`E1907 invalid temporal output format`

`E1908 invalid numeric text format`

`E1909 invalid polar startAngle`

`E1910 invalid polar direction or radius mapping`

`E1911 reserved`

`E1912 removed or unsupported frame operator`

`E1913 invalid event emitter declaration`

`E2001 render mark budget exceeded`

Diagnostic codes from the removed `Inset` block (the 2101–2110 range) were
retired in version 0.71.0 along with the block itself; they MUST NOT be reused.

`E2201 invalid or unsupported Glyph argument, or glyph name shadows a geometry`

`E2202 unknown or invalid Glyph data table`

`E2203 invalid or missing Glyph key`

`E2204 unresolved Glyph key in the host row-context chain`

`E2205 unsupported Glyph placement or incompatible key column types`

`E2206 invalid Glyph viewport sizing`

`E2207 reserved for glyph guide/legend policy errors`

`E2208 reserved for glyph placement policy errors`

`E2209 nested Glyph depth exceeded`

`E2210 recursive Glyph mark budget exceeded`

### 26.3 Warning Diagnostics

`W2001 empty Space block`

`W2002 geometry produced no marks`

Glyph rendering MAY also emit `W2002` when a glyph mark cannot produce child
marks for a host row. A single unmatched host row uses
`Glyph matched no child rows`; when more than one host row is unmatched for the
same glyph mark, renderers SHOULD emit one summary warning of the form
`Glyph matched no child rows for N of M host rows`.

`W2003 rows dropped due to missing values`

`W2004 legend omitted because too many categories`

`W2005 axis labels may overlap`

`W2006 unsupported declaration ignored`

`W2007 invalid values treated as missing`

`W2008 high-cardinality temporal nesting may create excessive bands or panels`

### 26.4 Hint Diagnostics

`H3001 use nested algebra for dodged bars`

`H3002 quote literal color names for clarity`

`H3003 parenthesize blend expressions`

`H3004 use Guide to override axis label`

`H3005 choose fill or stroke; colour is not a property alias`

### 26.5 Internal Render Diagnostics

The renderer may emit internal `R` warnings for non-semantic rendering
conditions. These are registered codes and may appear in CLI JSON/LSP output,
but they are implementation-oriented rather than authoring-rule diagnostics.

`R0001 geometry is not yet supported by the renderer`

`R0002 geometry is incompatible with the trained render space`

`R0003 space or facet could not be laid out`

`R0004 scale declaration could not be applied during rendering`

`R0005 scene element could not be represented by the selected backend`

`R0005` is reserved for a non-SVG backend (e.g. the draw-list or raster
backend) encountering a planned scene primitive it cannot serialize. The
canonical SVG backend never emits it. A backend that emits `R0005` MUST still
produce deterministic output for the remainder of the scene.

## 27. Testing Strategy

### 27.1 Test Categories

lexer tests

parser tests

resilience tests

semantic analysis tests

schema inference tests

stat tests

stat determinism tests

scale tests

layout tests

SVG snapshot tests

CLI integration tests

LSP request tests

formatter tests

Stat determinism tests MUST materialize or render each statistical transform
against equivalent input rows in different orders and assert byte-identical
output. The covered transforms MUST include 1D bins, 2D bins, hex bins,
calendar/temporal bins, density, 2D density contours, contour lines, contour
bands, rectangular z summaries, hex z summaries, count, smooth, and boxplot
quantiles. New stats SHOULD add a determinism fixture before they are exposed
through render.

### 27.2 Lexer Tests

Test identifiers.

Test quoted identifiers.

Test strings.

Test escaped strings.

Test numbers.

Test punctuation.

Test comments.

Test rowan trivia preservation.

Test invalid characters.

Test spans.

### 27.3 Parser Tests

Test minimal chart.

Test chart args.

Test `Chart(data: stdin)`.

Test derive declarations.

Test empty chart.

Test space algebra.

Test geometry args.

Test space `data` args.

Test space-local `Theme` declarations.

Test arrays.

Test nested parentheses.

Test operator precedence.

Test trailing commas.

Test quoted column identifiers in algebra and properties.

Parser tests SHOULD be backed by a grammar fixture corpus.

Recommended layout:

```text
tests/fixtures/parser/
  valid/
    minimal.ag
    derive-bin-rect.ag
    facet-wrap.ag
  invalid/
    missing-space-rhs.ag
    unparenthesized-blend.ag
    missing-brace-before-space.ag
  snapshots/
    minimal.ast.json
```

### 27.4 Resilience Tests

Test `Space(quarter / )`.

Test missing `}`.

Test missing `)`.

Test invalid geometry body.

Test unterminated string.

Test incomplete argument.

Test top-level garbage.

Test missing braces before a following `Space`, `Derive`, or `Theme` item.

Each resilience test MUST assert:

parser returns AST

parser returns diagnostics

parser does not panic

useful context remains available for completion

The resilience suite MUST also cover adversarial **nesting and malformed-input**
cases as deterministic fixtures: deeply nested algebra (parentheses and operator
chains), deeply nested array literals, densely unbalanced delimiters, and
truncation of a valid document at every byte boundary. The analyzer MUST tolerate
the same deeply nested algebra without panicking or overflowing. The formatter
MUST return invalid input unchanged (spec §21.10) and MUST be idempotent on valid
input. These are fixtures rather than a fuzzing harness, so they run in CI
without extra dependencies; property-based fuzzing remains OPTIONAL (spec §27.8).
The implementation currently relies on the host stack depth rather than an
explicit nesting limit; if a future change makes parsing or analysis recurse more
deeply per level, an explicit nesting-limit diagnostic SHOULD be added here
before relaxing these tests.

### 27.5 Semantic Tests

Test unknown columns.

Test quoted column resolution.

Test derived table resolution.

Test unknown derived table diagnostics.

Test unknown geometry.

Test duplicate property.

Test property type mismatch.

Test unsupported 3D space.

Test `x * y * group` diagnostic suggests `(x * y) / group` when applicable.

Test unparenthesized blend diagnostics.

Test bar dodge hint.

Test stacked bar validation.

Test facet validation.

Test temporal column inference.

Test provisional LSP sampled types do not emit hard errors.

Test late invalid numeric values are treated as missing with an aggregated warning.

Test temporal binning diagnostics when temporal binning is unavailable.

Test high-cardinality temporal nesting warning.

Test continuous fill and stroke scale validation.

Test ribbon required properties.

Test `Histogram` desugaring equivalence to `Derive` plus `Rect`.

### 27.6 Render Snapshot Tests

Render SVG snapshots SHOULD be stable.

Snapshots SHOULD avoid timestamps.

Snapshots SHOULD use small deterministic datasets.

Each core geometry SHOULD have at least one SVG snapshot.

The canonical render fixture set SHOULD include:

minimal point plot

dodged bar via nested algebra

stacked bar via `Bar(layout: "stack")`

facet wrap via `(x * y) / group`

histogram high-level form

histogram primitive `Derive` plus `Rect` form

ribbon with blended y domain

temporal line chart

space-local theme override

The high-level and primitive histogram fixtures SHOULD assert equivalent visual output after normalizing metadata.

### 27.7 LSP Tests

LSP tests SHOULD simulate:

initialize

didOpen

didChange

completion

hover

diagnostics

formatting if implemented

Completion tests SHOULD verify schema-aware suggestions.

Hover tests SHOULD verify operator docs, derived-table schema hover,
source-string schema/row previews, declaration/geometry call docs, and
non-ASCII UTF-16 ranges.

Diagnostics tests SHOULD verify ranges.

In addition to the cross-feature integration suite above, each LSP feature module
(completion, hover, semantic tokens, navigation, signature help, rename, inlay
hints, diagnostics conversion, code actions, preview, document symbols) SHOULD
own focused unit tests next to its implementation. At least one module-level test
MUST exercise non-ASCII byte-span ↔ UTF-16 position conversion (spec §11.2,
§21.x), since spans are byte offsets while LSP positions and token lengths are
UTF-16 units. The integration suite remains the home for protocol-level behavior
that spans more than one feature.

Version 0.81.0: document-version cache invariants, including stale lower-version
updates that must not clobber newer text, SHOULD have focused backend/document
management tests in addition to any protocol-level coverage. Those tests SHOULD
assert cached version/text behavior and at least one text-derived surface, such
as semantic tokens, without depending on server-to-client diagnostic socket
draining.

### 27.8 Property-Based Tests

Property tests MAY verify:

parser never panics on random token streams

span ranges are valid

formatter output parses

scale mapping stays within range

SVG escaping never emits raw unsafe characters

## 28. Performance

### 28.1 Parser Performance

Parser SHOULD be linear in token count.

Parser SHOULD avoid excessive allocation.

Parser SHOULD parse a 100-line file in under 5 ms on the reference development machine.

Parser SHOULD parse a 1,000-line file in under 25 ms on the reference development machine.

Benchmark machines and thresholds MUST be documented when benchmarks are added to CI.

Version 0.19 provides `scripts/perf-baseline.sh` for local parser/schema/render
timing. It is not a CI gate.

### 28.2 LSP Latency

Completion SHOULD respond under 50 ms from warm cache.

Hover SHOULD respond under 50 ms from warm cache.

Diagnostics SHOULD update within 250 ms after the debounce window for typical files.

Schema resolution SHOULD not block editor input.

### 28.3 Render Performance

Version 0.1 targets small to medium CSV files.

Rendering 10,000 points to SVG SHOULD complete in under 200 ms on the reference development machine.

Rendering a 25-bin histogram from 100,000 rows SHOULD complete in under 500 ms on the reference development machine.

Rendering 1,000,000 raw points to SVG is not a target; version 0.43 makes this
explicit with the mark budget in §18.13. Large-data rendering SHOULD use
aggregation, binning, sampling, SQLite queries, or Parquet columnar sources to
materialize bounded scene sizes.

Future versions MAY stream data and aggregate stats without materializing rows.

Current large-data benchmark strategy and baseline snapshots live in
[`V0_68_PLAN.md`](V0_68_PLAN.md). The follow-on performance implementation
plan lives in [`V0_69_PLAN.md`](V0_69_PLAN.md). Machine-specific timing
thresholds MUST NOT be made mandatory without recording the reference hardware
and variance policy.

### 28.4 Memory

LSP SHOULD cap cached document count if needed.

Schema cache SHOULD avoid storing full data.

Render command MAY load full CSV into memory in version 0.1.

## 29. Security

### 29.1 General Security

Algraf source is declarative.

Algraf MUST NOT execute arbitrary code. Chart source is never executable: the
only script Algraf can emit is the single fixed, audited interactive runtime
(spec §29.3), which is shipped by Algraf, identical across charts, opt-in
(§22.3), and never supplied or extended by `.ag` source.

Algraf MUST NOT shell out during render.

Algraf MUST NOT load remote resources by default.

Algraf MUST escape SVG text and attributes.

Algraf SHOULD cap resource usage for LSP schema reads.

SQLite sources MUST be opt-in through source syntax and the `sql` feature gate,
MUST open only local database paths, MUST execute read-only single statements,
and MUST require deterministic `ORDER BY` ordering. Remote SQL, URL-valued data
sources, credential lookup (`env("VAR")`), and command sources MUST remain
disabled unless a later version defines and tests explicit opt-in surfaces.

Parsing and analysis MUST remain resilient against adversarial nesting and
malformed input: recover and continue, never panic (spec §12.1, §27.4). This
version does not impose an explicit nesting-depth limit and relies on the host
stack; a deterministic resilience fixture suite (spec §27.4) guards the
guarantee. If a future change increases per-level recursion such that realistic
adversarial inputs could exhaust the stack, an explicit nesting-limit diagnostic
SHOULD be introduced (reserved for that purpose) rather than allowing a crash.

### 29.2 Path Handling

Data source paths resolve relative to chart source.

CLI MAY allow absolute paths.

`Sqlite(...)` database paths follow the same path rules. URL schemes are not
accepted as data sources in version 0.21.

LSP SHOULD respect workspace boundaries where possible.

Path traversal is not inherently unsafe for local CLI, but editor integrations SHOULD avoid surprising reads outside workspace.

### 29.2.1 Embedded Host I/O

Embedded rendering MUST NOT read process stdin, environment variables, network
resources, or run commands implicitly. The secure default exposes only
caller-provided primary input bytes and denies path reads. Hosts that need
filesystem data MUST provide an explicit `DriverIo` policy, such as an
allowlisted in-memory provider or a controlled filesystem provider.

Denied embedded host I/O MUST be reported distinctly from missing local files
where possible, using permission-denied wording such as `host I/O denied`.
Inline source uses a stable diagnostic label and resolves relative paths against
the configured base directory, or the current working directory when no base
directory is provided.

Embedded rendering exposes an `interactive` render option for SVG output. When
enabled, it MUST select the same fixed, audited runtime as CLI
`--interactive` (§22.3) and MUST NOT accept script text, script URLs, callbacks,
or runtime extensions from chart source or host configuration. The default MUST
remain script-free. The option MUST NOT change PNG output.

### 29.3 SVG Injection

All text labels are escaped.

All attribute values are escaped.

Color values are validated before insertion.

The renderer MUST NOT inject raw user strings into SVG (§18.9). This applies to
interaction metadata too: tooltip text, highlight group values, event names,
emitted field names, and emitted row values (spec §14.25) are escaped before
insertion into `<title>` children and `data-algraf-*` attributes (§18.10).

**Static SVG is script-free by default.** A `<script>` element MAY appear in the
output only when interactive output is explicitly requested (CLI `--interactive`,
spec §22.3; the interactive LSP preview, §21.18; or an embedded host's
interactive SVG render option, §29.2.1). When it does, the embedded script is a
single, fixed, audited runtime shipped by Algraf and identical across all charts:
it reads the inert per-mark metadata (`<title>` tooltips,
`data-algraf-highlight` groups, and event-emitter attributes), plus the emitted
plot rectangles and Cartesian axis tick labels, and implements tooltip-on-hover,
highlight-on-hover, and crosshair value readouts. Chart source can never supply,
extend, or parameterize this script — there is no path from `.ag` text to
executable code. The script
performs no network access and is deterministic given the same SVG. Absent the
opt-in, no `<script>` is emitted.

**URL-valued properties.** Source-authored URL-valued properties (hyperlinks,
image `href`s, tooltip links) are rejected rather than embedded. This is a
deliberate security decision: allowing chart-supplied URLs would create an
SVG-injection and exfiltration surface (a `data:`/`javascript:` href, or an
external fetch) that conflicts with the no-network rule (§29) and the
script-free-by-default guarantee above. `Image(src: ...)` (§14.24.1) is the
exception only for local image files: the source names a local path, and Algraf
loads it through the host I/O boundary and emits an Algraf-generated embedded
`data:image/...` href. If a future version allows source-authored URLs, they
MUST be gated by an explicit host/CLI policy that defaults to denied, and the
policy MUST specify their interaction with this section, sidecars, host
runtimes, and previews.

### 29.4 Denial of Service

Parser must not recurse unboundedly on malformed input.

Algebra nesting depth SHOULD be capped.

Array nesting depth SHOULD be capped.

CSV sample size for LSP SHOULD be capped.

Render command SHOULD offer user-visible errors for files that are too large if limits are added.

## 30. Versioning

### 30.1 Language Version

Released version 0.1 files have no source-level version declaration.

Version 0.20.0 supports an optional source-level version declaration. Version
0.21.0 keeps the same mechanism and uses feature gates for local SQLite sources.
Unversioned files remain valid and keep current behavior, but gated syntax
requires an explicit header.

Files MAY include:

```ag
Algraf(version: "0.21")
```

The canonical v0.21 spelling is `"0.21"`; `"0.21.0"` is accepted. Older
supported minor versions such as `"0.20"` remain accepted. Unsupported future
versions MUST emit a diagnostic while preserving parser recovery.

### 30.2 Stability

Before 1.0, syntax may change.

Published examples SHOULD include language version once version declarations exist.

Diagnostics codes SHOULD remain stable where practical.

### 30.3 Feature Gates

Version 0.20.0 recognizes feature gate names as reserved strings. The recognized
names are `sql`, `network`, `plugins`, and `experimental`; they do not enable
runtime access in version 0.20.0.

Version 0.21.0 enables the `sql` feature gate for local SQLite sources only
(spec §10.12). The `network`, `plugins`, and `experimental` gates remain
recognized but reserved; they do not enable runtime access in version 0.21.0.

Interactive SVG ships in version 0.30.0 as opt-in *output* (CLI `--interactive`,
spec §22.3/§29.3), not behind a source feature gate: it adds no gated syntax, so
no `Algraf(features: [...])` declaration is required.

Version 0.49.0 exposes the same opt-in interactive SVG output through the
embedded rendering facade. This is still output selection, not source syntax;
embedded callers enable it through render options, and no feature gate is
required.

Interaction sidecars ship in version 0.32.0 as opt-in output (`--metadata` or
`--format svg+json`, spec §22.3/§24.6), not behind a source feature gate. They
serialize existing declarative interaction metadata plus plot and scale data and
add no gated syntax.

The browser/WASM runtime ships in version 0.34.0 as packaging and host-runtime
surface, not as source syntax. The `algraf-wasm` crate defaults to a no-`sql`
dependency tree for browser builds and exposes the §24.7 manual render ABI.
Native CLI and LSP builds re-enable the `sql` Cargo feature so existing SQLite
behavior is unchanged.

Future feature gates MAY enable:

remote SQL sources

plugins

custom stats

advanced quoted-identifier escape modes

### 30.4 Release Planning

Each minor release is planned in a versioned plan file under `docs/`. A plan
states the release thesis, lists Must/Should items, and records what stays
deferred. Plans are guidance, not normative: a feature is real only once this
specification says `MUST`/`SHOULD` and the implementation provides it.

| Release | Plan | Thesis | Status |
| ------- | ---- | ------ | ------ |
| 0.2.0 | [`V0_2_PLAN.md`](V0_2_PLAN.md) | Chart control and editing polish | Released |
| 0.3.0 | [`V0_3_PLAN.md`](V0_3_PLAN.md) | Expressiveness — more charts users can draw | Implemented |
| 0.4.0 | [`V0_4_PLAN.md`](V0_4_PLAN.md) | Editor & authoring experience | Implemented |
| 0.5.0 | [`V0_5_PLAN.md`](V0_5_PLAN.md) | Composition & reuse | Implemented |
| 0.6.0 | [`V0_6_PLAN.md`](V0_6_PLAN.md) | External data sources & manual scales | Implemented |
| 0.7.0 | [`V0_7_PLAN.md`](V0_7_PLAN.md) | Data backends | Implemented |
| 0.8.0 | [`V0_8_PLAN.md`](V0_8_PLAN.md) | Geospatial — geometry, projection, choropleth | Implemented |
| 0.9.0 | [`V0_9_PLAN.md`](V0_9_PLAN.md) | Pipeline unification and source-loading deduplication | Implemented |
| 0.10.0 | [`V0_10_PLAN.md`](V0_10_PLAN.md) | Semantic analyzer modularization and typed stat IR | Implemented |
| 0.11.0 | [`V0_11_PLAN.md`](V0_11_PLAN.md) | Renderer modularization and SVG safety | Implemented |
| 0.12.0 | [`V0_12_PLAN.md`](V0_12_PLAN.md) | Tooling, diagnostics, and parser cleanup | Implemented |
| 0.13.0 | [`V0_13_PLAN.md`](V0_13_PLAN.md) | Driver cleanup and preparation | Implemented |
| 0.14.0 | [`V0_14_PLAN.md`](V0_14_PLAN.md) | Driver I/O seam and VFS preparation | Implemented |
| 0.15.0 | [`V0_15_PLAN.md`](V0_15_PLAN.md) | Diagnostic pipeline and partial preparation | Implemented |
| 0.16.0 | [`V0_16_PLAN.md`](V0_16_PLAN.md) | Schema cache and compilation-phase boundary | Implemented |
| 0.17.0 | [`V0_17_PLAN.md`](V0_17_PLAN.md) | Render execution boundary | Implemented |
| 0.18.0 | [`V0_18_PLAN.md`](V0_18_PLAN.md) | Semantic surface hardening | Complete |
| 0.19.0 | [`V0_19_PLAN.md`](V0_19_PLAN.md) | Data execution boundary | Complete |
| 0.20.0 | [`V0_20_PLAN.md`](V0_20_PLAN.md) | Language versioning and reuse | Complete |
| 0.21.0 | [`V0_21_PLAN.md`](V0_21_PLAN.md) | Data backends and source security | Implemented |
| 0.22.0 | [`V0_22_PLAN.md`](V0_22_PLAN.md) | Geospatial completion | Implemented |
| 0.23.0 | [`V0_23_PLAN.md`](V0_23_PLAN.md) | Stat and geometry polish | Implemented |
| 0.24.0 | [`V0_24_PLAN.md`](V0_24_PLAN.md) | Output backends and interactivity | In progress (backend contract shipped; raster/interaction/preview carried forward) |
| 0.25.0 | [`V0_25_PLAN.md`](V0_25_PLAN.md) | Extensibility and sandboxing | Planned (leapfrogged; pending) |
| 0.26.0 | [`V0_26_PLAN.md`](V0_26_PLAN.md) | Coordinate systems — polar transform | Implemented (`radial_bar` deferred) |
| 0.27.0 | [`V0_27_PLAN.md`](V0_27_PLAN.md) | Embedding and invocation ergonomics | Complete |
| 0.28.0 | [`V0_28_PLAN.md`](V0_28_PLAN.md) | Temporal I/O ergonomics | Complete |
| 0.29.0 | [`V0_29_PLAN.md`](V0_29_PLAN.md) | Render-model completeness and raster output | Implemented |
| 0.30.0 | [`V0_30_PLAN.md`](V0_30_PLAN.md) | Declarative interactivity and live preview | Implemented |
| 0.31.0 | [`V0_31_PLAN.md`](V0_31_PLAN.md) | Language-surface polish (temporal & polar) | Implemented |
| 0.32.0 | [`V0_32_PLAN.md`](V0_32_PLAN.md) | Host-runtime interaction sidecar and React reference | Implemented |
| 0.33.0 | [`V0_33_PLAN.md`](V0_33_PLAN.md) | Cartesian transpose for orientation-locked geoms | Implemented |
| 0.34.0 | [`V0_34_PLAN.md`](V0_34_PLAN.md) | Browser/WASM runtime and live playground | Implemented out of order |
| 0.35.0 | [`V0_35_PLAN.md`](V0_35_PLAN.md) | Internal architecture hardening: stats/parser decomposition, registry generation, determinism harness | Implemented |
| 0.35.5 | [`V0_35_5_PLAN.md`](V0_35_5_PLAN.md) | Browser editor parity for Monaco via shared editor services | Implemented |
| 0.36.0 | [`V0_36_PLAN.md`](V0_36_PLAN.md) | ggplot2 comparability: primitive construction and stroke styling | Implemented |
| 0.37.0 | [`V0_37_PLAN.md`](V0_37_PLAN.md) | ggplot2 comparability: uncertainty construction and exact sugar lowerings | Implemented |
| 0.38.0 | [`V0_38_PLAN.md`](V0_38_PLAN.md) | ggplot2 comparability: z-field statistics | Implemented |
| 0.39.0 | [`V0_39_PLAN.md`](V0_39_PLAN.md) | ggplot2 comparability: model and summary stats | Superseded; scope merged into 0.40.0 |
| 0.39.5 | [`V0_39_5_PLAN.md`](V0_39_5_PLAN.md) | Rust editor-service hover overhaul | Implemented |
| 0.40.0 | [`V0_40_PLAN.md`](V0_40_PLAN.md) | ggplot2 comparability: derived stats plus scale and guide control | Implemented |
| 0.41.0 | [`V0_41_PLAN.md`](V0_41_PLAN.md) | ggplot2 comparability: layout and position control | Implemented |
| 0.42.0 | [`V0_42_PLAN.md`](V0_42_PLAN.md) | ggplot2 comparability: presentation parity and closure | Implemented |
| 0.43.0 | [`V0_43_PLAN.md`](V0_43_PLAN.md) | Big-data readiness and backend-friendly data execution | Implemented |
| 0.44.0 | [`V0_44_PLAN.md`](V0_44_PLAN.md) | Compositional glyph charts and recursive render scenes | Implemented |
| 0.45.0 | [`V0_45_PLAN.md`](V0_45_PLAN.md) | Inset planning/emission separation | Implemented |
| 0.46.0 | [`V0_46_PLAN.md`](V0_46_PLAN.md) | Physical orientation migration and local image marks | Implemented |
| 0.46.1 | [`V0_46_1_PLAN.md`](V0_46_1_PLAN.md) | Table-source spelling consistency | Implemented |
| 0.47.0 | [`V0_47_PLAN.md`](V0_47_PLAN.md) | Explicit derived-table input sources | Implemented |
| 0.48.0 | [`V0_48_PLAN.md`](V0_48_PLAN.md) | Editor hover parity for named tables and constructor-backed sources | Implemented |
| 0.49.0 | [`V0_49_PLAN.md`](V0_49_PLAN.md) | Embedded host parity for interactive SVG output | Implemented |
| 0.50.0 | [`V0_50_PLAN.md`](V0_50_PLAN.md) | README tutorial split and release version alignment | Implemented |
| 0.51.0 | [`V0_51_PLAN.md`](V0_51_PLAN.md) | Caller-input editor diagnostics and planning artifact discipline | Implemented |
| 0.52.0 | [`V0_52_PLAN.md`](V0_52_PLAN.md) | Static browser demo deployment | Implemented |
| 0.53.0 | [`V0_53_PLAN.md`](V0_53_PLAN.md) | Language landing site and demo navigation | Implemented |
| 0.54.0 | [`V0_54_PLAN.md`](V0_54_PLAN.md) | Browser demo editor polish | Implemented |
| 0.55.0 | [`V0_55_PLAN.md`](V0_55_PLAN.md) | Size legend example polish | Implemented |
| 0.56.0 | [`V0_56_PLAN.md`](V0_56_PLAN.md) | Repository CI visibility and test-suite automation | Implemented |
| 0.57.0 | [`V0_57_PLAN.md`](V0_57_PLAN.md) | Multi-page docs site and browser projection ABI fix | Implemented |
| 0.57.5 | [`V0_57_5_PLAN.md`](V0_57_5_PLAN.md) | PDL and Unix-pipe interop for caller-provided data streams | Implemented |
| 0.58.0 | [`V0_58_PLAN.md`](V0_58_PLAN.md) | Two-axis text label decluttering for dense direct annotations | Implemented |
| 0.59.0 | [`V0_59_PLAN.md`](V0_59_PLAN.md) | CI artifacts for distributable editor and browser outputs | Implemented |
| 0.60.0 | [`V0_60_PLAN.md`](V0_60_PLAN.md) | GitHub Release assets for distributable editor and browser outputs | Implemented |
| 0.61.0 | [`V0_61_PLAN.md`](V0_61_PLAN.md) | Story-chart expression: stacked/fill Area, categorical axis order, numeric Text format, and terminal Label geometry | Implemented |
| 0.62.0 | [`V0_62_PLAN.md`](V0_62_PLAN.md) | Sparse stacked/fill Area continuity for story-chart tables | Implemented |
| 0.63.0 | [`V0_63_PLAN.md`](V0_63_PLAN.md) | Shared editor assets and first-party Monaco integration | Implemented |
| 0.64.0 | [`V0_64_PLAN.md`](V0_64_PLAN.md) | Declarative event emitters for host applications | Implemented |
| 0.65.0 | [`V0_65_PLAN.md`](V0_65_PLAN.md) | Explicit categorical position axes for numeric source columns | Implemented |
| 0.66.0 | [`V0_66_PLAN.md`](V0_66_PLAN.md) | Browser runtime invocation-variable parity | Implemented |
| 0.67.0 | [`V0_67_PLAN.md`](V0_67_PLAN.md) | npm-ready browser package build outputs | Implemented |
| 0.68.0 | [`V0_68_PLAN.md`](V0_68_PLAN.md) | Benchmark infrastructure and cross-repo baseline alignment | Implemented |
| 0.68.1 | [`V0_68_1_PLAN.md`](V0_68_1_PLAN.md) | Browser editor package asset contract patch | Implemented |
| 0.68.5 | [`V0_68_5_PLAN.md`](V0_68_5_PLAN.md) | Gradient color literal compatibility for alpha hex, rgb, and rgba | Implemented |
| 0.69.0 | [`V0_69_PLAN.md`](V0_69_PLAN.md) | Arrow-stream and large-data aggregate performance | In progress |
| 0.70.0 | [`V0_70_PLAN.md`](V0_70_PLAN.md) | Demo site and README CLI documentation alignment | Implemented |
| 0.71.0 | [`V0_71_PLAN.md`](V0_71_PLAN.md) | Replace the Inset block with a chart-valued glyph mark | Implemented |
| 0.72.0 | [`V0_72_PLAN.md`](V0_72_PLAN.md) | Glyph-body Scale(size:, …) precedence and size-legend pipeline | Implemented |
| 0.73.0 | [`V0_73_PLAN.md`](V0_73_PLAN.md) | Add another example to the README | Implemented |
| 0.74.0 | [`V0_74_PLAN.md`](V0_74_PLAN.md) | Internal CLI maintenance: split `main.rs` into command modules | Proposed (superseded in sequence by 0.75.0; renumber when it starts) |
| 0.75.0 | [`V0_75_PLAN.md`](V0_75_PLAN.md) | Comprehensive temporal axes: tickInterval, temporal scale type, extended tick ladder, adaptive labels, ingestion hardening | Implemented |
| 0.76.0 | [`V0_76_PLAN.md`](V0_76_PLAN.md) | Agent language-reference templates and safe root instruction-file initialization | Implemented |
| 0.77.0 | [`V0_77_PLAN.md`](V0_77_PLAN.md) | Default stacked legend order follows rendered visual stack order | Implemented |
| 0.78.0 | [`V0_78_PLAN.md`](V0_78_PLAN.md) | Overlaid spaces share the zero-baseline requirement when training position scales | Implemented |
| 0.79.0 | [`V0_79_PLAN.md`](V0_79_PLAN.md) | Measured legend layout reserve | Implemented |
| 0.80.0 | [`V0_80_PLAN.md`](V0_80_PLAN.md) | Default Cartesian data-mark clipping for explicit axis domains and ordinary panels | Implemented |
| 0.81.0 | [`V0_81_PLAN.md`](V0_81_PLAN.md) | LSP document-version test harness stability | Implemented |
| 0.82.0 | [`V0_82_PLAN.md`](V0_82_PLAN.md) | Editorial chart primitives: opposite-side axes, multi-line caption/source, callout badges, per-axis grid, numeric axis format | Implemented |
| 0.82.0 | [`V0_82_PLAN.md`](V0_82_PLAN.md) | Editorial chart design primitives: opposite-side axes, multi-line caption/source blocks, and annotation callout badges | Proposed |

The earliest unreleased plan is the active implementation target; later
unreleased plans are sequencing guidance and may be revised as earlier refactors
land. Released plans are a historical record and their scope is not reopened.

Promoted items MUST be copied into the relevant normative sections of this
specification before or alongside implementation. Deferred optional items remain
non-commitments until explicitly promoted. The standing deferred list is
maintained in [`V0_3_PLAN.md`](V0_3_PLAN.md) and referenced by later plans.

Feature work, maintenance back-ports, and release-scoped fixes MUST have a
current versioned plan artifact. If implementation starts before planning is
written down, the implementation change MUST create or update the appropriate
`docs/V0_<minor>_PLAN.md` artifact in the same change and MUST update this
specification before stopping. New work outside the active plan's declared
purpose/scope, or work that begins after all active plan items are already
`Implemented`, MUST start the next minor plan and immediately align
workspace/spec/package version stamps to that release. Completed release plans
are historical records; new work belongs in a new or currently active plan
rather than reopening old scope.

## 31. Implementation Milestones

### 31.1 Milestone 1: Parser Foundation

Create Rust crate.

Implement lexer.

Implement `rowan` CST.

Implement AST.

Implement recursive descent parser.

Implement Pratt algebra parser.

Implement derive declaration parsing.

Implement parse diagnostics.

Implement AST debug JSON.

Add parser tests.

### 31.2 Milestone 2: CLI Check

Implement `algraf check`.

Implement data source extraction.

Implement CSV schema header loading.

Implement `Chart(data: stdin)` validation.

Implement derived table symbol validation.

Implement semantic analyzer skeleton.

Implement diagnostics output.

### 31.3 Milestone 3: Basic Render

Implement CSV loading.

Implement homegrown columnar dataframe.

Implement linear scale.

Implement temporal scale.

Implement categorical band scale.

Implement layout.

Implement SVG root emission.

Implement `Point`.

Implement `Line`.

Implement `Bar` identity.

Implement primitive `Rect`.

### 31.4 Milestone 4: Algebraic Nesting

Implement nested frame IR.

Implement nested band scale.

Implement dodged bar example.

Add tests comparing nested coordinates.

### 31.5 Milestone 5: Stats

Implement histogram bin stat.

Implement `Derive name = Bin(...)`.

Implement `Histogram` desugaring to `Derive` plus `Rect`.

Implement bar count stat.

Implement smooth linear stat.

Implement boxplot stat.

Add snapshot tests.

### 31.6 Milestone 6: Guides and Themes

Implement x and y axes.

Implement grid lines.

Implement default theme.

Implement categorical fill and stroke scales.

Implement continuous fill and stroke scales.

Implement legends.

Implement chart title/subtitle/caption.

### 31.7 Milestone 7: LSP

Implement `algraf lsp`.

Implement document sync.

Implement parse diagnostics.

Implement schema cache.

Implement completions.

Implement hover.

Implement semantic diagnostics.

### 31.8 Milestone 8: Formatter

Implement pretty printer.

Implement `algraf format`.

Implement LSP formatting.

Add idempotence tests.

### 31.9 Milestone 9: Faceting and Blend

Implement union scale.

Implement ribbon.

Implement faceted space.

Implement facet layout.

Add examples.

## 32. Resolved Design Decisions

The canonical file extension is `.ag`.

Geometry names are PascalCase.

Visual properties use `fill` and `stroke`; there is no `color` aesthetic in Algraf source syntax.

CLI render supports caller-provided data from standard input with `--data -`.

`Chart(data: input)` is the bare sentinel for caller-provided data. `stdin`
remains a compatibility alias.

Quoted column identifiers use backticks.

Guide axis references use bare `x` and `y`.

Source-level theme selection uses `Theme(name: "minimal")`.

Faceting is included in version 0.1.

Stacking uses `Bar(layout: "stack")`.

Dodging is not a bar layout; dodging is expressed with nested algebra.

Primitive statistical graphics use `Derive` declarations plus primitive geometries.

`Histogram` is high-level sugar over `Derive ... = Bin(...)` plus `Rect`.

The parser uses `rowan` from the start.

SVG output uses inline attributes for core visual styling.

Continuous fill and stroke scales are included in version 0.1.

Temporal parsing and temporal position scales are included in version 0.1.

`algraf render` is the authoritative preview/render path.

The LSP may later expose preview rendering through a custom asynchronous request that calls the same internal pipeline.

Version 0.1 uses a homegrown columnar dataframe.

Future optional Polars support must implement the same internal table abstraction.

## 33. Appendix A: Complete EBNF Draft

```ebnf
Program        ::= Trivia* SourceHeader? TopLevelItem (Trivia* TopLevelItem)* Trivia* EOF
TopLevelItem   ::= TableDecl | ChartBlock

ChartBlock     ::= "Chart" ("(" ChartArgs? ")")? "{" ChartBody "}"
ChartArgs      ::= Arg ("," Arg)* ","?
ChartBody      ::= ChartItem*
ChartItem      ::= SpaceBlock
                 | DeriveDecl
                 | TableDecl
                 | GlyphDecl
                 | LetDecl
                 | ScaleDecl
                 | GuideDecl
                 | ThemeDecl
                 | LayoutDecl
                 | ErrorItem

SpaceBlock     ::= "Space" "(" Algebra SpaceArgs? ")" "{" SpaceBody "}"
SpaceArgs      ::= "," Arg ("," Arg)* ","?
SpaceBody      ::= SpaceItem*
SpaceItem      ::= GeometryCall
                 | LetDecl
                 | ScaleDecl
                 | GuideDecl
                 | ThemeDecl
                 | ErrorItem
GlyphDecl      ::= "Glyph" Ident "(" ArgList ")" "{" GlyphBody "}"
GlyphBody      ::= GlyphItem*
GlyphItem      ::= SpaceBlock
                 | LetDecl
                 | ScaleDecl
                 | GuideDecl
                 | ThemeDecl
                 | ErrorItem

DeriveDecl     ::= "Derive" Ident DeriveSource? "=" StatCall
DeriveSource   ::= "from" Ident
StatCall       ::= Ident "(" StatInput? StatArgs? ")"
StatInput      ::= Algebra
StatArgs       ::= "," Arg ("," Arg)* ","?

LetDecl        ::= "let" Ident "=" Value
TableDecl      ::= "Table" Ident "=" SourceExpr

SourceExpr     ::= String
                 | GeoJsonSource
                 | ShapefileSource
                 | ParquetSource
                 | SqliteSource
                 | TopoJsonSource

GeometryCall   ::= Ident "(" ArgList? ")"
ScaleDecl      ::= "Scale" "(" ArgList? ")"
GuideDecl      ::= "Guide" "(" ArgList? ")"
ThemeDecl      ::= "Theme" "(" ArgList? ")"
LayoutDecl     ::= "Layout" "(" ArgList? ")"

ArgList        ::= Arg ("," Arg)* ","?
Arg            ::= Ident ":" Value

Value          ::= Algebra
                 | Literal
                 | StdinSentinel
                 | Array
                 | CallValue
CallValue      ::= Ident "(" ArgList? ")"

Array          ::= "[" ValueList? "]"
ValueList      ::= Value ("," Value)* ","?

Literal        ::= String
                 | Number
                 | Boolean
                 | Null

Algebra        ::= BlendExpr
BlendExpr      ::= CrossExpr ("+" CrossExpr)*
CrossExpr      ::= NestExpr ("*" NestExpr)*
NestExpr       ::= PrimaryExpr ("/" PrimaryExpr)*
PrimaryExpr    ::= QualifiedName
                 | "(" Algebra ")"
                 | ErrorExpr
QualifiedName  ::= Ident
                 | QuotedIdent
                 | Ident "." Ident
                 | Ident "." QuotedIdent
```

`StdinSentinel` is the bare token `input` or `stdin` and is only semantically
valid as `Chart(data: input)` or `Chart(data: stdin)`.

## 34. Appendix B: Rust Type Sketch

```rust
pub type ByteOffset = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub start: ByteOffset,
    pub end: ByteOffset,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub chart: Option<Spanned<ChartBlock>>,
    pub diagnostics: Vec<ParseDiagnostic>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ChartBlock {
    pub args: Vec<Spanned<Argument>>,
    pub body: Vec<Spanned<ChartItem>>,
}

#[derive(Debug, Clone)]
pub enum ChartItem {
    Space(Spanned<SpaceBlock>),
    Derive(Spanned<DeriveDecl>),
    Glyph(Spanned<GlyphDecl>),
    Scale(Spanned<Call>),
    Guide(Spanned<Call>),
    Theme(Spanned<Call>),
    Layout(Spanned<Call>),
    Error(ErrorNode),
}

#[derive(Debug, Clone)]
pub struct SpaceBlock {
    pub frame: Spanned<AlgebraExpr>,
    pub args: Vec<Spanned<Argument>>,
    pub body: Vec<Spanned<SpaceItem>>,
}

#[derive(Debug, Clone)]
pub struct DeriveDecl {
    pub name: Spanned<String>,
    pub stat: Spanned<StatCall>,
}

#[derive(Debug, Clone)]
pub struct StatCall {
    pub name: Spanned<String>,
    pub input: Option<Spanned<AlgebraExpr>>,
    pub args: Vec<Spanned<Argument>>,
}

#[derive(Debug, Clone)]
pub enum SpaceItem {
    Geometry(Spanned<GeometryCall>),
    Scale(Spanned<Call>),
    Guide(Spanned<Call>),
    Theme(Spanned<Call>),
    Error(ErrorNode),
}

#[derive(Debug, Clone)]
pub struct GlyphDecl {
    pub name: Spanned<String>,
    pub args: Vec<Spanned<Argument>>,
    pub body: Vec<Spanned<GlyphItem>>,
}

#[derive(Debug, Clone)]
pub enum GlyphItem {
    Space(Spanned<SpaceBlock>),
    Let(Spanned<LetDecl>),
    Scale(Spanned<Call>),
    Guide(Spanned<Call>),
    Theme(Spanned<Call>),
    Error(ErrorNode),
}

#[derive(Debug, Clone)]
pub struct GeometryCall {
    pub name: Spanned<String>,
    pub args: Vec<Spanned<Argument>>,
}

#[derive(Debug, Clone)]
pub struct Call {
    pub name: Spanned<String>,
    pub args: Vec<Spanned<Argument>>,
}

#[derive(Debug, Clone)]
pub struct Argument {
    pub key: Spanned<String>,
    pub value: Spanned<ValueExpr>,
}

#[derive(Debug, Clone)]
pub enum ValueExpr {
    Algebra(Spanned<AlgebraExpr>),
    Literal(Literal),
    Stdin,
    Array(Vec<Spanned<ValueExpr>>),
    Error(ErrorNode),
}

#[derive(Debug, Clone)]
pub enum AlgebraExpr {
    Identifier(Identifier),
    Binary {
        op: AlgebraOp,
        left: Box<Spanned<AlgebraExpr>>,
        right: Box<Spanned<AlgebraExpr>>,
    },
    Paren(Box<Spanned<AlgebraExpr>>),
    Error(ErrorNode),
}

#[derive(Debug, Clone)]
pub struct Identifier {
    pub name: String,
    pub quoted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlgebraOp {
    Cross,
    Nest,
    Blend,
}

#[derive(Debug, Clone)]
pub enum Literal {
    String(String),
    Number(NumberLiteral),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone)]
pub enum NumberLiteral {
    Integer(i64),
    Float(f64),
}

#[derive(Debug, Clone)]
pub struct ErrorNode {
    pub message: String,
    pub expected: Vec<String>,
    pub found: Option<String>,
}
```

## 35. Appendix C: SVG Example

Input:

```ag
Chart(data: "points.csv", width: 400, height: 300) {
    Space(x * y) {
        Point(fill: group, size: 4)
    }
}
```

Output shape:

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="400" height="300" viewBox="0 0 400 300" role="img">
  <defs>
    <clipPath id="algraf-clip-0">
      <rect x="60" y="40" width="310" height="210" />
    </clipPath>
  </defs>
  <rect class="algraf-background" x="0" y="0" width="400" height="300" fill="#ffffff" />
  <g class="algraf-plot" clip-path="url(#algraf-clip-0)">
    <g class="algraf-layer algraf-geom-point">
      <circle cx="..." cy="..." r="4" fill="#4E79A7" />
    </g>
  </g>
  <g class="algraf-axis algraf-axis-x"></g>
  <g class="algraf-axis algraf-axis-y"></g>
  <g class="algraf-legend algraf-legend-fill"></g>
</svg>
```

## 36. Appendix D: Detailed Parser Pseudocode

```rust
impl Parser {
    pub fn parse_program(&mut self) -> Program {
        let start = self.current_span_start();
        let chart = self.parse_chart_block();
        self.consume_trailing_tokens();
        let end = self.previous_span_end();
        let diagnostics = std::mem::take(&mut self.diagnostics);

        Program {
            chart,
            diagnostics,
            span: Span { start, end },
        }
    }

    fn parse_chart_block(&mut self) -> Option<Spanned<ChartBlock>> {
        let Some(start) = self.expect_ident_named("Chart") else {
            self.error_current("expected Chart block");
            return None;
        };
        self.expect_recover(TokenKind::LParen, "expected '(' after Chart");
        let args = self.parse_arg_list_until(TokenKind::RParen);
        self.expect_recover(TokenKind::RParen, "expected ')' after Chart arguments");
        self.expect_recover(TokenKind::LBrace, "expected '{' to open Chart block");

        let mut body = Vec::new();

        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            if self.at_ident_named("Space") {
                if let Some(space) = self.parse_space_block() {
                    body.push(spanned_chart_item(ChartItem::Space(space)));
                }
            } else if self.at_ident_named("Derive") {
                if let Some(derive) = self.parse_derive_decl() {
                    body.push(spanned_chart_item(ChartItem::Derive(derive)));
                }
            } else if self.at_ident_named("Scale") {
                body.push(self.parse_chart_call_item("Scale"));
            } else if self.at_ident_named("Guide") {
                body.push(self.parse_chart_call_item("Guide"));
            } else if self.at_ident_named("Theme") {
                body.push(self.parse_chart_call_item("Theme"));
            } else if self.at_ident_named("Layout") {
                body.push(self.parse_chart_call_item("Layout"));
            } else {
                self.error_current("unexpected item in Chart block");
                self.advance();
            }
        }

        let end = self.expect_recover(TokenKind::RBrace, "expected '}' to close Chart block");

        Some(Spanned {
            node: ChartBlock { args, body },
            span: span_from(start, end),
        })
    }

    fn parse_space_block(&mut self) -> Option<Spanned<SpaceBlock>> {
        let start = self.expect_ident_named("Space")?;
        self.expect_recover(TokenKind::LParen, "expected '(' after Space");
        let frame = self.parse_algebra(0);
        let args = if self.at(TokenKind::Comma) {
            self.advance();
            self.parse_arg_list_until(TokenKind::RParen)
        } else {
            Vec::new()
        };
        self.expect_recover(TokenKind::RParen, "expected ')' after Space arguments");
        self.expect_recover(TokenKind::LBrace, "expected '{' to open Space block");

        let mut body = Vec::new();

        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            if self.current_token_can_start_geometry() {
                if let Some(geometry) = self.parse_geometry_call() {
                    body.push(spanned_space_item(SpaceItem::Geometry(geometry)));
                }
            } else if self.at_ident_named("Scale") {
                body.push(self.parse_space_call_item("Scale"));
            } else if self.at_ident_named("Guide") {
                body.push(self.parse_space_call_item("Guide"));
            } else if self.at_ident_named("Theme") {
                body.push(self.parse_space_call_item("Theme"));
            } else {
                self.error_current("unexpected item in Space block");
                self.advance();
            }
        }

        let end = self.expect_recover(TokenKind::RBrace, "expected '}' to close Space block");

        Some(Spanned {
            node: SpaceBlock { frame, args, body },
            span: span_from(start, end),
        })
    }

    fn parse_derive_decl(&mut self) -> Option<Spanned<DeriveDecl>> {
        let start = self.expect_ident_named("Derive")?;
        let name = self.expect_identifier("expected derived table name");
        let source = if self.eat_ident_named("from") {
            Some(self.expect_identifier("expected input table name after `from`"))
        } else {
            None
        };
        self.expect_recover(TokenKind::Equal, "expected '=' after derived table name");
        let stat = self.parse_stat_call();
        let end = stat.span.clone();

        Some(Spanned {
            node: DeriveDecl { name, source, stat },
            span: span_from(start, end),
        })
    }

    fn parse_algebra(&mut self, min_bp: u8) -> Spanned<AlgebraExpr> {
        let mut lhs = self.parse_primary();

        loop {
            let Some((op, left_bp, right_bp)) = self.peek_algebra_op() else {
                break;
            };

            if left_bp < min_bp {
                break;
            }

            self.advance();
            let rhs = self.parse_algebra(right_bp);
            let span = lhs.span.start..rhs.span.end;

            lhs = Spanned {
                node: AlgebraExpr::Binary {
                    op,
                    left: Box::new(lhs),
                    right: Box::new(rhs),
                },
                span,
            };
        }

        lhs
    }
}
```

`consume_trailing_tokens()` means consume unexpected tokens after the root chart and emit diagnostics until EOF.

## 37. Appendix E: Detailed LSP Completion Matrix

At empty file:

suggest `Chart`.

Inside `Chart(`:

suggest `data`, `width`, `height`, `title`, `subtitle`, `caption`.

Inside `Chart(data: )`:

suggest string path snippets and bare `stdin`.

Inside chart body:

suggest `Derive`, `Space`, `Scale`, `Guide`, `Theme`, `Layout`.

After `Derive `:

suggest a derived table name snippet.

After `Derive name from `:

suggest chart-scoped named and derived table names.

After `Derive name = `:

suggest stat calls such as `Bin(value, bins: 25)`.

Inside `Space(`:

suggest column names from the active table and `(`.

Column completions whose names are not plain identifiers MUST insert backtick-quoted identifiers.

Inside `Space(..., data: )`:

suggest chart-scoped derived table names.

After identifier in algebra:

suggest operators `*` and `/`.

Suggest `+` only when the cursor is inside an explicit parenthesized blend-capable expression.

After `*`:

suggest column names and `(`.

After `/`:

suggest categorical column names first.

After `+`:

suggest compatible column names.

Inside space body:

suggest geometry names.

Inside `Point(`:

suggest `fill`, `stroke`, `alpha`, `size`, `shape`.

Inside `Line(`:

suggest `stroke`, `strokeWidth`, `alpha`, `group`, `sort`.

Inside `Bar(`:

suggest `fill`, `alpha`, `layout`, `baseline`, `width`, `stat`.

Inside `Rect(`:

suggest `xmin`, `xmax`, `ymin`, `ymax`, `fill`, `stroke`, `alpha`, `strokeWidth`.

Inside `Histogram(`:

suggest `bins`, `binWidth`, `fill`, `alpha`.

After `fill:`:

suggest column names and color string snippets.

After `layout:` in `Bar`:

suggest `"identity"`, `"stack"`, `"fill"`.

After `method:` in `Smooth`:

suggest `"lm"`, `"loess"`.

## 38. Appendix F: Implementation Checklist

Parser checklist:

- [ ] Token spans.
- [ ] UTF-8 source handling.
- [ ] Line comments.
- [ ] String escapes.
- [ ] Number literals.
- [ ] Chart block.
- [ ] Derive declarations.
- [ ] Space block.
- [ ] Space data arguments.
- [ ] Space-local Theme declarations.
- [ ] Geometry calls.
- [ ] Argument lists.
- [ ] Array values.
- [ ] Pratt algebra.
- [ ] Recovery nodes.
- [ ] Parse diagnostics.
- [ ] AST JSON output.

Semantic checklist:

- [ ] Data argument extraction.
- [ ] Path resolution.
- [ ] CSV schema loading.
- [ ] Provisional LSP type inference policy.
- [ ] Symbol table.
- [ ] Column resolution.
- [ ] Derived table resolution.
- [ ] Derived table schema inference.
- [ ] Geometry registry.
- [ ] Property registry.
- [ ] Type checking.
- [ ] Late invalid value warning policy.
- [ ] Temporal binning diagnostics.
- [ ] High-cardinality temporal nesting warning.
- [ ] Algebra validation.
- [ ] Parenthesized blend validation.
- [ ] 3D Cartesian facet quick-fix diagnostics.
- [ ] Histogram desugaring.
- [ ] IR output.

Runtime checklist:

- [ ] CSV full load.
- [ ] Dataframe.
- [ ] Derived table execution.
- [ ] Type inference.
- [ ] Continuous scale.
- [ ] Band scale.
- [ ] Nested band scale.
- [ ] Fill and stroke scales.
- [ ] Temporal scale.
- [ ] Layout.
- [ ] SVG escape.
- [ ] SVG root.
- [ ] Axes.
- [ ] Legends.
- [ ] Point.
- [ ] Line.
- [ ] Bar.
- [ ] Rect.
- [ ] Histogram.

LSP checklist:

- [ ] `algraf lsp`.
- [ ] Initialize.
- [ ] Full document sync.
- [ ] Parse diagnostics.
- [ ] Schema cache.
- [ ] Semantic diagnostics.
- [ ] Completion.
- [ ] Hover.
- [ ] Document symbols.
- [ ] Formatting.

CLI checklist:

- [ ] `render`.
- [ ] `check`.
- [ ] `format`.
- [ ] `schema`.
- [ ] `ast`.
- [ ] `ir`.
- [ ] JSON diagnostics.
- [ ] Nonzero exit codes.
