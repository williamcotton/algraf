# Algraf v0.53.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_52_PLAN.md`](V0_52_PLAN.md)
Roadmap theme: language landing site and demo navigation.

## Purpose

v0.53.0 turns the static browser demo from a single full-demo screen into a
small language site. The site keeps the existing WASM demos, adds a landing
page with a simple live demo, and adds a guided docs route for first use.

## Scope

### Landing Page and Route Shell

Status: Implemented.

The demo app exposes a light-themed site shell with stable static routes and a
first-screen live Algraf example.

Acceptance criteria:

- The app MUST expose `/` as the landing page, `/docs` as a guided quickstart
  page, and `/demos` as the full browser demos route.
- The landing page MUST show a small editable Algraf source example near the
  top and render it through the existing `algraf-wasm` runtime.
- The landing page MUST document how to render the same checked-in chart source
  to SVG and PNG through the native CLI.
- Navigation MUST account for Vite's configured public base path so GitHub
  Pages project deployments under `/<repo>/` work.

### Demos Preservation

Status: Implemented.

The existing gallery route moves to `/demos` without losing the Monaco
editor, preset selector, bundled data editor, render diagnostics, or
interactive SVG preview.

Acceptance criteria:

- The `/demos` page MUST keep all existing chart presets and bundled data file
  behavior.
- The demos route MUST continue to pass host-supplied data files into the WASM
  runtime and editor-service providers as in v0.52.

### Quickstart Docs and Static Fallback

Status: Implemented.

The static site includes a guided docs page and supports direct clean-path
loads on GitHub Pages.

Acceptance criteria:

- `/docs` MUST include a Monaco-backed guided tutorial, live preview, basic CLI
  commands, browser demo commands, and links to the full specification and
  demos route.
- The demo build MUST emit a `404.html` fallback mirroring `index.html` so
  direct visits to `/docs` and `/demos` load the browser app on GitHub Pages.
- `demo/README.md` MUST describe the route layout and fallback behavior.

### Release Version Alignment

Status: Implemented.

Workspace, extension, demo, lockfile, and specification version stamps are
aligned to `0.53.0` for this release.

Acceptance criteria:

- `Cargo.toml` and `Cargo.lock` record workspace crates at `0.53.0`.
- `docs/ALGRAF_SPEC.md` records `0.53.0` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- `editors/vscode/package.json`, `editors/vscode/package-lock.json`,
  `demo/package.json`, and `demo/package-lock.json` record `0.53.0`.

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
