# Algraf v0.63.0 Plan

Status: Implemented
Target version: 0.63.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_62_PLAN.md`](V0_62_PLAN.md)
Roadmap theme: Shared editor assets and first-party Monaco integration.

## Purpose

v0.63.0 mirrors the PDL browser-package plan for Algraf. The release goal is to
stop duplicating static grammar assets, browser editor wiring, and ad hoc WASM
download logic across the Algraf VS Code extension, Algraf demo, Studio, and
external npm consumers.

The VS Code extension should remain a thin package-local language client, while
canonical TextMate grammar and language configuration assets live under
`editors/assets/`. A new `editors/monaco/` integration should provide the shared
browser editor setup used by the Algraf demo and future Studio versions, and a
new `packages/wasm/` package should provide the browser runtime loader and
packaged WASM artifact for npm users.

No `.ag` syntax, rendering, scale, guide, data, CLI, or LSP behavior changes are
planned for this release.

## Scope

### Canonical Editor Assets

Status: Implemented.

`editors/assets/algraf.tmLanguage.json` and
`editors/assets/language-configuration.json` become the source of truth for
Algraf static highlighting and editor typing behavior.

Acceptance criteria:

- VS Code and Monaco consume equivalent static grammar and language
  configuration content.
- The canonical assets are documented as the files to update when Algraf syntax,
  keyword, geometry, property, or punctuation highlighting changes.

### VS Code Asset Sync

Status: Implemented.

The VS Code extension remains self-contained by syncing canonical assets into
`editors/vscode/syntaxes/algraf.tmLanguage.json` and
`editors/vscode/language-configuration.json` before extension compile, check,
package, or prepublish workflows.

Acceptance criteria:

- `editors/vscode/package.json` continues to contribute package-local grammar
  and language configuration paths.
- A repeatable sync step keeps package-local VS Code assets aligned with
  `editors/assets/`.
- VS Code package checks still work from `editors/vscode/`.

### First-Party Monaco Integration

Status: Implemented.

`editors/monaco/` becomes a package-shaped TypeScript/React integration for
Algraf Monaco hosts and publishes as the org-free npm package `algraf-editor`.

Acceptance criteria:

- The integration exports reusable language registration, TextMate grammar
  wiring, language configuration, theme, marker conversion, provider
  registration, and structural runtime/editor-service types.
- The integration does not implement Algraf parsing, analysis, rendering,
  diagnostics, completion, hover, formatting, semantic tokens, code actions,
  symbols, definition/reference, or rename in TypeScript.
- The integration declares compatible runtime/editor peer dependencies and is
  suitable for npm publication as `algraf-editor`.
- The integration exposes reusable Monaco setup/provider helpers and a thin
  React editor component that accepts host-provided runtime, files, diagnostics,
  value, change handler, and model URI.
- The integration does not own execution buttons, preview panels, routing,
  example state, Studio story state, or WASM build/download policy.
- The integration provides useful defaults for grammar, language configuration,
  theme, Monaco registration, and provider wiring while allowing callers to
  override the runtime object, model URI, files map, diagnostics, editor
  options, theme name or theme definition, and provider lifecycle.
- The integration exports layered APIs: a high-level React editor for common
  hosts and lower-level setup/provider helpers for hosts that already manage
  Monaco models.

### Browser WASM Runtime Package

Status: Implemented.

`packages/wasm/` becomes a package-shaped TypeScript integration for browser
runtime loading and publishes as the org-free npm package `algraf-wasm`.

Acceptance criteria:

- The package exports runtime loader helpers, browser ABI types, and a generated
  `algraf.wasm` artifact in the npm tarball.
- The package exposes an explicit way to load a caller-provided WASM URL or
  generated local artifact for demos, Studio, and custom hosts.
- The package does not include Monaco, React, preview UI, editor chrome, or
  product-specific controls.
- The package exposes structural runtime/editor-service types that can be used
  by `algraf-editor` or by custom hosts without requiring a specific demo module.
- The generated WASM artifact is produced by release/build scripts and is not
  checked into the source repository.
- `algraf-wasm` and `algraf-editor` versions align with the Algraf release
  version, e.g. `0.63.0`.
- Existing GitHub Release WASM assets remain available for non-npm consumers.

### Unpublished Local Development

Status: Implemented.

Cross-repo development works without publishing `algraf-wasm` or
`algraf-editor` to npm.

Acceptance criteria:

- The repo documents source mode for daily iteration: the Algraf demo and
  Studio consume sibling package source or package build output directly from
  `../algraf`, while the WASM loader receives a caller-provided local `wasmUrl`
  pointing at a generated artifact copied into the host app's public assets.
- The repo documents packed mode for release validation: `algraf-wasm` and
  `algraf-editor` are built and packed into local npm tarballs outside tracked
  source, then installed into the demo or Studio with `file:` paths to
  approximate a real npm install before publishing.
- The workflow supports generated local WASM artifacts without checking them
  into the source repository.
- Package-level validation can approximate npm installs before publishing.
- `npm link` may be documented as an advanced option, but source mode and packed
  mode are the primary workflows to avoid accidental React or Monaco
  duplicate-dependency issues.

### Demo Consumption

Status: Implemented.

The Algraf browser demo consumes `editors/monaco/` instead of maintaining a
parallel local Algraf editor component, provider adapter, grammar import, or
theme definition.

Acceptance criteria:

- Demo behavior remains unchanged for loading WASM, editing source, running
  examples, rendering SVG, showing diagnostics, and using editor-service
  features.
- Demo highlighting continues to match the VS Code TextMate grammar scopes.

### Release Version Alignment

Status: Implemented.

Workspace, extension, demo, lockfile, and specification version stamps are
aligned to `0.63.0` when this implementation lands.

Acceptance criteria:

- `Cargo.toml` and `Cargo.lock` record workspace crates at `0.63.0`.
- `docs/ALGRAF_SPEC.md` records `0.63.0` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- `editors/vscode/package.json`, `editors/vscode/package-lock.json`,
  `demo/package.json`, and `demo/package-lock.json` record `0.63.0`.

## Non-Goals

- No `.ag` syntax changes.
- No new geometry, scale, guide, data source, renderer behavior, CLI flag, or
  LSP feature.
- No npm organization or scoped package-name requirement.
- No root JavaScript monorepo/workspace, unless package iteration proves it is
  needed.
- No Studio migration in this repository; Studio consumption belongs in the
  Studio plan that depends on Algraf v0.63 or a future published package.
- No changes to historical completed release plans.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Additional validation:

- From `editors/vscode/`, run the asset sync and VS Code package checks.
- From `packages/wasm/`, run the WASM package build/typecheck and verify the
  publish tarball includes `algraf.wasm` while the repository does not track the
  generated binary.
- From `editors/monaco/`, run `algraf-editor` package type/build checks once
  scripts exist.
- Validate at least one unpublished local-consumption workflow for the Algraf
  demo and Studio, using sibling source, local package builds, linked packages,
  or local tarballs.
- From `demo/`, run:

  ```bash
  npm run check
  npm run build
  ```

- Browser review should confirm the demo editor uses the shared grammar/theme,
  and diagnostics, hover, completion, signature help, formatting, semantic
  tokens, code actions, symbols, definition/reference, and rename still come
  from the upstream Algraf editor service.
