# Algraf v0.83.0 Plan

Status: Implemented
Target version: 0.83.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_82_PLAN.md`](V0_82_PLAN.md)
Roadmap theme: Typographic control over chart text chrome — alignment, weight,
style, and visibility for every titled text element (title, subtitle, caption,
source, axis titles/text, legend titles/text, strip text), so a house style can
fully art-direct the words on a chart without bespoke rendering code.
Cross-repo coordination: none required to ship 0.83.0. The browser packages
`algraf-wasm` and `algraf-editor` are not published at 0.83.0 implementation
time, so their package versions and consumer pins remain on the latest verified
published version, 0.75.0.

## Purpose

v0.82.0 made a chart's *editorial chrome* declarative: opposite-side axes,
stacked multi-line caption/source blocks, callout badges, per-axis grids, and
numeric axis formatting. What it did **not** do is give the author full
typographic control over the chrome text itself. Today the theme text tokens
(`plotTitle`, `plotSubtitle`, `plotCaption`, `plotSource`, `axisTitle`,
`axisText`, `stripText`, `legendTitle`, `legendText`) accept only
`Text(fontFamily?, size?, fill?)` — family, size, and color. Everything else
about how the words are set is hard-wired in the renderer:

1. **Weight is fixed.** The chart title is emitted with a hard-coded
   `font-weight="600"`; nothing else can be bold, and the title cannot be made
   lighter. There is no `weight` surface at all.
2. **Style is fixed.** Nothing can be italicized — a common need for a subtitle,
   a source/attribution line, or an axis caption.
3. **Alignment is fixed.** The title and subtitle are pinned to the left of the
   plot rectangle; the caption and source block is pinned to the bottom-right
   (`text-anchor="end"`). An author cannot center a title, right-align a
   subtitle, or move the footnote/source block to the left. The reference
   request — "change the left/right-hand side of all of that" — has no surface.
4. **Visibility is all-or-nothing and inconsistent.** Chart chrome text is
   suppressed only by omitting the `Chart(...)` argument, and *auto-generated*
   text (axis titles, tick labels, strip labels, legend titles/text) has no
   uniform off switch from the theme; an inherited theme that sets a subtitle or
   axis title style cannot be told "but hide it here."

The v0.83.0 goal is to close this gap with the smallest orthogonal set of
**typographic properties on the existing `Text(...)` style form** — `weight`,
`style`, `align`, and `hidden` — applied uniformly to every titled text token,
plus the layout/emission rules that honor them. No new declaration kinds, no
brand themes: the words on a chart become as art-directable as the marks, and
every default reproduces today's byte-stable output exactly.

## Release Thesis

Typography is data, not code. Every piece of text a chart draws — title,
subtitle, caption, source, axis titles, tick labels, legend titles, legend
entries, strip labels — should expose the same small, orthogonal style
vocabulary (family, size, color, weight, style, alignment, visibility) through
one reusable property form, layered over the existing theme grammar. A house
style sets its headline weight, its italic attribution, its centered deck, and
its hidden axis titles once in a `Theme(...)` block; the renderer stays neutral
and deterministic.

Three boundaries are preserved:

- **No new declaration kinds and no brand themes.** Typography rides on the
  existing grouped `Text(...)` override form (§20.8) and the existing text
  tokens. House styles live in author source, consistent with v0.82.0.
- **Defaults are byte-stable.** Every new property has a default that reproduces
  the current SVG and draw-list output exactly: title weight stays `600`,
  everything else stays normal weight / upright; title and subtitle stay
  left-of-plot; caption and source stay bottom-right; nothing is hidden. The
  corresponding attributes are emitted only when the resolved value departs from
  today's behavior, so the existing example corpus re-renders byte-for-byte.
- **Determinism is unchanged.** Alignment and visibility reuse the existing
  deterministic approximate text-measurement and layout-reserve model (§17.2–
  §17.3); hidden text reclaims its reserve; output stays byte-stable and
  locale-independent.

## Reference Intent

The motivating request is full art-direction of chart words: a centered bold
title, an italicized subtitle, a left-aligned (rather than right-pinned)
footnote/source block, and the ability to hide auto-generated chrome (e.g. the
axis titles) entirely — all expressed in one `Theme(...)` block over an
otherwise ordinary chart. The release ships a checked-in example
(`examples/text_chrome.ag`, item G) that exercises weight, style, alignment, and
hiding together to prove the composition.

| Requested control | Plan item |
| --- | --- |
| Title/subtitle/caption/source **color, family, size** | Already supported (v0.82 text tokens) |
| Title/subtitle/caption/source **weight** (bold/light) | A. `weight` on `Text(...)` |
| Italic subtitle / source / any text | A. `style` on `Text(...)` |
| Move title/subtitle/caption/source to the **left/center/right** | B/C. `align` on `Text(...)` |
| **Hide** any titled text element completely | D. `hidden` on `Text(...)` |
| The same controls on **axis/legend/strip** text | A–D apply uniformly to all text tokens |

## Must

### A. Typographic properties on the `Text(...)` style form

- The grouped `Text(...)` override form (§20.8) and the underlying `TextStyle`
  (§20.1) MUST gain three optional typographic properties, valid on every text
  token (`plotTitle`, `plotSubtitle`, `plotCaption`, `plotSource`, `axisTitle`,
  `axisText`, `stripText`, `legendTitle`, `legendText`):
  - `weight` — string literal `"normal"` or `"bold"`, or an integer in the SVG
    range `100`–`900` (multiples of `100`). Maps to the SVG `font-weight`
    attribute.
  - `style` — string literal `"normal"` or `"italic"`. Maps to the SVG
    `font-style` attribute.
  - `align` — string literal `"left"`, `"center"`, or `"right"`, selecting the
    horizontal placement/anchor of the text (items B, C). `"start"`/`"middle"`/
    `"end"` MUST be accepted as synonyms of `"left"`/`"center"`/`"right"` so the
    token vocabulary matches the §14.16 `Text` mark `anchor` vocabulary.
- `TextStyle` (§20.1) gains `weight: FontWeight`, `style: FontStyle`, and
  `align: TextAlign` fields with defaults that reproduce current output:
  per-token default weight is `bold`/`600` for `plotTitle` and `normal`
  elsewhere; default style is `normal` everywhere; default align is `left` for
  `plotTitle`/`plotSubtitle`/`axisText`/`stripText`/`legendText`, and `right`
  (`end`) for `plotCaption`/`plotSource` (preserving the v0.82 bottom-right
  block). `axisTitle`/`legendTitle` keep their current placement; their `align`
  default is documented in the spec section that owns each.
- A wrong-typed or out-of-range value (e.g. `weight: 150`, `weight: "heavy"`,
  `style: "oblique"`, `align: "justify"`, `align: 1`) MUST emit the existing
  theme-override value diagnostic `E1705`; an unknown sub-property key inside
  `Text(...)` MUST emit the existing unknown-override-key diagnostic `E1704`.
  Both reuse the v0.82 theme-override diagnostics unchanged.
- Emission MUST stay byte-stable: the renderer emits a `font-weight` attribute
  only when the resolved weight is not `normal` (so the title continues to emit
  `font-weight="600"` and nothing else changes); it emits `font-style` only when
  the resolved style is `italic`; it emits `text-anchor`/positions per items B
  and C.
  Acceptance: `plotSubtitle: Text(weight: "bold", style: "italic")` renders a
  bold-italic subtitle; `plotTitle: Text(weight: 400)` drops the title to normal
  weight; an invalid value emits `E1705` and the token keeps its default.

### B. Title and subtitle horizontal alignment

- The chart title and subtitle MUST honor the resolved `align` of `plotTitle`
  and `plotSubtitle` respectively, anchored to the plot rectangle's horizontal
  extent: `left` anchors at the plot left edge (current behavior), `center` at
  the plot horizontal center with `text-anchor="middle"`, and `right` at the
  plot right edge with `text-anchor="end"`.
- Default (`left`) output MUST be byte-for-byte unchanged: no `text-anchor`
  attribute, x at `layout.plot.x`, exactly as today.
- Title and subtitle align independently (a centered title over a left-aligned
  subtitle is valid).
  Acceptance: `plotTitle: Text(align: "center")` centers the title over the plot
  and `plotTitle: Text(align: "right")` right-aligns it, while a chart with no
  `align` override re-renders identically to v0.82.

### C. Caption and source block horizontal alignment

- The stacked caption + source block (§17.3) MUST honor the resolved `align` of
  `plotCaption` (and `plotSource` for the source lines), anchored to the
  viewport content box: `right` keeps today's bottom-right placement
  (`text-anchor="end"` at `width - 16`), `left` places the block at the left
  inset (`text-anchor="start"` at `16`), and `center` centers it
  (`text-anchor="middle"` at `width / 2`). Vertical stacking order and the
  multi-line `\n` handling from v0.82 are unchanged.
- Caption lines and source lines MAY align independently (e.g. left-aligned
  caption key with a right-aligned source line); each line uses the align of its
  own token.
- Default (`right`) output MUST be byte-for-byte unchanged.
  Acceptance: `plotCaption: Text(align: "left")` moves the footnote key to the
  bottom-left while `plotSource` can stay right-aligned, all within the viewport.

### D. Hiding any titled text element

- Each text token MUST accept `hidden` — a boolean literal, default `false`.
  When a token's resolved `hidden` is `true`, the renderer MUST suppress that
  text element entirely and the layout MUST reclaim its reserve:
  - `plotTitle`/`plotSubtitle`/`plotCaption`/`plotSource` hidden ⇒ not emitted
    and their top/bottom reserve (§17.3) is released.
  - `axisTitle` hidden ⇒ axis titles (x and y) are not drawn and their margin
    reserve is released; this composes with the existing
    `Guide(axis:, label: null)` path and the two MUST agree (hidden wins when
    either suppresses the title).
  - `axisText` hidden ⇒ tick labels are not drawn and the tick-label reserve is
    released (tick marks and the axis line are unaffected).
  - `legendTitle`/`legendText` hidden ⇒ the corresponding legend text is not
    drawn (`legendText` hidden collapses the legend to swatches only). As
    implemented, the legend slot keeps its measured size, so hiding legend text
    suppresses the words without re-flowing the swatch column; full
    measurement-shrink is deferred (see Deferred).
  - `stripText` hidden ⇒ facet strip labels are not drawn; the strip rectangle
    is retained.
- `hidden: true` is a theme-level suppression that composes with, and is
  independent of, omitting a `Chart(...)` chrome argument: hiding via the theme
  lets an inherited or named base theme suppress text the chart would otherwise
  draw, without the author having to drop the source string.
- A non-boolean `hidden` MUST emit `E1705`.
  Acceptance: `axisTitle: Text(hidden: true)` removes both axis titles and tints
  their reserve back into the plot; `plotSubtitle: Text(hidden: true)` drops a
  subtitle supplied by `Chart(subtitle:)` and closes the gap above the plot.

### E. Remove the hard-coded title weight

- The renderer MUST stop hard-coding `font-weight="600"` for the title and
  instead resolve it from `plotTitle.weight` (item A), whose default is `600`.
  This is a pure refactor at the default: byte output is unchanged, but the title
  weight becomes overridable (e.g. `plotTitle: Text(weight: 800)` or
  `weight: "normal"`).
  Acceptance: with no overrides the title still emits `font-weight="600"`;
  `git diff` over the example corpus is empty for this item alone.

### F. Draw-list / render metadata

- The text-mark primitive gains optional `font-weight`/`font-style` so themed
  text reaches the SVG backend (§18.7). The draw-list scene models text
  alignment through each text op's anchor and resolved x position (so SVG and
  draw-list agree on placement) and reflects `hidden` by omission — a hidden
  token produces no text op in either backend. As implemented, the draw-list
  text op does not model per-run font weight/style (it models neither font
  family nor size today); those remain SVG attributes, so weight/style are not
  surfaced through the draw-list. This keeps the change proportionate and the
  draw-list output byte-stable except where text is hidden or re-aligned.
  Acceptance: a hidden token produces no corresponding text node in SVG or
  draw-list output, and re-aligned chrome moves consistently in both.

### G. Worked example checked in

- Add `examples/text_chrome.ag` (reusing the existing
  `examples/strategic_reserves.csv` so no new data ships) plus generated
  `examples/text_chrome.svg` and `examples/text_chrome.png`, demonstrating, in
  one `Theme(...)` block: a centered bold title, an italic centered subtitle, a
  left-aligned caption + source block, and hidden axis titles.
- Wire `text_chrome` into `examples/generate.sh` so it regenerates with the rest
  of the corpus and is covered by example CI.
- The example's entire typographic treatment MUST live in one custom
  `Theme(...)` block — no engine-side brand theme — to keep the "typography is
  data" thesis honest.
- The pre-existing `examples/strategic_reserves.*` outputs MUST be byte-stable
  (this release changes no defaults), confirmed by a clean
  `git diff examples/strategic_reserves.*`.

The intended shape of `examples/text_chrome.ag` (final tokens may shift during
implementation, but every property below is one this plan adds or already
supports):

```ag
Chart(data: "strategic_reserves.csv", width: 640, height: 520,
    title: "Oil spill",
    subtitle: "Crude-oil strategic reserves, m barrels",
    caption: "Excludes private reserves",
    source: "Source: EIA") {
    Theme(name: "minimal",
        plotTitle: Text(size: 24, weight: "bold", align: "center", fill: "#111111"),
        plotSubtitle: Text(size: 13, style: "italic", align: "center", fill: "#666666"),
        plotCaption: Text(align: "left"),
        plotSource: Text(align: "left", style: "italic", fill: "#9a9a9a"),
        axisTitle: Text(hidden: true))
    Scale(stroke: country, range: ["United States" => "#e3120b", "Japan" => "#f6a6a1"])
    Guide(legend: false)

    Space(year * reserves) {
        Line(group: country, stroke: country, strokeWidth: 2)
    }
}
```

## Deferred

- **Per-line / rich text runs.** A single token still styles its whole text
  block uniformly; mixed weight/style/color within one title or caption line
  (markup runs) remains deferred.
- **Vertical text alignment / baseline control.** This release adds horizontal
  `align` only; vertical placement of the title/subtitle/caption blocks stays
  on the existing fixed reserves.
- **Letter spacing, line height, and text transform** (`letterSpacing`,
  `lineHeight`, `textTransform`/uppercasing). Out of scope for 0.83.0; the
  vocabulary stays family/size/color/weight/style/align/visibility.
- **`weight`/`style`/`align`/`hidden` on the `Text` *mark* (§14.16) and on
  geometry labels.** This release scopes the new properties to theme text
  *tokens* (chart/axis/legend/strip chrome). Extending them to data-driven
  `Text` marks and `Label`s is deferred (the mark already has `anchor`; adding
  `weight`/`style` there is a separate, larger change).
- **Custom web-font loading / `@font-face` embedding.** `fontFamily` continues to
  name a font; bundling font files is out of scope.
- **Numeric weight values that are not multiples of 100**, and named weights
  beyond `normal`/`bold` (`light`, `semibold`, etc.). Accept the SVG-canonical
  set only; richer aliases are deferred.
- **Legend/strip layout shrink on hidden text.** Hiding `legendTitle`/
  `legendText`/`stripText` suppresses the drawn text, but the legend rectangle
  and facet strip keep their measured size. Re-flowing the legend/strip layout
  to reclaim the suppressed text's space is deferred; full reserve reclamation
  in this release covers the chart chrome (title/subtitle/caption/source) and
  the axis title/tick-label bands.
- **Surfacing weight/style through the draw-list.** The draw-list text op does
  not model per-run typography (it omits font family/size today), so weight and
  style remain SVG attributes; extending the draw-list text schema is deferred.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Focused validation:

- `cargo test -p algraf-render` (weight/style/align emission, title/subtitle
  alignment, caption/source alignment, hidden-token suppression + reclaimed
  layout reserve, byte-stability of defaults).
- `cargo test -p algraf-semantics` (new `weight`/`style`/`align`/`hidden`
  property validation and diagnostics: `E1705` for bad values, `E1704` for
  unknown sub-keys).
- `./examples/generate.sh` then `git diff --stat examples/` to confirm the new
  `text_chrome` outputs render and the rest of the corpus — especially
  `strategic_reserves.*` — is byte-stable.
- Manual: render `examples/text_chrome.ag` and confirm the centered bold title,
  italic centered subtitle, left-aligned footnote/source block, and absent axis
  titles, with the plot area expanded into the reclaimed reserve.

## Promotion Workflow

Implemented in this change:

1. Add the `weight`/`style`/`align` typographic properties and the `hidden`
   visibility flag to the grouped `Text(...)` theme-override form and the
   `TextStyle` shape (A), wire alignment for title/subtitle (B) and
   caption/source (C), implement per-token hiding with reclaimed layout reserve
   (D), refactor the hard-coded title weight onto `plotTitle.weight` (E), and
   surface the resolved fields plus hidden-omission in draw-list/metadata (F),
   each guarded by the existing theme-override diagnostics (`E1704`/`E1705`).
2. Promote the normative text into `ALGRAF_SPEC.md`: §20.1 (`TextStyle` gains
   `weight`/`style`/`align`/`hidden`; per-token defaults), §20.8 (the new
   `Text(...)` sub-properties and their diagnostics), §17.2/§17.3
   (alignment-aware title/subtitle/caption/source placement and hidden-token
   reserve reclamation), the axis sections (§19.x) for `axisTitle`/`axisText`
   hiding composing with `Guide(... label: null)`, the legend/strip sections for
   `legendTitle`/`legendText`/`stripText` hiding, and §18.7 (draw-list/metadata
   `TextStyle` fields and hidden omission).
3. Add `examples/text_chrome.{ag,svg,png}` and register the chart in
   `examples/generate.sh` (G); confirm `strategic_reserves.*` stays byte-stable.
4. Update the language-reference template
   `crates/algraf-cli/templates/ALGRAF_LANG.md` to document the new surface — the
   `weight`/`style`/`align`/`hidden` properties on theme `Text(...)` tokens and
   their value enums — in the same change the features land (see `CLAUDE.md`).
   The template documents only implemented surface.
5. Add a `README.md` tutorial section for the `text_chrome` example in the
   theming part of the progression.
6. Add the 0.83.0 row to the milestone table and mark this plan Implemented.
7. Align Rust, spec, and VS Code release version stamps to `0.83.0`; keep the
   unpublished browser packages (`algraf-wasm`, `algraf-editor`, demo) on their
   latest verified published pins (0.81.0 / 0.75.0), since browser package
   publication is independent of the Rust/CLI release (see `CLAUDE.md`).
8. Run the validation commands listed above.
