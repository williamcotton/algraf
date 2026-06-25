# Algraf v0.88.0 Plan

Status: Implemented
Target version: 0.88.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_87_PLAN.md`](V0_87_PLAN.md)
Roadmap theme: Ridgeline distribution charts and finer guide styling so Algraf
can reproduce common statistical chart designs without leaving the DSL.
Cross-repo coordination: browser package publication remains independent from
the Rust/CLI release. The npm registry has `algraf-wasm` and `algraf-editor`
published through 0.87.0 at implementation time, so consumer dependency pins
stay on published versions unless a separate unpublished package release is
prepared.

## Purpose

Algraf already supports density and violin summaries, but common ridgeline
charts require one-sided density envelopes, observation points constrained to
the density shape, and enough guide styling to match editorial chart output.

This release promotes those charting features into the language so a chart like
a plotnine-style annual temperature ridgeline can be written directly in Algraf
with deterministic SVG output.

## Scope

### One-Sided Violin Envelopes

Status: Implemented.

Acceptance criteria:

- `Violin` accepts `side: "both" | "left" | "right" | "top" | "bottom"`.
- Omitted `side` preserves the existing mirrored violin behavior.
- One-sided violins work in both categorical-x/continuous-y and
  continuous-x/categorical-y frames.
- Quantile reference lines respect the selected side.
- Invalid `side` values produce the existing enum diagnostic path.

### Sina Observation Geometry

Status: Implemented.

Acceptance criteria:

- The geometry registry advertises `Sina`.
- `Sina` supports grouped categorical/continuous distribution frames matching
  `Violin`.
- `Sina` computes the same deterministic per-group KDE envelope controls as
  `Violin` (`bandwidth`, `n`, `width`, `side`) and places points within that
  envelope.
- Point placement is deterministic from row identity and does not use runtime
  randomness.
- `fill`, `alpha`, and `size` styling works with literal or scale-backed forms
  where the registry permits them.

### Guide Styling Controls

Status: Implemented.

Acceptance criteria:

- `Theme(...)` accepts `axisLine: Line(...)`, `axisTicks: Line(...)`, and
  `axisTickLength: number`.
- `Line(...)` theme style overrides accept `dash: "solid" | "dotted" |
  "dashed"` for grid and axis guide lines.
- `axisColor` continues to provide a compatibility shorthand for the default
  axis line and tick stroke color.
- A `Line(stroke: "none", strokeWidth: 0)` guide style suppresses that guide ink
  while leaving labels and layout behavior intact.

### Examples, Docs, And Release Artifacts

Status: Implemented.

Acceptance criteria:

- Add a runnable ridgeline temperature example with source CSV, SVG, and PNG
  output.
- Update `examples/README.md` with the new source and rendered SVG.
- Update the normative spec, language reference templates, and TextMate grammar
  for `Sina`, `side`, and guide style tokens.
- Create this v0.88 plan rather than reopening v0.87.
- Bump Rust workspace, spec, and VS Code extension release stamps to 0.88.0.
- Keep npm package and consumer dependency pins on published package versions
  unless a separate package release is prepared.

## Validation

- `cargo fmt --all`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- Render and visually inspect `examples/ridgeline_temperatures.png`.

## Explicitly Deferred Past v0.88.0

- Arbitrary SVG dash arrays beyond the existing named dash styles.
- Random or seed-configurable jitter for `Sina`; deterministic placement is the
  v0.88 behavior.
- A `coord_flip`-style chart-level transform. Algraf continues to express
  orientation through the physical order of the algebraic space.
- Generalized ridgeline layout syntax beyond composing one-sided `Violin` and
  `Sina` layers.
