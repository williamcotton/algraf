# Algraf WASM Playground

Root-level Vite + React playground for `algraf.wasm`.

```bash
cd demo
npm install
npm run dev
```

`npm run dev` builds `crates/algraf-wasm` for `wasm32-unknown-unknown`, copies
the generated binary to `public/wasm/algraf.wasm`, then starts Vite. The app
ships a small gallery of chart presets backed by public CSV datasets in
`public/data/`, and passes the selected file to the WASM runtime as an
in-memory Algraf data source.

Included gallery data:

- `penguins.csv` - full Palmer penguins data, 344 rows.
- `gapminder.csv` - Gapminder five-year country panel, 1,704 rows.
- `iris.csv` - Iris flower measurements, 150 rows.
- `stocks.csv` - example stock prices, 559 rows.
- `seattle-weather.csv` - daily Seattle weather, 1,461 rows.

See `public/data/README.md` for source URLs.

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
- document symbols.

Hover includes derived-table schemas, source schema/row previews from
host-supplied files, and call attributes/examples from the shared Rust registry.

Browser limitations:

- Data/schema-aware features can only see host-supplied in-memory files, such
  as the selected gallery CSV; arbitrary workspace filesystem access is not
  available.
- Navigation to in-memory data uses synthetic `inmemory://algraf/...` URIs.
- SQLite sources are not available in the WASM build.
- The playground does not run a JSON-RPC LSP transport or publish an
  `@algraf/editor` package.
- Editor-service calls currently run synchronously in the same WASM instance as
  preview rendering. A worker should be added if larger documents make typing
  latency visible.

`npm run check` type-checks the Monaco provider mappings. The project does not
currently include a Playwright or equivalent browser smoke harness.
