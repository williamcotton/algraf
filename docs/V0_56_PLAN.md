# Algraf v0.56.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_55_PLAN.md`](V0_55_PLAN.md)
Roadmap theme: repository CI visibility and test-suite automation.

## Purpose

v0.56.0 is a narrow repository maintenance release. It adds a visible test-suite
status badge to the root README and introduces a GitHub Actions workflow that
runs the required Rust validation suite on pushes and pull requests.

## Scope

### Test Suite Workflow

Status: Implemented.

The repository has a `CI` GitHub Actions workflow that checks formatting, runs
clippy over all workspace targets, and runs the full workspace test suite.

Acceptance criteria:

- The root README MUST display a test-suite status badge for the `CI` workflow.
- The `CI` workflow MUST run on pushes to `main`, pull requests, and manual
  dispatch.
- The `CI` workflow MUST run `cargo fmt --all --check`.
- The `CI` workflow MUST run `cargo clippy --workspace --all-targets` with
  warnings denied.
- The `CI` workflow MUST run `cargo test --workspace`.

### Release Version Alignment

Status: Implemented.

Workspace, extension, demo, lockfile, and specification version stamps are
aligned to `0.56.0` for this maintenance release.

Acceptance criteria:

- `Cargo.toml` and `Cargo.lock` record workspace crates at `0.56.0`.
- `docs/ALGRAF_SPEC.md` records `0.56.0` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- `editors/vscode/package.json`, `editors/vscode/package-lock.json`,
  `demo/package.json`, and `demo/package-lock.json` record `0.56.0`.

## Non-Goals

- No `.ag` syntax changes.
- No parser, analyzer, renderer, LSP, CLI, or WASM runtime behavior changes.
- No changes to historical completed release plans.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```
