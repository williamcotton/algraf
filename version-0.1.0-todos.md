# Version 0.1.0 TODOs

This file tracks the remaining work needed to finish the 0.1.0 scope described
in `docs/ALGRAF_SPEC.md`. Treat each item as an independent implementation
slice. For every slice, run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

When changing examples, also run:

```bash
./examples/generate.sh
```

## Current Baseline

Already implemented:

- Parser, formatter, CLI `render`, `check`, `format`, `schema`, `ast`, and `ir`.
- CSV loading, schema inference, homegrown dataframe, temporal inference.
- Core semantic analysis: columns, derived tables, geometry/property registry,
  blend validation, facet validation, 3D Cartesian rejection.
- Rendering for `Point`, `Line`, `Bar`, `Rect`, `Tile`, `Histogram`, `Smooth(lm)`,
  `Boxplot`, `Ribbon`, `HLine`, `VLine`, and `Rug`.
- `Bin` stat with `bins`, `binWidth`, `boundary`, and `closed`.
- Facet wrap, nested x bands, legends, axes, grids, themes, titles, subtitles,
  captions, and `Guide(legend: false)`.
- Warnings currently include `W2001`, `W2006`, `W2008`; hint `H3001` exists.

## Must Finish Before 0.1.0

### LSP

`algraf lsp` currently returns a not-implemented error. Build the LSP from the
same parser, schema, semantic-analysis, formatter, and diagnostics paths used by
the CLI.

Required pieces:

- Initialize/shutdown over stdio.
- Full document sync.
- Parse diagnostics.
- Semantic diagnostics.
- Schema cache for source-declared CSV paths.
- Completion for chart items, space items, geometry names, property names,
  enum values, and data columns.
- Hover for geometry/property/stat names and resolved data columns.
- Document symbols for chart, spaces, derives, and major declarations.
- Formatting via the existing formatter.

Implementation note: do not create a separate LSP-only analyzer. CLI diagnostics
and LSP diagnostics must come from the same parser/analyzer behavior.

### Count Stat

Implement `Count` / bar count behavior from spec §15.5.

Required behavior:

- `Bar(stat: "count")` over a 1D categorical space should produce counts.
- The generated y label should default to `count`.
- Add IR/runtime support for count-derived tables or a well-documented direct
  equivalent.
- Add semantic tests, render tests, and an example if useful.

### Space-Local Theme Semantics

The spec says space-local `Theme(...)` overrides chart-level theme values for
that space only. Current CLI theme extraction effectively applies the strongest
source theme globally.

Choose one:

- Implement per-space theme config in IR/rendering.
- Or reject/warn on space-local `Theme(...)` for 0.1.0 and document the
  deliberate deferral.

Do not leave it silently global.

## Important Spec Gaps

### Scale and Guide Declarations

Current behavior: `Scale(...)` is parsed and emits `W2006`; `Guide(legend: false)`
works.

Remaining useful 0.1.0 pieces:

- `Guide(axis: x, label: "...")`
- `Guide(axis: y, label: "...")`
- `Guide(fill: null)` or a documented warning if unsupported.
- Optional: minimal `Scale(...)` implementation if desired.

If scale declarations remain unsupported, keep the current clear diagnostic.

### Documented But Unimplemented Geometries

These are described in the spec but are not currently registered:

- `Area`
- `Text`
- `Segment`

For 0.1.0, either implement them or explicitly document that they are deferred
and keep them out of the registry so users do not hit render-time unsupported
warnings.

### Smooth and Boxplot Stat Model

`Smooth(lm)` and `Boxplot` currently render directly. The spec describes them
as stats that compute derived data.

Decide whether 0.1.0 requires reusable stat output:

- If yes, add explicit stat execution paths and output schemas.
- If no, document that direct renderer implementations are accepted for 0.1.0
  because visual behavior is deterministic and tested.

### Density Output

`Bin` currently outputs `bin_start`, `bin_end`, `bin_center`, and `count`.

Spec status:

- `count` is required.
- `density` is SHOULD.

This can be deferred, but if implemented, add `density` to the derived schema,
runtime computation, and primitive histogram examples/tests.

## Diagnostics Polish

Missing or incomplete diagnostic codes from the spec:

- `W2002` geometry produced no marks.
- `W2003` rows dropped due to missing values.
- `W2004` legend omitted because too many categories.
- `W2005` axis labels may overlap.
- `W2007` invalid values treated as missing.
- `H3002` quote literal color names for clarity.
- `H3003` parenthesize blend expressions.
- `H3004` use `Guide` to override axis label.

Prioritize warnings that catch real user-facing ambiguity. Keep diagnostics
aggregated and deterministic; avoid one warning per row.

## Accepted Deferrals

These are already safe to leave out of 0.1.0 unless the release bar changes:

- Temporal binning. It has a targeted diagnostic.
- `Violin`. The spec allows deferral and it is not advertised in the registry.
- `Smooth(method: "loess")`. It is rejected semantically for now.
- KDE/density plots.
- Network data sources.
- Block comments.
- Incremental parsing.
- Custom theme fields.

## Future-Agent Instructions

When picking up this file:

1. Read `CLAUDE.md` and the relevant section of `docs/ALGRAF_SPEC.md` first.
2. Pick one section above and keep the change narrowly scoped.
3. Add tests next to the crate being changed.
4. Regenerate examples only when examples or rendered output change.
5. Run formatter, clippy, and tests before considering the slice complete.
6. Do not silently advertise unsupported language features in the registry.
