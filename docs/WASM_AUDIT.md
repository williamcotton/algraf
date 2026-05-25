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
