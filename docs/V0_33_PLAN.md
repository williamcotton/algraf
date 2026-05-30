# Algraf v0.33.0 Plan

Status: Planned (draft)
Chosen syntax: **A3 — algebraic frame operator** (`Space(transpose(a * b))`).
Chosen semantics: **B1 — orientation-aware geometries.**
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_32_PLAN.md`](V0_32_PLAN.md)
Follow-on plan: [`V0_34_PLAN.md`](V0_34_PLAN.md)
Coordinate-system foundation: [`V0_31_PLAN.md`](V0_31_PLAN.md) (polar
`startAngle` / `direction`)

## Purpose

This release adds a **Cartesian coordinate transform** — transposing the x and
y axes — so that the orientation-locked statistical geometries (`Bar`,
`Histogram`, `Boxplot`, `Violin`) can render horizontally. Today they cannot:
every one of them hard-requires a categorical (band) x axis and a continuous y
axis, so a horizontal bar/box/histogram is simply not expressible with the
high-level geoms. Authors work around this by hand-rolling `Rect`/`Segment`
primitives (`temperature_range.ag`, `gantt.ag`, `flights.ag`).

This is the genuine, narrow gap left after an audit of "reflection and
rotation" against the existing implementation. Everything adjacent is already
done and is **out of scope** here:

- **Polar rotation / reflection** — shipped in v0.31 as `startAngle` and
  `direction` (spec §16.16).
- **Cartesian axis reflection** — shipped as `Scale(axis: …, reverse: true)`,
  which swaps the pixel-range endpoints (`render/space.rs` `apply_range`). A
  "Cartesian reflect operator" would only duplicate this; we are **not**
  adding one.
- **Cartesian point/line/area transposition** — already achievable by swapping
  the algebra (`Space(value * category)`); those geoms are not orientation-locked.
- **Arbitrary Cartesian rotation** (`rotate(θ)` for non-90° angles) — produces
  off-axis gridlines and rotated tick labels; a large, low-value rabbit hole
  that even ggplot2 declines. **Deferred indefinitely.**

This document deliberately leaves the **syntax and semantic model unchosen** —
see "Open Decisions" below. The rest of the plan (scope, diagnostics, the list
of geoms that must learn orientation, acceptance) holds regardless of which
surface we pick.

## Release Thesis

v0.33.0 is the **transpose** release: a single new Cartesian coordinate
transform that swaps the role of the x and y axes, making horizontal
categorical/stat charts first-class. It slots into Algraf's existing COORD
layer — the `coords`/`theta`/`startAngle`/`direction` argument family on
`Space`, lowered to `CoordsIr` (`crates/algraf-semantics/src/ir.rs`) and
trained in `crates/algraf-render/src/space.rs`. Cartesian output without the
new transform stays byte-for-byte identical.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
is updated with concrete `MUST` / `SHOULD` / `MUST NOT` language and a stable
diagnostic code.

## Chosen design

### Syntax: A3 — algebraic frame operator

```ag
Chart(data: "demographics.csv", width: 640, height: 420) {
    Space(transpose(gender * count)) {
        Bar(stat: "count", fill: gender)
    }
}
```

`transpose` is a **frame-level operator** that wraps a frame expression and
swaps the role of its two position axes. It is the first member of a small,
extensible family of algebraic coordinate transforms — the deliberate choice
that keeps coordinate manipulation inside the grammar's algebra rather than as
a configuration flag.

Grammar and analysis rules:

- `transpose(F)` is a prefix function application that takes a frame
  sub-expression `F` and yields a frame. The lexer/parser
  (`crates/algraf-syntax`) must accept a function-call node inside frame
  algebra; the analyzer (`analyzer/frames.rs`) lowers it.
- **Disambiguation**: a bare identifier in frame position is still a column
  reference; `transpose` is recognized as an operator *only* when immediately
  followed by `(`. A column literally named `transpose` remains addressable via
  backtick-quoting (`` `transpose` ``), consistent with the existing
  quoted-identifier rule (spec §4.3).
- **Arity / shape**: `F` MUST be a 2-D cross frame (`a * b`). Applying
  `transpose` to a 1-D frame, a `+`-blend that is not 2-D, or a geometry/spatial
  frame is a diagnostic (E1913), mirroring the polar shape rules.
- **Composition**: `transpose` MAY wrap a parenthesized cross frame and compose
  with faceting outside it — `transpose((time * sales)) / region` applies the
  swap per panel. Nesting `transpose(transpose(a * b))` is the identity (lowered
  away, no double-swap). Whether `transpose` may appear *inside* a facet's panel
  vs. only wrapping the cross part is settled during spec drafting.
- **Resilient recovery**: an unterminated or empty `transpose(...)`, or an
  unknown operator name used in call position, recovers and emits E1912 without
  panicking (spec §12.1).

Lowering: `transpose` is **not** carried as user-authored algebra into render;
the analyzer resolves it into the trained space's orientation. Concretely it
sets a transpose flag on `CoordsIr` (a `transpose: bool` on the `Cartesian`
representation), so the renderer's COORD layer — not the geoms' algebra — owns
the swap. This keeps `transpose(a * b)` and a hypothetical future `coords` knob
converging on the same IR.

### Semantics: B1 — orientation-aware geometries

The transform sets an orientation flag on the trained space. Each
orientation-locked geom checks it and swaps which axis is the band (position)
axis vs. the value axis; guides swap which axis is drawn on the bottom vs. the
left; stacking accumulates along the value axis. The authored algebra inside
`transpose(…)` stays `category * value`.

- This matches Algraf's architecture, where geoms emit pixels directly. The
  existing guards (`if !space.x.is_band()`) become "band on the position
  axis," a localized edit per geom.
- The cost is that every orientation-locked geom + guide rendering must be
  touched (the scope list below).

> Rejected alternative — **post-hoc pixel reflection across the y = x diagonal**:
> render normally then mirror emitted coordinates. Incompatible with a direct
> pixel renderer (text renders mirrored; bar widths, tick labels, and stroke
> widths are computed in pixel space and would distort; band geoms still reject
> categorical-on-y up front).

## Scope — what "orientation-aware" touches (under B1)

The transform itself is small; the cost is threading orientation through the
geoms whose stat has a fixed orientation today. Confirmed orientation guards:

| Geometry  | Current guard (file)                                      |
| --------- | --------------------------------------------------------- |
| `Bar`     | `geom/bar.rs:58` — "Bar requires a categorical x dimension" |
| `Boxplot` | `geom/distribution.rs:33` — "categorical x and continuous y" |
| `Violin`  | `geom/distribution.rs:214` — "categorical x and continuous y" |
| `Histogram` | desugars through `Bar` (binned var → band axis)         |

Also in scope:

- **Guides**: `guide/emit.rs` hardcodes the x axis along the bottom
  (`emit.rs:321`) and the y axis on the left; transpose must swap which trained
  axis renders where, including tick-label sizing
  (`max_x_tick_label_height` vs `max_y_tick_label_width`) and the plot-rect
  margin computation that depends on them.
- **Stacking / dodging** (`Bar(layout: "stack" | "fill" | "dodge")`): must
  accumulate along the value axis, not always y.
- **`Rug(sides: …)`**: side semantics ("l"/"b"/…) are orientation-relative;
  decide whether sides stay absolute (left/bottom) or follow the transpose.
- **Polar interaction**: `transpose` + `coords: "polar"` is rejected
  (a new diagnostic), since polar already owns axis-role assignment via
  `theta`.
- **Faceting**: `transpose` must compose with `/` nesting (faceted frames),
  applying per panel.

## Diagnostics to reserve (before coding)

Reserve in the spec first (next free codes after the polar family, which ends
at E1910):

- **E1911** — `transpose` used with a coordinate system that does not support
  it (i.e. the same space also requests `coords: "polar"`).
- **E1912** — malformed frame operator: an unknown name used in call position
  inside frame algebra, or an empty/unterminated `transpose(...)`.
- **E1913** — `transpose` applied to a frame shape that cannot be transposed
  (1-D frame, non-2-D blend, or a spatial/geometry frame), mirroring the polar
  `E1904` shape rules.

## v0.33.0 Must

### 1. Frame operator parsing + lowering
Status: Planned.
- Extend frame-algebra parsing in `crates/algraf-syntax` to accept a
  function-call node (`transpose(...)`) in frame position, with resilient
  recovery (E1912) and correct spans for the operator, parens, and inner frame.
- Lower in `analyzer/frames.rs`: validate the 2-D cross-frame shape (E1913),
  fold nested/identity transposes, and set `transpose: bool` on the
  `Cartesian` representation of `CoordsIr`
  (`crates/algraf-semantics/src/ir.rs`). Disambiguate the operator from a
  column named `transpose` (only an immediately-following `(` makes it an
  operator; backtick-quoting forces a column).
- Reject `transpose` + `coords: "polar"` on one space with E1911.
- Cartesian output is byte-identical when `transpose` is absent.

### 2. Orientation-aware position + guides
Status: Planned.
- The trained space exposes which axis is the "position/category" axis and
  which is the "value" axis; `space.rs` builds scales accordingly.
- `guide/emit.rs` draws the band axis and the continuous axis on the correct
  edges, with correct tick-label sizing and margin reservation.

### 3. Orientation-aware stat geometries
Status: Planned.
- `Bar` (incl. stack/fill/dodge), `Histogram`, `Boxplot`, `Violin`, and `Rug`
  render correctly when transposed; the "requires categorical x" guards become
  "requires categorical position axis."

### 4. Spec, examples, README, release hygiene
Status: Planned.
- Spec §4.2 / §16.16 (or a new §16.x) describe the Cartesian transpose
  transform with `MUST`/`SHOULD` language and the reserved diagnostics.
- Add examples: a horizontal bar chart, a horizontal stacked bar, and a
  horizontal boxplot. Each gets a README section in the layouts/derived
  progression. Run `./examples/generate.sh`; confirm no drift in untouched
  examples.
- Bump workspace `Cargo.toml` and `editors/vscode/package.json` to `0.33.0`.
- Update the VS Code TextMate grammar to highlight `transpose` as a frame
  operator, and add LSP completion/hover/semantic-token support for it.

## v0.33.0 Should

- **Horizontal-orientation legend/axis-title polish**: ensure rotated layouts
  keep titles and legends readable (no overlap), reusing the v0.30 declutter
  machinery where applicable.
- **`coord_flip` parity check**: a doc note mapping Algraf transpose semantics
  to ggplot2 `coord_flip()` for users migrating mental models.

## Explicitly Deferred Past v0.33.0

- Arbitrary (non-axis-swap) Cartesian rotation.
- A separate Cartesian `reflect` operator (already covered by
  `Scale(reverse: true)`).
- Additional frame operators beyond `transpose` (e.g. a future `flip`/`rotate`
  family) — the parser is built to extend, but only `transpose` ships here.
- Transpose under polar coordinates.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples   # untouched examples must not drift
```

## Promotion Workflow

1. Reserve E1911–E1913 in the spec; write the normative `transpose`
   frame-operator section (grammar, shape rules, lowering) before coding.
2. Add the frame-operator parse node in `algraf-syntax` with resilient
   recovery and span coverage.
3. Lower it in `analyzer/frames.rs`, set the `CoordsIr` transpose flag, and
   emit the incompatibility/shape diagnostics (Must §1).
4. Make the trained space and guides orientation-aware (Must §2).
5. Make the stat geometries orientation-aware (Must §3).
6. Add examples, README, LSP metadata, grammar; bump versions; confirm static
   examples have no drift (Must §4).
