# Algraf v0.100.0 Plan

Status: Implemented
Target version: 0.100.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_99_PLAN.md`](V0_99_PLAN.md)
Roadmap theme: Close the v0.97-v0.99 review findings before new feature work.

## Purpose

Algraf v0.100 should turn the post-v0.99 review findings into a deliberately
small hardening release. The priority is to remove one user-reachable renderer
panic, make an unplanned parser recovery drift explicit, improve diagnostic
wording where the analyzer knows more than it says, and keep panic-free
invariant rewrites debuggable.

This release should not add language features, public API redesigns, or broad
renderer rewrites. When behavior changes, it should be named in this plan and
covered by focused regression tests.

## Release Thesis

v0.100.0 is a **review closeout and panic-hardening** release. v0.97-v0.99
successfully reduced duplication, but the static review found a few places
where consolidation either missed a reachable panic or subtly shifted a recovery
heuristic. This release should make those review points explicit and keep future
cleanup from hiding errors.

## Current Review Surface

- `crates/algraf-render/src/geom/distribution.rs` clamps width settings with
  `min = 1.0` and `max = bandwidth`; categorical panels with bandwidth below
  one pixel can panic through ordinary boxplot, violin, or sina input.
- The same distribution helper has guarded and unguarded group access patterns
  after the v0.99 unwrap sweep.
- `crates/algraf-syntax/src/parser/mod.rs::is_near_keyword` now delegates to
  `closest`, whose length-scaled threshold is wider than the previous fixed
  two-edit keyword typo rule for identifiers with nine or more characters.
- `crates/algraf-semantics/src/analyzer/stats.rs::summary_reducer_arg` reports
  a known-but-disallowed reducer as unknown.
- `crates/algraf-render/src/geom/stack.rs::display_order` now silently breaks
  on an invariant violation, and `crates/algraf-driver/src/resolution.rs` still
  carries path-resolution `expect` calls across duplicated enum matches.
- `crates/algraf-render/tests/common/mod.rs` is useful but still preserves two
  render-helper naming vocabularies and asymmetric diagnostic strictness.
- Guide merge maintenance is documented by convention rather than enforced by
  exhaustive destructuring.

## v0.100.0 Must

### Distribution Bandwidth Clamp Panic

Status: Implemented.

Fix the user-reachable `f64::clamp` panic in distribution geometry width
calculation.

Acceptance criteria:

- Boxplot, violin, and sina width calculation must not call `f64::clamp` with
  `min > max` when categorical bandwidth is below one pixel.
- The chosen width remains within the available bandwidth and stays finite for
  high-cardinality categorical axes and small faceted panels.
- Add a regression test with enough categories or small enough panel extent to
  produce sub-pixel bandwidth before mark emission.
- Keep existing distribution output unchanged for ordinary bandwidths.
- Normalize nearby group access in `density_layouts` and boxplot whisker setup
  so equivalent invariants use equivalent guard style.

### Parser Keyword-Typo Threshold

Status: Implemented.

Decide intentionally whether keyword typo detection should keep the old fixed
two-edit threshold or use `closest`'s length-scaled threshold.

Default implementation direction:

- Restore the prior binary typo rule: same first character ignoring ASCII case,
  and edit distance no greater than two.
- Keep the shared Unicode-aware edit-distance implementation from `algraf-core`
  if that is the desired v0.98 behavior; document that choice in the test or
  helper comment.
- Continue using `closest` for suggestion ranking, where its adaptive threshold
  is the intended behavior.

Acceptance criteria:

- Add a parser regression test for a long identifier such as `derived_x` at
  statement start so it does not silently become a misspelled `Derive` unless
  maintainers explicitly choose and document that broader recovery behavior.
- Add or update a positive misspelled-keyword recovery test to prove ordinary
  near typos still recover.
- If the wider `closest` threshold is intentionally kept, record that decision
  in this plan's status notes and pin the new behavior with tests instead of
  leaving it implicit.

### Reducer Diagnostic Accuracy

Status: Implemented.

Split unknown reducer diagnostics from known-but-disallowed reducer diagnostics.

Acceptance criteria:

- Unknown reducer strings keep the current unknown-reducer diagnostic path and
  stat-specific help text.
- Known reducers that are unsupported for a given stat, such as `mean_se` for
  z-field summary stats, report that the reducer is not supported in that
  context rather than unknown.
- Existing diagnostic codes and spans remain stable unless the implementation
  records a deliberate diagnostic change in this plan.
- Focused semantics tests pin both the unknown and unsupported wording.

### Debuggable Invariant Cleanup

Status: Implemented.

Keep release builds panic-free on user input while ensuring impossible internal
states remain loud in debug builds and tests.

Acceptance criteria:

- `display_order` keeps a non-panicking release fallback, but the unexpected
  missing pending category arm gains a `debug_assert!` with an invariant message.
- `resolution.rs` is restructured so path-source resolution is carried by the
  type shape instead of re-matching and `expect`ing the same enum variant.
- Any remaining `expect` kept in these paths must have a local invariant message
  explaining why user input cannot reach it.
- Existing stack order and source-resolution tests continue to pass; add a
  focused test if the restructure exposes an uncovered branch.

## v0.100.0 Should

### Render Test Helper Polish

Status: Implemented.

Finish the most obvious cleanup left by the shared render test helper.

Acceptance criteria:

- Collapse `svg`/`svg_with_tables` and `render_svg`/`render_result_with_tables`
  onto one naming vocabulary, preferably the `render_*` family already dominant
  in `render.rs`.
- Make `analyze_fixture` assert parse and analysis error diagnostics in both
  named-table and no-named-table paths. If any test intentionally renders with
  errors, give that case an explicit allowing-errors helper.
- Make `RenderFixture` private or replace it with private preparation helpers
  so the common module exposes only the operations tests actually use.
- Migrate the remaining small hand-rolled render-test setup only where the
  common helper fits without weakening test-specific assertions.

### Guide Merge Compile-Time Discipline

Status: Implemented.

Make guide override maintenance fail at compile time when a new field is added.

Acceptance criteria:

- `GuideOverridesIr::merge_with` exhaustively destructures the local override
  parameter before merging fields.
- `GuideIr::with_overrides` similarly exhaustively destructures the override
  parameter or otherwise forces new override fields through the compiler.
- Existing guide override semantics and tests remain unchanged.
- A method rename such as `overridden_by` is optional and should happen only if
  the churn is small and call sites become clearer.

### Small Style And Import Cleanups

Status: Implemented.

Apply the low-risk cleanup that is easiest to verify while the review findings
are fresh.

Acceptance criteria:

- Replace the guarded `temporal_policy.unwrap()` in `algraf-data` inference with
  an `if let Some(policy)` shape.
- Migrate semantics call sites from the local `crate::util::closest` re-export
  to `algraf_core::closest`, then delete the shim if nothing else needs it.
- Consider extracting the repeated `ColumnOnlyTable` unit-test double only if a
  local shared test helper keeps the individual stat tests readable.

## Explicitly Deferred Past v0.100.0

- Broad semantics or render test-file splitting.
- A guide-struct macro or broad guide API rename.
- Git LFS, generated artifact, or example asset policy changes.
- New language features, new renderer algorithms, or output changes unrelated
  to the bandwidth panic fix.
- Publishing new `algraf-wasm` or `algraf-editor` npm package versions.

## Validation

Required for v0.100.0 implementation:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- Focused render regression tests for high-cardinality/sub-pixel-bandwidth
  boxplot and at least one density-based distribution geometry.
- Focused parser tests for keyword typo recovery threshold behavior.
- Focused semantics tests for unknown versus unsupported reducer wording.
- Existing stack-order and source-resolution tests after invariant cleanup.
- Existing render, draw-list, and parity tests after test-helper polish.

Manual image inspection is required only if example output or render snapshots
change. The intended output change surface is limited to avoiding a crash when
bandwidth is below one pixel.

## Promotion Workflow

1. Keep the already-aligned v0.100.0 version stamps in sync while
   implementation proceeds.
2. Fix the distribution bandwidth panic first and add the regression test.
3. Resolve the parser threshold decision with tests before touching unrelated
   parser recovery cleanup.
4. Improve reducer diagnostics with focused analyzer tests.
5. Add debug assertions or type-structured resolution cleanup for invariants.
6. Apply Should-level helper and style cleanup only after the Must fixes are
   green.
7. Run the full required checks and update this plan's `Status:` lines as each
   item lands, defers, or changes scope.
