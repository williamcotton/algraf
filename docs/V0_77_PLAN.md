# Algraf v0.77.0 Plan

Status: Implemented
Target version: 0.77.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_76_PLAN.md`](V0_76_PLAN.md)
Roadmap theme: Stacked legend ordering follows the rendered visual stack.
Cross-repo coordination: none required to ship 0.77.0.

## Purpose

Algraf v0.77 fixes the default legend order for stacked marks. When a mark uses
a stacked layout, the legend should read in the same visual order as the stack
appears in the rendered chart, rather than exposing the raw categorical scale
domain or the internal accumulation sequence.

The motivating case is a vertical positive stack where the renderer may need to
accumulate `deletions` first and `additions` second so additions render on top:

```ag
Space(day * count) {
    Bar(fill: change_segment, layout: "stack")
}
```

In that chart, the default legend should read top-to-bottom like the rendered
stack:

```text
Additions
Deletions
```

not baseline-outward if that would show `Deletions` before `Additions`.

The concrete downstream failure is the Social API monthly additions/deletions
chart. Its manual fill range deliberately binds colors in baseline-outward
stack order:

```ag
Scale(
    fill: change_segment,
    range: [
        "Before Apr 7 deletions" => "#8ecae6",
        "Before Apr 7 additions" => "#1f77b4",
        "After Apr 7 deletions" => "#ffbf69",
        "After Apr 7 additions" => "#ff7f0e"
    ],
    label: "Lines"
)

Space(plot_month * lines) {
    Area(fill: change_segment, alpha: 0.76, layout: "stack")
}
```

The current legend follows the manual range/domain order:

```text
Before Apr 7 deletions
Before Apr 7 additions
After Apr 7 deletions
After Apr 7 additions
```

The desired default keeps the `Before` cohort before the `After` cohort, but
orders each visible stack cohort top-to-bottom:

```text
Before Apr 7 additions
Before Apr 7 deletions
After Apr 7 additions
After Apr 7 deletions
```

This distinction matters: the implementation must not blindly reverse the full
domain when a stacked chart has disjoint cohorts that do not visibly stack
together.

## Release Thesis

Stack accumulation order is a geometry-placement detail. Legend order is a guide
presentation detail. For stacked layouts, Algraf's default guide presentation
should follow the reader's visual scan of the rendered stack.

This keeps the common case author-free: chart authors should not need a guide
override just to make a stacked legend match the chart's visible bands.

## Current Baseline

The renderer currently builds discrete color legends from the trained aesthetic
scale categories. That is correct for ordinary non-stacked marks because the
domain is the user's stable category order and drives color assignment.

Stacked marks add another ordering layer:

- Bar stack/fill layouts accumulate segments from the baseline outward.
- Area stack/fill layouts accumulate groups from the baseline outward across x
  positions.
- Grouped Histogram stacked output is equivalent to pre-stacked Rect rows with
  `stack_lower` and `stack_upper` bounds.

Those stack orders can differ from the order a reader sees. In a vertical
positive stack, the last outward segment is visually highest, so the legend
should normally list that segment first.

Charts can also contain multiple visible stack cohorts under one aesthetic
domain. In the Social API chart, `Before Apr 7 ...` and `After Apr 7 ...`
entries never visibly contribute to the same month stack. The default legend
should preserve the cohort ordering implied by the scale/domain while applying
visual stack ordering inside each cohort.

## Proposed Spec Changes

Update the normative guide and stacked-layout sections in `ALGRAF_SPEC.md`.

- For Bar, Area, and other stack/fill layouts that produce visibly stacked
  bands, stack accumulation order controls geometry placement from the baseline
  outward.
- For those same layouts, the default legend order for the stacked aesthetic
  follows rendered visual stack order.
- For vertical positive stacks, rendered visual stack order is top-to-bottom.
- For horizontal positive stacks that grow rightward, rendered visual stack
  order is right-to-left.
- Where negative stacks are supported, the positive and negative sides each
  follow their own outward visual order. The implementation should keep the
  side ordering deterministic and documented.
- When a single categorical domain contains multiple disjoint visible stack
  cohorts, each cohort follows its own rendered visual order and cohorts remain
  ordered by their earliest category in scale/domain order.
- Legend ordering must be derived from visible stack contributions, not from
  zero-height placeholder cells introduced for sparse stacked areas.
- Non-stacked legends continue to use scale/domain order.
- Manual color assignments continue to bind colors to categories by scale
  domain. Reordering a legend must not change the color assigned to any mark.

The spec should avoid adding new author syntax in v0.77. This release changes
only the default for stacked legends.

## Scope

### Render Model

Status: Implemented.

Add a render-side way for stack-capable geometries to report the visual legend
entry order for the categorical aesthetic that forms the stack. The mechanism
should be local to rendering and guides; it should not mutate trained scale
domains, color assignment, interaction group domains, or source table order.

Acceptance criteria:

- A stacked vertical `Bar(fill: category, layout: "stack")` legend lists the
  top visual band first for positive stacks.
- A stacked horizontal Bar legend lists the farthest right visual band first
  when the positive stack grows rightward.
- `Area(fill: category, layout: "stack")` and
  `Area(fill: category, layout: "fill")` legends use the same visual ordering
  rule.
- A stacked Area with disjoint `Before deletions`/`Before additions` and
  `After deletions`/`After additions` cohorts preserves the `Before` then
  `After` cohort order while listing additions above deletions within each
  cohort.
- Grouped Histogram stacked legends use the visual stack order of the generated
  stacked bars.
- The same category keeps the same color before and after legend reordering.
- The implementation does not reverse the entire domain as a shortcut.
- Rendering is deterministic across repeated runs.

### Legend Collection

Status: Implemented.

Teach legend collection to prefer a stack visual-order hint when a discrete
fill or stroke legend comes from a stacked geometry. When no stack hint exists,
or when the mapped aesthetic is not the stacked grouping aesthetic, preserve the
existing scale/domain order.

Acceptance criteria:

- Fill/stroke legend merging still works only when labels match in the same
  final displayed order.
- Shape legends folded into color legends remain aligned after reordering.
- Continuous, binned, size, image, and identity-color legend behavior is
  unchanged.
- `Guide(fill: null)`, `Guide(stroke: null)`, and `Guide(legend: false)` keep
  their existing suppression behavior.

### Tests and Examples

Status: Implemented.

Add focused regression coverage for the default ordering rule.

Acceptance criteria:

- Renderer tests cover vertical positive stacked Bar ordering.
- Renderer tests cover horizontal positive stacked Bar ordering.
- Renderer tests cover stacked Area ordering.
- Renderer tests cover a manual `Scale(fill: ..., range: [...])` map whose color
  binding order differs from the displayed stacked legend order.
- Renderer tests cover disjoint visible stack cohorts so a full-domain reverse
  cannot pass by accident.
- A grouped Histogram test covers the desugared/pre-stacked path if the
  implementation routes it separately from Bar.
- At least one example SVG change is inspected when an existing example's legend
  order changes.

## Deferred Syntax

Status: Deferred.

Do not add guide-order overrides in v0.77. Keep these as future design options
only if real author needs appear after the default is fixed:

```ag
Guide(fillOrder: "scale")
Guide(fillOrder: "stack")
Guide(fillOrder: "reverse-stack")
```

Possible future generalizations:

```ag
Guide(strokeOrder: "scale")
Guide(shapeOrder: "scale")
Area(fill: change_segment, layout: "stack", stackOrder: "scale")
```

The v0.77 default should make these unnecessary for ordinary stacked charts.

## Non-Goals

- No new `.ag` syntax.
- No new scale target, palette, or guide property.
- No change to categorical color assignment.
- No change to raw scale/domain order for non-stacked marks.
- No change to geometry stacking math beyond what is needed to expose visual
  stack order to legends.
- No package publication or npm dependency pin change.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Additional release validation:

- Regenerate affected examples with `./examples/generate.sh`.
- Inspect changed stacked chart PNG/SVG output and confirm the legend reads in
  the same visual order as the rendered stack.
- Confirm interaction metadata and color domains remain stable unless a test
  intentionally documents a guide-only ordering change.

## Promotion Workflow

When implemented:

1. Update `ALGRAF_SPEC.md` §14 stacked-layout behavior and §19 legend behavior.
2. Update `Status:` lines in this document as each item lands.
3. Add focused renderer tests before changing examples broadly.
4. Regenerate and inspect affected examples.
5. Run the validation commands listed above.
6. Bump release version stamps to `0.77.0` only when this release plan ships,
   following the repository's version-promotion requirements.
7. Begin `V0_78_PLAN.md` for the next release scope.
