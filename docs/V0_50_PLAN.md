# Algraf v0.50.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_49_PLAN.md`](V0_49_PLAN.md)
Roadmap theme: documentation structure and release version alignment.

## Purpose

v0.50.0 refreshes Algraf's top-level documentation and aligns repository version
stamps for the next packaged release. The root README becomes a compact feature
tour, while the full visual gallery moves to the examples directory where it can
grow without making the project overview unwieldy.

## Scope

### README Tour Split

Status: Implemented.

The root [`README.md`](../README.md) shows a six-chart progression from a basic
scatter plot through statistical layers, faceting, multi-space layering,
annotations, named tables, and data-driven paths.

The complete visual gallery lives in [`examples/README.md`](../examples/README.md)
with each runnable `.ag` source followed by its rendered SVG.

### Release Version Alignment

Status: Implemented.

Workspace, extension, demo, lockfile, and specification version stamps are
aligned to `0.50.0`.

### Install Instructions

Status: Implemented.

The README documents the Homebrew install path:

```bash
brew tap williamcotton/algraf
brew install algraf
```

## Non-Goals

- No new `.ag` syntax.
- No renderer, analyzer, parser, or LSP behavior changes.
- No new examples beyond the existing generated example set.

## Validation

- Root README links point to existing rendered example SVGs.
- `examples/README.md` keeps the full source-plus-SVG tutorial.
- Full workspace checks remain clean.
