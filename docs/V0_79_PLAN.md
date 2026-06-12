# Algraf v0.79.0 Plan

Status: Implemented
Target version: 0.79.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_78_PLAN.md`](V0_78_PLAN.md)
Roadmap theme: Measured legend layout reserve.
Cross-repo coordination: none required to ship 0.79.0. The browser packages
`algraf-wasm` and `algraf-editor` are not published at 0.79.0 implementation
time, so their package versions and consumer pins remain on the latest verified
published version, 0.75.0.

## Purpose

Algraf's right-side legend reserve used a fixed 120px slot. Charts with longer
legend labels, such as labels containing split-period prose, could render the
legend text beyond the SVG viewport unless authors added manual margins.

The v0.79.0 goal is to make legend layout author-free for ordinary labels:
after legends are collected, the final layout pass reserves enough side space
for the measured legend text using the renderer's deterministic text-width
approximation.

## Release Thesis

Legends are chart content, not decorative overflow. If Algraf can derive the
legend labels from trained scales, it can also reserve a deterministic content
box for them. Manual `marginRight` should remain available as a floor, but it
should not be required to keep generated legend labels visible.

## Must

- Right and left legends reserve width from the collected legend titles and
  entry labels before the final render plan is built.
  Status: Implemented. `guide::legend_size` measures vertical legend content
  and `Layout::compute_with_text_and_legend_size` uses that width for the final
  legend side reserve.

- Top and bottom legend reserves continue through the same measured-size path,
  using the existing compact legend wrapping model to estimate required height.
  Status: Implemented.

- Existing explicit chart margins remain floors when axes are present; measured
  legend reserve composes with those margins and is not clipped by a smaller
  configured margin.
  Status: Implemented. Covered by
  `test_long_right_legend_labels_reserve_width`.

- Spec §17.3 documents measured legend reserve as v0.79.0 behavior.
  Status: Implemented.

## Deferred

- Exact browser/font-engine text measurement remains deferred. The renderer
  continues to use its deterministic approximate text width, matching the axis
  margin path.
- General legend collision avoidance and multi-column vertical legends remain
  deferred.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Focused validation:

- `cargo test -p algraf-render test_long_right_legend_labels_reserve_width`

## Promotion Workflow

When implemented:

1. Update `ALGRAF_SPEC.md` §17.3 and the milestone table.
2. Add the focused renderer regression test.
3. Align Rust, spec, VS Code, and demo release version stamps to `0.79.0`;
   keep unpublished browser package pins on the latest verified npm version.
4. Run the validation commands listed above.
