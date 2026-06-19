# Algraf v0.84.2 Plan

Status: Implemented
Target version: 0.84.2
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_84_1_PLAN.md`](V0_84_1_PLAN.md)

## Purpose

Algraf v0.84.2 fixes two legend issues found while embedding generated timeline
charts in Source Sleuth:

- Right/left legends with many entries could render past the requested SVG
  height and be clipped by the root viewport.
- Temporal columns mapped to categorical `fill`/`stroke` scales had no compact
  legend-label formatter, so entries displayed raw RFC3339 category keys.

## Scope

### Tall Vertical Legend Viewports

Status: Implemented.

Acceptance criteria:

- Right and left legend layout rectangles use the measured legend height.
- If the measured vertical legend is taller than the requested chart viewport,
  SVG and draw-list output height expands to include the legend plus the bottom
  guide reserve.
- Plot coordinates and requested chart width remain stable.

### Temporal Color Legend Labels

Status: Implemented.

Acceptance criteria:

- `Scale(fill: temporal_col, timeFormat: "...")` and
  `Scale(stroke: temporal_col, timeFormat: "...")` format categorical legend
  entry labels using the same named/custom temporal formats as guides.
- Explicit `labels:` maps continue to take precedence over `timeFormat:`.
- Invalid formats and non-temporal color columns produce `E1907`; non-color
  scale targets produce targeted semantic diagnostics.
- The CLI language template, registry docs, spec, semantics tests, and renderer
  tests stay in sync.

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
