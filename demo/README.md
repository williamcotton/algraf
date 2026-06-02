# Algraf Browser Demo

Root-level Vite + React site for `algraf.wasm`. It includes a landing page,
guided docs, and the full Monaco-backed demos route.

```bash
cd demo
npm install
npm run dev
```

`npm run dev` builds `crates/algraf-wasm` for `wasm32-unknown-unknown`, copies
the generated binary to `public/wasm/algraf.wasm`, then starts Vite.

Routes:

- `/` - light-themed language landing page with a small live Monaco editor and
  WASM-rendered chart.
- `/docs` - guided quickstart with Monaco editor services and a live preview.
- `/demos` - full demo gallery with chart presets, Monaco editor services,
  editable data, diagnostics, and interactive SVG preview.

The app ships a small gallery of chart presets backed by public CSV datasets in
`public/data/`, and passes the selected dataset files to the WASM runtime as
in-memory Algraf data sources.

Included gallery data:

- `penguins.csv` - full Palmer penguins data, 344 rows.
- `gapminder.csv` - Gapminder five-year country panel, 1,704 rows.
- `iris.csv` - Iris flower measurements, 150 rows.
- `stocks.csv` - example stock prices, 559 rows.
- `seattle-weather.csv` - daily Seattle weather, 1,461 rows.
- `astronauts.csv` - astronaut age measurements, 564 rows.
- `minard_troops.csv` - Minard campaign troop positions, 50 rows.
- `minard_cities.csv` - Minard campaign city labels, 19 rows.
- `homepage-starter.csv` - small homepage chart fixture, 12 rows.

See `public/data/README.md` for source URLs.

The homepage chart is also checked in as `public/homepage.ag`, so it can be
rendered from the repository root:

```bash
algraf render demo/public/homepage.ag --output /tmp/algraf-homepage.svg
algraf render demo/public/homepage.ag --output /tmp/algraf-homepage.png
```

## Editor Features

The Monaco editor uses the `algraf-wasm` editor-service ABI, which calls the
same Rust feature helpers as the native `algraf lsp` server. Monaco only maps
LSP-shaped JSON into browser provider objects.

Supported in the browser demos:

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
- The browser site does not run a JSON-RPC LSP transport or publish an
  `@algraf/editor` package.
- Editor-service calls currently run synchronously in the same WASM instance as
  preview rendering. A worker should be added if larger documents make typing
  latency visible.

`npm run check` type-checks the Monaco provider mappings. The project does not
currently include a Playwright or equivalent browser smoke harness.

## GitHub Pages Deployment

The repository includes a GitHub Actions workflow at
`.github/workflows/demo-pages.yml`. It builds the Rust `wasm32-unknown-unknown`
target, runs the demo's Vite build, and publishes `demo/dist` to GitHub Pages.
The build copies `dist/index.html` to `dist/404.html` so direct visits to clean
paths such as `/docs` and `/demos` load the browser app on static Pages hosting.

Before the first deployment, enable Pages for the repository in GitHub:
**Settings -> Pages -> Build and deployment -> Source -> GitHub Actions**.
GitHub requires that repository setting before an Actions-based Pages deployment
can publish a site.

The workflow computes the Vite base path from the repository name. A user or
organization Pages repository such as `owner.github.io` is served from `/`;
ordinary project repositories are served from `/<repo>/`. For this repository,
the deployed demo URL is:

```text
https://williamcotton.github.io/algraf/
```
