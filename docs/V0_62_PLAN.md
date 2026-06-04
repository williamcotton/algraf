# Algraf v0.62.0 Plan

Status: Implemented
Target version: 0.62.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_61_PLAN.md`](V0_61_PLAN.md)
Roadmap theme: Renderer correctness for sparse story-chart tables.

## Purpose

v0.62.0 fixes stacked and fill-normalized `Area` rendering for sparse grouped
tables. Prepared story tables often omit zero-valued group/date cells; Algraf
should render those omitted cells as zero contributions rather than cutting
short a group's area path and leaving visible holes between stacked bands.

## Scope

### Sparse Stacked Area Continuity

Status: Implemented.

Grouped `Area(layout: "stack")` and `Area(layout: "fill")` render over the
union of valid physical x-positions observed by any group. Missing group/x cells
contribute zero height. Duplicate rows for the same group/x cell are aggregated
before stacking.

Acceptance criteria:

- Sparse grouped area tables render one contiguous polygon per non-empty group.
- `layout: "fill"` preserves share normalization while treating omitted cells as
  zero contributions.
- Regression tests cover sparse grouped stack and fill layouts.
- The stacked-area gallery example uses a sparse source table and renders
  without holes.

### Release Version Alignment

Status: Implemented.

Workspace, extension, demo, lockfile, and specification version stamps are
aligned to `0.62.0` for this maintenance release.

Acceptance criteria:

- `Cargo.toml` and `Cargo.lock` record workspace crates at `0.62.0`.
- `docs/ALGRAF_SPEC.md` records `0.62.0` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- `editors/vscode/package.json`, `editors/vscode/package-lock.json`,
  `demo/package.json`, and `demo/package-lock.json` record `0.62.0`.

## Non-Goals

- No `.ag` syntax changes.
- No new geometry, scale, guide, data source, CLI flag, or LSP surface.
- No changes to historical completed release plans.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Additional validation:

- Regenerate and inspect the affected stacked/fill area examples.
