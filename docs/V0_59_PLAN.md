# Algraf v0.59.0 Plan

Status: Implemented
Target version: 0.59.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_58_PLAN.md`](V0_58_PLAN.md)
Follow-on plan: [`V0_60_PLAN.md`](V0_60_PLAN.md)
Roadmap theme: CI artifacts for distributable editor and browser outputs.

## Purpose

Algraf v0.59 opens after the v0.58 text label decluttering release. Its release
theme is publishing distributable editor and browser artifacts from GitHub
Actions before adding new language, runtime, editor, or browser surface.

Maintenance fixes may be promoted into this plan as they land, but the spec,
plan, code, tests, examples, and version stamps must remain aligned in the same
change.

Deferred language, runtime, editor, and browser themes remain candidates for
later releases: richer browser output controls, package publication flows,
full LSP code actions and cross-document navigation, broader plugin and
sandboxing work, additional source connector work, or later renderer and layout
features.

## Must

- Use CI artifact publication as the narrow v0.59 implementation theme before
  adding new language or runtime surface.

  Status: Implemented. This plan promotes artifact publication as the release
  theme; no new language or runtime surface is added.

- Publish CI build artifacts for distributable editor and browser outputs.

  Status: Implemented for 0.59.0. The CI workflow packages the VS Code `.vsix`
  and the standalone browser `algraf.wasm`, verifies that the VS Code extension
  and demo package versions match the Rust workspace `algraf-wasm` crate
  version, and uploads one VSIX artifact and one WASM artifact. Each uploaded
  artifact contains both a versioned file and a `latest` alias.

- Align release version stamps.

  Status: Implemented. Workspace, extension, demo, lockfile, and specification
  version stamps are aligned to `0.59.0`.

## Should

- Preserve v0.58 shipped behavior while landing maintenance fixes.

  Status: Implemented. No behavior-changing maintenance fixes were promoted
  beyond CI artifact publication and release version alignment.

## Deferred

- Package publication to the VS Code Marketplace or npm.
- Standalone browser output controls beyond publishing `algraf.wasm`.
- Full LSP code actions and cross-document navigation.
- Broader plugin, extension, and sandboxing work.
- Additional local or remote source connector work.
- Later renderer and layout feature work.
