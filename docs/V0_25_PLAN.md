# Algraf v0.25.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_24_PLAN.md`](V0_24_PLAN.md)

## Purpose

This document defines the intended v0.25.0 release shape: adding controlled
extensibility for custom computation and marks after the language, data, and
render boundaries have been hardened.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.25.0 is an **extensibility and sandboxing** release. It addresses the
standing deferred items around plugins, custom stats, custom geometries,
user-defined functions, and macros, but only inside a strict capability model.

Algraf's core value is deterministic declarative charts. Extensibility must not
turn render into arbitrary code execution by default.

## Current Debt Surface

The plan/spec audit found:

- Plugins and custom stats have been deferred since the earliest plans.
- The spec says future plugin geometry support must be carefully sandboxed.
- User-defined functions and macros were deferred when v0.5 added only constant
  `let` bindings.
- Feature gates are needed before experimental or unsafe capabilities can be
  introduced cleanly.
- Render backends and data engines will have clearer contracts by v0.24, which
  makes a controlled extension surface more realistic.

## Scope Rules

- Extensibility is opt-in and capability-scoped.
- Plugins MUST NOT execute arbitrary host commands by default.
- Built-in behavior remains available without plugins.
- Core parser, semantics, and SVG rendering must remain deterministic for
  plugin-free files.
- Extension APIs must declare version compatibility.
- Do not add a broad package manager in this release.

## Capstone Acceptance Target

The capstone is a sandboxed custom stat or geometry example that can be enabled
explicitly, rejected when disabled, and tested deterministically.

The release must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

## Design Decisions (settled)

1. **Capabilities before execution.** A file or invocation must declare which
   extension capabilities it needs.
2. **Custom stats are safer than custom render code.** Start with pure data
   transforms before allowing mark renderers.
3. **Macros/functions need clear hygiene.** They must not undermine source spans,
   diagnostics, or LSP navigation.
4. **No hidden I/O.** Extension code cannot read files, network, environment, or
   processes unless a separate capability explicitly allows it.

## v0.25.0 Must

### 1. Extension capability model

Status: Planned.

Acceptance criteria:

- Define how source files, CLI, and LSP opt into extension capabilities.
- Capabilities are named, versioned, and visible in diagnostics.
- Disabled capabilities produce deterministic diagnostics before execution.
- The model integrates with v0.20 feature gates if they exist.

### 2. Custom stat API

Status: Planned.

Acceptance criteria:

- Define a plugin or extension API for pure table-to-table stats.
- Custom stats declare input requirements, output schema, settings, and docs.
- Semantic analysis can obtain output schemas without executing unsafe code on
  the editor hot path.
- Execution is deterministic and cannot perform hidden I/O.
- LSP completion/hover/signature help can surface registered custom stats.

### 3. Custom geometry API

Status: Planned.

Acceptance criteria:

- Define whether custom geometries produce render-model operations, SVG
  fragments, or backend-neutral marks.
- Custom geometry rendering is sandboxed and cannot inject unsafe SVG or script
  by default.
- Unsupported backends report clear diagnostics or fallbacks.
- Built-in geometry output remains unchanged.

### 4. User-defined functions

Status: Planned.

Acceptance criteria:

- Specify a narrow expression function model if functions are promoted.
- Functions are pure, deterministic, and typed.
- Functions do not shadow columns or built-in syntax in ambiguous positions.
- Diagnostics and LSP navigation preserve useful source spans.

### 5. Macro or template model

Status: Planned.

Acceptance criteria:

- Decide whether Algraf supports macros/templates distinct from functions.
- If implemented, expansion is hygienic, deterministic, and inspectable in
  diagnostics or debug output.
- Formatter and LSP behavior are specified before implementation.
- If the design is not mature, this item may land as a rejection/design note
  rather than implementation.

### 6. Plugin packaging and trust policy

Status: Planned.

Acceptance criteria:

- Define local plugin discovery, manifest format, compatibility checks, and
  trust prompts or CLI flags.
- No global package registry is required.
- LSP and CLI use the same plugin resolution policy.
- Failure modes are deterministic and user-facing.

### 7. Spec, plan, and example hygiene

Status: Planned.

Acceptance criteria:

- Workspace and VS Code versions are bumped to `0.25.0` when the release branch
  is ready.
- Spec §2, §3, §13, §14, §15, §21, §22, §23, §26, §29, and §30 are updated for
  promoted extension behavior.
- README and examples include a minimal extension walkthrough if implementation
  lands.
- Examples are regenerated with `./examples/generate.sh`.

## v0.25.0 Should

### Extension test harness

Status: Planned.

Add a first-class harness for testing plugin diagnostics, schemas, rendering,
and denied capabilities without requiring external package installation.

### Package-manager deferral note

Status: Planned.

Document what a future extension registry or package manager would require, and
why this release stops at local plugins.

## Explicitly Deferred Past v0.25.0

- Global plugin marketplace or package registry.
- Unsandboxed host-language execution.
- Hidden network/environment/process access.
- Third-party render backends that bypass the backend contract.

## Optional-Item Audit

### Promote In v0.25.0 (Must)

- Extension capability model.
- Custom stat API.
- Custom geometry API.
- User-defined functions.
- Macro/template model.
- Plugin packaging and trust policy.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- Extension test harness.
- Package-manager deferral note.

### Keep Deferred

- Global package distribution and unsandboxed execution.

## Promotion Workflow

1. Specify capabilities and denial diagnostics first.
2. Add custom stat schema and execution API.
3. Add custom geometry API only after backend safety is clear.
4. Decide on functions and macros with source-span tests.
5. Add plugin packaging/trust policy.
6. Add extension tests, examples, and docs.
7. Run formatter, clippy, workspace tests, regenerate examples, and review
   intentional diffs.
