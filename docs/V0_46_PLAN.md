# Algraf v0.46.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_45_PLAN.md`](V0_45_PLAN.md)
Roadmap theme: replace `transpose(...)` with a physical-axis orientation model.

## Purpose

This release corrects the orientation model introduced in v0.33.0.
`transpose(...)` made horizontal charts possible, but it did so by adding a
prefix frame operator that rewrites `Space(transpose(a * b))` as though the user
had written `Space(b * a)`. That rewrite leaks into authoring: `Scale(axis: x)`
and `Guide(axis: x)` target the post-rewrite physical axis, while the source
still reads as though the author mapped `a` first. The result is a permanent
mental inversion at exactly the point where a grammar-of-graphics language
should be most declarative.

v0.46.0 replaces that model with an explicit physical-axis contract:

- the left operand of a Cartesian frame is the physical x axis;
- the right operand of a Cartesian frame is the physical y axis;
- horizontal two-axis charts are authored by writing the value column on x and
  the category/group column on y;
- `orientation` belongs only to high-level marks or stats that synthesize a
  missing positional axis.

The release must remove the old syntax and update existing docs, examples,
tests, editor affordances, and diagnostics without creating a second hidden-axis
mechanism.

## Release Thesis

v0.46.0 is the **physical orientation** release. Its success criterion is that a
reader can determine the screen axes from the `Space(...)` expression alone:
`Space(a * b)` means x is `a` and y is `b`, with no macro rewrite, no
compatibility lowering, no logical axis indirection, and no renderer-wide
transpose flag.

The replacement is intentionally conservative. Existing two-axis horizontal
geometries such as bars, boxplots, violins, lollipops, and faceted variants
should use the physical frame order directly. One-dimensional generated-axis
geometries such as histograms and frequency polygons may gain an `orientation`
argument because they invent the count/density axis themselves. General
`Space(orientation: ...)` is out of scope because it would recreate the same
axis-targeting confusion under a different spelling.

## Scope Rules

- Source frame order is the coordinate contract: `Space(x * y)` maps x to the
  physical horizontal axis and y to the physical vertical axis.
- `Scale(axis: x)`, `Guide(axis: x)`, interaction metadata, draw-list axes, and
  renderer internals all target physical axes. The implementation MUST NOT add
  logical/pre-transpose axis selectors.
- Do not add `orientation` to `Space`, `Layout`, or general Cartesian algebra.
  Orientation is a mark/stat-level concern only when the mark/stat synthesizes a
  positional dimension that does not appear in source algebra.
- Keep existing two-dimensional categorical/value geometry inference working in
  both physical orders: categorical x plus numeric y remains vertical, numeric x
  plus categorical y remains horizontal.
- Remove `transpose(...)` as accepted frame syntax in v0.46.0. Old source should
  produce an error with rewrite help when the equivalent physical frame can be
  determined; it should not render through a compatibility path.
- Do not change blend, cross, nest, facet, polar, inset, scale, or guide
  semantics beyond the explicit transpose removal.
- Do not make `orientation` a synonym for swapping axes in two-dimensional
  spaces. Authors who have two physical axes should write the physical order
  they want.
- Keep generated examples visually equivalent after rewrite. Any changed
  example output must be inspected as an intentional orientation-preserving
  rewrite, not accepted as a byte diff alone.

## Current Problem Audit

| Area | Current shape | Problem | v0.46.0 direction |
| ---- | ------------- | ------- | ----------------- |
| Frame algebra | `transpose(F)` is a prefix frame operator | A procedural layout rewrite lives inside declarative algebra. | Remove the operator and make Cartesian operand order physical. |
| Axis selectors | `Scale(axis: ...)` and `Guide(axis: ...)` target post-rewrite axes | Authors must mentally invert axis rules when reading transposed source. | Axis selectors always target the physical screen axis named in source. |
| Examples | Horizontal examples use `transpose(category * value)` | The tutorial teaches the workaround as the primary orientation model. | Rewrite examples to `Space(value * category)` and explain physical order. |
| Editor services | Completion, hover, and semantic tokens promote `transpose` as a frame operator | New users discover the legacy mechanism before the physical-axis model. | Remove it from positive discovery and replace with removal diagnostics/actions. |
| Diagnostics | `E1911`-`E1913` describe malformed transpose use | Erroring on only bad transpose keeps the operator alive as a normal feature. | Replace transpose-specific diagnostics with a hard-removal error and rewrite help where possible. |
| Generated-axis geoms | One-dimensional stats generate a second axis implicitly | Horizontal histograms/frequency polygons need a real orientation knob. | Add `orientation` only to the specific marks/stats that synthesize axes. |
| Tests | Horizontal coverage is named and authored through transpose | Regression tests protect the old design. | Rewrite tests around physical frames and keep rejection/rewrite tests for removed source. |

## Target Semantics

### Physical Cartesian axes

The normative rule after v0.46.0 should be:

- `Space(a)` trains a one-dimensional physical x axis.
- `Space(a * b)` trains physical x from `a` and physical y from `b`.
- `Space((a * b) / group)` facets the physical plane `a * b`; the axes inside
  each panel remain physical x=`a`, physical y=`b`.
- A horizontal categorical/value chart is authored by putting the value on x and
  the category on y.

Current replacement recipes that already fit this model:

```text
Chart(data: "sales_by_rep.csv", width: 720, height: 440, title: "Sales by rep") {
    Guide(axis: x, label: "Sales")
    Guide(axis: y, label: "Rep")
    Space(amount * rep) {
        Bar(fill: "#4E79A7", alpha: 0.86)
    }
}
```

```text
Chart(data: "financials.csv", width: 760, height: 460, title: "Quarterly amount by type") {
    Guide(axis: x, label: "Amount")
    Guide(axis: y, label: "Quarter")
    Space(amount * (quarter / type)) {
        Bar(fill: type, alpha: 0.88)
    }
}
```

These are the shapes the tutorial should teach instead of a frame operator.

### Generated-axis orientation

Some high-level marks and stats consume one source dimension and synthesize the
other positional dimension. For those cases, v0.46.0 should introduce an
explicit `orientation` argument with values `"vertical"` and `"horizontal"`.

Required generated-axis targets:

- `Histogram`
- `FreqPoly`
- `Density`, if the existing area/path desugaring can support the same physical
  contract without a larger renderer change

Semantics:

- vertical orientation maps the input/bin axis to physical x and the generated
  count/density axis to physical y;
- horizontal orientation maps the generated count/density axis to physical x and
  the input/bin axis to physical y;
- generated axis labels, scale domains, guide diagnostics, metadata axes, and
  interaction sidecar fields must all use the same physical axes that emission
  uses;
- if a generated-axis mark is placed in a two-dimensional frame, the physical
  frame order wins and `orientation` must either be rejected as redundant or
  ignored with a diagnostic; choose one behavior in the spec before
  implementation.

### Two-axis geometry orientation

For geometries that already have a two-dimensional frame, orientation is inferred
from physical axis types and mappings:

- categorical x plus numeric y is vertical;
- numeric x plus categorical y is horizontal;
- categorical nesting remains a band/sub-band structure on whichever physical
  axis contains the nested expression;
- `Scale(axis: x, reverse: true)` reverses the physical x axis regardless of
  whether the resulting chart is visually horizontal or vertical.

No `orientation` argument should be added to `Bar`, `Boxplot`, `Violin`,
`LineRange`, `PointRange`, `Ribbon`, or similar two-axis geometries as part of
this release unless an existing geometry already has one for a different,
non-transpose reason.

### Removed transpose

`transpose(...)` is removed source syntax in v0.46.0.

Removal behavior should be deliberately simple:

- `transpose(a * b)` MUST NOT lower to `b * a`.
- Old source that uses `transpose(...)` must produce an error and no rendered
  output unless the caller explicitly chooses a non-blocking diagnostic mode.
- Diagnostics should explain that Cartesian frame order is physical and, when
  the operand is a valid two-axis frame, include the replacement frame in help
  text.
- LSP code actions may still rewrite old source mechanically:
  `transpose((a * b)) / group` becomes `(b * a) / group`.
- A bare `transpose` and quoted `` `transpose` `` remain ordinary column names
  where the grammar permits identifiers.
- New code generated by examples, snippets, code actions, or docs must never
  introduce `transpose(...)`.

## v0.46.0 Must

### 1. Update the spec to make axes physical

Status: Implemented.

- Rewrite the Cartesian algebra sections that currently describe `transpose` so
  the normative source contract is physical axis order.
- Remove `transpose(...)` from normative frame algebra rather than keeping any
  transitional syntax.
- Reserve or reassign an error diagnostic for removed transpose usage before
  emitting it. Do not add a warning-only transition.
- Reserve a code-action/hint code for the mechanical rewrite if the removal
  error is not itself actionable.
- Update scale, guide, metadata, and draw-list sections to say that axis names
  are physical and never pre-rewrite logical axes.
- Update the milestone table to record v0.46.0 as the physical-axis hard-removal
  release.

### 2. Add generated-axis `orientation` where needed

Status: Implemented.

- Add `orientation` parsing, validation, IR storage, and registry metadata for
  `Histogram` and `FreqPoly`.
- Add `orientation` to `Density` only if the desugaring and renderer can support
  it without inventing a second orientation model.
- Default `orientation` to `"vertical"` for backward compatibility.
- Ensure generated columns, guide labels, scale training, metadata, SVG,
  draw-list JSON, and raster output use the same physical axis assignment.
- Add tests for vertical default behavior and horizontal generated-axis behavior.
- Reject or warn on redundant `orientation` in a two-dimensional frame according
  to the spec decision made in Must 1.

### 3. Remove `transpose(...)` in analyzer and editor services

Status: Implemented.

- Delete compatibility lowering for valid `transpose(a * b)` forms.
- Emit the removed-transpose error on the operator span, with help text that
  prefers physical frame order.
- Add an LSP code action that rewrites valid transpose calls to the equivalent
  physical frame order while preserving surrounding nesting/faceting.
- Remove `transpose` from normal completions in `Space(...)` argument position.
- Remove frame-operator hover text for `transpose`; if the old call is present,
  diagnostics and code actions carry the rewrite guidance.
- Keep semantic token handling deterministic. `transpose` should not be promoted
  as preferred operator syntax.
- Add parser/analyzer/editor tests for valid rewrites, malformed calls,
  faceted forms, quoted `` `transpose` `` column names, and nested expressions.

### 4. Rewrite horizontal examples and tutorial docs

Status: Implemented.

- Rewrite every checked-in tutorial or example chart that uses
  `transpose(...)` to the physical-axis equivalent.
- Update the README orientation section to teach physical frame order first:
  use value-on-x/category-on-y for horizontal charts.
- Remove `transpose(...)` from README prose except in a short migration note.
- Update example names and comments only when the old wording explicitly refers
  to transposition rather than horizontal orientation.
- Run `./examples/generate.sh`, inspect changed rendered PNGs, and confirm that
  the visual orientation and mark positions match the pre-migration examples.

### 5. Rewrite tests around physical frames

Status: Implemented.

- Replace horizontal renderer tests authored with `transpose(...)` by tests that
  author physical horizontal frames directly.
- Add tests showing that old `transpose(...)` source is rejected and that the
  code-action rewrite renders the same SVG, draw-list, and metadata as the old
  intended output.
- Update semantic analysis tests so `Space(value * category)` is the primary
  accepted horizontal form.
- Keep tests for malformed transpose only as removal/recovery tests, not as
  evidence that transpose is a supported design target.

### 6. Remove positive product surface for frame operators

Status: Implemented.

- Remove `transpose` from registry snippets, completions, hover examples,
  tutorial text, and generated docs.
- Rename internal helper comments where they describe `transpose` as a normal
  frame operator.
- Remove parser CST names for frame-operator calls unless needed for resilient
  error recovery.
- Leave completed release plans as historical records; put migration notes in
  the spec, README, and current release plan instead.

### 7. Add local image marks

Status: Implemented.

This promoted feature adds a point-like `Image(...)` geometry for local raster
and SVG assets such as company logos and team logos. It is not part of the
physical-orientation thesis, but it landed in the active release window and is
therefore documented here to keep the spec, plan, examples, and implementation
in sync.

- Add `Image(src: ...)` to the geometry registry and editor grammar.
- Require `src`; accept either a local path string literal or a string column
  mapping. Reject URL-like sources with `E1204`.
- Load local `.png`, `.jpg`, `.jpeg`, `.gif`, and `.svg` assets through the
  existing `DriverIo` path boundary, then embed them as Algraf-generated
  `data:image/...` hrefs in SVG/draw-list output.
- Render images centered at point-like positions with aspect-preserving `size`,
  `alpha`, `jitter`, `nudge`, and `nudgeData` support.
- Emit image legends for mapped `src` values; constant `src` produces no
  legend.
- Include image marks in declarative interaction metadata.
- Add focused semantic/render tests and a checked-in example with README
  tutorial coverage.

## v0.46.0 Should

### Mechanical formatter assistance

Status: Implemented.

- Consider a formatter-preserving rewrite path for `transpose(a * b)` that the
  LSP code action and CLI format command can share.
- The formatter should not silently rewrite source without an explicit user
  action in v0.46.0.
  The shipped rewrite is an explicit LSP quick fix; the CLI formatter remains
  non-mutating for removed syntax.

### Removal diagnostic polish

Status: Implemented.

- Make CLI human diagnostics include the replacement frame in the help text when
  the operand is a simple two-axis Cartesian frame.
- JSON/LSP diagnostics should include stable diagnostic codes and spans suitable
  for one-click migration.

### Generated-axis orientation examples

Status: Implemented.

- Add a compact checked-in example for horizontal generated-axis output only
  after `orientation` has landed and the example can run from checked-in data.
- Place it in the README near the existing histogram/frequency polygon material
  rather than near two-axis horizontal bars.

## Explicitly Deferred Past v0.46.0

- A general `Space(orientation: "horizontal")` or `Layout(orientation: ...)`
  argument.
- Logical axis selectors such as `Scale(axis: value)` or axis selectors that
  target pre-rewrite source operands.
- General frame operator calls.
- Reopening blend precedence or the explicit-parentheses blend rule.
- Changing polar orientation semantics, radial/theta mapping, or polar
  diagnostics as part of this work.
- Adding `orientation` to every geometry. Two-axis geometries should use
  physical frame order.
- Any runtime or renderer backend rewrite motivated solely by the source syntax
  migration.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

For any example rewrite, inspect the regenerated PNGs before closing the item.
The expected visual result is orientation-preserving: horizontal examples remain
horizontal, categorical order remains stable, value baselines remain correct,
legends and guides still target the same physical screen axes, and metadata
axis ranges match the rendered output.

Focused checks should also cover:

- semantic diagnostics for removed and malformed transpose usage;
- LSP code action edits for simple, nested, and faceted transpose forms;
- horizontal `Bar`, grouped `Bar`, stacked/fill `Bar`, `Boxplot`, and `Violin`
  authored through physical frame order;
- generated-axis horizontal `Histogram` and `FreqPoly` after `orientation`
  lands;
- quoted `` `transpose` `` column references, which must remain ordinary column
  names.

## Promotion Workflow

1. Update the spec first: physical axis contract, removed transpose wording,
   generated-axis `orientation` rules, and any new diagnostic codes.
2. Remove compatibility lowering and implement the removed-transpose error.
3. Add the LSP rewrite action and remove `transpose` from positive editor
   discovery.
4. Implement generated-axis `orientation` for the approved mark/stat set.
5. Rewrite tests from transpose-authored horizontal charts to physical
   frame-order charts; keep rejection and rewrite coverage for removed source.
6. Rewrite examples and README tutorial sections, regenerate examples, and
   inspect changed PNGs.
7. Leave older release plans as historical records; add migration notes to the
   spec and README rather than reopening completed release scope.
8. Do not close v0.46.0 while any checked-in tutorial example still teaches
   `transpose(...)` as the normal way to make a horizontal chart.
