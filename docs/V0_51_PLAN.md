# Algraf v0.51.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_50_PLAN.md`](V0_50_PLAN.md)
Follow-on plan: [`V0_52_PLAN.md`](V0_52_PLAN.md)
Roadmap theme: editor diagnostics and planning artifact discipline.

## Purpose

v0.51.0 is a maintenance release for caller-provided input workflows and release
process hygiene. It keeps `Chart(data: input)` useful in editors where the data
bytes are supplied only at render/invocation time, and it documents that new
work gets a current versioned plan artifact even when implementation starts
before planning is written down.

## Scope

### Caller-Input Editor Diagnostics

Status: Implemented.

When an editor or LSP session sees `Chart(data: input)` or the compatibility
alias `Chart(data: stdin)` without caller-provided data bytes or an injected
schema, it treats the primary table schema as unknown rather than empty.

Acceptance criteria:

- The editor/LSP MUST NOT publish `E1101` unknown-column diagnostics for primary
  table column references that can only be validated against caller-provided
  runtime data.
- The editor/LSP MUST continue to report syntax diagnostics, unknown properties,
  invalid geometry names, invalid options, named-table schema diagnostics, and
  other diagnostics that do not depend on a missing caller-input schema.
- CLI and embedded render/check paths that receive actual caller-provided bytes
  continue to infer a schema and validate columns normally.
- Tests cover `Chart(data: input)` in the LSP and prove a genuine non-column
  semantic diagnostic is still reported while column diagnostics are suppressed.

### Stdin Conflict Diagnostic Hygiene

Status: Implemented.

Partial preparation reports source-level stdin conflicts without continuing into
empty-schema column validation.

Acceptance criteria:

- `algraf check -` with `Chart(data: stdin)` reports the stdin conflict and does
  not add unrelated `E1101` diagnostics for `Space(...)` columns.
- Driver partial-preparation tests and CLI integration tests cover the behavior.

### Planning Artifact Discipline

Status: Implemented.

Repository guidance and the specification state that feature or maintenance work
gets a current versioned plan artifact. Completed historical plans are not
reopened for new implementation work.

Acceptance criteria:

- `AGENTS.md` and `CLAUDE.md` carry byte-similar guidance.
- `docs/ALGRAF_SPEC.md` records the rule in release planning.
- This v0.51 plan is the artifact for the maintenance fixes above.

## Non-Goals

- No new `.ag` syntax.
- No change to runtime schema validation when caller-provided bytes are
  available.
- No release version bump until the release is cut.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```
