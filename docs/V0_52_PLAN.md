# Algraf v0.52.0 Plan

Status: In progress
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_51_PLAN.md`](V0_51_PLAN.md)
Roadmap theme: static browser demo deployment.

## Purpose

v0.52.0 makes the root browser demo deployable as a static GitHub Pages project
site without committing generated build output. It also starts a fresh current
release plan for this deployment work and aligns repository version stamps to
`0.52.0`.

## Scope

### GitHub Pages Demo Deployment

Status: Implemented.

The root browser demo can be deployed to GitHub Pages from the repository's
`main` branch without committing generated `demo/dist` files or a generated
WASM binary.

Acceptance criteria:

- The demo MUST resolve public `wasm/` and `data/` assets through Vite's
  configured public base path, so project Pages deployments under `/<repo>/`
  work as well as user/organization Pages deployments at `/`.
- A GitHub Actions workflow MUST install the Rust stable WASM target, install
  demo npm dependencies with the lockfile, run the existing demo build, and
  publish `demo/dist` as a GitHub Pages artifact.
- The workflow MUST compute `/` for `owner.github.io` repositories and
  `/<repo>/` for ordinary project repositories.
- The README and demo README MUST state the deployed URL shape.

### Release Version Alignment

Status: Implemented.

Workspace, extension, demo, lockfile, and specification version stamps are
aligned to `0.52.0` when this plan becomes the current implementation target.

Acceptance criteria:

- `Cargo.toml` and `Cargo.lock` record workspace crates at `0.52.0`.
- `docs/ALGRAF_SPEC.md` records `0.52.0` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- `editors/vscode/package.json`, `editors/vscode/package-lock.json`,
  `demo/package.json`, and `demo/package-lock.json` record `0.52.0`.

### Planning Guidance Clarification

Status: Implemented.

Repository guidance makes explicit that new work outside the current plan's
declared purpose/scope starts the next minor release plan rather than appending
to the previous release.

Acceptance criteria:

- `AGENTS.md` and `CLAUDE.md` carry byte-similar guidance.
- `docs/ALGRAF_SPEC.md` records the same release-planning rule.

## Non-Goals

- No new `.ag` syntax.
- No renderer, analyzer, parser, or LSP behavior changes.
- No new public npm package for the browser demo.
- No custom domain configuration.

## Validation

Required checks:

```bash
cd demo && npm run check
cd demo && VITE_BASE_PATH=/algraf/ npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```
