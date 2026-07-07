# Algraf v0.96.0 Plan

Status: Implemented
Target version: 0.96.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_95_PLAN.md`](V0_95_PLAN.md)
Follow-on plan: [`V0_97_PLAN.md`](V0_97_PLAN.md)
Roadmap theme: Share Arrow-family table conversion in `algraf-data`.

## Purpose

Algraf v0.96 should remove the duplicated Arrow-to-`DataFrame` conversion logic
in `algraf-data` while keeping the important semantic difference between native
Parquet loading and Arrow IPC stream loading explicit.

This release should not change which file formats are available in which build.
Native data-engine features remain CLI/native concerns, and WASM builds must
continue to exclude native-only backends unless a separate plan changes that.

## Release Thesis

v0.96.0 is a **data conversion deduplication** release. Parquet and Arrow IPC
stream readers both receive Arrow `RecordBatch` values and build the same
Algraf `DataFrame` representation. The conversion mechanics should live once,
while format-specific policy stays visible:

- Arrow IPC stream should continue to error on unsupported Arrow types.
- Parquet should continue to apply its current fallback policy for unsupported
  types.
- Any Float16 behavior discovered during the extraction must be locked by tests
  before being preserved or corrected.

## Current Debt Surface

- `crates/algraf-data/src/parquet.rs` and
  `crates/algraf-data/src/arrow_stream.rs` each define near-identical
  `ColumnBuilder` enums and append helpers.
- The modules differ mainly in Arrow type policy: stream loading returns
  unsupported-type errors, while Parquet maps more unsupported shapes to string.
- `arrow_stream.rs` is compiled with stub public functions when the
  `arrow-stream` feature is disabled; the shared conversion module must preserve
  that build shape.
- Feature gates are deliberately strict: `parquet`, `arrow-array`,
  `arrow-ipc`, and `arrow-schema` are optional dependencies.

## v0.96.0 Must

### Shared `arrow_convert` Module

Status: Implemented.

Add an internal shared conversion module for Arrow `SchemaRef` and
`RecordBatch` values.

Acceptance criteria:

- The shared module is compiled only when one of the Arrow-family features needs
  it, for example `#[cfg(any(feature = "parquet", feature = "arrow-stream"))]`.
- The module owns the common `ColumnBuilder`, schema definition construction,
  batch append loop, typed array downcasts, examples, reserve behavior, and
  final `Column` construction.
- `parquet.rs` and `arrow_stream.rs` keep only format-specific reader setup,
  projection, error mapping, feature stubs, and policy selection.
- No Arrow, Parquet, or IPC symbols leak out of `algraf-data` or into
  `algraf-render`, `algraf-semantics`, editor services, LSP, CLI command
  parsing, or WASM public APIs.

### Explicit Unsupported-Type Policy

Status: Implemented.

Represent the Parquet-vs-stream type behavior as an explicit policy.

Acceptance criteria:

- The policy has names that describe behavior, such as
  `UnsupportedTypePolicy::Error` and `UnsupportedTypePolicy::FallbackToString`.
- Arrow IPC stream uses the error policy.
- Parquet uses the fallback policy that matches current behavior.
- Unsupported nested, dictionary, list, struct, binary, decimal, and other
  non-scalar types have focused coverage for both policies where feasible.
- Error messages remain at least as specific as today.

### Float And Temporal Behavior Audit

Status: Implemented.

Implementation note: v0.96.0 preserves the existing Float16 split. Arrow IPC
stream schema loading rejects Float16 through the unsupported-type error policy.
Parquet schema loading advertises Float16 as `Float`, but value materialization
still fails with a Parquet unsupported-type error. Focused tests lock that
behavior before any future Float16 support change.

Lock the subtle Arrow type behaviors before deleting the old copies.

Acceptance criteria:

- Float32 and Float64 behavior is covered for both readers.
- Float16 behavior is audited against current code and Arrow crate support. If
  the current code only advertises Float16 as a float but later errors while
  appending, decide explicitly whether v0.96 preserves that behavior or fixes it
  with a documented test.
- Date32, Date64, and timestamp unit conversions are covered by focused tests or
  existing tests named in the implementation notes.
- UInt64 values larger than `i64::MAX` keep the existing saturation behavior
  unless maintainers intentionally change it with a test and status note.

## v0.96.0 Should

### Feature-Matrix Checks

Status: Implemented.

Run or add compile checks for important feature combinations.

Acceptance criteria:

- Default `algraf-data` build still compiles.
- `algraf-data --no-default-features` still compiles.
- `algraf-data --no-default-features --features arrow-stream` compiles.
- `algraf-data --no-default-features --features parquet` compiles.
- Full workspace checks still pass with default features.

### Shared Error Helpers

Status: Implemented.

Share only the Arrow error helpers that are truly identical.

Acceptance criteria:

- Format-specific errors still identify whether the failure came from Parquet or
  Arrow stream loading.
- Downcast and unsupported-type helpers are shared only where their output
  remains correct for both callers.

## Explicitly Deferred Past v0.96.0

- Lazy or streaming table execution.
- New Arrow-family formats, Arrow file support, or dataset scanning.
- WASM support for native Parquet or Arrow loaders.
- A new public `algraf-data` conversion API; this is an internal extraction.

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- `cargo test -p algraf-data --no-default-features`
- `cargo test -p algraf-data --no-default-features --features arrow-stream`
- `cargo test -p algraf-data --no-default-features --features parquet`
- Focused data tests for unsupported-type policy, numeric conversion, temporal
  conversion, projection, and disabled-feature stubs.

## Promotion Workflow

1. Align version stamps for v0.96.0 when implementation begins.
2. Add `arrow_convert` behind the right feature gates.
3. Move one reader to the shared module while tests still cover old behavior.
4. Move the second reader and delete the duplicate builder.
5. Add policy and feature-matrix tests.
6. Run the full required checks and mark this plan's statuses.
