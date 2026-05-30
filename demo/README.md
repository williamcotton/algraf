# Algraf WASM Playground

Root-level Vite + React playground for `algraf.wasm`.

```bash
cd demo
npm install
npm run dev
```

`npm run dev` builds `crates/algraf-wasm` for `wasm32-unknown-unknown`, copies
the generated binary to `public/wasm/algraf.wasm`, then starts Vite. The app
fetches `public/data/penguins.json` and passes that JSON text to the WASM
runtime as an in-memory Algraf data source named `penguins.json`.

## Editor Features

The Monaco editor uses the `algraf-wasm` editor-service ABI, which calls the
same Rust feature helpers as the native `algraf lsp` server. Monaco only maps
LSP-shaped JSON into browser provider objects.

Supported in the playground:

- diagnostics markers from the WASM render/check path;
- hover;
- completion with snippets, documentation, kinds, and trigger characters;
- signature help;
- document and range formatting;
- semantic tokens, with TextMate grammar retained as a static fallback;
- code actions;
- definition, references, and document highlights;
- prepare-rename and rename;
- document symbols;
- inlay hints when supported by the bundled Monaco version.

Browser limitations:

- Data/schema-aware features can only see host-supplied in-memory files, such
  as `penguins.json`; arbitrary workspace filesystem access is not available.
- Navigation to in-memory data uses synthetic `inmemory://algraf/...` URIs.
- SQLite sources are not available in the WASM build.
- The playground does not run a JSON-RPC LSP transport or publish an
  `@algraf/editor` package.
- Editor-service calls currently run synchronously in the same WASM instance as
  preview rendering. A worker should be added if larger documents make typing
  latency visible.

`npm run check` type-checks the Monaco provider mappings. The project does not
currently include a Playwright or equivalent browser smoke harness.
