# Algraf VS Code Extension

This extension registers `.ag` files as Algraf documents, provides TextMate
syntax highlighting, and starts the Algraf language server with:

```bash
algraf lsp
```

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
