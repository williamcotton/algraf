# Algraf v0.66.0 Plan

Status: Implemented
Target version: 0.66.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_65_PLAN.md`](V0_65_PLAN.md)
Cross-repo coordination: `../studio/` if browser hosts pass invocation
variables into Algraf story charts.

## Purpose

Algraf v0.66 closes a browser runtime parity gap: invocation-time variables are
already specified and implemented for CLI and embedded callers, but the
`algraf-wasm` browser JSON ABI accepted only source text and in-memory files.
Browser hosts should be able to pass the same raw source-fragment variable map
without reimplementing Algraf template expansion in TypeScript.

The release keeps variable expansion as an invocation preprocessing layer. It
does not add source-language variables beyond existing `let` bindings, and it
does not give the browser runtime access to environment variables, files,
network resources, clocks, or process state.

## Scope

### Browser Render JSON Variables

Status: Implemented.

Acceptance criteria:

- The `algraf_render_json` request accepts an optional `variables` object whose
  keys and values are UTF-8 strings.
- Missing `variables` defaults to an empty map for backward-compatible hosts.
- The WASM render adapter expands variables through
  `algraf_driver::expand_variables` before parse/analyze/render.
- Variable substitution errors, including undefined variables and malformed
  placeholders, return the standard render response shape with null SVG and
  sidecar, an empty diagnostics array, and a span-less `error` string. They
  must not panic.
- Expanded source continues through the same in-memory `DriverIo`, semantic
  analysis, renderer, sidecar, and diagnostic paths as all other WASM renders.

### TypeScript Runtime Surface

Status: Implemented.

Acceptance criteria:

- `AlgrafRuntime.render` accepts an optional third
  `Record<string, string>` variables argument.
- Existing two-argument render calls remain source-compatible.
- The TypeScript wrapper includes the variables map in the JSON payload sent to
  the manual WASM pointer/length ABI.
- Package documentation mentions the v0.66 browser package surface.

### Specification And Version Alignment

Status: Implemented.

Acceptance criteria:

- Spec §24.7 documents the optional browser render `variables` request field
  and its preprocessing/error behavior.
- The release planning table records v0.66.0.
- Workspace, package, demo, and lockfile version stamps are aligned to
  `0.66.0`.

## Non-Goals

- Host-side TypeScript variable interpolation.
- Generated `wasm-bindgen` bindings.
- Editor-service expansion of invocation variables.
- Source maps from expanded source back to unexpanded source.
- Non-string JSON variable values.
- Environment-variable interpolation, file includes, conditionals, loops, or
  expression evaluation.

## Implementation Notes

- Keep `render_to_svg(source, files)` as the low-level Rust render entry point.
  The browser JSON adapter owns request decoding and invocation preprocessing.
- Reuse the existing driver helper rather than adding a WASM-local parser or
  substituter.
- Preserve deterministic response serialization and the existing catastrophic
  error channel for substitution failures because they are span-less invocation
  errors, not source diagnostics in the expanded document.

## Validation

Required checks before this plan can be marked implemented:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Additional validation:

- WASM crate tests for successful browser JSON variable expansion.
- WASM crate tests for undefined variables returning a render response error.
- TypeScript package type checking for the optional variables parameter.
