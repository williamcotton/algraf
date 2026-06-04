# Algraf v0.60.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_59_PLAN.md`](V0_59_PLAN.md)
Roadmap theme: GitHub Release assets for distributable editor and browser outputs.

## Purpose

v0.60.0 corrects the v0.59 distributable packaging path. The VS Code extension
`.vsix` and standalone browser `algraf.wasm` are public downloads, so they are
published as GitHub Release assets rather than temporary GitHub Actions
artifacts.

This release keeps the language, runtime, editor, LSP, renderer, WASM ABI,
browser demo, examples, and generated graphics stable while moving the packaged
outputs to durable release assets with both versioned filenames and `latest`
aliases.

## Scope

### Release Asset Publication

Status: Implemented.

The CI workflow packages the VS Code `.vsix` and standalone browser
`algraf.wasm`, verifies that the VS Code extension and demo package versions
match the Rust workspace `algraf-wasm` crate version, and uploads those files to
the GitHub Release tagged for the current workspace version.

Acceptance criteria:

- CI MUST continue to package the VS Code extension with `npm run package`.
- CI MUST continue to build the browser WASM runtime with `npm run build:wasm`.
- CI MUST verify that the VS Code extension, demo package, and Rust workspace
  WASM crate versions match before publishing.
- Release assets MUST include `algraf-vscode-<version>.vsix`,
  `algraf-vscode-latest.vsix`, `algraf-wasm-<version>.wasm`, and
  `algraf-wasm-latest.wasm`.
- Pull requests MUST build the distributable files without publishing a GitHub
  Release.

### Release Version Alignment

Status: Implemented.

Workspace, extension, demo, lockfile, README, and specification version stamps
are aligned to `0.60.0` for this maintenance release.

Acceptance criteria:

- `Cargo.toml` and `Cargo.lock` record workspace crates at `0.60.0`.
- `docs/ALGRAF_SPEC.md` records `0.60.0` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- `editors/vscode/package.json`, `editors/vscode/package-lock.json`,
  `demo/package.json`, and `demo/package-lock.json` record `0.60.0`.
- The root README links the latest release downloads for the VSIX and WASM
  assets.

## Non-Goals

- No `.ag` syntax changes.
- No parser, analyzer, renderer, LSP, CLI, WASM ABI, browser demo, or examples
  behavior changes.
- No VS Code Marketplace publication.
- No npm package publication for the WASM runtime.
- No changes to historical completed release plans.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
cd editors/vscode && npm run package
cd demo && npm run build:wasm
```
