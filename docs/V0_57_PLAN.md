# Algraf v0.57.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_56_PLAN.md`](V0_56_PLAN.md)
Roadmap theme: browser documentation depth and projection-correct map rendering.

## Purpose

v0.57.0 is a browser-host release. It restructures the static demo's `/docs`
route from a single guided quickstart into a multi-page documentation section,
and it corrects a host-side WebAssembly import-marshaling bug that silently
distorted every projected map in the browser runtime.

No `.ag` syntax, parser, analyzer, renderer, LSP, or CLI behavior changes. The
projection fix lives entirely in the browser host glue; the Rust pipeline and
its native output are unchanged.

## Scope

### Multi-Page Documentation Section

Status: Implemented.

The demo `/docs` route is a typical project documentation section: a sticky
sidebar of topic pages, a content column, and previous/next paging. Topics are
the overview plus the algebra, bar layouts, facets, insets, statistics, theming
and guides, and tooling. Each topic embeds live editor + preview pairs over the
browser render ABI, and the inset page leads with the projected map-with-pies
example.

Acceptance criteria:

- `/docs` and `/docs/<topic>` resolve to the documentation section; navigation
  between topics updates the rendered page without a full reload.
- Every live example validates against its bundled data with `algraf check` and
  renders without errors in the browser runtime.
- Examples reuse only features the implementation accepts (the algebra
  operators `*`, `/`, `+`; `Bar(layout: ...)`; `Layout(facet*)`; `Inset(...)`;
  `Histogram`, `Smooth(method: "lm")`, `Violin`/`Boxplot`).
- Links to the dense normative specification are removed from the demo's
  user-facing navigation.

### Browser Projection ABI Fix

Status: Implemented.

`proj4rs` (the projection backend, spec §16.14) parses every numeric
proj-string parameter through `js_sys` `parseFloat`/`parseInt` when compiled for
`wasm32-unknown-unknown`. `wasm-bindgen` passes the Rust `&str` to those imports
as a `(ptr, len)` pair into the module's linear memory. The demo host's manual
import shim coerced the pointer integer to a string instead of decoding the
slice, so every projection parameter (`lat_1`, `lon_0`, ...) parsed to garbage
and projected maps rendered mirrored and collapsed while non-projected charts
looked fine.

Acceptance criteria:

- The host import shim decodes the `js_sys` number-parser `&str` argument as a
  `(ptr, len)` UTF-8 slice of wasm memory.
- Projected SVG from the browser runtime is coordinate-identical to the native
  renderer for the same source and data (verified on the `albers_usa`
  county-basemap inset-pie example).
- Spec §24.7 records the host import-marshaling requirement and reasserts that
  projection output, being a capability available in the WASM build, must match
  the native render scene.

### Release Version Alignment

Status: Implemented.

Workspace, extension, demo, lockfile, and specification version stamps are
aligned to `0.57.0`.

Acceptance criteria:

- `Cargo.toml` and `Cargo.lock` record workspace crates at `0.57.0`.
- `docs/ALGRAF_SPEC.md` records `0.57.0` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- `editors/vscode/package.json`, `editors/vscode/package-lock.json`,
  `demo/package.json`, and `demo/package-lock.json` record `0.57.0`.

## Non-Goals

- No `.ag` syntax changes.
- No parser, analyzer, renderer, LSP, CLI, or Rust WASM-crate behavior changes;
  the projection fix is host JavaScript glue only.
- No move to generated `wasm-bindgen` bindings for the shipped ABI; the manual
  pointer/length ABI is retained, only its dependency-emitted imports are
  marshaled correctly.
- No changes to historical completed release plans.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Browser checks:

```bash
cd demo
npm run check
npm run build
```
