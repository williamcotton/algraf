# Algraf v0.94.0 Plan

Status: Implemented
Target version: 0.94.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_93_PLAN.md`](V0_93_PLAN.md)
Follow-on plan: [`V0_95_PLAN.md`](V0_95_PLAN.md)
Roadmap theme: Replace telescoping render entry points with options.

## Purpose

Algraf v0.94 should stop the combinatorial growth in the public
`algraf-render` entry-point API and clean up the repeated CLI render epilogue
that grew around those wrappers.

This release is mostly internal, but it touches public Rust crate APIs. It
should preserve in-tree behavior and keep compatibility shims for at least one
release unless maintainers decide the crate has no external consumers to
protect.

## Release Thesis

v0.94.0 is a **render entry-point hygiene** release. The renderer already has a
good planning/emission boundary; the public API should reflect that by exposing
one options object rather than a family of increasingly long function names.

The intended end state is a small stable surface:

- one SVG render entry point;
- one interactive SVG render entry point, or an explicit option if that reads
  better at implementation time;
- one draw-list render entry point;
- one raster render entry point;
- one `RenderOptions` object that carries named tables, image assets, render
  limits, CLI theme override, and any future optional knobs.

## Current Debt Surface

- `crates/algraf-render/src/render.rs` exposes `render`,
  `render_with_tables`, `render_with_tables_and_limits`,
  `render_with_tables_and_assets`, and
  `render_with_tables_and_assets_and_limits`, then repeats the same pattern for
  interactive SVG, draw-list, and raster backends.
- `crates/algraf-cli/src/cmd_render.rs` already calls the longest variants
  because real callers usually need tables, assets, limits, and theme override.
- `render_chart_svg`, `render_chart_draw_list`, and `render_chart_raster`
  repeat the same diagnostic-report epilogue after backend rendering.
- `cmd_check.rs`, `cmd_ir.rs`, and `cmd_schema.rs` repeat the same source/data
  Clap fields, which makes command-line surface drift easier than it should be.

## v0.94.0 Must

### `RenderOptions`

Status: Implemented.

Introduce a public options object for render calls.

Acceptance criteria:

- `RenderOptions<'a>` carries the optional render context currently expressed by
  wrapper names: named tables, image assets, render limits, and CLI theme
  override.
- The default case is ergonomic. Callers that only have an IR, primary table,
  and theme can render without constructing empty `HashMap` or `ImageAssets`
  values at every call site.
- Future optional render knobs can be added to `RenderOptions` without adding
  another wrapper family.
- Ownership and lifetimes are clear. Borrowed tables and assets remain borrowed;
  the options object must not clone large loaded data by default.
- Documentation states which options are ordinary embedding options and which
  represent CLI-only semantics such as the strongest theme override.

### Small Backend Entry-Point Set

Status: Implemented.

Collapse the public render functions to a small backend-oriented set.

Acceptance criteria:

- SVG, interactive SVG, draw-list, and raster rendering have one primary public
  entry point each, or SVG and interactive SVG share one entry point with an
  explicit option if that produces a cleaner API.
- Raster scale remains explicit and discoverable; it may live in
  `RenderOptions`, a raster-specific options field, or a small raster options
  struct, but it must not restart the telescoping pattern.
- `crates/algraf-render/src/lib.rs`, `embed.rs`, `bin/render_timing.rs`,
  `algraf-cli`, and `algraf-wasm` are updated to the new API.
- Old wrapper names remain as deprecated shims for one release unless maintainers
  explicitly choose a direct cutover. Shims must forward through the new options
  path so behavior cannot drift.
- Existing SVG, draw-list, interactive metadata, and raster behavior remains
  unchanged.

### Shared CLI Render Report Epilogue

Status: Implemented.

Extract the repeated render-diagnostic epilogue in `cmd_render.rs`.

Acceptance criteria:

- SVG, draw-list, and raster paths share one helper that appends render
  diagnostics to the preparation report, prints human diagnostics, and enforces
  `--strict` blocking behavior.
- The helper keeps data warnings, parse diagnostics, semantic diagnostics, and
  render diagnostics in the same user-visible order as today.
- Render-specific output shaping remains in the backend-specific functions:
  SVG layout augmentation, metadata sidecars, draw-list JSON, and raster PNG
  encoding stay local to their outputs.

## v0.94.0 Should

### Shared Source Clap Arguments

Status: Implemented.

Use Clap flattening for the common source/data fields shared by `check`, `ir`,
and `schema`, and consider whether `render` can use the same shared struct.

Acceptance criteria:

- The shared struct owns `input`, `eval`, `base_dir`, `data`, `data_format`, and
  `vars`.
- Commands that also own `json`, `strict`, `sample_size`, or output-specific
  flags keep those fields in their command-specific structs.
- Flag names, conflicts, aliases, help text, and JSON output behavior remain
  compatible.
- CLI tests or focused command invocations cover at least one source-file path,
  one `--eval` path, one `--data` override, and one `--var` template expansion
  path after the extraction.

### Embed API Watch

Status: Implemented.

Check `crates/algraf-render/src/embed.rs` for the same wrapper-growth pattern.

Acceptance criteria:

- If `render_embedded`, `render_embedded_json`, and `render_embedded_with_io`
  can share the new options style with little risk, update them in this release.
- If the embed API needs a separate design, leave a short follow-up note rather
  than mixing a second public redesign into this release.

## Explicitly Deferred Past v0.94.0

- Language registry and editor-service source-of-truth work; see
  [`V0_95_PLAN.md`](V0_95_PLAN.md).
- Renderer geometry and domain-pipeline helper extraction; see
  [`V0_97_PLAN.md`](V0_97_PLAN.md).
- Test-suite fixture reorganization; see [`V0_99_PLAN.md`](V0_99_PLAN.md).

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- Focused public API compile coverage through in-tree callers.
- CLI smoke checks for `render --format svg`, `render --format svg+json`,
  `render --format draw-list`, and `render --format raster`.
- If deprecated shims stay, a small compile-only or unit test should exercise at
  least one old wrapper name so it does not rot before removal.

## Promotion Workflow

1. Align version stamps for v0.94.0 when implementation begins.
2. Add `RenderOptions` while keeping old wrappers intact.
3. Move internal render entry points to consume the new options object.
4. Update in-tree callers.
5. Add deprecation attributes or remove old wrappers according to the chosen
   compatibility policy.
6. Extract the CLI render report epilogue and shared Clap arguments.
7. Run the full required checks and mark this plan's statuses accurately.
