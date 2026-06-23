# Algraf v0.87.0 Plan

Status: Implemented
Target version: 0.87.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_86_PLAN.md`](V0_86_PLAN.md)
Roadmap theme: Split the agent-facing language reference into language and
tooling parts while preserving the full reference used by project
initialization.
Cross-repo coordination: downstream tools such as source-sleuth may consume the
language-only reference from `algraf-wasm` 0.87.0 once the package is published
or packed locally.

## Purpose

Algraf v0.86 exposed the full `ALGRAF_LANG.md` template through the browser
WASM package. That is useful for project initialization and coding-agent setup,
but source-inspection tools often only need the language surface: syntax,
declarations, geometries, properties, scales, guides, enums, data-source forms,
and rendering semantics.

This release splits the maintained reference prose so callers can choose a
small language-only context, a tooling-only context, or the same full composed
reference that `algraf init --codex`, `algraf init --claude`, and
`algraf init --agy` write into projects.

## Scope

### Split Language Reference Templates

Status: Implemented.

Acceptance criteria:

- The current full `crates/algraf-cli/templates/ALGRAF_LANG.md` content is
  split into two maintained source templates:
  `crates/algraf-cli/templates/ALGRAF_LANGUAGE.md` for Algraf syntax,
  declarations, algebra, geometries, properties, scales, guides, themes, enum
  values, data-source forms, and rendering semantics; and
  `crates/algraf-cli/templates/ALGRAF_TOOLING.md` for CLI commands, caller-data
  workflows, agent setup, package/WASM usage, common workflow advice, and
  operational pitfalls.
- The full `ALGRAF_LANG.md` written by `algraf init --codex`,
  `algraf init --claude`, and `algraf init --agy` is composed from the language
  and tooling templates in a deterministic order and contains no prose that is
  not present in one of the maintained source templates.
- Existing generated project behavior remains compatible: `algraf init` still
  writes a project-root `ALGRAF_LANG.md`, still refuses to overwrite different
  existing content, and still points agent instruction files at
  `ALGRAF_LANG.md`.
- Tests assert that the composed full reference contains both parts, that the
  language-only template omits CLI/tooling sections such as `CLI Commands` and
  `Project Agent Setup`, and that the full reference cannot drift from its
  source parts.
- `AGENTS.md` tells future maintainers which changes belong in the language
  template, which changes belong in the tooling template, and that release
  version work must keep those templates aligned with the implemented surface.

### WASM Language Reference APIs

Status: Implemented.

Acceptance criteria:

- The existing manual browser ABI export
  `algraf_language_reference_json() -> packed_ptr_len` remains available and
  continues to return the full composed reference for compatibility with v0.86
  consumers.
- The manual browser ABI adds
  `algraf_language_reference_part_json(ptr, len) -> packed_ptr_len`, where
  request JSON is `{ "part": "language" | "tooling" | "full" }`.
- Language-reference responses contain the selected Markdown, the package
  version, the selected part name, and source path metadata for the maintained
  template file or files used to produce that response.
- The implementation embeds the maintained templates with `include_str!` and
  composes the full reference at the boundary; it does not fork a second copy of
  the same prose.
- Source-only browser consumers such as source-sleuth can request the
  language-only Markdown without receiving CLI, package, or agent setup
  details.

### TypeScript Package Surface

Status: Implemented.

Acceptance criteria:

- `packages/wasm` keeps `runtime.languageReference()` as the full composed
  reference by default.
- `packages/wasm` extends `runtime.languageReference(options?)` so
  `options.part` can request `language`, `tooling`, or `full`.
- TypeScript declarations describe language-reference parts, source metadata,
  and language-reference results.
- `packages/wasm/README.md` documents the full and part-selecting APIs and
  notes that the language-only reference is intended for small LLM context
  windows.

### Spec And Release Artifacts

Status: Implemented.

Acceptance criteria:

- `ALGRAF_SPEC.md` records v0.87 as a language-reference split and WASM
  part-selection release.
- WASM ABI documentation describes the compatibility `full` call and the new
  part-selecting request/response payload.
- `ALGRAF_SPEC.md` documents that the project-root `ALGRAF_LANG.md` produced by
  `algraf init` is a full composed language+tooling reference, while WASM
  callers can request either part or the combined reference.
- Version stamps are updated to `0.87.0` when the implementation lands.
- Browser package publication remains independent from the Rust/CLI release.
  Before changing npm package manifests or downstream consumer pins, verify the
  exact published versions with `npm view` as required by `AGENTS.md`.

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- `npm run check` from `packages/wasm`
- `npm run build:wasm` from `packages/wasm`
- `npm pack --dry-run` from `packages/wasm`

## Explicitly Deferred Past v0.87.0

- Any change to Algraf source syntax, chart analysis, rendering behavior, data
  loading, or diagnostics.
- Semantic IR or chart-metadata-only browser APIs.
- Browser `check` or `format` convenience APIs beyond the existing editor
  service surface.
- Native data backends in `algraf-wasm`.
