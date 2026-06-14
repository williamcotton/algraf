# Algraf v0.82.0 Plan

Status: Implemented
Target version: 0.82.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_81_PLAN.md`](V0_81_PLAN.md)
Roadmap theme: Editorial chart design primitives — opposite-side axes,
multi-line caption/source blocks, and annotation callout badges — proven by
reproducing a wire-service house-style line chart.
Cross-repo coordination: none required to ship 0.82.0. The browser packages
`algraf-wasm` and `algraf-editor` are not published at 0.82.0 implementation
time, so their package versions and consumer pins remain on the latest verified
published version, 0.75.0.

## Purpose

Algraf can already express the *content* of a publication-quality editorial
chart: multiple lines, manual per-category colors, direct on-line labels,
reference rules, fixed domains, and themed grids. What it cannot yet express is
the *editorial chrome* that distinguishes a newsroom chart from a default
plot. Concretely, a reproduction attempt against the reference chart described
below (a two-series strategic-reserves line chart in the wire-service house
style) stalls on four missing primitives:

1. The value axis cannot move to the right-hand side. Algraf fixes the y axis at
   the left (spec §19.3) with no opposite-side option.
2. The caption is a single line. Newlines in `caption:` do not wrap, so a stacked
   footnote/key plus a sources line overflows the viewport instead of stacking.
3. Numbered event markers (the small circled "1"/"2" badges that sit at the top
   of each reference rule) have no primitive. Bare `Text` approximates the digit
   but not the badge, and nothing ties the badge to its `VLine`.

These are not one brand's quirks. Right-hand value axes, stacked
footnote/source blocks, and keyed event callouts are the recurring vocabulary
of editorial chart design across many publications. The
v0.82.0 goal is to ship these as **generic, composable design primitives** so
that any host — not just one newsroom — can encode its own house style in a
reusable `Theme(...)` plus a handful of annotation arguments, with no bespoke
rendering code.

## Release Thesis

Design rules are data, not code. A publication's house style should be a
declarative composition of theme tokens and annotation properties layered over
the existing grammar of graphics — never a fork of the renderer. This release
adds the smallest set of orthogonal primitives that, composed, reproduce a real
editorial chart end to end, and it proves the composition with a checked-in
example whose `.ag` carries the entire house style in one custom `Theme(...)`
block.

Three boundaries are preserved:

- **No brand-specific built-in theme.** v0.82.0 ships primitives and tokens, not
  an `economist`/`nyt`/`ft` theme. House styles live in author source so the
  engine stays neutral and deterministic, consistent with the existing neutral
  presets (`gray`, `bw`, `linedraw`, §20.6).
- **Determinism is unchanged.** Axis side, caption wrapping, and badge sizing
  all use the existing deterministic text-measurement model (§17.3, §14.16
  decluttering) and emit byte-stable SVG and draw-list output.
- **Annotations stay inside the grammar.** Callout badges extend the existing
  `VLine`/`HLine` marks rather than introducing an imperative overlay layer.

## Reference Chart

The implementation target is the wire-service "strategic reserves" line chart:
crude-oil strategic reserves (m barrels) for the United States and Japan,
2000–2026, in the publication house style. Its distinguishing editorial features,
mapped to this plan:

| Reference feature | Plan item |
| --- | --- |
| Value (y) axis labels on the **right** | A. Opposite-side axes |
| Stacked event key + footnote + **sources** line below the plot | B. Multi-line caption & source block |
| Small circled **"1"/"2"** badges atop the two event rules | C. Annotation callout badges |
| Direct on-line labels, no legend; manual reds; fixed `[0,800]` domain | Already supported (v0.81) |

The reproduction must be a single `.ag` file plus a small CSV, with the house
style fully contained in one custom `Theme(...)` block — demonstrating that the
same primitives recompose into any other publication's rules.

## Must

### A. Opposite-side axis placement

- `Guide(axis: y, position: "right")` MUST render the y axis (ticks, tick labels,
  and title) on the right edge of the plot rectangle; `Guide(axis: x, position:
  "top")` MUST render the x axis on the top edge. `position` accepts only
  `"left"`/`"right"` for `axis: y` and `"top"`/`"bottom"` for `axis: x`. The
  defaults (`"left"`, `"bottom"`) are unchanged when `position` is absent.
- An invalid `position` value, or a value not valid for the named axis (e.g.
  `axis: y, position: "top"`), MUST emit `E1204` (reusing the existing guide
  argument diagnostic), and the axis MUST fall back to its default side.
- Layout (§17.2–§17.3) MUST reserve the axis rectangle on the chosen side: a
  right y axis reserves right margin instead of left; a top x axis reserves top
  margin instead of bottom. `marginRight`/`marginLeft`/`marginTop`/`marginBottom`
  floors continue to compose as `max(computed, configured)` on whichever side now
  carries the axis.
- Grid lines, plot clipping (§18.5, v0.80), and data-mark placement MUST be
  unaffected by axis side — only guide placement and margin reservation move.
- Draw-list / render metadata (§18.7) MUST expose the resolved side: `axes.x` and
  `axes.y` each gain a `position` string (`"left"`/`"right"`/`"top"`/`"bottom"`).
  Acceptance: a chart with `Guide(axis: y, position: "right")` renders tick
  labels right of the plot with no left-margin tick reserve, and round-trips the
  `position` field in `--format svg+json` and `--format draw-list`.

### B. Multi-line caption and source block

- `Chart(caption: "...")` MUST honor newline (`\n`) characters in the caption
  string, rendering each line as a separate stacked text line below the plot in
  source order, reusing the per-line escaping rule from `Text` (§14.16). Layout
  MUST reserve the measured multi-line height for the caption rectangle so no
  line is clipped or overlaps the x axis.
- `Chart` MAY include a `source: "..."` string argument. When present, it renders
  as a final caption line styled by a new theme token (`plotSource`, item D),
  visually de-emphasized relative to the caption body, and its height is included
  in the caption reserve. `source:` honors `\n` with the same stacking rule.
- Draw-list / render metadata MUST keep `chart.caption` as the raw author string
  (newlines preserved) and MUST add `chart.source` (string or `null`).
- A non-string `caption:` or `source:` MUST emit the existing chart-argument type
  diagnostic.
  Acceptance: a three-line caption plus a `source:` line stacks below the plot
  within the viewport, deterministically, in both SVG and raster output.

### C. Annotation callout badges on reference rules

- `VLine` and `HLine` MUST accept the existing `label:` plus new badge controls:
  - `labelPosition` — string literal selecting where the label sits along the
    rule. For `VLine`: `"top"` (default) or `"bottom"`. For `HLine`: `"start"`
    or `"end"`. Invalid values MUST emit `E1204`.
  - `labelShape` — string literal `"none"` (default, plain text as today),
    `"circle"`, or `"square"`, drawing a deterministically sized badge box behind
    the label. Badge size derives from the label text via the existing estimated
    text-measurement model so output stays byte-stable.
  - `labelFill` and `labelStroke` — color literals for the badge fill and border;
    default to a readable contrast pair derived from the rule `stroke`.
- A badge MUST render as `Rect`/circle primitive plus `Text` in the draw-list
  scene (no new primitive kind), so it participates in existing scene metadata.
- Existing `VLine(..., label: "Marker")` source without the new arguments MUST be
  byte-for-byte unchanged (plain text at top), preserving backward compatibility.
  Acceptance: two `VLine`s with `label: "1"`/`label: "2"`, `labelShape:
  "circle"`, `labelPosition: "top"` render circled digits centered on each rule
  at the plot top, matching the reference event markers.

### D. Theme tokens for editorial chrome

- §20.1 `Theme` gains two tokens, settable via the §20.8 custom-theme override
  syntax and resolved by layering over the named base:
  - `plotSource: Text(fontFamily?, size?, fill?)` — styles the `source:` line
    (item B). Defaults to a smaller, lighter variant of `plotCaption`.
  - `axisYPosition` / `axisXPosition` — optional string tokens (`"left"`/
    `"right"`, `"top"`/`"bottom"`) giving a theme-level default axis side that a
    per-chart `Guide(axis:, position:)` overrides. This lets a house style set
    "value axis on the right" once, the way the reference chart does.
- An unknown override key MUST emit `E1704`; a wrong-typed/wrong-shaped override
  value MUST emit `E1705` (reusing the existing theme-override diagnostics).
  Acceptance: a single custom `Theme(...)` block sets the right-side value axis
  and the source-line style, and the reference chart needs no other styling
  arguments.

### E. Numeric axis tick label formatting

- `Guide(axis: x, format: "...")` and `Guide(axis: y, format: "...")` MUST format
  numeric (continuous, non-temporal) axis tick labels using the deterministic
  numeric format vocabulary already defined for `Text` (§14.16: `.0f`, `.1f`,
  `.2f`, `$.2f`, `.0%`, `.1%`, `.2%`). This gives editorial control over value
  axis labels (e.g. `"800"` vs `"800.0"`) without a temporal column.
- `format` on a temporal axis (use `timeFormat` instead), on a categorical axis,
  combined with `timeFormat`, or with an unknown format string MUST emit `E1909`.
  Acceptance: `Guide(axis: y, format: ".0f")` renders integer value labels
  deterministically across SVG, raster, and draw-list output.

### G. Per-axis grid lines (added during implementation)

- `Guide(axis: x, grid: false)` MUST suppress the vertical grid lines at x ticks,
  and `Guide(axis: y, grid: false)` the horizontal grid lines at y ticks, while a
  bare `Guide(grid: false)` continues to toggle all grid lines. A theme MAY set
  the per-axis default with `gridX`/`gridY` booleans (§20.1), which a per-chart
  `Guide(axis:, grid:)` overrides. This lets a house style keep only horizontal
  rules, as the reference chart does. Per-axis grid control affects only grid
  lines, not axis lines, ticks, or tick labels.
- Editorial value-axis tick steps (e.g. `0 200 400 600 800`) reuse the existing
  `Scale(axis: y, breaks: [...])`; the pale editorial background reuses the
  existing `background`/`plotBackground` theme overrides. No new surface is
  required for those two; only per-axis grid control is new in item G.
  Acceptance: a `minimal` theme with `gridX: false`, `breaks: [0,200,400,600,800]`,
  and `background:`/`plotBackground:` reproduces the wire-service grid + scale.

### F. Worked example checked in

- Add `examples/strategic_reserves.csv`, `examples/strategic_reserves.ag`, and the
  generated `examples/strategic_reserves.svg` + `examples/strategic_reserves.png`,
  reproducing the reference chart using only the primitives above.
- Wire `strategic_reserves` into `examples/generate.sh` so it regenerates with the
  rest of the corpus and is covered by example CI.
- The example's entire house style MUST live in one custom `Theme(...)` block plus
  per-annotation arguments — no engine-side brand theme — to demonstrate the
  "design rules are data" thesis.

The target `examples/strategic_reserves.ag` (final styling tokens may shift during
implementation, but this is the intended shape):

```ag
Chart(data: "strategic_reserves.csv", width: 600, height: 560,
    marginTop: 70, marginRight: 64,
    title: "Oil spill",
    subtitle: "Crude-oil strategic reserves*, m barrels",
    caption: "1 Russia invades Ukraine\n2 US-Israeli air strikes on Iran begin\n*Excludes private reserves",
    source: "Sources: EIA; Japan's Ministry of Economy, Trade and Industry") {
    Theme(name: "minimal",
        background: "#f3f0eb",
        plotBackground: "#f3f0eb",
        gridMajor: Line(stroke: "#d8d4cc", strokeWidth: 1),
        axisText: Text(size: 11, fill: "#7a7a7a"),
        plotSource: Text(size: 10, fill: "#9a9a9a"),
        axisYPosition: "right",
        gridX: false)
    Scale(stroke: country, range: ["United States" => "#e3120b", "Japan" => "#f6a6a1"])
    Scale(axis: y, domain: [0, 800], breaks: [0, 200, 400, 600, 800])
    Guide(axis: x, label: null)
    Guide(axis: y, label: null, format: ".0f")
    Guide(legend: false)

    Space(year * reserves) {
        Line(group: country, stroke: country, strokeWidth: 3)
        VLine(x: 2022, stroke: "#111111", label: "1", labelShape: "circle", labelPosition: "top")
        VLine(x: 2025, stroke: "#111111", label: "2", labelShape: "circle", labelPosition: "top")
        Text(x: 2008, y: 640, label: "United States", fill: "#e3120b", size: 13, anchor: "middle")
        Text(x: 2010, y: 250, label: "Japan", fill: "#d98884", size: 13, anchor: "middle")
    }
}
```

The CSV is the small long-format table used in the reproduction
(`year,country,reserves` with the US and Japan series, 2000–2026); it ships
verbatim as `examples/strategic_reserves.csv`.

## Deferred

- A built-in named brand theme (`economist`, `ft`, etc.). House styles stay in
  author source for this release.
- Mixed-form numeric tick labels (e.g. `2000` then `05`, `10`, `15` — full first
  tick, abbreviated successors). v0.82.0 ships uniform numeric `format`; the
  century-anchored abbreviation pattern remains deferred.
- Leader/connector lines between a callout badge and an arbitrary data point.
  Badges attach to `VLine`/`HLine` only; free-floating connected callouts remain
  deferred (consistent with the §14.16 connector-line deferral).
- Auto-placed direct series labels (label-at-line without explicit `x`/`y`).
  Continue using `Text`/`Label` (§14.16) with author coordinates.
- Per-side independent dual axes (different left and right y scales). v0.82.0
  moves a single trained axis to a chosen side; it does not introduce a second
  independent value scale.
- Footnote-key auto-linking (binding the caption "1"/"2" entries to their badge
  marks as one declarative structure). The example keys them by convention.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Focused validation:

- `cargo test -p algraf-render` (axis-side layout, caption stacking, badge
  emission, numeric axis format).
- `cargo test -p algraf-semantics` (new `position`/`format`/badge property
  validation and diagnostics: `E1204`, `E1909`; theme overrides: `E1704`,
  `E1705`).
- `./examples/generate.sh` then `git diff --stat examples/` to confirm the
  `strategic_reserves` outputs and the rest of the corpus are byte-stable.
- Manual: render `examples/strategic_reserves.ag` and compare against the
  reference chart feature-by-feature (right axis, stacked caption + source,
  circled event badges).

## Promotion Workflow

To be implemented in this change:

1. Add opposite-side axis placement (A), multi-line caption + `source:` (B),
   callout badges (C), theme tokens (D), and numeric axis tick formatting (E),
   each behind targeted diagnostics.
2. Promote the normative text into `ALGRAF_SPEC.md`: §19.2/§19.3 (axis
   `position`), §19.4 (numeric axis `format`, `E1909`), §14.17/§14.18 (HLine/
   VLine badge properties), §17.2/§17.3 (right/top axis rect reservation,
   multi-line caption + source reserve), §20.1/§20.8 (`plotSource`,
   `axisYPosition`/`axisXPosition` tokens), and §18.7 (draw-list `axes.position`,
   `chart.source`, badge marks).
3. Add `examples/strategic_reserves.{csv,ag,svg,png}` and register the chart in
   `examples/generate.sh` (F).
4. Update the language-reference template
   `crates/algraf-cli/templates/ALGRAF_LANG.md` to document the new surface — axis
   `position`, multi-line `caption`/`source`, `VLine`/`HLine` badge properties,
   the `plotSource`/`axisYPosition`/`axisXPosition` theme tokens, and numeric
   axis `format` — in the same change the features land (see `AGENTS.md`/
   `CLAUDE.md`). The template documents only implemented surface, so it is
   updated alongside implementation, not ahead of it.
5. Add the 0.82.0 row to the milestone table and mark this plan Implemented.
6. Align Rust, spec, VS Code, and demo release version stamps to `0.82.0`; keep
   unpublished browser package pins on the latest verified npm version (0.75.0).
7. Run the validation commands listed above.
