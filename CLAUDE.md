# CLAUDE.md

Guidance for working in the Algraf repository.

## What Algraf is

Algraf is a block-scoped, algebraic grammar-of-graphics DSL (file extension
`.ag`). It parses a declarative chart description, validates it against CSV
data, trains scales, and emits deterministic SVG. The whole system — parser,
language server, runtime, and renderer — ships as one Rust binary named
`algraf`.

## Spec and versioned plans

Three artifacts govern behavior, and they must stay in sync:

1. **`docs/ALGRAF_SPEC.md` — the normative reference.** It describes what the
   implementation *does*, not just what it might do. **Read the relevant section
   before implementing or changing behavior.** The spec uses RFC-2119-style
   keywords (`MUST`, `SHOULD`, `MAY`, `MUST NOT`) and assigns stable diagnostic
   codes (e.g. `E0012`); honor both. When code intentionally deviates from a
   `SHOULD`, document why in a comment.

2. **`docs/V0_<minor>_PLAN.md` — per-release planning files.** Each release gets
   one (e.g. `V0_2_PLAN.md`, `V0_3_PLAN.md`). A plan states the release thesis,
   lists Must/Should items with a `Status:` line each, and records what stays
   deferred. Plans are *guidance*, not normative: a feature is only real once the
   spec says `MUST`/`SHOULD` and the code implements it. The newest plan file is
   the active one; older plan files are a historical record — don't reopen a
   completed release's scope.

3. **The code** under `crates/`, plus its tests and `examples/`.

### How they tie together

- **Promoting a deferred feature** (an old `MAY`, "later versions", or "deferred"
  item) into a release: add it to the active plan file, then move it into the
  relevant normative spec section (`MUST`/`SHOULD`), reserve any new diagnostic
  codes in the spec *before* implementing, then implement + test + add an example.
  See the Promotion Workflow at the bottom of each plan file.
- **The spec must match the implementation, not run ahead of it.** If you ship a
  geometry, property, theme, scale/guide key, CLI flag, or diagnostic, document
  it in the spec in the same change. If you defer something, the spec must say so
  (`MAY`, "deferred") rather than describe it as though it exists. Diagnostic
  codes emitted by code must exist in the spec; the spec MAY reserve codes the
  code does not yet emit.
- **Keep plan examples runnable.** `.ag` snippets in plan files must use only
  features the implementation actually accepts (e.g. `Smooth(method: "lm")`, not
  `"loess"` while loess is deferred).
- **When a plan item lands, update its `Status:` line**; when a release ships,
  start the next `V0_<minor>_PLAN.md`.

If you find the spec, a plan, and the code disagreeing, treat it as drift to
fix — reconcile all three rather than picking one.

## Workspace layout

Cargo workspace with seven crates under `crates/` (see spec §23):

| Crate              | Responsibility                                              |
| ------------------ | ----------------------------------------------------------- |
| `algraf-core`      | Shared primitives: `Span`, `Diagnostic`, `Severity`         |
| `algraf-syntax`    | Lexer, parser, AST/CST (rowan), parse diagnostics, formatter|
| `algraf-data`      | CSV loading, schema inference, dataframe, type inference     |
| `algraf-semantics` | Name resolution, validation, IR, geometry registry          |
| `algraf-render`    | Scale training, layout, stats, geometries, SVG emission     |
| `algraf-lsp`       | tower-lsp backend, document cache, completion, hover        |
| `algraf-cli`       | The `algraf` binary: arg parsing, command dispatch, I/O     |

Dependency direction flows downward: `core` depends on nothing internal;
`cli` depends on everything. Do not introduce cycles. Keep parser, LSP,
semantics, and render decoupled from concrete dataframe internals (spec §10.5).

## Building and running

Build the binary with `cargo build -p algraf-cli`; it lands at
`target/debug/algraf`. To try a chart while iterating:

```bash
cargo run -p algraf-cli -- render examples/scatter.ag --output /tmp/out.svg
cargo run -p algraf-cli -- check examples/scatter.ag      # parse + analyze, no render
```

Other subcommands: `format`, `schema`, `ast`, `ir`, and `lsp` (see spec §22).
`./examples/generate.sh` builds the CLI and re-renders every example.

## Required checks before finishing any change

Run all three from the repo root and make sure they pass:

```bash
cargo fmt --all          # format (CI uses `cargo fmt --all --check`)
cargo clippy --workspace --all-targets   # lint; treat warnings as failures
cargo test --workspace   # run the full test suite
```

A change is not done until `cargo fmt --all --check`, `cargo clippy
--workspace --all-targets`, and `cargo test --workspace` are all clean. Clippy
runs over tests too (`--all-targets`), so keep test code lint-clean as well.

## VS Code client

`editors/vscode/` is a thin VS Code language client (TypeScript, `src/extension.ts`,
bundled with esbuild). It does **not** reimplement any language logic: it spawns
the `algraf` binary as a language server (`algraf lsp` by default, configurable
via `algraf.server.path`/`algraf.server.args`) and talks LSP to it. All
completion, hover, diagnostics, semantic tokens, formatting, and code actions
come from the `algraf-lsp` crate — so LSP features are exercised by that crate's
tests, and the extension just wires them into the editor.

Practical implications:

- Improve editor behavior by changing `algraf-lsp`, not the extension. Touch the
  extension only for client wiring (config, commands, activation, packaging).
- Two things in the extension are local copies that must track the language:
  the TextMate grammar (`syntaxes/algraf.tmLanguage.json`) and
  `language-configuration.json`. If you add keywords, geometry names, or
  punctuation, update the grammar so static highlighting matches what the LSP
  reports via semantic tokens.
- Keep `package.json`'s `version` aligned with the workspace release version
  when cutting a release (it is currently versioned alongside the crates).

## Example Generation

Run `./examples/generate.sh` to regenerate the SVG and PNG outputs for all examples.

Create new examples by adding a new file to the `examples/` directory.

Create new examples when adding new features or fixing bugs.

When you add a new example, also add a section for it to the top-level
`README.md` — the README is a tutorial that shows every example's `.ag`
source followed by its rendered SVG, so it must stay in sync with the
contents of `examples/`. Place the new section where it fits the tutorial
progression (basics → layering → stats → layouts → derived tables → annotations
→ theming), not just at the end.

## Conventions

- Diagnostics are values, not exceptions. Parser/analyzer/renderer return their
  output plus a `Vec<Diagnostic>`; reserve `panic!` for programmer bugs (spec
  §23.4).
- Every token and syntax node carries a byte-offset `Span` (spec §6.12, §11.2).
  Spans are half-open `[start, end)`. Always test byte offsets with non-ASCII
  input — they are bytes, not chars.
- The lexer/parser are resilient: recover and continue on bad input, emit a
  diagnostic, and never panic (spec §12.1, §27.4).
- Output must be deterministic — stable ordering, no time/locale dependence
  (spec §18.12, §23.6).
- Tests live in each crate's `tests/` directory and follow the categories in
  spec §27. Add tests alongside new behavior.
