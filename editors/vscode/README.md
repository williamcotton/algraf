# Algraf VS Code Extension

This extension registers `.ag` files as Algraf documents, provides TextMate
syntax highlighting, and starts the Algraf language server with:

```bash
algraf lsp
```

All language behavior comes from the `algraf` binary over LSP. The server
provides:

- Diagnostics, completion, hover, document symbols, and semantic tokens.
- Go to definition: derived columns jump to their `Derive`; the `data:` string
  opens the CSV; source columns jump to the CSV header.
- Find references and document highlight for columns and derived-table names.
- Signature help inside geometry and `Scale`/`Guide`/`Theme`/`Layout` calls.
- Code actions, including quick fixes (quote color/string, suggested
  geometry/column/property) and a refactor that desugars a `Histogram` into an
  explicit `Derive` + `Rect`.
- Rename for derived-table names, whole-document formatting (also via range
  formatting), and inlay hints showing the columns a `Derive` produces.
- A live preview pane: run **Algraf: Open Preview** (also on the editor title
  bar) to render the chart beside the source. It refreshes as you type, watches
  the chart's data file and re-renders when it changes on disk, and offers
  **Algraf: Refresh Preview** for a manual refresh. Rendering goes through the
  same pipeline as `algraf render`.

Code actions surface through the editor lightbulb (enable
`editor.lightbulb.enabled`) or `Cmd/Ctrl+.`.

The preview is served by the `algraf/preview` LSP request; the extension only
displays the SVG the server returns.

## Development

Install dependencies:

```bash
npm install
```

Compile:

```bash
npm run compile
```

Build a VSIX package:

```bash
npm run vsix
```

If `algraf` is not on `PATH`, set `algraf.server.path` to an absolute path to
the binary. For local Rust development, you can also set:

```json
{
  "algraf.server.path": "cargo",
  "algraf.server.args": ["run", "--", "lsp"],
  "algraf.server.cwd": "${workspaceFolder}"
}
```
