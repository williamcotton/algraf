# Algraf v0.74.0 Plan

Status: Proposed
Target version: 0.74.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_73_PLAN.md`](V0_73_PLAN.md)
Roadmap theme: Internal CLI maintenance; carve `algraf-cli/src/main.rs`
into focused command and helper modules.
Cross-repo coordination: none required to ship 0.74.0.

## Purpose

Algraf v0.74 is an internal maintenance release. It restructures
`crates/algraf-cli/src/main.rs` (1,637 lines) into a dispatcher plus focused
command and helper submodules without changing any CLI flag, argument,
output byte, diagnostic, or exit status. No spec sections move; ALGRAF_SPEC
§23 already documents the CLI crate's responsibility ("argument parsing,
command dispatch, I/O") and does not constrain internal module structure.

The analyzer phase modules (`crates/algraf-semantics/src/analyzer/frames.rs`
at 1,895 lines, `…/stats.rs` at 2,209 lines, `…/lowering.rs` at 1,707 lines)
and `crates/algraf-semantics/src/registry.rs` (1,690 lines) are explicitly
out of scope for v0.74: each is large but cohesive, with one clear phase
concern per file and no hidden subphases that warrant further splitting.
Re-evaluate only if a future plan adds enough volume to push any of those
files past ~2,500 lines, or if a feature reveals subphase seams that are
not visible today.

## Release Thesis

`main.rs` was the right shape when the CLI shipped two subcommands; today
it hosts seven (`render`, `check`, `format`, `schema`, `ast`, `ir`, `lsp`
via wiring), three render backends (`svg`, `draw-list`, `png`), an IR JSON
serializer with twenty-plus nested converter functions, an SVG metadata
augmenter, and shared path/io helpers. Reviewers and contributors are
expected to scroll the same 1,600-line file regardless of which command
they touch.

The v0.74 work is mechanical extraction. Each subcommand becomes a
sibling module. Shared helpers (input reading, output writing, SVG path
helpers, IR JSON) become their own modules. `main.rs` shrinks to ~150
lines: module declarations, `Cli`/`Command` enums, `main()`, `run()`
dispatch.

Three discipline rules:

1. **Behavior preservation.** Every CLI flag, default, output byte, and
   diagnostic stays identical. `examples/generate.sh` produces
   byte-identical SVG/PNG output. `crates/algraf-cli/tests/cli.rs` (888
   lines, end-to-end coverage of every subcommand) passes without
   modification.
2. **No public crate surface change.** Nothing currently exported from
   `algraf-cli` (the binary has no library API) is renamed or moved across
   crate boundaries. Internal `pub(crate)` visibility replaces in-file
   private items where modules need to reach across.
3. **Tests stay green at every commit.** `cargo fmt --all --check`, `cargo
   clippy --workspace --all-targets`, `cargo test --workspace`, and
   `./examples/generate.sh` produce the same output before and after each
   commit.

## Proposed Spec Changes

None.

ALGRAF_SPEC §23.2 documents `algraf-cli` at the crate level only ("CLI
binary: argument parsing, command dispatch, I/O") and does not prescribe
internal submodule layout. The spec gains only the v0.74 history line and
`Status:` / "current implementation is version" lines when the plan ships.

## Scope: crates/algraf-cli/src/main.rs (1,637 lines)

Current shape, by line range:

- Lines 1–256: module declarations (`astjson`, `diagnostics`, `error`,
  `png`); enum and struct definitions (`Cli`, `Command`, `RenderFormat`,
  `DataFormatArg`, `RenderArgs`, `CheckArgs`, `FormatArgs`, `SchemaArgs`,
  `AstArgs`, `IrArgs`).
- Lines 258–285: `main()` and `run()` dispatch.
- Lines 289–339: shared input helpers (`read_source`,
  `read_template_source`, `driver_error`).
- Lines 341–428: render output plumbing (`render_cmd`, `write_outputs`,
  `write_primary_output`).
- Lines 464–677: render backend dispatch (`render_chart_output`,
  `render_chart_svg`, `render_chart_draw_list`, `render_chart_raster`).
- Lines 681–806: `prepare_render_inputs` — load data, analyze, resolve
  IR, load assets, apply theme.
- Lines 808–877: SVG metadata and path helpers (`should_write_metadata`,
  `primary_output_path`, `metadata_output_path`, `chart_output_path`,
  `is_png_path`).
- Lines 879–950: `check_cmd` (parse + multi-chart analysis).
- Lines 952–966: `format_cmd` (calls `algraf_syntax::format`).
- Lines 968–1052: `schema_cmd` (load data source, infer schema, print).
- Lines 1054–1067: `ast_cmd` (parse + print AST).
- Lines 1069–1127: `ir_cmd` (parse + analyze + print IR).
- Lines 1129–1440: shared IR/JSON serialization (`augment_svg`,
  `debug_layout_svg`, `debug_rect`, `insert_before_svg_end`, `ir_to_json`,
  and twenty-plus nested converters: `data_source_json`, `derive_json`,
  `space_json`, `guide_overrides_json`, `scale_json`, `gradient_json`,
  `scale_target_json`, `scale_type_str`, `space_data_json`,
  `geometry_json`, `interaction_json`, `mapping_json`, `frame_json`,
  `column_json`, `setting_value_json`, `span_json`, `stat_options_json`,
  `levels_json`, `stat_kind_str`, `geometry_kind_str`, `dtype_str`).

Target shape: a dispatcher plus eleven sibling modules under
`crates/algraf-cli/src/`.

Move targets:

- `cli/src/cmd_render.rs`. `RenderArgs`, `RenderFormat`, `DataFormatArg`,
  `render_cmd`, `render_chart_output`, `render_chart_svg`,
  `render_chart_draw_list`, `render_chart_raster`, `RenderOutput`,
  `RenderOutputData`, `RenderInputs`, `prepare_render_inputs`, plus the
  render-only limits/options helpers.
- `cli/src/cmd_check.rs`. `CheckArgs`, `check_cmd`.
- `cli/src/cmd_format.rs`. `FormatArgs`, `format_cmd`.
- `cli/src/cmd_schema.rs`. `SchemaArgs`, `schema_cmd`.
- `cli/src/cmd_ast.rs`. `AstArgs`, `ast_cmd`.
- `cli/src/cmd_ir.rs`. `IrArgs`, `ir_cmd`.
- `cli/src/cmd_lsp.rs`. The `lsp` subcommand entry; today the LSP wiring
  is inline in `run()`. Move it to its own module so future LSP transport
  changes do not perturb `main.rs`.
- `cli/src/io.rs`. `write_outputs`, `write_primary_output`,
  `primary_output_path`, `metadata_output_path`, `chart_output_path`,
  `is_png_path`, `should_write_metadata`.
- `cli/src/svg_debug.rs`. `augment_svg`, `debug_layout_svg`, `debug_rect`,
  `insert_before_svg_end`.
- `cli/src/ir_json.rs`. `ir_to_json` plus all nested converter functions
  (`data_source_json` through `dtype_str`).
- `cli/src/input.rs`. `read_source`, `read_template_source`, `driver_error`.
- `cli/src/main.rs` (slimmed). Module declarations, `Cli` and `Command`
  enums, `main()`, `run()` dispatch. No business logic.

The existing top-level modules `astjson`, `diagnostics`, `error`, `png`
stay where they are. The split adds sibling modules; it does not nest or
rename existing files.

Cross-cutting considerations:

- Inputs and outputs continue to flow through `algraf-driver` and the
  render backends. No new traits, no new feature flags.
- Errors continue to be `algraf-cli`'s `error::CliError`. No error variants
  are added or removed.
- `prepare_render_inputs` is the chokepoint between render and the
  semantics/render crates. It moves to `cmd_render.rs` because it is
  render-specific; if a second subcommand later needs the same prep, it
  promotes to `cli/src/prepare.rs` in a follow-up plan.

## Non-Goals

The following large files are deliberately out of scope; documenting them
here so they do not get revisited piecemeal during v0.74 implementation.

- `crates/algraf-semantics/src/analyzer/frames.rs` (1,895 lines). Frame
  algebra (Cartesian/Union/Nested/Vector variants), projection, coords,
  views, geometry collection, theme parsing, guides, scales. One primary
  public entry (`space`) with well-scoped helpers. Leave alone.
- `crates/algraf-semantics/src/analyzer/stats.rs` (2,209 lines). One entry
  (`resolve_chart_derives`) and per-stat-family argument validation.
  Splitting by stat family (bin, density, smooth, summary) would fragment
  stat knowledge during analysis. Leave alone.
- `crates/algraf-semantics/src/analyzer/lowering.rs` (1,707 lines). A
  collection of independent desugaring functions (`desugar_histogram`,
  `desugar_interval_sugar`, `desugar_freq_poly`, `desugar_bin2d`,
  `desugar_density`, `desugar_count_bar`), each self-contained.
- `crates/algraf-semantics/src/registry.rs` (1,690 lines). Geometry,
  aesthetic-property, scale, theme, guide metadata. Data-heavy; splitting
  would fragment lookup tables that need to be co-located for validation.
- Integration tests. `crates/algraf-semantics/tests/analysis.rs` (3,345
  lines) and `crates/algraf-render/tests/render.rs` (3,005 lines) are
  large because they exercise comprehensive integration coverage; not
  refactor targets.

Re-evaluate any of these only if a future plan pushes file size past
~2,500 lines or reveals subphase seams that are not visible today.

## Must

- Split `crates/algraf-cli/src/main.rs` into the eleven sibling modules
  described above (`cmd_*.rs`, `io.rs`, `svg_debug.rs`, `ir_json.rs`,
  `input.rs`) plus a slimmed `main.rs` that retains module declarations,
  `Cli` and `Command` enums, `main()`, and `run()` dispatch.

  Status: Proposed.

  Land as a single PR or as one commit per logical extraction; either
  way, every commit must pass `cargo fmt --all --check`, `cargo clippy
  --workspace --all-targets`, and `cargo test --workspace`. End-to-end
  parity must be confirmed via `./examples/generate.sh` producing
  byte-identical SVG and PNG outputs.

- Preserve every CLI flag, default value, argument name, output byte,
  diagnostic, and exit status.

  Status: Proposed.

  `crates/algraf-cli/tests/cli.rs` (888 lines) exercises every subcommand
  end-to-end and must pass without modification.

- Hold spec text alone outside the release `Status:` and history lines.

  Status: Proposed.

  ALGRAF_SPEC.md §23.2 documents the CLI crate boundary; submodule layout
  is not normative. No diagnostic codes added or removed.

- Hold crate boundaries.

  Status: Proposed.

  No new public exports from `algraf-cli`. No new dependencies on
  `algraf-data` or `algraf-render` from previously decoupled crates. The
  `pub use` re-exports from `lib.rs` (if any are introduced) stay
  `pub(crate)` unless a second internal consumer needs them.

- Bump release version stamps to 0.74.0 when this plan ships.

  Status: Proposed.

  Updates: workspace `Cargo.toml`, `Cargo.lock` workspace member entries,
  `docs/ALGRAF_SPEC.md` (`Status:` line and "current implementation is
  version" prose, plus a v0.74 history line), `editors/vscode/package.json`,
  `editors/vscode/package-lock.json`, `demo/package.json`,
  `demo/package-lock.json`. Internal package version-stamp updates only;
  AGENTS CLAUDE.md "NPM package version checks" still apply if a future
  change wants to retarget consumer dependency pins.

## Should

- Land each extraction in its own commit on the v0.74 branch so the diff
  is reviewable per module.

  Status: Proposed.

  Suggested order: `input.rs` (smallest, no command dependencies); `io.rs`
  + `svg_debug.rs` (path and SVG helpers); `ir_json.rs` (large but
  self-contained); `cmd_check.rs`, `cmd_format.rs`, `cmd_schema.rs`,
  `cmd_ast.rs`, `cmd_ir.rs`, `cmd_lsp.rs` (small command handlers);
  `cmd_render.rs` last (largest, most surface area).

- Add a module-layout comment at the top of the slimmed `main.rs`
  pointing readers at the per-command modules.

  Status: Proposed.

  One short paragraph. No spec change, no doc reorganization.

- Add unit tests for `io.rs` path helpers (`primary_output_path`,
  `metadata_output_path`, `chart_output_path`, `is_png_path`,
  `should_write_metadata`) and for the `svg_debug.rs` helpers
  (`augment_svg`, `insert_before_svg_end`) once the split lands.

  Status: Proposed.

  These helpers are currently covered only transitively by end-to-end
  tests; small unit tests around the path and SVG logic make future
  changes cheaper.

## Could

- Promote a `cli/src/prepare.rs` if a second subcommand needs the
  load/analyze/resolve-IR prep currently used only by `render`.

  Status: Deferred.

  Premature today; revisit only when a real second caller arrives.

- Document the LSP transport split (currently inline `lsp` wiring → its
  own module) as a precedent for future LSP work in a v0.75+ plan.

  Status: Deferred.

  The mechanical move in v0.74 is sufficient; deeper LSP refactoring
  needs its own thesis.

## Validation

Per CLAUDE.md, the workspace checks are authoritative.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./examples/generate.sh
git diff -- examples   # expected empty
```

`git diff -- examples` empty means the renderer produced byte-identical
SVG and PNG outputs. Any non-empty diff is a regression that must be
investigated before the plan can be marked Implemented.

Spot-check three representative subcommands end-to-end with the locally
built binary, captured under `--help`-style comparison against `main`:

```bash
cargo run -p algraf-cli -- render examples/scatter.ag --output /tmp/out.svg
cargo run -p algraf-cli -- check examples/scatter.ag
cargo run -p algraf-cli -- ir examples/scatter.ag
```

All three must produce identical bytes on stdout/stderr/output files
compared with the `main`-branch binary.

## Open Questions

1. Should the eleven submodules nest under `cli/src/commands/` and
   `cli/src/cli_helpers/` to advertise grouping, or stay flat as
   sibling files? Recommendation: stay flat for v0.74. Renames are
   reversible; the immediate goal is shrinking `main.rs`.
2. Does the `lsp` subcommand wiring belong in `cmd_lsp.rs` or in
   `algraf-lsp` directly? Recommendation: keep the CLI entry in
   `cmd_lsp.rs` so the binary still owns the transport and process
   lifecycle; deeper lsp work is its own plan.

## Promotion Workflow

This plan promotes no spec-level features. No new geometries, themes,
scales, guides, CLI flags, or diagnostics. The only normative artifact
that moves is the spec `Status:` and history line when v0.74 ships.

When the plan is implemented, update `Status:` lines, bump release
version stamps per the Must item above, and start `V0_75_PLAN.md` for
the next release scope.
