# WASM Build-Shape Audit

v0.19.0 treats WASM as a build-shape audit. It does not ship a browser runtime,
JS bindings, web preview, or a WASM output backend.

## Target Scope

The minimum audited surface is:

- `algraf-core`: diagnostics and spans.
- `algraf-syntax`: lexer, parser, CST/AST, formatter.
- `algraf-semantics`: schema-free analyzer logic plus typed IR. This crate still
  depends on `algraf-data` for shared `ColumnDef`/`DataType`, so it is not yet a
  small standalone WASM semantic core.

## Current Dependency Shape

- `algraf-core` has no internal dependencies and no OS I/O in production code.
- `algraf-syntax` depends on `logos` and `rowan`; neither requires OS I/O.
- `algraf-data` owns CSV/JSON/GeoJSON/shapefile loading and path compatibility
  helpers. Path-backed APIs are not browser runtime APIs, but the data crate also
  exposes reader/byte-slice loading for single-file formats.
- `algraf-driver` owns default OS I/O adapters and is not part of the minimum
  WASM surface.
- `algraf-lsp` and `algraf-cli` are native process surfaces.
- `algraf-render` uses `proj4rs` and SVG string emission; browser packaging is
  deferred.

## v0.19.0 Changes

- The workspace `tokio` dependency was narrowed from `full` to the features the
  LSP actually uses: `rt-multi-thread`, `macros`, `io-std`, and `sync`.
- No data-format feature split was added in v0.19.0 because the current shared
  `DataType::Geometry` and `DataValueRef::Geometry` surface means removing
  geospatial dependencies is not behavior-neutral yet.

## Audit Commands

After installing the target with `rustup target add wasm32-unknown-unknown`, run:

```bash
cargo check -p algraf-core --target wasm32-unknown-unknown
cargo check -p algraf-syntax --target wasm32-unknown-unknown
cargo check -p algraf-semantics --target wasm32-unknown-unknown
```

On the v0.19.0 audit machine, the default shell used Homebrew `cargo`/`rustc`,
while `rustup target add` installed the WASM standard library for rustup's
stable toolchain. The successful audit therefore pinned Cargo to rustup's
compiler:

```bash
RUSTC=/Users/williamcotton/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  rustup run stable cargo check -p algraf-core --target wasm32-unknown-unknown
RUSTC=/Users/williamcotton/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  rustup run stable cargo check -p algraf-syntax --target wasm32-unknown-unknown
RUSTC=/Users/williamcotton/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  rustup run stable cargo check -p algraf-semantics --target wasm32-unknown-unknown
```

All three checks passed under `rustc 1.92.0` from rustup stable. The native
workspace checks still use the default Homebrew toolchain recorded in
`docs/PERFORMANCE_BASELINE.md`.

## v0.34.0 Update — full pipeline builds to WASM

A v0.34 spike extended the audited surface to `algraf-data` and `algraf-render`
and built (not just checked) an actual WASM binary that renders SVG. Findings:

- **The only native blocker was `libsqlite3-sys`.** It is the one C dependency;
  `proj4rs`, `shapefile`, `geojson`, and the `resvg`/`tiny-skia`/`png` raster
  stack all compile *and link* for `wasm32` with no changes. The v0.19 audit's
  pessimism about geo/raster did not bear out — linking a `wasm32-wasip1`
  binary failed solely with `rust-lld: error: unable to find library -lsqlite3`.
- **Gate added.** `libsqlite3-sys` is now optional behind a `sql` Cargo feature
  on `algraf-data` (default on), forwarded through `algraf-semantics`,
  `algraf-driver`, and `algraf-render`. The four crates default `sql` off in
  `[workspace.dependencies]`; `algraf-cli` and `algraf-lsp` re-enable it, so
  native builds and all 637 workspace tests are unchanged. When `sql` is off,
  `algraf-data/src/sqlite_stub.rs` replaces the FFI module and a SQLite source
  reports a clear `DataError`, never a link error or panic.
- **New crate `algraf-wasm`** runs the existing `algraf-driver` → `algraf-render`
  path over an in-memory `DriverIo` (host-supplied `name -> bytes`), exposing
  `render_to_svg(source, files) -> { svg, sidecar, diagnostics, error }`. The
  sidecar is the v0.32 interaction metadata JSON. A WASI demo bin built to
  `wasm32-wasip1` and run under `node:wasi` produced SVG **byte-identical
  (sha256-equal)** to `algraf render` for `examples/scatter.ag`.
- **Browser ABI and demo shipped.** The `wasm32-unknown-unknown` build exports a
  manual pointer/length JSON ABI (`algraf_alloc`, `algraf_dealloc`,
  `algraf_render_json`) rather than generated bindgen. The root-level
  [`demo/`](../demo) Vite/React app builds and loads `algraf.wasm`, sends
  `{ source, files }`, and consumes the returned SVG, sidecar, diagnostics, and
  error fields.

### Updated audit commands (sql gated off)

```bash
RUSTC=/Users/williamcotton/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  rustup run stable cargo check -p algraf-data   --target wasm32-wasip1
RUSTC=/Users/williamcotton/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  rustup run stable cargo check -p algraf-render --target wasm32-wasip1
RUSTC=/Users/williamcotton/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  rustup run stable cargo build -p algraf-wasm --bin algraf-wasm-demo --target wasm32-wasip1
node --experimental-wasi-unstable-preview1 \
  crates/algraf-wasm/web/run-wasi.mjs target/.../algraf-wasm-demo.wasm
```

Because `algraf-wasm` does not enable `sql` and the workspace defaults it off,
its dependency tree never links `libsqlite3-sys`. The v0.34 runtime surface is
now complete in-tree as a manual ABI plus private demo. Generated bindings, a
publishable npm package, and separate browser `check`/`parse`/`format` exports
remain future packaging/API work, not crate-level portability blockers.
