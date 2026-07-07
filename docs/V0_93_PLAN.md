# Algraf v0.93.0 Plan

Status: Implemented
Target version: 0.93.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_92_PLAN.md`](V0_92_PLAN.md)
Follow-on plans: [`V0_94_PLAN.md`](V0_94_PLAN.md),
[`V0_95_PLAN.md`](V0_95_PLAN.md), [`V0_96_PLAN.md`](V0_96_PLAN.md),
[`V0_97_PLAN.md`](V0_97_PLAN.md), [`V0_98_PLAN.md`](V0_98_PLAN.md),
[`V0_99_PLAN.md`](V0_99_PLAN.md)
Roadmap theme: Make render stat table building single-policy and explicit.

## Purpose

Algraf v0.93 should start the post-v0.92 refactor sequence by removing the
highest-risk duplicated implementation in `algraf-render`: the parallel stat
output column builders in `crates/algraf-render/src/stats/primitive.rs` and
`crates/algraf-render/src/stats/summary.rs`.

This release should not change Algraf source syntax, chart semantics, public
CLI behavior, rendered output, or data-loading behavior. Its job is to turn an
accidental copy-paste divergence into an explicit policy choice.

## Release Thesis

v0.93.0 is a **render stats correctness guardrail** release. It should keep the
current behavior of primitive stat passthrough and summary reducer outputs, but
make the differing integer coercion rules visible in one shared helper:

- primitive passthrough remains strict and does not silently coerce floats into
  integer columns;
- summary reducers may round finite reducer results when writing back into an
  integer-typed output column, because reducer computations are numeric and may
  produce floats.

Future maintainers should be able to see, test, and change that policy in one
place rather than "fixing" one copy to match the other by accident.

## Current Debt Surface

- `stats/primitive.rs` and `stats/summary.rs` each define a payload-only
  `ColumnBuilder` enum with matching `builders_for_schema`,
  `push_passthrough`, `finish_builders`, and `value_to_string` logic.
- The copies have intentionally or accidentally diverged: the summary builder
  rounds finite floats for integer output, while the primitive builder drops
  floats for integer output.
- `stats/util.rs` already exists as the right module boundary for shared stat
  table helpers and deterministic output contracts.
- The existing render and parity tests are well suited to catching output drift,
  but the integer coercion rule needs focused unit coverage so it is not hidden
  inside broad snapshots.

## v0.93.0 Must

### Shared Stat Column Builder

Status: Implemented.

Move the duplicated render stat output builder into `stats/util.rs` or a small
child module owned by `stats/util.rs`.

Acceptance criteria:

- One shared builder owns schema-to-builder construction, passthrough pushing,
  builder finishing, and `DataValue` to string conversion.
- The helper supports every current `algraf_data::DataType` arm used by the two
  existing copies: boolean, integer, float, temporal, string-like fallback, and
  geometry.
- `primitive.rs` and `summary.rs` no longer contain private copies of the full
  builder enum or `value_to_string`.
- Call sites stay small enough that the stat family still owns its actual row
  ordering and reducer logic.
- The shared helper remains `pub(crate)` to `algraf-render`; no public API is
  added for this internal refactor.

### Explicit Integer Coercion Policy

Status: Implemented.

Represent the primitive-vs-summary integer behavior as an explicit option,
`IntCoercion::Strict` and `IntCoercion::RoundFiniteFloats`.

Acceptance criteria:

- Primitive passthrough uses strict integer handling.
- Summary reducer output uses the rounding policy where current behavior already
  rounds finite floats into integer columns.
- The summary call site or helper documentation states why summary uses the
  rounding policy, so the divergence is no longer silent.
- Non-finite floats never become integers under either policy.
- Existing behavior is preserved unless an implementation audit finds a bug; any
  intentional correction must get a focused test and a plan status note.

### Focused Regression Tests

Status: Implemented.

Add tests that lock the behavior being made explicit.

Acceptance criteria:

- A primitive passthrough test covers a float value targeting an integer-typed
  output column and verifies it stays missing rather than rounded.
- A summary reducer test covers a finite float reducer value targeting an
  integer-typed output column and verifies the documented rounding behavior.
- String conversion behavior for at least boolean, integer, float, temporal, and
  geometry values is covered either directly or through existing stat tests.
- Existing render stat tests continue to pass without fixture churn.

## v0.93.0 Should

### Opportunistic Small Stat Helper Cleanup

Status: Implemented.

If it stays mechanical, remove nearby small scaffolding duplication in
`stats/bin.rs`, `stats/density.rs`, and `stats/zfield.rs`.

Acceptance criteria:

- Only extract helpers whose behavior is already identical and covered by tests.
- Do not mix algorithm changes into this release.
- Leave lower-confidence grid or accumulation rewrites deferred to v0.97 or a
  later maintenance release.

## Explicitly Deferred Past v0.93.0

- The `algraf-data` Arrow/Parquet conversion builder extraction; see
  [`V0_96_PLAN.md`](V0_96_PLAN.md).
- The public render API options struct; see [`V0_94_PLAN.md`](V0_94_PLAN.md).
- Registry, completion, and analyzer language-surface source-of-truth work; see
  [`V0_95_PLAN.md`](V0_95_PLAN.md).
- Geometry, domain-pipeline, and larger render helper extractions; see
  [`V0_97_PLAN.md`](V0_97_PLAN.md).

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- Focused `algraf-render` stat tests for integer coercion policy.
- Existing draw-list and render parity tests, especially any summary and
  primitive stat fixtures.

Package-version note: `npm view algraf-wasm versions --json` and
`npm view algraf-editor versions --json` showed no published `0.93.0`
browser packages during implementation, so `algraf-wasm`, `algraf-editor`, and
consumer pins remain on the latest verified published 0.92.x package versions.

## Promotion Workflow

1. When implementation begins, align version stamps for v0.93.0 according to
   `AGENTS.md`; this plan file alone is planning guidance.
2. Extract the shared builder with no call-site behavior change.
3. Add the explicit integer coercion option and wire current primitive and
   summary behavior through it.
4. Add focused tests before deleting the old private copies.
5. Run the full required checks.
6. When complete, update each `Status:` line, align release stamps, and record
   the maintenance release in the spec milestone table if that table tracks
   internal-only releases.
