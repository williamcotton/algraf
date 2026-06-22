# Algraf v0.86.0 Plan

Status: Implemented
Target version: 0.86.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_85_PLAN.md`](V0_85_PLAN.md)
Roadmap theme: Expose source-structure and language-reference data through the
browser WASM package so downstream tools can inspect Algraf files without
reimplementing parsing or copying docs.
Cross-repo coordination: downstream tools may consume `algraf-wasm` 0.86.0
once the package is published or packed locally.

## Purpose

Downstream tools need two source-level capabilities that already exist inside
Algraf but were not exposed through the browser package:

- parse-tree JSON equivalent to `algraf ast --json`;
- the current `ALGRAF_LANG.md` language reference for LLM/tool context.

Both are source/docs surfaces. They must not load data, analyze schemas, render
charts, or enable native-only data engines in `algraf-wasm`.

## Scope

### Shared AST JSON Serializer

Status: Implemented.

Acceptance criteria:

- The JSON serializer used by `algraf ast --json` lives below CLI ownership so
  `algraf-cli` and `algraf-wasm` call the same implementation.
- The existing CLI JSON shape remains stable: nodes contain `node`, `span`, and
  `children`; tokens contain `token`, `text`, and `span`.
- Byte spans remain byte offsets and are tested with non-ASCII source text.

### WASM AST API

Status: Implemented.

Acceptance criteria:

- The manual browser ABI exports `algraf_ast_json(ptr, len) -> packed_ptr_len`.
- Request JSON is `{ "source": "...", "variables": { ... } }`.
- Response JSON is
  `{ "ast": object | null, "diagnostics": [], "error": string | null }`.
- Variable expansion uses the same `algraf_driver::expand_variables` behavior
  as render and CLI `--var`.
- Resilient parse diagnostics are returned with the AST; malformed request JSON
  or variable expansion failures return `ast: null` and a span-less `error`.

### Embedded Language Reference API

Status: Implemented.

Acceptance criteria:

- The manual browser ABI exports
  `algraf_language_reference_json() -> packed_ptr_len`.
- The response contains the template text from
  `crates/algraf-cli/templates/ALGRAF_LANG.md`, the package version, and the
  source path string.
- The implementation embeds the existing template with `include_str!`; it does
  not copy or fork the reference text.

### TypeScript Package Surface

Status: Implemented.

Acceptance criteria:

- `packages/wasm` exposes `runtime.ast(source, variables?)`.
- `packages/wasm` exposes `runtime.languageReference()`.
- TypeScript declarations describe AST nodes, tokens, spans, AST results, and
  language-reference results.
- `packages/wasm/README.md` documents both APIs.

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- `npm run check` from `packages/wasm`
- `npm run build:wasm` from `packages/wasm`
- `npm pack --dry-run` from `packages/wasm`

## Explicitly Deferred Past v0.86.0

- Semantic IR or chart-metadata-only browser APIs.
- Browser `check` or `format` convenience APIs.
- Native data backends in `algraf-wasm`.
- Any change to Algraf source syntax or rendering behavior.
