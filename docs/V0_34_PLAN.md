# Algraf v0.34.0 Plan

Status: Planned (draft)
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_33_PLAN.md`](V0_33_PLAN.md)
Prior art:
[`WASM_AUDIT.md`](WASM_AUDIT.md) (v0.19 build-shape audit),
[`V0_32_PLAN.md`](V0_32_PLAN.md) (host-runtime SVG + JSON sidecar contract),
[`WEBGL_FEASIBILITY.md`](WEBGL_FEASIBILITY.md) (future GL backend note).

## Purpose

This release ships a **browser-embeddable WASM runtime**: a feature-gated WASM
build of the Algraf pipeline plus a thin JS package that runs
parse → analyze → render **in the browser**, taking `.ag` source text and
host-supplied data bytes and returning the same SVG + JSON sidecar the native
CLI produces. The deliverable that motivates it is an embeddable, no-server
demo — a live editor that re-renders as you type — but the runtime is a general
library; the demo is one consumer.

This is the missing half of the v0.32 host-runtime story. v0.32 defined a stable
SVG + sidecar contract and shipped reference consumers (`editors/react`,
`editors/host-runtime`), but those consumers `fetch()` artifacts that were
**pre-rendered offline by the native CLI**. They cannot render an arbitrary
chart in the browser. v0.34 closes that gap: the host produces the SVG +
sidecar locally, from source, with no native process and no network round-trip.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
is updated with concrete `MUST` / `SHOULD` / `MUST NOT` language and a stable
diagnostic code (or feature-gate reservation).

## Release Thesis

v0.34.0 is the **WASM runtime** release. It is primarily a **packaging and
boundary** release, not a language release: the algebra, scales, geometries, and
sidecar contract do not change, and SVG output for any chart that renders today
on the CLI is **byte-identical** when rendered through the WASM build. The work
is making the existing pipeline *compile to and run on* `wasm32-unknown-unknown`
without the native-only dependencies, and exposing it through one small,
documented JS surface.

Three decisions anchor the release:

1. **The browser is a host, not a fork.** The WASM runtime reuses the exact same
   `algraf-driver` → `algraf-render` path the CLI uses. It does not get its own
   rendering logic. Determinism (spec §18.12) means a chart rendered in the
   browser and on the CLI must produce the same bytes.

2. **No filesystem; data arrives as bytes.** The browser has no path-backed I/O.
   The host supplies data sources as an in-memory name → bytes map; the runtime
   drives the existing `DriverIo` abstraction with a virtual, in-memory adapter.
   Path *resolution* (relative paths, base dirs) still works against virtual
   keys; only the *byte fetch* changes.

3. **Native-only capabilities are absent, not faked.** SQLite sources
   (`libsqlite3-sys`), PNG raster output (`resvg`/`tiny-skia`/`png`), and — as a
   first cut — geo projections (`proj4rs`) and shapefile loading are **excluded
   from the WASM build** via Cargo features. The browser renders SVG only and
   reads only single-file text formats (CSV/TSV/JSON/NDJSON, and GeoJSON if it
   fits without `proj4rs`). A source that requires an excluded capability fails
   with a clear, existing diagnostic, not a panic.

## Current Debt Surface — what blocks WASM today

> **Update (v0.34 spike, landed):** building an actual `wasm32-wasip1` binary
> showed the only native blocker is `libsqlite3-sys`. `proj4rs`, `shapefile`,
> `geojson`, and the `resvg`/`tiny-skia`/`png` raster stack all compile *and
> link* for WASM unchanged — so the `raster`/`geo` feature splits below are
> **not required** for a browser SVG runtime and are deferred. The shipped gate
> is a single `sql` Cargo feature (see `WASM_AUDIT.md` § v0.34.0 Update); the
> `algraf-wasm` crate already renders SVG byte-identical to the CLI under
> `node:wasi`. Items §2–§5 (bindgen, JS package, demo) remain.

The v0.19 audit (`WASM_AUDIT.md`) proved `algraf-core`, `algraf-syntax`, and
`algraf-semantics` compile to `wasm32-unknown-unknown`. It deliberately stopped
short of `algraf-data` and `algraf-render`, and named why. Those reasons are the
debt this release pays down:

- **`algraf-data` hard-depends on `libsqlite3-sys`** — native C bindings that do
  not build for `wasm32-unknown-unknown`. This is the single biggest blocker:
  every crate above `data` (semantics already depends on it for `ColumnDef` /
  `DataType`, plus render and driver) transitively pulls it in.
- **`algraf-data` hard-depends on `shapefile`** and **`geojson`**; shapefile is
  multi-file/path-oriented and out of scope for a single-bytes browser input.
- **`algraf-render` hard-depends on the raster stack** (`resvg`, `tiny-skia`,
  `png`) for `render_raster`, and on **`proj4rs`** for map projections. None are
  needed to emit SVG in the browser; the raster stack is large and the projection
  crate is heavy.
- **No crate has a `[features]` table.** The workspace is monolithic; there is
  currently no way to ask for "the SVG pipeline without native data backends."
  Introducing a feature split is the enabling refactor.
- **The reference host consumers fetch artifacts.** `editors/react/demo` and
  `editors/host-runtime/demo` read `examples/*.svg` + `*.meta.json` over
  `fetch()`. There is no in-browser render entry point for them to call.

The §24.6 render execution boundary already isolates planning from emission and
notes a "browser runtime and no system fonts" determinism goal, so the runtime
seam exists — what is missing is a build that excludes native deps and a JS shim
over the boundary.

## Chosen design

### 1. Cargo feature split (the enabling refactor)

Introduce additive Cargo features so the pipeline can be built without native
backends. Mirror the **existing source-language feature gates** where they line
up (the `sql` language gate already makes SQLite opt-in per spec §30 / §2532),
so the Cargo split and the language gate tell one story.

- `algraf-data`: gate the native backends.
  - `sql` (default on for native builds) → `libsqlite3-sys`.
  - `geo` (default on) → `shapefile`, `proj4rs`-adjacent geometry handling.
  - Core CSV/TSV/JSON/NDJSON loading and `read_bytes_as` stay always-on.
  - `DataType::Geometry` / `DataValueRef::Geometry` **stay in the type system**
    (removing them is not behavior-neutral, per the v0.19 note); only the
    *loading and projection* code is gated. A GeoJSON/shapefile source under a
    `geo`-less build fails to *load* with an existing data diagnostic.
- `algraf-render`: gate emission backends.
  - `raster` (default on) → `resvg`, `tiny-skia`, `png`, `render_raster*`.
  - `geo` (default on) → `proj4rs` projection training.
  - SVG and draw-list emission stay always-on.
- `algraf-driver` / `algraf-semantics`: forward the `sql`/`geo` features to
  `algraf-data`; no behavior change for native builds.
- **Default features reproduce today's behavior exactly.** Native CLI/LSP builds
  enable all features by default; `cargo test --workspace` is unchanged. The
  WASM build opts *out* (`--no-default-features --features svg`).

This is the bulk of the work and the highest-risk part: it touches every crate's
`Cargo.toml` and requires `#[cfg(feature = …)]` around the gated modules and
their public re-exports, without changing any default-build behavior.

### 2. `algraf-wasm` crate + JS package

A new crate `crates/algraf-wasm` (workspace member, **not** built by default
native tooling; CI builds it with `wasm-pack`/`wasm-bindgen` against the
no-native feature set). It is a thin adapter, not new pipeline logic:

- Implements a `DriverIo` (or `AsyncDriverIo`) backed by an in-memory
  `HashMap<String, Vec<u8>>` of host-supplied data, keyed by the resolved source
  name. Path resolution reuses `algraf-driver::resolution`; only byte fetch is
  virtual. A missing key surfaces as the existing "data source not found" driver
  diagnostic (no new code).
- Exposes one primary `wasm-bindgen` entry point, roughly:

  ```ts
  // render(source, files) -> { svg, sidecar, diagnostics }
  function render(source: string, files: Record<string, Uint8Array>): RenderResult;
  ```

  plus `check(source, files) -> diagnostics` (parse + analyze only, mirroring
  `algraf check`) and `parse(source) -> ast`/`format(source)` conveniences that
  need no data. Diagnostics are returned as structured JSON (code, severity,
  span, message) using the existing `Diagnostic` shape — the same the LSP emits.
- The sidecar returned is exactly the v0.32 contract (§24.6); the demo reuses the
  v0.32 inversion + nearest-mark logic unchanged.
- Build hygiene: `console_error_panic_hook` in debug for legible traces; default
  allocator (no `wee_alloc` unless size demands it). Determinism is preserved by
  reusing the existing self-contained font metrics (§24.6) — no system fonts.

### 3. JS package + embeddable demo

- Publish the generated bindings as `editors/wasm` (or `@algraf/wasm`), a small
  package wrapping the `wasm-pack` output with a typed `render`/`check` API and
  lazy `.wasm` loading.
- Wire the existing `editors/react` `AlgrafChart` so it can take **source +
  data** and render live via the WASM package, in addition to its current
  "consume pre-rendered SVG + sidecar" mode. The v0.32 interaction overlay is
  unchanged — it now sits on top of a locally produced sidecar.
- Ship an **embeddable demo** under `editors/wasm/demo`: a single static page
  with an `.ag` editor textarea, a data picker (a couple of bundled CSVs), and a
  live SVG preview with the v0.32 tooltip/crosshair overlay, re-rendering on
  input (debounced). No server, no build-time render — this is the artifact the
  release is "for." Diagnostics render inline under the editor.

## Diagnostics and gates to reserve (before coding)

- **No new source-language diagnostic codes** are required: a source that needs
  an excluded capability (SQLite, shapefile, map projection, PNG output) fails
  through the *existing* data/driver diagnostics for "unsupported source" /
  "data source not found" / "feature gate not enabled" (`E0024`/`E0025`,
  spec §30). The plan must confirm each excluded path lands on a *specific,
  non-panicking* existing code, and add a spec note mapping
  "capability absent in WASM build" → which diagnostic.
- **Spec reservations**: a new normative subsection (proposed **§24.7 — Browser
  / WASM runtime**) describing the JS contract (`render`/`check` signatures,
  the in-memory file map, the returned `{ svg, sidecar, diagnostics }` shape and
  its determinism guarantee), and a **§30** note recording the `sql`/`geo`/
  `raster` Cargo features and which capabilities the default WASM build omits.
  Reserve these before implementing.

## v0.34.0 Must

### 1. Feature-gate the native backend
Status: **Landed (spike).** Only `libsqlite3-sys` needed gating.
- Added a `sql` Cargo feature to `algraf-data` (default on) gating
  `libsqlite3-sys`, with a `sqlite_stub.rs` fallback; forwarded through
  `algraf-semantics`/`algraf-driver`/`algraf-render`. The four crates default
  `sql` off in `[workspace.dependencies]`; `algraf-cli`/`algraf-lsp` re-enable
  it, so native builds and all 637 workspace tests are unchanged and green.
- The `raster`/`geo` splits are **deferred** (not needed for WASM; those deps
  link fine). The audit command set in `WASM_AUDIT.md` is updated.

### 2. Virtual driver I/O + `algraf-wasm` render path
Status: **Landed (spike).** SVG byte-identical to the CLI.
- New `crates/algraf-wasm` with `MemoryIo` (in-memory `DriverIo` over a host
  `name -> bytes` map), reusing `algraf-driver` resolution and `algraf-render`
  emission. `render_to_svg(source, files) -> { svg, diagnostics, error }`.
- A WASI demo bin built to `wasm32-wasip1` renders `examples/scatter.ag` to SVG
  that is **sha256-equal** to `algraf render` output. Missing data surfaces as a
  driver diagnostic, never a panic (covered by `algraf-wasm` unit tests).
- Still open: serialize the v0.32 **sidecar** alongside the SVG through the same
  entry point (the spike returns SVG + diagnostics only).

### 3. `check`/`parse`/`format` entry points + structured diagnostics
Status: Planned.
- `check(source, files)`, `parse(source)`, `format(source)` exposed via
  `wasm-bindgen`, returning JSON that matches the LSP's `Diagnostic` shape
  (code, severity, byte-offset span, message). Spans are bytes; covered by a
  non-ASCII test (CLAUDE.md span rule).

### 4. JS package + embeddable live demo
Status: Planned.
- `editors/wasm` package wrapping the `wasm-pack` output with a typed API and
  lazy `.wasm` load; `editors/react` `AlgrafChart` gains a source-driven mode.
- `editors/wasm/demo`: static, serverless page with editor + data picker +
  live SVG preview reusing the v0.32 interaction overlay; debounced re-render;
  inline diagnostics.

### 5. Spec, README, examples, release hygiene
Status: Planned.
- Spec §24.7 (browser/WASM runtime contract) and §30 (Cargo feature notes)
  added with `MUST`/`SHOULD` language; the capability-absence → diagnostic
  mapping documented. Update `WASM_AUDIT.md` to record the now-WASM-clean
  `data`/`render` checks.
- README gains a "Running Algraf in the browser" section after the v0.32
  "Embedding in a host runtime" section, linking the live demo.
- Bump workspace `Cargo.toml`, `editors/vscode/package.json`,
  `editors/react/package.json`, and the new `editors/wasm` package to
  `0.34.0`. Confirm static `examples/` outputs do not drift
  (`git diff -- examples`).

## v0.34.0 Should

- **WASM binary size budget**: record the release `.wasm` size (gzipped) as a
  baseline in `docs/PERFORMANCE_BASELINE.md`; try `wasm-opt -Oz` and evaluate
  `wee_alloc` only if size warrants it. Document the number, don't chase it.
- **Geo-in-browser stretch**: evaluate whether a `proj4rs`-free path can render
  *already-projected* GeoJSON (no on-the-fly projection) so a basic map demo is
  possible without pulling the projection crate. If non-trivial, defer.
- **Web Worker guidance**: a README note on running `render` off the main thread
  for large inputs (the runtime is synchronous and CPU-bound).

## Explicitly Deferred Past v0.34.0

- **SQLite, shapefile, and on-the-fly map projection in the browser.** These
  stay native-only; the WASM build omits them by design.
- **PNG/raster output in the browser.** SVG only; hosts that need a bitmap can
  rasterize the SVG themselves (canvas/`<img>`).
- **A WebGL/Canvas draw-list backend in the browser.** Tracked separately in
  [`WEBGL_FEASIBILITY.md`](WEBGL_FEASIBILITY.md); the draw-list contract is
  already serializable, but wiring a GL consumer is its own release.
- **LSP-over-WASM** (running `algraf-lsp` in the browser for a Monaco editor).
  The demo uses `check`/`format`, not a full language server.
- **Streaming/lazy data**, npm publishing/CDN distribution, and framework
  adapters beyond the React reference.

## Required checks before finishing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
# WASM build-shape audit (extends WASM_AUDIT.md):
cargo check -p algraf-data   --no-default-features --features svg --target wasm32-unknown-unknown
cargo check -p algraf-render --no-default-features --features svg --target wasm32-unknown-unknown
cargo check -p algraf-wasm   --target wasm32-unknown-unknown
./examples/generate.sh
git diff -- examples   # untouched examples must not drift
```

## Promotion Workflow

1. Reserve spec §24.7 (browser/WASM contract) and the §30 Cargo-feature note;
   write the capability-absence → diagnostic mapping before coding.
2. Land the feature split (Must §1) with default builds byte-for-byte unchanged
   and the WASM `cargo check`s green — this is the gate for everything else.
3. Build `algraf-wasm` with the virtual `DriverIo` and prove CLI parity on the
   example corpus (Must §2).
4. Add `check`/`parse`/`format` and structured diagnostics (Must §3).
5. Ship the JS package, source-driven `AlgrafChart`, and the live demo
   (Must §4).
6. Spec/README/examples/version bump; confirm no example drift (Must §5).
