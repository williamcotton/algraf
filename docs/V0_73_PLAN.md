# Algraf v0.72.0 Plan

Status: Implemented
Target version: 0.72.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_71_PLAN.md`](V0_72_PLAN.md)
Roadmap theme: Add another example to the README.
Cross-repo coordination: none required to ship 0.73.0.

## Purpose
Add another example to the README.

## Release Thesis

Add another example to the README.

## Proposed Spec Changes

None

## Must

- Add another example to the README.

  Status: Implemented.

- Bump release version stamps to 0.73.0.

  Status: Implemented.

  Updates: workspace `Cargo.toml`, `Cargo.lock` workspace member
  entries, `docs/ALGRAF_SPEC.md` (`Status:` line and the inline
  "current implementation is version" prose, plus a new v0.73 history
  line), `editors/vscode/package.json`,
  `editors/vscode/package-lock.json`, `demo/package.json`, and
  `demo/package-lock.json`.

## Validation

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./examples/generate.sh
git diff -- examples   # only the inset_city_pies.ag move + its re-rendered SVG/PNG
```

No regressions.

## Open Questions

1. None

## Promotion Workflow

1. None
