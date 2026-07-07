# Algraf v0.99.0 Plan

Status: Implemented
Target version: 0.99.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_98_PLAN.md`](V0_98_PLAN.md)
Roadmap theme: Clear small clones and make large tests easier to change.

## Purpose

Algraf v0.99 should sweep up the lower-risk clone findings that do not justify
their own release and improve the highest-friction test files with shared test
helpers.

This release should be deliberately boring: no language changes, no renderer
algorithm changes, no public API redesign, and no broad file moves unless the
test helper work makes them clearly safer.

## Release Thesis

v0.99.0 is a **maintenance consolidation** release. After v0.93-v0.98 remove the
larger refactor hazards, this release should reduce the remaining small
duplication and make future contributor edits faster by shrinking repeated test
setup.

## Current Debt Surface

- `crates/algraf-lsp/src/backend.rs` has formatting and range-formatting paths
  that deliberately perform the same holistic formatting work.
- `crates/algraf-editor-services/src/hover.rs` repeats source-preview markdown
  assembly.
- `crates/algraf-lsp/tests/lsp.rs` repeats JSON request/response scaffolding.
- `crates/algraf-semantics/tests/analysis.rs` and
  `crates/algraf-render/tests/render.rs` are large monoliths with repeated
  setup blocks.
- `crates/algraf-render/tests/draw_list.rs` and `parity.rs` share fixture
  setup.
- Library paths still contain a number of `panic!`, `unwrap`, and `expect`
  calls that should be audited for user-input reachability.

## v0.99.0 Must

### Small Production Clone Sweep

Status: Implemented.

Implementation notes:

- `algraf-lsp` now routes full-document formatting and range formatting through
  one private whole-document edit helper. Range requests still use the holistic
  formatter documented by spec §21.10.
- `algraf-editor-services` now uses one private source sample markdown helper
  for the schema/sample/provisional body shared by named-table and data-source
  hovers.
- The remaining small production clone surface identified for this maintenance
  sweep was either extracted above or left local because the repeated code is
  test-specific setup now covered by the dedicated test helper items below.

Remove confirmed small production clones whose behavior is already known to be
identical.

Acceptance criteria:

- `range_formatting` in `algraf-lsp` delegates to the same helper as full
  formatting while keeping LSP response behavior unchanged.
- Hover source-preview markdown assembly has one helper used by both call sites.
- Any remaining small clone from the v0.92 review is either extracted or left
  with a short reason why local duplication is clearer.
- No public API changes are made for these small helpers.

### LSP Test Request Helpers

Status: Implemented.

Implementation notes:

- `crates/algraf-lsp/tests/lsp.rs` has a shared JSON-RPC request/notification
  helper layer.
- Test sites still name the LSP method, params object, request id, and expected
  response type explicitly.

Introduce a small common helper layer for LSP request/response tests.

Acceptance criteria:

- Repeated JSON-RPC request construction lives in one test helper.
- Response assertion helpers keep expected JSON readable at the test site.
- Existing test names and behavioral coverage remain recognizable.
- The helper does not hide the method name, params, or expected result in a way
  that makes protocol regressions harder to see.

### Render Test Fixture Helpers

Status: Implemented.

Implementation notes:

- `crates/algraf-render/tests/common/mod.rs` owns the repeated primary CSV,
  named-table, image-asset, SVG, and draw-list fixture setup.
- `render.rs`, `draw_list.rs`, and `parity.rs` share that helper while keeping
  assertions and failure messages local to each test.

Share repeated render fixture setup across render, draw-list, and parity tests.

Acceptance criteria:

- A helper can build a chart source plus in-memory CSV/table fixture for common
  render tests.
- Draw-list and parity tests share the repeated fixture setup identified by the
  clone review.
- Test failure output remains specific enough to diagnose the rendered artifact
  or metadata field that drifted.
- No golden output is changed as part of helperization.

## v0.99.0 Should

### Split Giant Test Files By Feature Area

Status: Deferred after audit.

Implementation notes:

- Helper extraction reduced repeated setup in `render.rs`, `draw_list.rs`, and
  `parity.rs`, but splitting `render.rs` or
  `crates/algraf-semantics/tests/analysis.rs` is not mechanical yet: both files
  still have broad local helper/constants surfaces and many tightly clustered
  assertions that would need a larger module-boundary pass.
- Keeping the helper extraction and deferring the split matches this item's
  fallback criterion; avoiding a broad file move also keeps test harness behavior
  and compile-time shape unchanged for v0.99.

If helper extraction makes it safe, split the largest test files into smaller
feature-oriented modules or integration test files.

Acceptance criteria:

- `algraf-semantics/tests/analysis.rs` is split only along obvious feature
  boundaries, such as scales, themes, stats, frames, variables, and diagnostics.
- `algraf-render/tests/render.rs` is split only where shared fixture helpers
  prevent repeated setup from getting worse.
- Test names stay searchable.
- Total test runtime should not increase meaningfully.
- If compile time or harness behavior gets worse, keep the helper extraction and
  defer the split.

### Panic And Unwrap Audit

Status: Implemented.

Implementation notes:

- User-input-adjacent production unwraps in boxplot distribution grouping,
  z-field polygon clipping, and stack category ordering were replaced with
  explicit empty-case handling.
- The raster 1x1 fallback keeps an invariant `expect`, now with a message that
  explains why it is intended to be infallible.
- Remaining `unwrap`, `expect`, and `panic!` hits in `algraf-render` and
  `algraf-semantics` are either test-only or invariant checks with local
  reasoning; no new user-facing diagnostics were needed.
- `#![warn(clippy::unwrap_used)]` is still too noisy for these crates today
  because integration-style unit tests and small invariant helpers intentionally
  use unwraps for failure clarity. A future cleanup should first isolate or
  allow test modules before enabling the lint.

Do a targeted audit of panics, unwraps, and expects reachable from library paths.

Acceptance criteria:

- User-input-reachable panics in `algraf-render` and `algraf-semantics` are
  converted to diagnostics or `RenderError` where feasible.
- Truly infallible `expect` calls get messages that explain the invariant.
- Tests remain allowed to use `expect` where it improves failure messages.
- Decide whether `#![warn(clippy::unwrap_used)]` is practical for selected
  library crates after the audit. If not, document why it is too noisy today.

### Repo Weight Note

Status: Implemented.

Implementation notes:

- Committed example SVG/PNG outputs currently total 362 files and about 56M:
  180 PNGs at about 39M, and 182 SVGs at about 17M.
- The largest committed example outputs are SVGs around 1.0-1.1M each, mostly
  spatial examples.
- Git LFS, generated PNGs, or a separate artifact workflow are worth a future
  plan, but this maintenance sweep intentionally leaves example assets intact.

Record, but do not necessarily solve, the committed example asset weight issue.

Acceptance criteria:

- Measure the size of committed PNG/SVG example outputs.
- Decide whether Git LFS, generated PNGs, or a separate artifact workflow is
  worth a future plan.
- Do not delete or rewrite examples in this maintenance sweep.

## Explicitly Deferred Past v0.99.0

- Broad benchmark infrastructure.
- Git LFS migration or artifact retention policy changes.
- New formatter behavior.
- Any language, CLI, WASM, or rendering behavior changes not required by the
  panic audit.

## Validation

Completed for v0.99.0:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- Focused LSP formatting and range-formatting tests.
- Focused editor hover tests for source preview markdown.
- Existing LSP integration tests after helper extraction.
- Existing render, draw-list, and parity tests after fixture helper extraction.
- If any panic/unwrap behavior changes, focused tests for the converted error or
  diagnostic path.

## Promotion Workflow

1. Align version stamps for v0.99.0 when implementation begins.
2. Land production clone helpers first.
3. Add test helpers without splitting files.
4. Split giant tests only if the helper layer makes the move mechanical.
5. Run the panic/unwrap audit after behavior-preserving cleanup, so any real
   error-path changes are easy to review.
6. Run the full required checks and mark statuses.
