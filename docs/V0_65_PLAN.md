# Algraf v0.65.0 Plan

Status: Implemented
Target version: 0.65.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_64_PLAN.md`](V0_64_PLAN.md)
Cross-repo coordination: `../studio/` if Studio story charts rely on numeric
categorical axis positions.

## Purpose

Algraf v0.65 adds an explicit categorical position-axis override for numeric
source columns. The release goal is to let authors use numeric identifiers such
as day numbers, week numbers, ranks, bins, or small integer codes as discrete
bar, tile, and nested-band positions without preparing duplicate string columns
outside Algraf.

The motivating chart is:

```ag
Chart(data: "visible_days.csv", width: 520, height: 230) {
    Scale(axis: x, type: "categorical")

    Space(day * value) {
        Bar(fill: value)
    }
}
```

Today `day` infers as `Integer`, so `Space(day * value)` trains a continuous x
axis and `Bar` reports that it requires one categorical position axis and one
continuous value axis. v0.65 should make the author's categorical intent
explicit without changing CSV inference or mutating the data schema globally.

This plan is not normative until [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) promotes the
scale syntax, diagnostics, and renderer semantics with concrete MUST/SHOULD
language. Existing numeric axis behavior must remain unchanged unless the author
opts into `type: "categorical"`.

## Scope

### Categorical Axis Scale Type

Status: Implemented.

Acceptance criteria:

- Add `Scale(axis: x, type: "categorical")` and
  `Scale(axis: y, type: "categorical")`.
- The spelling is axis-only. `Scale(fill: value, type: "categorical")` remains
  invalid because aesthetic scale classification is controlled by the mapped
  column type and existing aesthetic `mode:` options.
- On a scalar position axis backed by a non-geometry column, `type:
  "categorical"` trains a band axis even when the schema type is `Integer`,
  `Float`, or `Temporal`.
- The column's schema type is not changed. Other spaces, aesthetics, stats, and
  geometry mappings still see the original data type unless they independently
  opt into categorical axis treatment.
- Existing `"linear"`, `"log10"`, and `"sqrt"` behavior remains continuous-axis
  behavior and is unaffected.

### Category Keys, Ordering, And Domains

Status: Implemented.

Acceptance criteria:

- Category keys are produced through the same deterministic category formatting
  used by `cell_category`:
  - integers format as decimal strings;
  - finite floats format through the existing float-category formatter;
  - booleans, strings, mixed values, and temporal values keep existing category
    formatting;
  - missing values produce no category and affected marks are skipped as they
    are on ordinary band axes.
- Default category order is first appearance order, matching ordinary
  categorical domains.
- Existing string-array position-axis domains order the categorical override:

  ```ag
  Scale(axis: x, type: "categorical", domain: ["1", "2", "3"])
  ```

- Declared categories with no matching rows still reserve bands, and observed
  categories not listed in the declaration are appended in first-appearance
  order, matching the v0.61 categorical-domain policy.
- Numeric `domain: [min, max]` is not reinterpreted as categories for this
  release. A categorical axis override with a numeric domain must produce a
  targeted diagnostic.

### Geometry Coverage

Status: Implemented.

Acceptance criteria:

- `Bar` accepts `Space(day * value)` when `Scale(axis: x, type:
  "categorical")` is present and the opposite axis is continuous.
- Horizontal bars work symmetrically with `Scale(axis: y, type:
  "categorical")`.
- Other geometries that already consume band axes, including `Tile`, `Text`,
  `Rect` categorical bounds, `Boxplot`, and `Violin`, continue to work through
  the same trained-axis abstraction where applicable.
- Continuous-only geoms such as `Line`, `Smooth`, `Density`, and `Histogram`
  continue to diagnose invalid categorical axis usage according to their
  existing geometry requirements.

### Diagnostics And Invalid Combinations

Status: Implemented.

Acceptance criteria:

- Unknown scale types still produce the existing `E1204` style diagnostic.
- `type: "categorical"` with `breaks:`, numeric `domain:`, or `integer:` should
  produce a targeted diagnostic because those controls are continuous-axis
  controls.
- `type: "categorical"` on a geometry column should diagnose rather than train
  a category domain; geometry remains spatial-only.
- `type: "categorical"` on blended/union axes is deferred. The implementation
  should reject or warn clearly rather than guessing a combined category label
  policy.
- `Scale(axis: x, type: "categorical")` without any x axis in the rendered
  spaces should follow existing unused scale behavior; do not add special-case
  errors unless existing scale infrastructure already reports them.

### Editor, CLI, WASM, And Examples

Status: Implemented.

Acceptance criteria:

- Parser and formatter continue to accept the new value as an ordinary string
  literal enum value.
- Semantic registry metadata, completions, hover, and signature help include
  `"categorical"` for `Scale(type:)`.
- CLI `check`, `schema`, `ir`, and `render` expose the new scale type
  deterministically.
- WASM rendering uses the same renderer and diagnostics as the native CLI.
- Add an example chart using a numeric categorical axis. The README tutorial
  gets a corresponding section and rendered SVG/PNG output as required by the
  repository example-generation rules.

## Non-Goals

- Automatic low-cardinality numeric-to-categorical inference.
- Global schema mutation or a chart-scoped type declaration that changes a
  column's inferred data type.
- Generic cast syntax such as `Categorical(day)` or `as_string(day)`.
- A derived `Cast`/`Format` transform.
- Numeric arrays as categorical domains. Authors should use string-array
  domains that match the formatted category keys.
- Categorical overrides for blended/union axes in the first release.
- Locale-aware numeric formatting or custom tick/category formatting. Existing
  deterministic formatting stays authoritative.

## Implementation Notes

- Add `ScaleTypeIr::Categorical` and extend `ScaleTypeIr::as_str()`.
- Add `"categorical"` to `registry::SCALE_TYPE_NAMES` so completion and
  registry-driven editor surfaces can share the same source of truth.
- In scale analysis, keep `type: "categorical"` axis-only and validate
  incompatible controls before lowering to IR.
- In renderer axis training, make scalar `FrameIr::Vector` axes build
  `AxisScale::Band` when their axis config requests `ScaleTypeIr::Categorical`
  and the backing column is not geometry.
- Reuse `ordered_categorical_domain` and `cell_category` rather than adding a
  second conversion path.
- Leave dataframe inference unchanged: `day` remains `Integer`, and the axis
  override affects only the trained position scale for that axis.

## Validation

Required checks before this plan can be marked implemented:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Additional validation:

- Semantic tests for parsing and lowering `Scale(axis: x, type:
  "categorical")`.
- Render tests proving vertical and horizontal numeric categorical bars no
  longer emit the `Bar` categorical-position diagnostic.
- Render tests for first-appearance category order and explicit string-array
  domain order.
- Diagnostic tests for numeric `domain:`, `breaks:`, `integer:`, geometry axes,
  aesthetic scale misuse, and blended/union axes.
- Editor-service tests for completions, hover, and signature help.
- CLI tests for `check`, `ir`, and render metadata where scale type appears.
- Example generation plus visual inspection of the new rendered example.

## Deferred

- `Categorical(column)` or other algebra-level cast syntax.
- Derived-table casts or formatting transforms.
- Categorical overrides for blended/union axes.
- Position-axis label maps independent of category keys.
- Custom category-key formatting for numeric or temporal values.
