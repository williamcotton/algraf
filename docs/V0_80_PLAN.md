# Algraf v0.80.0 Plan

Status: In progress
Target version: 0.80.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_79_PLAN.md`](V0_79_PLAN.md)
Roadmap theme: Default Cartesian plot clipping.
Cross-repo coordination: none required to ship 0.80.0. The browser packages
`algraf-wasm` and `algraf-editor` are not published at 0.80.0 implementation
time, so their package versions and consumer pins remain on the latest verified
published version, 0.75.0.

## Purpose

Explicit axis domains can intentionally exclude values from the visible y or x
range. Before this release, ordinary Cartesian panels still emitted unclipped
marks outside the plot rectangle unless the space also used `zoomX` or `zoomY`.
That made `Scale(axis: y, domain: [lo, hi])` set axis ticks correctly while
allowing areas, lines, and other marks to bleed below or above the panel.

The v0.80.0 goal is to make Cartesian plot clipping match author expectations:
scale mapping may place a primitive outside the panel, but the renderer masks
data-mark layers to the final plot rectangle by default.

## Release Thesis

Scale domains, coordinate zoom, and clipping stay distinct:

- `Scale(axis: ..., domain: ...)` trains and bounds the data domain.
- `Space(zoomX:/zoomY:)` changes the visual coordinate view without filtering
  rows.
- Cartesian data-mark clipping is the final renderer mask around the plot
  rectangle, applied after scale training and coordinate-view resolution.

This preserves row/stat semantics while preventing out-of-domain mark geometry
from bleeding into margins, legends, titles, or neighboring panels.

## Must

- Cartesian panels open a deterministic rectangular clip scope around data-mark
  layers by default, including panels with explicit axis domains and panels
  without coordinate zoom.
  Status: In progress.

- The same clip flag drives SVG, draw-list, raster, and interaction metadata so
  all render backends agree on visible plot bounds.
  Status: In progress.

- Glyph child panels use the same Cartesian default inside their own child plot
  rectangle.
  Status: In progress.

- Spec §16.11, §18.5, and §24.3 document that explicit scale domains do not
  filter rows, but Cartesian data marks are clipped to the final plot rectangle.
  Status: In progress.

## Deferred

- Circular or radial default clipping for polar panels remains deferred. Polar
  marks continue to use their coordinate transform and explicit glyph clips.
- Geometry-level opt-out for Cartesian panel clipping remains deferred.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Focused validation:

- `cargo test -p algraf-render explicit_axis_domain_clips_cartesian_marks_by_default`

## Promotion Workflow

When implemented:

1. Update `ALGRAF_SPEC.md` §16.11, §18.5, §24.3, and the milestone table.
2. Add focused renderer regression tests for explicit-domain clipping.
3. Align Rust, spec, VS Code, and demo release version stamps to `0.80.0`;
   keep unpublished browser package pins on the latest verified npm version.
4. Run the validation commands listed above.
