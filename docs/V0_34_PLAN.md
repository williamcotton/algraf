# Algraf v0.34.0 Plan

Status: Implemented out of order (manual browser ABI + root demo; no
`wasm-bindgen` package)
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_33_PLAN.md`](V0_33_PLAN.md)
Prior art:
[`WASM_AUDIT.md`](WASM_AUDIT.md) (v0.19/v0.34 build-shape audit),
[`V0_32_PLAN.md`](V0_32_PLAN.md) (host-runtime SVG + JSON sidecar contract),
[`WEBGL_FEASIBILITY.md`](WEBGL_FEASIBILITY.md) (future GL backend note).

## Purpose

This release ships a **browser-embeddable WASM runtime** for rendering Algraf
documents in a host page. The shipped shape is the one already present in the
tree:

- `crates/algraf-wasm` runs parse -> analyze -> render through the existing
  `algraf-driver` and `algraf-render` pipeline.
- The runtime accepts `.ag` source plus an in-memory data-source map and returns
  SVG, the v0.32 interaction sidecar, structured diagnostics, and an optional
  span-less fatal error string.
- The browser build exposes a small manual pointer/length JSON ABI
  (`algraf_alloc`, `algraf_dealloc`, `algraf_render_json`) instead of generated
  `wasm-bindgen` bindings.
- The root-level [`demo/`](../demo) Vite/React app loads
  `public/wasm/algraf.wasm`, calls that ABI through `demo/src/algrafWasm.ts`,
  and renders the result with `demo/src/AlgrafChart.tsx`.

This is the missing runtime half of the v0.32 host-sidecar story. v0.32 defined
the SVG + JSON sidecar contract. v0.34 lets a browser host produce that SVG and
sidecar locally from source and host-supplied data, with no native process and
no server-side render step.

## Release Thesis

v0.34.0 is a **packaging and boundary** release, not a language release. The
algebra, scales, geometries, and sidecar contract do not change. The browser
runtime is a host adapter over the same pipeline as the CLI, so charts that use
capabilities present in the WASM build must render deterministically with the
same SVG and sidecar semantics.

Three decisions anchor the implemented release:

1. **The browser is a host, not a fork.** The WASM runtime reuses
   `algraf-driver` -> `algraf-render`. It does not carry browser-specific
   parsing, analysis, layout, or SVG emission logic.
2. **No filesystem; data arrives from the host.** The host supplies data as an
   in-memory name -> bytes/text map. The runtime satisfies the existing
   `DriverIo` boundary from that map.
3. **Native-only capabilities fail clearly.** The browser crate does not enable
   the native `sql` Cargo feature, so SQLite sources fail through the existing
   data/driver diagnostic path instead of linking `libsqlite3-sys` or panicking.
   Browser raster/PNG output and filesystem discovery are out of scope.

## Reconciled Debt Surface

The original draft expected broad feature splits (`geo`, `raster`, `sql`) and a
generated `wasm-bindgen` package under `editors/wasm`. The actual implementation
proved that only `libsqlite3-sys` needed gating for the SVG browser runtime:
`proj4rs`, `shapefile`, `geojson`, and the raster dependencies compile and link
for the audited WASM targets. The shipped gate is therefore the existing `sql`
Cargo feature, forwarded through the pipeline and re-enabled by native CLI/LSP
crates.

The implementation also chose a manual ABI and a private root-level demo instead
of a publishable npm package. That is now the documented v0.34 surface. A
publishable package or generated bindings can still be added later, but they are
not required to call the current runtime.

## Implemented Design

### 1. Cargo feature gate

Status: Implemented.

- `algraf-data` gates `libsqlite3-sys` behind the `sql` Cargo feature.
- `algraf-semantics`, `algraf-driver`, and `algraf-render` forward the feature.
- `algraf-cli` and `algraf-lsp` re-enable `sql` for native behavior.
- `crates/algraf-wasm` builds without `sql`, avoiding the native SQLite C
  dependency.

The earlier `geo` and `raster` split ideas are deferred because they were not
needed for the shipped browser SVG runtime.

### 2. `algraf-wasm` render path

Status: Implemented.

- `crates/algraf-wasm` provides `MemoryIo`, an in-memory `DriverIo` adapter over
  host-supplied files.
- `render_to_svg(source, files)` runs the full pipeline and returns
  `{ svg, sidecar, diagnostics, error }`.
- The returned sidecar is the v0.32 §24.6 interaction metadata JSON.
- Missing data sources surface as diagnostics, not panics.
- A WASI demo binary exercises the same Rust entry point for build-shape
  verification.

### 3. Browser ABI

Status: Implemented as a manual ABI; generated bindgen is intentionally not
required.

The `wasm32-unknown-unknown` build exports:

```text
algraf_alloc(len) -> ptr
algraf_dealloc(ptr, len)
algraf_render_json(ptr, len) -> packed_ptr_len
```

Request JSON:

```json
{ "source": "Chart(...)", "files": { "penguins.json": "[...]" } }
```

Response JSON:

```json
{
  "svg": "string or null",
  "sidecar": "string or null",
  "diagnostics": [],
  "error": "string or null"
}
```

`check`, `parse`, and `format` convenience exports are **deferred** from the
v0.34 runtime contract. The demo uses render diagnostics for the live editor.

### 4. Live playground

Status: Implemented as a private root-level demo.

- [`demo/package.json`](../demo/package.json) builds
  `crates/algraf-wasm` for `wasm32-unknown-unknown`, copies
  `algraf_wasm.wasm` into `demo/public/wasm/algraf.wasm`, and starts Vite.
- `demo/src/algrafWasm.ts` loads the `.wasm`, manages request/response buffers,
  and exposes a typed `render(source, files)` wrapper.
- `demo/src/App.tsx` provides the live editor, data text area, diagnostics, and
  SVG preview.
- `demo/src/AlgrafChart.tsx` consumes the sidecar for nearest-mark tooltips,
  crosshair readouts, and highlight overlays.

The previously planned `editors/wasm` npm package and `editors/react` source
mode are not present in the current tree and remain future packaging work.

### 5. Spec, README, examples, release hygiene

Status: Reconciled.

- Spec §24.7 documents the browser/WASM runtime contract.
- Spec §30 records that v0.34 is a runtime/package surface, not a source feature
  gate.
- [`WASM_AUDIT.md`](WASM_AUDIT.md) records the now-shipped sidecar/manual ABI
  and root demo.
- The README points host-runtime readers at the actual `demo/` source.
- Workspace/package version bumps are release-cut hygiene and are not required
  to describe the current implemented surface.

## Explicitly Deferred Past v0.34.0

- Generated `wasm-bindgen` bindings or a publishable `@algraf/wasm` package.
- Separate browser `check`, `parse`, and `format` convenience exports.
- SQLite sources in the browser.
- PNG/raster output in the browser.
- Filesystem-backed source discovery in the browser.
- LSP-over-WASM for Monaco.
- Streaming/lazy data and framework adapters beyond the current demo.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
# Optional browser-demo verification when touching demo code:
cd demo && npm run check
cd demo && npm run build
```

The optional demo commands rebuild `demo/public/wasm/algraf.wasm` and
`demo/dist/`; avoid running them during documentation-only changes unless the
build artifacts are intentionally being refreshed.

## Promotion Workflow

This plan has already been promoted out of order. Future changes should treat
§24.7 as the normative browser/WASM contract and should update the spec before
adding new exported runtime functions, generated bindings, package names, or
browser-only diagnostics.
