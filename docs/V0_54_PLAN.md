# Algraf v0.54.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_53_PLAN.md`](V0_53_PLAN.md)
Roadmap theme: browser demo editor polish.

## Purpose

v0.54.0 is a narrow browser-demo maintenance release. It keeps Monaco hover
content readable when the editor sits inside clipped panels on the landing,
docs, and demos routes.

## Scope

### Monaco Hover Overflow

Status: Implemented.

The browser demo host configures Monaco so hover and other overflow widgets can
paint outside the editor viewport instead of being clipped by the editor pane.

Acceptance criteria:

- Monaco-backed editors in `/`, `/docs`, and `/demos` MUST keep shared Algraf
  hover content visible outside the editor viewport when the surrounding panel
  clips its own layout.
- The change MUST remain a host UI setting and MUST NOT alter the
  `algraf-wasm` editor-service ABI, Rust hover selection logic, or LSP-shaped
  hover payloads.

### Release Version Alignment

Status: Implemented.

Workspace, extension, demo, lockfile, and specification version stamps are
aligned to `0.54.0` for this release.

Acceptance criteria:

- `Cargo.toml` and `Cargo.lock` record workspace crates at `0.54.0`.
- `docs/ALGRAF_SPEC.md` records `0.54.0` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- `editors/vscode/package.json`, `editors/vscode/package-lock.json`,
  `demo/package.json`, and `demo/package-lock.json` record `0.54.0`.

## Non-Goals

- No new `.ag` syntax.
- No renderer, analyzer, parser, LSP, or editor-service behavior changes.
- No route, preset, dataset, or deployment behavior changes.

## Validation

Required checks:

```bash
cd demo && npm run check
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```
