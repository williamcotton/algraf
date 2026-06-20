# Algraf v0.84.3 Plan

Status: Implemented
Target version: 0.84.3
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_84_2_PLAN.md`](V0_84_2_PLAN.md)

## Purpose

Algraf v0.84.3 is a focused renderer maintenance patch for bar charts whose
source data keeps numeric or temporal bucket columns typed as data rather than
pre-stringifying them for display.

The intended authoring path is explicit:
`Scale(axis: x, type: "categorical")` or `Scale(axis: y, type:
"categorical")` forces a scalar position axis to train as a categorical band
axis while preserving the inferred backing column type. When a `Bar` geometry is
used against an incompatible trained space, the renderer should point authors at
that scale override instead of only reporting that the space is incompatible.
For temporal bucket columns, the same authoring path should also let
`Guide(axis:, timeFormat:)` control categorical tick-label presentation while
the underlying category keys stay deterministic.

## Scope

### Bar Categorical Axis Diagnostic Help

Status: Implemented.

Acceptance criteria:

- `Bar` continues to require one categorical position axis and one continuous
  value axis in Cartesian spaces.
- Numeric and temporal source columns are not reshaped or retyped during data
  loading.
- `Scale(axis: x, type: "categorical")` allows temporal bucket columns to drive
  stacked bars as discrete categories.
- The `R0002` diagnostic for an incompatible Cartesian `Bar` space includes
  help suggesting explicit categorical axis scale declarations.
- Renderer tests cover both the temporal-bucket success path and the diagnostic
  help text.

### Temporal Categorical Axis Labels

Status: Implemented.

Acceptance criteria:

- `Parse(...)` continues to control only input temporal conversion; no new
  output formatting syntax is added to `Parse`.
- `Guide(axis: x, timeFormat: "month")`,
  `Guide(axis: x, timeFormat: "%b %Y")`, and
  `Guide(axis: x, timeFormat: "year")` format labels on temporal columns forced
  to categorical band axes with `Scale(axis: x, type: "categorical")`.
- The same behavior applies to y axes through `Guide(axis: y, timeFormat: ...)`.
- Without `Guide(timeFormat:)`, temporal category labels remain deterministic
  UTC RFC3339 strings.
- Non-temporal categorical axes ignore `Guide(timeFormat:)` as before.
- Renderer tests and examples cover temporal categorical axis formatting.

## Validation

- `cargo fmt --all --check`
- `cargo test -p algraf-render categorical_axis_type_allows_temporal_bar_position`
- `cargo test -p algraf-render temporal_categorical_axis_uses_custom_guide_time_format`
- `cargo test -p algraf-render temporal_categorical_axis_uses_named_year_guide_time_format`
- `cargo test -p algraf-render temporal_categorical_axis_without_guide_keeps_rfc3339_labels`
- `cargo test -p algraf-render non_temporal_categorical_axis_ignores_guide_time_format`
- `cargo test -p algraf-render bar_space_mismatch_diagnostic_suggests_categorical_axis_type`
- `./examples/generate.sh`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
