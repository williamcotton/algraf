# Algraf v0.98.0 Plan

Status: Implemented
Target version: 0.98.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_97_PLAN.md`](V0_97_PLAN.md)
Follow-on plan: [`V0_99_PLAN.md`](V0_99_PLAN.md)
Roadmap theme: Consolidate semantics helpers and shared core utilities.

## Purpose

Algraf v0.98 should remove duplicated helper logic in `algraf-semantics` and
move genuinely cross-crate utility functions to `algraf-core`.

This release should preserve diagnostics, spans, analyzer behavior, and editor
behavior. If a helper extraction reveals existing drift, the drift should be
named, tested, and fixed only when maintainers intentionally choose the new
behavior.

## Release Thesis

v0.98.0 is a **semantic helper consolidation** release. The analyzer is already
split by concern, but a few large files still repeat small parsing and lowering
patterns. Moving shared helpers into local modules, and moving cross-crate
helpers into `algraf-core`, should make future language changes safer without
rewriting the analyzer architecture.

## Current Debt Surface

- `crates/algraf-semantics/src/analyzer/stats.rs` repeats reducer argument
  parsing and duplicate-argument skeletons across summary-style stats.
- `crates/algraf-semantics/src/analyzer/properties.rs` has nearly identical
  `interaction_key` and `event_emit_column` methods.
- `crates/algraf-semantics/src/analyzer/context.rs` duplicates document and
  local `let` duplicate-binding handling.
- `crates/algraf-semantics/src/analyzer/lowering.rs` repeats grouped histogram
  geometry and derived-space construction.
- `GuidesIr` and `GuideOverridesIr` mirror many fields by hand, and guide merge
  logic maps those fields one by one.
- Levenshtein/edit-distance helpers live separately in syntax and semantics.
- `is_url_like` lives separately in semantics property validation and render
  asset loading.

## v0.98.0 Must

### Shared Reducer Argument Parsing

Status: Implemented.

Implementation note: `Summary2D`/`SummaryHex` now enforce the spec's z-field
reducer set and reject `mean_se`; `Summary` and `SummaryBin` continue to accept
`mean_se`.

Extract reducer parsing and validation helpers in `analyzer/stats.rs`.

Acceptance criteria:

- Summary-family stats call one helper for the `"reducer"` argument value shape,
  string extraction, allowed-name parsing, and diagnostic construction.
- Allowed reducer sets remain explicit at call sites so per-stat differences
  such as `mean_se` support are visible and testable.
- Existing diagnostic codes, spans, and messages remain stable unless a copied
  branch is intentionally corrected.
- Tests cover at least one valid reducer, one invalid reducer, one wrong-form
  reducer, and the per-stat reducer-set difference that currently risks drift.

### Shared Column-Name Argument Helper

Status: Implemented.

Parameterize the duplicated `interaction_key` and `event_emit_column` logic.

Acceptance criteria:

- One helper resolves bare column names and string-literal column names for both
  properties.
- The wrong-form diagnostic remains property-specific: `interactionKey` and
  event emit columns keep their own codes and help text.
- Unknown-column diagnostics and unknown-columns-passthrough behavior remain
  unchanged.
- Tests cover both properties for valid bare names, valid string names,
  unknown names, passthrough, and wrong value forms.

### Cross-Crate Utilities In `algraf-core`

Status: Implemented.

Move shared utility logic that has no syntax, semantics, data, or render
dependency into `algraf-core`.

Acceptance criteria:

- One Unicode-aware edit-distance implementation and a `closest` helper live in
  `algraf-core`.
- Parser and analyzer diagnostics use the shared closest-match helper, or the
  plan documents why one caller needs different ASCII-only behavior.
- One `is_url_like` helper lives in `algraf-core`.
- Semantics property validation and render asset loading use the same URL-like
  check.
- Tests include non-ASCII closest-match behavior and URL-like strings accepted
  or rejected by both former callers.

## v0.98.0 Should

### Let Declaration Deduplication

Status: Implemented.

Extract the duplicate-binding loop shared by document-scope and local
`let` collection.

Acceptance criteria:

- One helper handles duplicate detection, span reporting, and returned binding
  order.
- Scope-specific storage remains clear: document bindings and chart/space
  bindings still populate the right maps.
- Existing `E1702` diagnostics remain stable.

### Guide Struct And Merge Maintenance

Status: Implemented.

Reduce the manual coupling between `GuidesIr`, `GuideOverridesIr`, and merge
logic.

Acceptance criteria:

- At minimum, colocate the merge function directly next to the structs and add a
  comment that new guide fields must update all three places.
- Prefer a small declarative macro if it can generate `GuidesIr`,
  `GuideOverridesIr`, and merge behavior from one field list without making
  debugging or rustdoc worse.
- Add a test that fails when a guide override key is parsed but not merged.
- Do not change guide semantics.

### Grouped Histogram Lowering Cleanup

Status: Implemented.

Remove duplicated grouped-histogram lowering construction.

Acceptance criteria:

- Branches compute the mappings that differ, then construct the shared
  `GeometryIr` and derived space once.
- Existing histogram, summary bin, and grouped histogram tests remain stable.
- The refactor does not change derived table names, spans, or guide behavior.

### Semantics File-Size Follow-Up Notes

Status: Implemented.

Implementation note: `analyzer/stats.rs` remains large after helper extraction,
but this release intentionally avoided a broad module split. A future split
should separate stat families only after the shared helper behavior remains
stable.

If `analyzer/stats.rs` remains hard to navigate after helper extraction, leave a
short follow-up note for a future split into stat-family modules.

Acceptance criteria:

- Do not perform a broad module split in the same change unless helper
  extraction has already landed cleanly.
- Any future split should mirror the renderer's stat-family organization where
  that helps readers.

## Explicitly Deferred Past v0.98.0

- A full analyzer rewrite.
- New diagnostic-code enum types.
- Changes to user-facing language behavior.
- Test-suite file splitting and broad fixture helper work; see
  [`V0_99_PLAN.md`](V0_99_PLAN.md).

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- Focused semantics tests for reducer parsing, `interactionKey`, event emit
  columns, duplicate lets, guide override merging, and grouped histogram
  lowering.
- Focused syntax/parser and semantics tests for closest-match behavior after the
  edit-distance move.
- Focused render and semantics tests for URL-like strings after `is_url_like`
  moves to core.

## Promotion Workflow

1. Align version stamps for v0.98.0 when implementation begins.
2. Move `closest`/edit-distance and `is_url_like` to core first because they are
   small, easily tested cross-crate utilities.
3. Extract local analyzer helpers with focused tests after each move.
4. Decide whether the guide-struct macro is worth it; otherwise land the
   colocated merge/comment/test minimum.
5. Run the full required checks and mark statuses.
