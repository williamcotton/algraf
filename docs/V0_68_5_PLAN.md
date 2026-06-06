# Algraf v0.68.5 Plan

Status: Implemented
Target version: 0.68.5
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_68_1_PLAN.md`](V0_68_1_PLAN.md)
Follow-on plan: [`V0_69_PLAN.md`](V0_69_PLAN.md)

## Purpose

Algraf v0.68.5 is a small color-literal compatibility patch for continuous
color gradients. It preserves the existing `Scale(fill: value, gradient:
["#d8f3dc", "#145f52"])` behavior while accepting common CSS color spellings
that authors already expect to work in SVG output.

This patch does not change scale training, guide layout, `.ag` syntax shape, or
the browser JSON ABI.

## Scope

### Gradient Color Literal Compatibility

Status: Implemented.

Acceptance criteria:

- `Scale(..., gradient: [...])` accepts ordinary hex colors, alpha hex colors
  (`#rgba` and `#rrggbbaa`), `rgb(r, g, b)`, and `rgba(r, g, b, a)` string
  stops.
- Positioned `Stop(color: ...)` gradients accept the same color literal forms.
- Invalid gradient colors continue to emit `E1601`.
- Rendering consumes the accepted color forms when interpolating gradient stops
  and preserves alpha channels in generated SVG colors.
- The stricter identity-color safety contract remains unchanged.

## Validation

- `cargo test -p algraf-semantics gradient`
- `cargo test -p algraf-render gradient`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
