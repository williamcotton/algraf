# Algraf v0.58.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_57_5_PLAN.md`](V0_57_5_PLAN.md)
Roadmap theme: text label decluttering for dense direct annotations.

## Purpose

v0.58.0 closes a practical label-layout gap in the existing `Text(declutter:
true)` behavior. The v0.57 renderer could spread labels apart only when their
final positions shared an x column and overlapped vertically. Direct labels on a
shared y row but neighboring x positions could still collide, as in bubble or
point charts where several labels sit above points with the same y value.

This release keeps decluttering opt-in and deterministic while expanding it to
handle same-row horizontal text overlap.

## Scope

### Two-Axis Text Declutter

Status: Implemented.

`Text(declutter: true)` handles both existing vertical column collisions and new
horizontal row collisions.

Acceptance criteria:

- Vertical decluttering MUST retain the existing behavior for labels sharing a
  rounded x column.
- Horizontal decluttering MUST run on final label positions after `nudgeData`,
  `nudge`, `dx`, and `dy`.
- Horizontal decluttering MUST group labels sharing a rounded y baseline and
  separate estimated text boxes with deterministic stable ordering.
- The renderer MUST use the existing coarse text-width estimate rather than
  browser- or platform-dependent font metrics.
- Adjusted horizontal positions SHOULD stay within the plot extent when the
  estimated label group fits.
- Connector lines and arbitrary two-dimensional force layout remain deferred.
- Tests MUST cover a same-row collision representative of labeled point/bubble
  charts.

### Station Throughput Example

Status: Implemented.

The examples gallery includes a station-throughput bubble chart that exercises
same-row label decluttering.

Acceptance criteria:

- Add a runnable `.ag` example and CSV fixture under `examples/`.
- Register the example in `examples/generate.sh`.
- Update the root README and `examples/README.md` with source and rendered SVG.
- Render SVG and PNG outputs and inspect the PNG for sensible label placement.

### Release Version Alignment

Status: Implemented.

Workspace, extension, demo, lockfile, and specification version stamps are
aligned to `0.58.0` for this release.

Acceptance criteria:

- `Cargo.toml` and `Cargo.lock` record workspace crates at `0.58.0`.
- `docs/ALGRAF_SPEC.md` records `0.58.0` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- `editors/vscode/package.json`, `editors/vscode/package-lock.json`,
  `demo/package.json`, and `demo/package-lock.json` record `0.58.0`.

## Non-Goals

- No new `.ag` syntax.
- No default automatic label movement; users continue to opt in with
  `declutter: true`.
- No connector lines.
- No hidden or dropped text labels.
- No browser text measurement dependency.
- No changes to axis tick-label thinning.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Feature-specific validation:

- Renderer test for same-row station labels.
- Existing vertical declutter regression remains passing.
- `examples/station_throughput.ag` renders to SVG and PNG.
- PNG output is visually inspected for clear labels on the `trips = 6` row.
