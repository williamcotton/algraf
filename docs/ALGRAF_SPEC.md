# Algraf Detailed Specification

Status: Draft 0.2.0
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

This working copy is the active Draft 0.2.0 specification.

The v0.2.0 release plan and optional-item audit live in [`V0_2_PLAN.md`](V0_2_PLAN.md).

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

Algraf MAY support interactive output in later versions.

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

Algraf does not initially support runtime interactivity.

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

Nested spaces are reserved for later versions.

The first implementation SHOULD reject nested `Space` blocks with a diagnostic.

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
3. Language sentinel where the property explicitly accepts sentinels, such as `Chart(data: stdin)`.
4. Symbol reference where the property explicitly accepts chart symbols, such as `Space(..., data: bins)`.
5. Diagnostic if unresolved.

User-facing enum-valued options MUST use string literals in version 0.1.

Examples include `Bar(layout: "stack")`, `Smooth(method: "lm")`, and `Theme(name: "minimal")`.

Bare identifiers MUST NOT be accepted as enum values for ordinary properties in version 0.1.

If a user writes `Bar(layout: stack)`, the analyzer MUST produce a diagnostic suggesting `Bar(layout: "stack")`.

The bare `x` and `y` values in guide declarations are language selectors, not general enum values.

The bare `stdin` value in `Chart(data: stdin)` is a language sentinel, not a general enum value.

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

### 6.5 Keywords

The following identifiers are reserved in version 0.1:

`Chart`

`Derive`

`Space`

`Scale`

`Guide`

`Theme`

`Layout`

`true`

`false`

`null`

`stdin` is a contextual keyword only in `Chart(data: stdin)`.

Outside `Chart(data: stdin)`, `stdin` is an ordinary plain identifier.

`Derive stdin = Bin(value, bins: 25)` is syntactically valid, though style guides SHOULD discourage it because it is visually confusing.

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

Unicode escapes MAY be added later.

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
Program        ::= Trivia* ChartBlock Trivia* EOF
```

`Trivia` means whitespace and comments retained by the lexer/CST layer.

Trivia is not represented as typed AST children.

A source file MUST contain exactly one chart block in version 0.1.

If extra top-level tokens appear after `ChartBlock`, the parser MUST emit diagnostics and recover.

### 7.2 Chart Block

```ebnf
ChartBlock     ::= "Chart" "(" ChartArgs? ")" BlockStart ChartBody BlockEnd
ChartArgs      ::= Arg ("," Arg)* ","?
ChartBody      ::= ChartItem*
ChartItem      ::= SpaceBlock
                 | DeriveDecl
                 | ScaleDecl
                 | GuideDecl
                 | ThemeDecl
                 | LayoutDecl
                 | ErrorItem
```

`Chart` MUST include a `data` argument in version 0.1.

`Chart` MAY include `width` and `height` arguments.

`Chart` MAY include `title`, `subtitle`, and `caption` arguments.

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

### 7.3 Space Block

```ebnf
SpaceBlock     ::= "Space" "(" Algebra SpaceArgs? ")" BlockStart SpaceBody BlockEnd
SpaceArgs      ::= "," Arg ("," Arg)* ","?
SpaceBody      ::= SpaceItem*
SpaceItem      ::= GeometryCall
                 | ScaleDecl
                 | GuideDecl
                 | ThemeDecl
                 | ErrorItem
```

`Space` MUST include exactly one algebra expression.

`Space` with an empty expression MUST produce a diagnostic and an error expression node.

`Space` body MAY be empty during editing.

An empty `Space` body SHOULD produce a warning in CLI render mode.

`Space` MAY include a `data` argument.

`Space(..., data: name)` binds that space to a chart-scoped derived table.

The `data` argument MUST be a bare identifier that resolves to a derived table.

Example:

```ag
Space(bin_start * count, data: bins) {
    Rect(xmin: bin_start, xmax: bin_end, ymin: 0, ymax: count)
}
```

`Space` MAY include `Theme` declarations in its body.

Space-local themes override chart-level theme values for that space only.

Space-local themes MUST NOT mutate chart-level theme state.

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
DeriveDecl     ::= "Derive" Ident "=" StatCall
StatCall       ::= Ident "(" StatInput? StatArgs? ")"
StatInput      ::= Algebra
StatArgs       ::= "," Arg ("," Arg)* ","?
```

`Derive` creates a named derived table in chart scope.

The derived table name MUST be unique among derived tables.

The derived table name MUST NOT conflict with reserved keywords.

The stat call name is PascalCase.

Version 0.1 MUST support `Bin`.

Example:

```ag
Derive bins = Bin(value, bins: 25)
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
```

This grammar admits algebra expressions as property values.

`StdinSentinel` is the bare token `stdin`.

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
PrimaryExpr    ::= Ident
                 | QuotedIdent
                 | "(" Algebra ")"
                 | ErrorExpr
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

### 7.8 Literals

```ebnf
Literal        ::= String
                 | Number
                 | Boolean
                 | Null
Array          ::= "[" ValueList? "]"
ValueList      ::= Value ("," Value)* ","?
```

Arrays MAY be nested.

Array element types SHOULD be homogeneous where the receiving property requires homogeneity.

Heterogeneous arrays MAY produce semantic diagnostics.

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

Left operand conventionally maps to horizontal position.

Right operand conventionally maps to vertical position.

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

Facet columns MUST be chosen automatically when not specified.

Facet column count MAY be overridden with `Layout(facetColumns: n)`.

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

In `Space(x * y)`, `x` and `y` are column references.

In `Derive bins = Bin(value, bins: 25)`, `bins` is a derived table name and `value` is a column reference in the active input table.

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

Version 0.1 does not have user variables.

There is no user-defined shadowing in version 0.1.

Future versions with variables MUST define shadowing explicitly.

Column names SHOULD NOT shadow keywords inside grammar positions.

Quoted identifiers MUST be used to reference keyword-like column names in version 0.1.

## 10. Data Sources

### 10.1 Initial Data Source Model

Version 0.1 supports CSV files.

`Chart(data: "path.csv")` resolves `path.csv` relative to the source file directory by default.

`Chart(data: stdin)` reads CSV data from standard input.

`stdin` is a bare sentinel, not a string path.

`Chart(data: "stdin")` refers to a file literally named `stdin`.

If source is read from stdin, relative paths resolve against the current working directory.

The canonical command for CSV data from stdin is:

```bash
cat data.csv | algraf render chart.ag --data -
```

When `--data -` is used, it overrides the chart's `data` argument for the render command and supplies CSV rows from standard input.

The recommended source pattern for piped CSV data is `Chart(data: stdin)`.

When `Chart(data: stdin)` is used, the render command MUST read CSV rows from standard input without requiring `--data -`.

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
Chart(data: stdin) {
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

### 10.3 Schema Inference

The schema resolver reads enough data to infer column names and basic types.

For LSP completion, reading only headers is sufficient.

For LSP hover, inferred types are useful but optional.

LSP type inference from sampled rows is provisional.

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

Values outside these formats MUST remain strings unless an explicit temporal parsing declaration is added in a later version.

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

Suggested trait boundary:

```rust
pub trait Table {
    fn schema(&self) -> &[ColumnDef];
    fn row_count(&self) -> usize;
    fn value(&self, column: &str, row: usize) -> Option<DataValueRef<'_>>;
}
```

### 10.6 Derived Tables

Derived tables are produced by `Derive` declarations.

Derived tables live in memory during one render or LSP analysis session.

Derived tables MUST use the same dataframe abstraction as primary CSV data.

Derived tables MUST have schemas.

Derived table schemas MUST be available to semantic analysis after the stat declaration is validated.

Derived table schemas MUST be available to LSP completions inside `Space(..., data: derived_name)` blocks.

Derived tables MAY be lazily computed by the renderer.

Derived table schemas SHOULD be computed without running expensive full-data transforms where possible.

Derived table names are chart-scoped.

Derived table names MUST be unique within a chart.

Derived table names MUST NOT shadow the primary data source.

Derived table columns are referenced like ordinary columns inside spaces bound to that derived table.

Example:

```ag
Chart(data: "distribution.csv") {
    Derive bins = Bin(value, bins: 25)

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
}
```

`DataValue` MUST support deterministic ordering for categorical domains.

Continuous comparisons MUST handle NaN carefully.

NaN SHOULD be treated as missing.

### 10.8 Source Security

The renderer MUST NOT fetch network resources by default.

The LSP MUST NOT fetch network resources by default.

The CLI MUST restrict data reads to explicit paths.

The CLI SHOULD provide an option to allow network sources if implemented later.

The LSP SHOULD avoid reading very large files on the hot path.

The LSP SHOULD cap schema preview read size.

### 10.9 Schema Cache

The LSP maintains schema cache by data source path.

Cache key SHOULD include:

absolute path

last modified timestamp

file size

optional content hash

The cache MUST invalidate when the source file changes.

The cache SHOULD degrade gracefully when file watching is unavailable.

The cache MUST distinguish parse errors from missing files.

Completion requests SHOULD return cached schemas if available.

Completion requests SHOULD NOT block for full data loading.

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
    Scale(Spanned<Call>),
    Guide(Spanned<Call>),
    Theme(Spanned<Call>),
    Layout(Spanned<Call>),
    Error(ErrorNode),
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

### 11.7 Derive Node

```rust
pub struct DeriveDecl {
    pub name: Spanned<String>,
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

The parser SHOULD parse bare `stdin` as `ValueExpr::Stdin` only in value positions.

The analyzer interprets them by property context.

The analyzer MUST accept `ValueExpr::Stdin` only for `Chart(data: stdin)`.

Using `stdin` as a geometry property value MUST produce a semantic diagnostic unless a future property explicitly allows it.

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

stdin sentinel

array

algebra expression

Value parser SHOULD prefer literal parsing when current token is literal.

Value parser SHOULD parse bare `stdin` as the stdin sentinel in value positions.

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
    pub geometries: Vec<GeometryIr>,
    pub span: Span,
}
```

```rust
pub enum SpaceDataRef {
    Primary,
    Derived(String),
}
```

### 13.4 Derived Table IR

```rust
pub struct DeriveIr {
    pub name: String,
    pub stat: StatCallIr,
    pub output_schema: Vec<ColumnDef>,
    pub span: Span,
}
```

```rust
pub struct StatCallIr {
    pub kind: StatKind,
    pub input: FrameIr,
    pub settings: Vec<StatSetting>,
    pub span: Span,
}
```

```rust
pub enum StatKind {
    Bin,
    Count,
    Smooth,
    Boxplot,
}
```

Version 0.1 MUST support `StatKind::Bin` for explicit `Derive` declarations.

Other stat kinds MAY be exposed through high-level geometries before they are exposed through `Derive`.

Derived table IR MUST be ordered by source order.

Derived table IR MUST be validated before spaces that reference derived data are validated.

Derived table names MUST be available to later `Derive` declarations and `Space` blocks.

Forward references to derived tables MAY be rejected in version 0.1.

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

`Rect.xmin`, `Rect.xmax`, `Rect.ymin`, and `Rect.ymax` accept column mappings or numeric/temporal literals.

`Smooth.method` accepts string literal `"lm"` or `"loess"`.

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
5. Build initial symbol table.
6. Resolve and validate `Derive` declarations.
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

Geometry-specific properties are defined below.

### 14.2 Point

Syntax:

```ag
Point(fill: species, alpha: 0.7, size: 3)
```

Supported spaces:

2D Cartesian

nested 2D Cartesian where x or y has nested bands

faceted Cartesian

Required inherited frame:

x coordinate

y coordinate

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

Point rendering emits SVG `circle`, `path`, or `use` elements.

Point MUST skip rows with missing x or y.

Point SHOULD skip rows with non-finite x or y after scale mapping.

### 14.3 Line

Syntax:

```ag
Line(stroke: series, strokeWidth: 2)
```

Supported spaces:

2D Cartesian

faceted Cartesian

Required inherited frame:

x coordinate

y coordinate

Optional mappings:

stroke

alpha

group

Optional settings:

stroke color

strokeWidth number

alpha number

curve string option

Default grouping:

group aesthetic if present

stroke mapping if present

fill mapping if present

otherwise all rows one group

Line MUST sort rows by x within each group unless `sort: false`.

Line MUST skip missing coordinates.

Line SHOULD break paths on missing coordinates rather than connecting across gaps.

### 14.4 Step

Syntax:

```ag
Step(direction: "mid", strokeWidth: 2)
```

Supported spaces:

2D Cartesian

Properties:

`direction`: `hv`, `vh`, or `mid`

Step uses line grouping behavior.

Step renders SVG paths with orthogonal segments.

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

continuous x with explicit binning if stat count implemented

Required inherited frame:

x coordinate

y coordinate unless `stat: "count"`

Optional mappings:

fill

alpha

group

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

`fill` stacks bars and normalizes height to 100 percent.

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

Bar MUST skip rows with missing x.

Bar MUST skip rows with missing y unless `stat: "count"`.

Bar MUST treat negative values according to stack rules.

Positive and negative stacks SHOULD be separated around baseline.

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

fill

alpha

Histogram computes counts by bin.

Histogram produces an internal Cartesian frame of bin position by count.

Histogram SHOULD expose computed y label as `count`.

Histogram SHOULD support grouping by fill mapping in later versions.

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

The generated y-axis label defaults to `count`.

The generated `count` column is a synthetic stat output column.

The implementation MAY keep `Histogram` as a direct IR node internally, but its visual output MUST match the derived-table plus `Rect` model.

### 14.8 Frequency Polygon

Syntax:

```ag
FreqPoly(bins: 25, stroke: "steelblue")
```

Supported spaces:

1D continuous vector

Frequency polygon shares binning with histogram.

It renders bin centers connected by lines.

### 14.9 Density

> Promoted from a v0.1 `MAY` to a v0.2.0 requirement; see `docs/V0_2_PLAN.md`.

Syntax:

```ag
Density(fill: "#4c78a8", alpha: 0.4)
Density(bandwidth: 0.5, n: 256)
```

Supported spaces:

1D continuous (numeric) vector

Density computes a kernel density estimate of the input column and renders it
as a filled area from the curve down to a zero baseline.

Version 0.2.0 MUST advertise `Density` in the registry and implement the KDE
described in §15.11.

`Density` accepts `bandwidth` (positive number) and `n` (grid points, at least
2) settings, plus the `fill`, `stroke`, `strokeWidth`, and `alpha` visual
settings.

A `Density` over a non-numeric column MUST emit `E1404`; over a non-vector
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

`loess` MAY be later

Default method:

`lm`

Smooth computes predicted values.

Smooth renders a line.

Smooth grouping SHOULD follow stroke, fill, or group mappings.

Smooth MUST report diagnostic if x or y is non-continuous for `lm`.

### 14.11 Boxplot

Syntax:

```ag
Boxplot(fill: gender)
```

Supported spaces:

categorical x by continuous y

nested categorical x by continuous y

Boxplot computes:

minimum whisker

first quartile

median

third quartile

maximum whisker

outliers MAY be rendered by default.

Properties:

fill

stroke

alpha

width

outliers

Boxplot MUST group by x coordinate and nested coordinate.

### 14.12 Violin

Syntax:

```ag
Violin(fill: gender, quantiles: [0.25, 0.5, 0.75])
```

Supported spaces:

categorical x by continuous y

Violin computes density per group.

Version 0.1 MAY defer Violin.

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
```

Supported spaces:

2D Cartesian

Area fills between y and baseline.

Properties:

baseline

fill

alpha

group

Area MUST sort by x within group.

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

Text SHOULD support alignment properties.

### 14.17 HLine

Syntax:

```ag
HLine(y: 12, stroke: "red", label: "Target")
```

Supported spaces:

2D Cartesian

HLine uses y scale to map literal y.

It spans the plot x range.

### 14.18 VLine

Syntax:

```ag
VLine(x: 3, stroke: "gray40", label: "Marker")
```

Supported spaces:

2D Cartesian

VLine uses x scale to map literal x.

It spans the plot y range.

### 14.19 Segment

Syntax:

```ag
Segment(x: 160, y: 55, xend: 185, yend: 85)
```

Supported spaces:

2D Cartesian

Segment maps literal endpoints through scales.

Segment MAY support column mappings later.

### 14.20 Rug

Syntax:

```ag
Rug(sides: "bl", alpha: 0.55)
```

Supported spaces:

1D vector

2D Cartesian

Rug renders tick marks along axis edges.

### 14.21 Geometry Extensibility

The registry MUST be data-driven enough that LSP docs and completions can use the same metadata as semantic analysis.

Future plugin geometry support MUST be carefully sandboxed.

Version 0.1 SHOULD keep built-in geometries compiled into the binary.

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
```

The left-hand identifier is the derived table name.

The right-hand side is a stat call.

The first positional expression is the stat input.

Named arguments configure the stat.

Derived stat declarations MUST be pure.

Derived stat declarations MUST NOT render marks.

Derived stat declarations MUST produce an output schema.

The output schema MUST be available before spaces using `data: bins` are analyzed.

Derived stat declarations MAY depend on previously declared derived tables in later versions.

Version 0.1 SHOULD require derived stats to read from the primary data table.

### 15.4 Identity Stat

Identity stat passes input data through unchanged.

Point, Line, Rect, Area, Ribbon, Tile, Text, HLine, VLine, and Segment usually use identity stat.

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

This produces a derived frame `gender * count`.

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

`bin_start`, `bin_end`, and `bin_center` have the same domain type as the input column.

For numeric inputs, bin boundary columns are numeric.

For temporal inputs, bin boundary columns are temporal if temporal binning is implemented.

Version 0.1 MUST support numeric binning.

Version 0.2.0 MUST support temporal binning for `Bin` and `Histogram`.

For temporal inputs, `bins`, `boundary`, and `closed` use the same interval assignment semantics as numeric binning over UTC-equivalent microsecond instants.

Calendar-aware interval syntax such as `interval: "month"` is not required in version 0.2.0.

`Histogram` over a temporal vector MUST trigger the same diagnostic when temporal binning is unavailable.

Nesting a high-cardinality temporal vector directly with `/` SHOULD produce a warning when it would create one panel or band per timestamp.

The warning SHOULD suggest deriving or precomputing a coarser period column such as day, week, month, or year.

### 15.7 Smooth Stat

Smooth stat computes fitted y values.

Method `lm` fits linear regression.

Method `loess` may be implemented later.

Output columns:

x

y

group

se MAY be later.

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

log10 SHOULD be later

sqrt SHOULD be later

reverse SHOULD be later

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

numeric range default `[2, 8]`.

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

### 16.11 Scale Declarations

Syntax example:

```ag
Scale(axis: x, type: "log10")
Scale(axis: x, domain: [0, 100])
Scale(axis: y, reverse: true)
Scale(fill: species, palette: "accent")
```

Version 0.2.0 MUST implement source-level `Scale` declarations.

`Scale` declarations MAY appear at chart scope or space scope.

Space-local scale declarations override chart-level declarations for the same target.

Axis scale targets use `Scale(axis: x, ...)` or `Scale(axis: y, ...)`; axis selectors MUST be bare `x` and `y`, not string literals.

Version 0.2.0 MUST support continuous position scale types `"linear"` and `"log10"`.

Version 0.2.0 MUST support numeric position domains with `domain: [min, max]`.

Version 0.2.0 MUST support `reverse: true` for position axes.

Version 0.2.0 MUST support categorical `fill` and `stroke` palette selection with `palette: "default"` and `palette: "accent"`.

Invalid scale/domain combinations MUST emit targeted diagnostics.

Version 0.2.0 MUST support a `label` argument on `fill`/`stroke` scales that
overrides the column-derived legend title (see §16.13).

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

If legend present on right, right margin increases.

If title present, top margin increases.

If x tick labels rotated, bottom margin increases.

Dynamic text measurement is hard in pure SVG.

Version 0.1 MAY approximate text dimensions.

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

### 17.6 Layer Order

Rendering order follows source order.

Earlier geometries render below later geometries.

Earlier spaces render below later spaces.

Guides render above or outside plot depending on guide type.

Background renders first.

Grid renders before data marks.

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

SVG SHOULD include title and description when chart title/subtitle exist.

Example:

```svg
<title>How old are astronauts?</title>
<desc>Histogram of astronaut age at selection and mission.</desc>
```

The renderer SHOULD preserve meaningful text labels.

Purely decorative groups MAY use `aria-hidden="true"`.

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

## 19. Guides

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

### 19.3 Y Axis

Y axis usually appears at left.

Continuous y axis uses nice ticks.

Categorical y axis uses category labels.

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

### 19.5 Legend Generation

Legends are generated for mapped aesthetics by default.

No legend is generated for literal settings.

Fill mapping creates fill legend.

Stroke mapping creates stroke legend.

Size mapping creates size legend.

Shape mapping creates shape legend.

Alpha mapping creates alpha legend.

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
equal and in the same order.

A merged legend MUST render each swatch with the fill color as the swatch face
and the stroke color as the swatch outline.

Aesthetics mapped to different columns MUST keep separate legends.

Continuous (gradient) legends are not merged in version 0.2.0.

## 20. Theme

### 20.1 Theme Object

Recommended theme structure:

```rust
pub struct Theme {
    pub font_family: String,
    pub font_size: f64,
    pub background: Color,
    pub plot_background: Color,
    pub axis_color: Color,
    pub grid_major_color: Color,
    pub grid_minor_color: Option<Color>,
    pub text_color: Color,
    pub title_size: f64,
    pub subtitle_size: f64,
    pub caption_size: f64,
    pub point_size: f64,
    pub line_width: f64,
}
```

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

### 20.6 Theme Syntax

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

### 20.7 Custom Theme

Custom theme syntax is deferred.

Future syntax MAY be:

```ag
Theme(
    name: "minimal",
    axisText: Text(size: 12, fill: "#333333"),
    gridMajor: Line(stroke: "#dddddd", strokeWidth: 1)
)
```

This is not version 0.1.

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

The LSP MAY provide inline previews later through a custom request, but it MUST call the same internal render pipeline as `algraf render`.

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
    schema_cache: DashMap<DataSourceKey, SchemaState>,
}
```

Document state:

```rust
pub struct DocumentState {
    pub text: String,
    pub version: i32,
    pub parse: Option<ParseState>,
    pub analysis: Option<AnalysisState>,
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

start schema resolution

analyze when schema available

publish diagnostics

On `didChange`:

update text

parse

debounce schema resolution if data source changed

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

If schema unavailable, completion SHOULD return syntax keywords and optionally a loading message.

### 21.7 Hover

Hover contexts:

operator hover explains algebra operator

column hover shows type and source

geometry hover shows geometry docs

property hover shows property docs

string-valued option hover shows option docs

Hover over `/`:

Explains nest operator.

Hover over `*`:

Explains cross operator.

Hover over `+`:

Explains blend operator.

### 21.8 Go To Definition

Version 0.1 MAY not support go to definition.

Column identifiers could go to CSV header if editor can open CSV and position is known.

This is optional.

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

### 21.13 Cancellation and Shutdown

The LSP MUST honor client cancellation for long-running custom requests.

Preview rendering MUST be cancellable through the LSP request cancellation mechanism.

Implementation SHOULD associate each long-running preview task with a cancellation token.

When a newer preview request supersedes an older preview request for the same document, the older task SHOULD be cancelled.

Cancelled preview tasks MUST NOT publish stale preview output.

The LSP MUST handle `shutdown` by stopping new work and allowing in-flight lightweight requests to finish promptly.

The LSP MUST handle `exit` by terminating the process after shutdown according to LSP conventions.

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

If output omitted, SVG writes to stdout.

If input omitted or `-`, source reads from stdin.

If `--data -` is supplied, CSV data reads from stdin.

The command MUST reject using `-` for both source and CSV data in version 0.1.

If the source contains `Chart(data: stdin)`, CSV data reads from stdin unless `--data <path>` overrides it.

Render options:

`--output <path>`

`--width <px>`

`--height <px>`

`--png-scale <factor>`

`--png-dpi <dpi>`

`--base-dir <path>`

`--data <path|->`

`--theme <name>`

`--debug-layout`

`--emit-metadata`

`--strict`

`--theme <name>` is a render-time override.

It does not change source syntax.

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

### 22.9 Exit Codes

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

### 22.10 Diagnostic Output

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
    algraf-syntax/
    algraf-semantics/
    algraf-data/
    algraf-render/
    algraf-lsp/
    algraf-core/
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

`semantics`:

name resolution

schema-aware validation

IR

geometry registry

semantic diagnostics

`data`:

CSV loading

schema inference

dataframe

type inference

`render`:

scale training

layout

stats

geometries

SVG emission

`lsp`:

tower-lsp backend

document cache

completion

hover

diagnostics publication

`cli`:

argument parsing

command dispatch

I/O

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

`thiserror` for errors

`tower-lsp` for LSP

`tokio` for async LSP runtime

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
6. Resolve schema asynchronously.
7. Infer derived table schemas where possible.
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

### 26.2 Semantic Diagnostics

`E1001 Chart requires data argument`

`E1002 duplicate Chart argument`

`E1003 unsupported Chart argument`

`E1004 data source must be string literal or stdin sentinel`

`E1005 data file not found`

`E1006 data file could not be read`

`E1007 CSV header missing`

`E1008 duplicate CSV column`

`E1101 unknown column`

`E1102 ambiguous column`

`E1103 unknown derived table`

`E1104 duplicate derived table`

`E1201 unknown geometry`

`E1202 unknown property`

`E1203 duplicate property`

`E1204 invalid property type`

`E1205 missing required property`

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

### 26.3 Warning Diagnostics

`W2001 empty Space block`

`W2002 geometry produced no marks`

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

## 27. Testing Strategy

### 27.1 Test Categories

lexer tests

parser tests

resilience tests

semantic analysis tests

schema inference tests

stat tests

scale tests

layout tests

SVG snapshot tests

CLI integration tests

LSP request tests

formatter tests

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

Hover tests SHOULD verify operator docs.

Diagnostics tests SHOULD verify ranges.

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

### 28.2 LSP Latency

Completion SHOULD respond under 50 ms from warm cache.

Hover SHOULD respond under 50 ms from warm cache.

Diagnostics SHOULD update within 250 ms after the debounce window for typical files.

Schema resolution SHOULD not block editor input.

### 28.3 Render Performance

Version 0.1 targets small to medium CSV files.

Rendering 10,000 points to SVG SHOULD complete in under 200 ms on the reference development machine.

Rendering a 25-bin histogram from 100,000 rows SHOULD complete in under 500 ms on the reference development machine.

Rendering 1,000,000 points is not a version 0.1 target.

Future versions MAY stream data and aggregate stats without materializing rows.

### 28.4 Memory

LSP SHOULD cap cached document count if needed.

Schema cache SHOULD avoid storing full data.

Render command MAY load full CSV into memory in version 0.1.

## 29. Security

### 29.1 General Security

Algraf source is declarative.

Algraf MUST NOT execute arbitrary code.

Algraf MUST NOT shell out during render.

Algraf MUST NOT load remote resources by default.

Algraf MUST escape SVG text and attributes.

Algraf SHOULD cap resource usage for LSP schema reads.

### 29.2 Path Handling

Data source paths resolve relative to chart source.

CLI MAY allow absolute paths.

LSP SHOULD respect workspace boundaries where possible.

Path traversal is not inherently unsafe for local CLI, but editor integrations SHOULD avoid surprising reads outside workspace.

### 29.3 SVG Injection

All text labels are escaped.

All attribute values are escaped.

Color values are validated before insertion.

URL-valued properties are not supported in version 0.1.

### 29.4 Denial of Service

Parser must not recurse unboundedly on malformed input.

Algebra nesting depth SHOULD be capped.

Array nesting depth SHOULD be capped.

CSV sample size for LSP SHOULD be capped.

Render command SHOULD offer user-visible errors for files that are too large if limits are added.

## 30. Versioning

### 30.1 Language Version

Released version 0.1 files have no source-level version declaration.

Draft version 0.2.0 continues to treat source files as unversioned unless this section is amended before release.

Future files MAY include:

```ag
Algraf(version: "0.2")
```

This is not version 0.1 syntax and is not part of the v0.2.0 plan unless explicitly promoted.

### 30.2 Stability

Before 1.0, syntax may change.

Published examples SHOULD include language version once version declarations exist.

Diagnostics codes SHOULD remain stable where practical.

### 30.3 Feature Gates

Future feature gates MAY enable:

SQL sources

interactive SVG

plugins

custom stats

advanced quoted-identifier escape modes

### 30.4 Version 0.2.0 Planning

Version 0.2.0 development is tracked in [`V0_2_PLAN.md`](V0_2_PLAN.md).

The release theme is chart control and editing polish.

The intended v0.2.0 scope promotes a selected subset of original optional or deferred items.

Promoted items MUST be copied into the relevant normative sections of this specification before or alongside implementation.

Deferred optional items remain non-commitments until explicitly promoted.

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

CLI render supports CSV data from standard input with `--data -`.

`Chart(data: stdin)` is the bare sentinel for stdin CSV data.

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
Program        ::= Trivia* ChartBlock Trivia* EOF

ChartBlock     ::= "Chart" "(" ChartArgs? ")" "{" ChartBody "}"
ChartArgs      ::= Arg ("," Arg)* ","?
ChartBody      ::= ChartItem*
ChartItem      ::= SpaceBlock
                 | DeriveDecl
                 | ScaleDecl
                 | GuideDecl
                 | ThemeDecl
                 | LayoutDecl
                 | ErrorItem

SpaceBlock     ::= "Space" "(" Algebra SpaceArgs? ")" "{" SpaceBody "}"
SpaceArgs      ::= "," Arg ("," Arg)* ","?
SpaceBody      ::= SpaceItem*
SpaceItem      ::= GeometryCall
                 | ScaleDecl
                 | GuideDecl
                 | ThemeDecl
                 | ErrorItem

DeriveDecl     ::= "Derive" Ident "=" StatCall
StatCall       ::= Ident "(" StatInput? StatArgs? ")"
StatInput      ::= Algebra
StatArgs       ::= "," Arg ("," Arg)* ","?

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
PrimaryExpr    ::= Ident
                 | QuotedIdent
                 | "(" Algebra ")"
                 | ErrorExpr
```

`StdinSentinel` is the bare token `stdin` and is only semantically valid as `Chart(data: stdin)`.

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
        self.expect_recover(TokenKind::Equal, "expected '=' after derived table name");
        let stat = self.parse_stat_call();
        let end = stat.span.clone();

        Some(Spanned {
            node: DeriveDecl { name, stat },
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

suggest `"lm"`, `"loess"` if implemented.

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
