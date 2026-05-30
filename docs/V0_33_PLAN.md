# Algraf v0.33.0 Plan

Status: Implemented (release version bump deferred to cut)
Chosen syntax: **A3 — algebraic frame operator** (`Space(transpose(a * b))`).
Chosen semantics: **B1 — orientation-aware geometries.**
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_32_PLAN.md`](V0_32_PLAN.md)
Follow-on plan: [`V0_34_PLAN.md`](V0_34_PLAN.md)
Coordinate-system foundation: [`V0_31_PLAN.md`](V0_31_PLAN.md) (polar
`startAngle` / `direction`)

## Purpose

This release adds a **Cartesian frame operator** — transposing the x and y axes
— so that the orientation-locked statistical geometries (`Bar`, `Boxplot`,
`Violin`) can render horizontally. Before this, each hard-required a categorical
(band) x axis and a continuous y axis, so horizontal bar/box/violin charts were
not expressible with the high-level geoms. Direct 1-D `Histogram` and
`Bar(stat: "count")` transposition remains deferred because those geoms desugar
before a 2-D value axis exists.

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

v0.33.0 is the **transpose** release: a single algebraic frame operator that
swaps the two axes of a 2-D Cartesian frame, making horizontal categorical/stat
charts first-class. The implementation keeps the swap small: analysis rewrites
`transpose(a * b)` to the existing physical frame shape `b * a`, and the few
orientation-locked geoms detect whether the categorical position axis is x or y.
There is no renderer-wide transpose flag and no WASM-specific change.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
is updated with concrete `MUST` / `SHOULD` / `MUST NOT` language and a stable
diagnostic code.

## Chosen design

### Syntax: A3 — algebraic frame operator

```ag
Chart(data: "sales_by_rep.csv", width: 720, height: 440) {
    Space(transpose(rep * amount)) {
        Bar(fill: rep)
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
the analyzer resolves it by swapping the two `FrameIr::Cartesian` axes before
scale training. `Scale(axis: ...)` and `Guide(axis: ...)` therefore address the
physical axes after rewrite.

### Semantics: B1 — targeted orientation-aware geometries

The transform itself is an analyzer rewrite. Each orientation-locked geom checks
which physical axis is the band (position) axis vs. the continuous value axis;
stacking accumulates along that value axis. The authored algebra inside
`transpose(…)` stays `category * value`.

- This matches Algraf's architecture, where geoms emit pixels directly. The
  existing guards (`if !space.x.is_band()`) become "band on the position
  axis," a localized edit per geom.
- The scope is intentionally limited to the geoms that already require a
  categorical position axis.

> Rejected alternative — **post-hoc pixel reflection across the y = x diagonal**:
> render normally then mirror emitted coordinates. Incompatible with a direct
> pixel renderer (text renders mirrored; bar widths, tick labels, and stroke
> widths are computed in pixel space and would distort; band geoms still reject
> categorical-on-y up front).

## Scope — what "orientation-aware" touches (under B1)

The transform itself is small; the work is localized to the geoms whose stat has
a fixed orientation today. Confirmed orientation guards:

| Geometry  | Current guard (file)                                      |
| --------- | --------------------------------------------------------- |
| `Bar`     | `geom/bar.rs:58` — "Bar requires a categorical x dimension" |
| `Boxplot` | `geom/distribution.rs:33` — "categorical x and continuous y" |
| `Violin`  | `geom/distribution.rs:214` — "categorical x and continuous y" |

Also in scope:

- **Stacking / nesting** (`Bar(layout: "stack" | "fill")` and algebraic
  `quarter / type` dodging): must
  accumulate along the value axis, not always y.
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
Status: Implemented.
- Extend frame-algebra parsing in `crates/algraf-syntax` to accept a
  function-call node (`transpose(...)`) in frame position, with resilient
  recovery (E1912) and correct spans for the operator, parens, and inner frame.
- Lower in `analyzer/frames.rs`: validate the 2-D cross-frame shape (E1913),
  swap the two `FrameIr::Cartesian` axes before render, and disambiguate the
  operator from a column named `transpose` (only an immediately-following `(`
  makes it an operator; backtick-quoting forces a column).
- Reject `transpose` + `coords: "polar"` on one space with E1911.
- Cartesian output is byte-identical when `transpose` is absent.

### 2. Orientation-aware position scales
Status: Implemented.
- `Bar`, `Boxplot`, and `Violin` derive the "position/category" axis and the
  "value" axis from the existing trained physical axes.
- Guides intentionally stay physical: after analysis rewrites the frame, x-axis
  guides render along the bottom and y-axis guides render along the left.

### 3. Orientation-aware stat geometries
Status: Implemented for 2-D categorical/value frames; direct 1-D stat
transposition remains deferred.
- `Bar` (identity/stack/fill, including algebraic nested/dodged bars),
  `Boxplot`, and `Violin` render correctly when transposed; the "requires
  categorical x" guards are now "requires a categorical position axis."
- Direct 1-D `Histogram` and `Bar(stat: "count")` transpose are deferred because
  they desugar before a 2-D value axis exists.

### 4. Spec, examples, README, release hygiene
Status: Implemented except release-version bump.
- Spec §4.2 / §16.16 (or a new §16.x) describe the Cartesian transpose
  transform with `MUST`/`SHOULD` language and the reserved diagnostics.
- Add examples: a horizontal bar chart, grouped bar, stacked bar, boxplot, and
  violin. Each gets a README section in the layouts/distribution
  progression. Run `./examples/generate.sh`; confirm no drift in untouched
  examples.
- Bump workspace `Cargo.toml` and `editors/vscode/package.json` to `0.33.0`
  when cutting the release.
- Update the VS Code TextMate grammar to highlight `transpose` as a frame
  operator, add LSP completion/hover/semantic-token support for it, and keep the
  demo editor theme in sync.

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
- Direct 1-D stat transposition for `Histogram` and `Bar(stat: "count")`.

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
3. Lower it in `analyzer/frames.rs` by swapping the two Cartesian frame axes,
   and emit the incompatibility/shape diagnostics (Must §1).
4. Make the targeted stat geometries detect categorical/value orientation
   from the trained physical axes (Must §2).
5. Make the stat geometries orientation-aware (Must §3).
6. Add examples, README, LSP metadata, and grammar; leave release-version bumps
   to the release cut; confirm static examples have no drift (Must §4).
