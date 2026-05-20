# CLAUDE.md

Guidance for working in the Algraf repository.

## What Algraf is

Algraf is a block-scoped, algebraic grammar-of-graphics DSL (file extension
`.ag`). It parses a declarative chart description, validates it against CSV
data, trains scales, and emits deterministic SVG. The whole system — parser,
language server, runtime, and renderer — ships as one Rust binary named
`algraf`.

The normative reference is `docs/ALGRAF_SPEC.md`. **Read the relevant section
of the spec before implementing or changing behavior.** The spec uses
RFC-2119-style keywords (`MUST`, `SHOULD`, `MAY`, `MUST NOT`) and assigns stable
diagnostic codes (e.g. `E0012`); honor both. When code intentionally deviates
from a `SHOULD`, document why in a comment.

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

## Example Generation

Run `./examples/generate.sh` to regenerate the SVG and PNG outputs for all examples.

Create new examples by adding a new file to the `examples/` directory.

Create new examples when adding new features or fixing bugs.

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
  (spec §22, §23.6).
- Tests live in each crate's `tests/` directory and follow the categories in
  spec §27. Add tests alongside new behavior.
