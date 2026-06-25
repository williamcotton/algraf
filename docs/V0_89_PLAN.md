# Algraf v0.89.0 Plan

Status: Implemented
Target version: 0.89.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_88_PLAN.md`](V0_88_PLAN.md)
Roadmap theme: Keep the normative specification aligned with the implemented
language, diagnostics, and rendered output before promoting new behavior.
Cross-repo coordination: none for the initial spec-drift cleanup. Browser package
publication remains independent from the Rust/CLI release.

## Purpose

Algraf v0.88 shipped the implementation surface, but parts of the normative
specification still described stale or reserved behavior. This release starts
with a conservative documentation-alignment pass: update the spec to describe
the current code and examples, without changing runtime behavior unless a later
plan item explicitly requires code.

## Scope

### Normative Spec Drift Cleanup

Status: Implemented.

Acceptance criteria:

- `ChartItem` and declaration grammar include the implemented `Parse(...)`
  declaration.
- Appendix A documents both implemented map literals and `ParseDecl`.
- Appendix B's type sketch includes implemented `Table`, `let`, `Parse`,
  call-value, and map-value variants.
- Named-table source-file diagnostics describe the current shared driver/data
  diagnostics (`E1005`/`E1006`) while keeping `E1106`/`E1107` reserved.
- The orphaned `20.x` theme subsections have an explicit `## 20. Themes`
  parent heading.
- The diagnostics catalog distinguishes emitted codes from registered but
  reserved/deferred codes.
- Appendix C's SVG example uses the actual emitted group/class shape:
  `algraf-plot-area`, `algraf-grid`, `algraf-layer`, `algraf-axes`, and
  `algraf-legends`.

### Version And Release Artifacts

Status: Deferred.

Acceptance criteria:

- When v0.89 moves from in-progress planning to implemented release, update the
  workspace/spec/package version stamps required by `AGENTS.md`.
- Keep npm package and consumer dependency pins on published package versions
  unless a separate unpublished package release is prepared.

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- `cargo test -p algraf-core spec_diagnostic_catalog_is_registered`
- `cargo test -p algraf-core registered_codes_are_unique_and_well_formed`

## Explicitly Deferred Past Initial v0.89 Work

- Runtime diagnostic changes for currently reserved codes such as `E1106`,
  `E1107`, `W2003`, `W2007`, and `H3003`–`H3005`.
- Broad prose rewrites outside the confirmed drift areas.
- Release version stamp updates until v0.89 is ready to close.
