# Algraf v0.78.0 Plan

Status: Implemented
Target version: 0.78.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_77_PLAN.md`](V0_77_PLAN.md)
Roadmap theme: Overlaid spaces train one shared position-scale domain by
default, including the zero-baseline padding decision.
Cross-repo coordination: none required to ship 0.78.0.

## Purpose

Algraf v0.78 closes a gap in the spec §17.5 shared-position-scale union. Since
0.6.0 the renderer unions continuous and temporal min/max extents across
compatible overlaid spaces, so a secondary layer backed by a named `Table`
aligns with the primary layer. But the union carried only the extents — not the
zero-baseline requirement that geometries like `Bar` and zero-baseline `Area`
impose on domain padding.

The motivating case is the Social API monthly additions/deletions chart: a
stacked `Area` from the primary table overlaid with a cloc code-lines `Line`
from a named table:

```ag
Space(plot_month * lines) {
    Area(fill: change_segment, alpha: 0.76, layout: "stack")
}

Space(snapshot_date * code_lines, data: cloc) {
    Line(stroke: "#111111", strokeWidth: 1.2)
    Point(fill: "#111111", size: 1.6)
}
```

Both spaces agreed on the unioned y min/max, but only the area space carried
the zero baseline. The area space therefore trained `[0, max + pad]` (no
padding below a zero baseline) while the line space trained
`[-pad, max + pad]` (ordinary symmetric padding). The drawn axis came from the
zero-pinned space, so the line layer rendered shifted down by the padding
amount — and authors had to paper over it with a manual
`Scale(axis: y, domain: [0, 160000])` discovered by observation.

## Release Thesis

Compatible overlaid spaces share one position scale; that is only true if they
train *identical* domains, not merely overlapping extents. Every input to
domain resolution that can change the trained result — including the
zero-baseline padding decision — must be shared across the overlay. The common
case must be author-free: no manual `Scale(domain:)` should be needed just to
keep two overlaid layers on one scale.

An explicit chart-level `Scale(axis: …, domain: […])` keeps working as before:
chart-level scale configs merge into every space, so the override applies to
the joined scale in all overlaid spaces at once.

## Must

- When any compatible overlaid space requires a numeric axis domain to include
  zero, every compatible overlaid space sharing that axis adopts the
  requirement, so all spaces resolve identical padding and train identical
  domains. Spaces with locked bounds (a `fill`-layout bar, a `Rect`) keep their
  exact values and do not adopt the requirement.
  Status: Implemented. `AxisExtent` in `render/panels.rs` unions the
  `include_zero` hint across spaces and propagates it through
  `AxisDomainHints::merge_include_zero`, which skips locked bounds.

- Spec §17.5 documents the shared zero-baseline requirement as a 0.78.0
  normative note.
  Status: Implemented.

- A renderer regression test proves the secondary layer's marks land exactly on
  the primary layer's geometry when both plot the same values.
  Status: Implemented. `test_named_table_overlay_shares_zero_baseline` in
  `crates/algraf-render/tests/render.rs`; it fails without the renderer change.

- An example exercises the default: a stacked area overlaid with a named-table
  reference line, no manual y domain.
  Status: Implemented. `examples/stacked_area_capacity_line.ag` with
  `examples/fleet_capacity.csv`, wired into `examples/generate.sh` and the
  README tutorial.

## Deferred

- Sharing locked bounds themselves (e.g. aligning a `fill`-layout bar space
  with an overlaid unlocked space) stays out of scope; the 0.6.0 rule that a
  locked bound is never widened by the union is unchanged.
- Space-local `Scale` declarations remain local to their space; only
  chart-level scales merge into every overlaid space.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Additional release validation:

- Render the motivating chart without `Scale(axis: y, domain: [0, 160000])` and
  confirm the cloc line lands on the same y scale as the stacked area.
- Inspect `examples/stacked_area_capacity_line.png` and confirm the dashed
  capacity line and the stack share one zero-based y axis.

## Promotion Workflow

When implemented:

1. Update `ALGRAF_SPEC.md` §17.5 shared-position-scale behavior.
2. Update `Status:` lines in this document as each item lands.
3. Add the focused renderer test before regenerating examples.
4. Regenerate and inspect affected examples.
5. Run the validation commands listed above.
6. Bump release version stamps to `0.78.0` only when this release plan ships,
   following the repository's version-promotion requirements.
7. Begin `V0_79_PLAN.md` for the next release scope.
