# Algraf v0.2.0 Plan

Status: Implemented in working tree
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)

## Purpose

This document defines the intended v0.2.0 release shape and audits optional or deferred items from the original v0.1 specification.

The v0.2.0 release should not automatically include every original `MAY`, "later versions", or "deferred" item. `MAY` means the design permits the feature, not that the next release promises it.

Items in this document are planning guidance. A feature becomes normative when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language.

## Release Thesis

v0.2.0 is a chart-control and polish release.

The current implementation already covers much of the original v0.1 surface: rendering, CLI checks, formatting, LSP basics, themes, guides, faceting, statistical layers, annotations, PNG output, and a broad set of geoms.

The next release should make existing charts easier to control and easier to edit rather than expanding into major new platforms or data backends.

## Scope Rules

- Prefer backwards-compatible syntax additions.
- Prefer features that improve charts users can already express.
- Promote a deferred item only when the syntax, diagnostics, tests, and examples can be finished together.
- Keep product-scale directions out of v0.2.0 unless they are already mostly implemented.
- When a v0.1 `MAY` is not chosen for v0.2.0, leave it explicitly deferred rather than silently implied.

## v0.2.0 Must

### 1. Real Scale Declarations

Status: Implemented.

Implement source-level `Scale(...)` declarations instead of treating them as parsed-but-unsupported declarations.

Minimum target:

```ag
Scale(axis: x, type: "linear")
Scale(axis: y, type: "log10")
Scale(axis: x, domain: [0, 100])
Scale(axis: y, reverse: true)
Scale(fill: species, palette: "accent")
Scale(stroke: series, palette: "default")
```

Acceptance criteria:

- `Scale` declarations are valid at chart scope and space scope.
- Space-local scales override chart-level scales for that space.
- `axis` selectors use bare `x` and `y`, not string literals.
- `type: "linear"` is the default for continuous position scales.
- `type: "log10"` is supported for positive continuous position scales.
- Invalid scale/domain combinations emit targeted diagnostics.
- Palette selection works for `fill` and `stroke` categorical mappings.
- Scale behavior is covered by semantic tests and SVG render tests.

### 2. Guide Controls

Status: Implemented.

Finish the guide controls already sketched in the spec.

Minimum target:

```ag
Guide(axis: x, label: "Flipper Length (mm)")
Guide(axis: y, label: "Body Mass (g)")
Guide(legend: false)
Guide(fill: null)
Guide(stroke: null)
Guide(grid: false)
```

Acceptance criteria:

- Axis label overrides work for x and y.
- Global legend suppression works with `Guide(legend: false)`.
- Aesthetic-specific legend suppression works with `Guide(fill: null)` and `Guide(stroke: null)`.
- Grid suppression works with `Guide(grid: false)`.
- Guide declarations are valid at chart scope and space scope.
- Space-local guide declarations override chart-level guide declarations for that space.
- LSP completion suggests supported guide keys and selector values.

### 3. Temporal Binning

Status: Implemented.

Support `Bin(...)` and `Histogram(...)` for temporal columns.

Minimum target:

```ag
Derive bins = Bin(day, bins: 30)

Chart(data: "series.csv") {
    Space(day) {
        Histogram(bins: 20)
    }
}
```

Acceptance criteria:

- Temporal `Bin` no longer emits `E1405` for supported temporal inputs.
- `bins`, `boundary`, and `closed` work for temporal input with semantics parallel to numeric binning.
- Output columns `bin_start`, `bin_end`, and `bin_center` preserve temporal value type.
- `Histogram` over a temporal vector uses the same temporal binning path as `Derive ... = Bin(...)`.
- Temporal bin tests cover date-only values, naive datetimes, offset-aware RFC3339 datetimes, and boundary assignment.
- Calendar-aware interval syntax such as `interval: "month"` remains optional unless promoted separately.

### 4. LSP Editing Polish

Status: Implemented.

Add LSP features that directly support existing diagnostics and syntax.

Minimum target:

- Semantic tokens for keywords, geometry names, declaration names, properties, columns, operators, strings, numbers, booleans, nulls, and comments.
- Code actions for high-confidence existing diagnostics:
  - quote enum-like literal values;
  - replace likely misspelled geometry names with known geometry names;
  - replace `x * y * group` with `(x * y) / group` when `group` is categorical;
  - parenthesize mixed algebra expressions when diagnostics already identify the issue.

Acceptance criteria:

- LSP capabilities advertise semantic tokens and code actions only when implemented.
- Code actions do not require rendering.
- Code actions preserve unrelated formatting where practical.
- Tests cover the protocol shape and at least one edit for each supported action family.

### 5. Spec, Version, and Example Hygiene

Status: Implemented for the working tree; release tagging remains a packaging step.

Bring documentation and package metadata into alignment with the release.

Acceptance criteria:

- `Cargo.toml` workspace version is updated when the release branch is ready.
- The spec records which v0.1 deferred items are now v0.2 requirements.
- The README shows at least one example using v0.2 scale declarations.
- The README shows at least one example using v0.2 guide controls.
- Examples and generated SVG/PNG outputs are regenerated for changed examples.
- Any feature already implemented but still described as future work is corrected or explicitly labeled as implementation-ahead-of-spec.

## v0.2.0 Should

### Block Comments

Status: Implemented.

`/* ... */` block comments are supported in the lexer (as `Comment` trivia),
parser, formatter, CST trivia preservation, and LSP semantic tokens. Nesting is
not supported: the first `*/` closes the comment. An unterminated block comment
emits `E0020`. Spec §6.3 is now normative; see the `density` example for usage.

### Legend Merging

Status: Implemented.

A `fill` legend and a `stroke` legend mapped to the same categorical column with
identical, in-order entry labels merge into one legend whose swatches draw the
fill color with the stroke color as an outline. Spec §19.7 is now normative; see
the `legend_merge` example.

### Distribution Polish

Status: Implemented (`Density`).

`Density` is implemented as a Gaussian KDE stat (spec §15.11) with deterministic
defaults — Silverman's rule-of-thumb bandwidth, a 256-point grid, and a
3-bandwidth extension — desugaring to an `Area` over a `(density_x, density)`
derived table. `bandwidth` and `n` settings override the defaults. `Violin`
remains unadvertised and deferred.

### Scale Completeness

Status: Partially implemented.

Implemented: scale-driven legend labels via `Scale(fill|stroke: col, label:
"...")` (spec §16.13), power-of-ten log ticks, and targeted diagnostics for
impossible domains (equal endpoints, non-positive log domains). The named
palette registry (`"default"`, `"accent"`) is documented in spec §16.13.

Still deferred: explicit continuous color gradients declared in source (gradient
fills already render for continuous mappings, but there is no source syntax to
configure the gradient stops).

## Explicitly Deferred Past v0.2.0

These original optional or future-facing items should remain out of v0.2.0 unless a later planning decision changes the release thesis.

- SQL-backed data sources.
- WebAssembly runtime.
- Interactive SVG or interactive output.
- IDE preview panes through custom LSP requests.
- Polars backend.
- Streaming or million-row rendering architecture.
- Multi-chart or multi-page documents.
- Nested `Space` blocks.
- User variables or user-defined shadowing.
- Plugins.
- Custom stats.
- Custom theme object syntax.
- Go to definition.
- Feature gates.
- URL-valued properties.
- 3D Cartesian rendering.
- Qualified names using `.`.
- Unicode escape syntax.
- Advanced quoted-identifier escape modes.

## Optional-Item Audit

This audit groups the original spec's `MAY`, "later versions", and "deferred" items by v0.2.0 disposition.

### Promote In v0.2.0

- `Scale(...)` declarations.
- Axis label overrides through `Guide(...)`.
- Legend and guide suppression.
- Temporal binning for `Bin` and `Histogram`.
- LSP semantic tokens.
- LSP code actions for existing diagnostics.
- Documentation updates for implemented raster output and other implementation-ahead-of-spec areas.

### Implemented In v0.2.0 (from "Consider If Capacity Allows")

- Block comments.
- Legend merging (`fill` + `stroke` on the same categorical column).
- `Density` (Gaussian KDE). `Violin` remains deferred.
- Better scale diagnostics and scale-driven legend labels.

### Consider If Capacity Allows

- Renderer/debug documentation polish.
- Explicit continuous color gradient declarations in source.

### Keep Deferred

- Interactive output.
- SQL sources.
- WASM.
- IDE previews.
- Multi-page or multi-chart documents.
- Nested spaces.
- Polars.
- Derived stats depending on earlier derived tables.
- Property aliases such as `colour`.
- Custom themes.
- Go to definition.
- Large-data streaming.
- Feature gates and plugin infrastructure.

## Promotion Workflow

1. Move the chosen behavior into the relevant normative section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md).
2. Add or update diagnostic codes before implementation if behavior can fail.
3. Implement parser, semantic, render, CLI, and LSP changes as needed.
4. Add focused tests in the crate closest to the behavior.
5. Add or update examples when the behavior affects user-facing charts.
6. Regenerate examples when rendered output changes.
7. Update this document when a candidate is completed, rejected, or moved out of scope.
