# Algraf v0.84.1 Plan

Status: Implemented
Target version: 0.84.1
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_84_PLAN.md`](V0_84_PLAN.md)

## Purpose

Algraf v0.84.1 is a focused LSP/VS Code preview maintenance patch. It restores
live preview refreshes for generated data files whose chart source uses
parent-directory path segments, such as `Chart(data: "../outputs/data.csv")`.

The regression shape is narrow: editing or live-typing the `.ag` document still
refreshes the preview, because document changes schedule an `algraf/preview`
request. Rewriting the generated CSV does not reliably refresh, because the
preview dependency path reported to the VS Code watcher can retain lexical
`..` segments. File-system events arrive for the normalized path, so the watcher
may not match the changed file.

This patch does not change `.ag` syntax, rendering semantics, data loading,
diagnostics, CLI behavior, browser WASM behavior, or the preview render
pipeline.

## Scope

### Normalize Preview Data Dependency Paths

Status: Implemented.

Acceptance criteria:

- The `algraf/preview` result's `dataPaths` entries are resolved and lexically
  normalized before serialization, removing `.` and reducible `..` path
  segments without consulting the filesystem.
- The VS Code preview client also normalizes returned `dataPaths` before
  comparing watcher sets and registering `FileSystemWatcher` patterns.
- A chart such as `Chart(data: "../outputs/data.csv")` renders successfully and
  reports the normalized dependency path for watcher registration.
- Manual preview refresh and document-edit refresh behavior remain unchanged.

## Validation

- `cargo fmt --all --check`
- `cargo test -p algraf-lsp preview_reports_normalized_data_dependency_paths`
- `npm run check` from `editors/vscode`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
