# Algraf v0.55.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_54_PLAN.md`](V0_54_PLAN.md)
Roadmap theme: size legend example polish.

## Purpose

v0.55.0 is a narrow documentation and example maintenance release. It makes
checked-in size and stroke-width examples use explicit, human-friendly legend
breaks instead of relying on fractional default ticks or zero anchor labels.

## Scope

### Size Legend Breaks in Examples

Status: Implemented.

Representative `Scale(size: ...)` and `Scale(strokeWidth: ...)` examples
declare meaningful `breaks:` values, and where raw values are large absolute
counts, paired `labels:` keep the legend text readable.

Acceptance criteria:

- Example charts that map point `size` or path `strokeWidth` MUST declare
  explicit `breaks:` values aligned to the data units.
- README tutorial snippets MUST match the checked-in example sources.
- Browser demo presets with mapped size or stroke-width scales MUST use clean
  size legend breaks.
- Renderer tests that exercise mapped size legends SHOULD include explicit
  breaks so the source examples cover the intended spelling.

### Release Version Alignment

Status: Implemented.

Workspace, extension, demo, lockfile, and specification version stamps are
aligned to `0.55.0` for this release.

Acceptance criteria:

- `Cargo.toml` and `Cargo.lock` record workspace crates at `0.55.0`.
- `docs/ALGRAF_SPEC.md` records `0.55.0` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- `editors/vscode/package.json`, `editors/vscode/package-lock.json`,
  `demo/package.json`, and `demo/package-lock.json` record `0.55.0`.

## Non-Goals

- No new `.ag` syntax.
- No renderer, analyzer, parser, LSP, or editor-service behavior changes.
- No changes to historical completed release plans.

## Validation

Required checks:

```bash
./examples/generate.sh
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```
