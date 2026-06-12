# Algraf v0.81.0 Plan

Status: Implemented
Target version: 0.81.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_80_PLAN.md`](V0_80_PLAN.md)
Roadmap theme: LSP document-version test harness stability.
Cross-repo coordination: none required to ship 0.81.0. The browser packages
`algraf-wasm` and `algraf-editor` are not published at 0.81.0 implementation
time, so their package versions and consumer pins remain on the latest verified
published version, 0.75.0.

## Purpose

The LSP backend already enforces the document-version invariant needed for
concurrent editor traffic: a stale lower-version `didChange` or stale analysis
result must not replace newer document text. The previous regression test covered
that behavior through a full `tower-lsp` JSON-RPC integration path. That made the
test sensitive to transport details, especially the bounded server-to-client
diagnostics socket, and could leave the workspace test suite parked even though
the backend invariant itself was simple.

The v0.81.0 goal is to keep the stale-version regression covered while removing
the fragile transport dependency from that specific test.

## Release Thesis

Protocol integration tests should prove protocol behavior. Backend invariants
that do not require JSON-RPC should be tested next to backend document
management, where failures point at the cache/version logic instead of client
socket backpressure or unrelated notification plumbing.

## Must

- The stale lower-version document update regression is covered by a focused
  backend unit test that calls document upsert directly.
  Status: Implemented by
  `stale_upsert_does_not_clobber_newer_document_text`.

- The full LSP integration suite no longer includes the hanging
  stale-`didChange` transport test.
  Status: Implemented. `cargo test -p algraf-lsp` now runs the remaining
  protocol-level tests and the focused backend regression quickly.

- The backend behavior remains unchanged: newer document text stays visible to
  text-derived features, and a lower document version is ignored.
  Status: Implemented. The replacement test asserts the cached version, cached
  text, and semantic-token stream after a stale update.

- Spec §27.7 documents that document-version cache invariants should be covered
  at backend/document-management level in addition to protocol integration tests.
  Status: Implemented.

## Deferred

- A broader rewrite of the LSP integration harness to automatically drain
  uninspected client notifications remains deferred.
- Switching LSP integration helpers from direct `LspService` calls to a full
  client/server loopback harness remains deferred until a protocol-level test
  needs that fidelity.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Focused validation:

- `cargo test -p algraf-lsp`
- `cargo test -p algraf-lsp stale_upsert_does_not_clobber_newer_document_text -- --nocapture`

## Promotion Workflow

Implemented in this change:

1. Move stale document-version coverage from the hanging integration case to a
   backend unit test.
2. Update `ALGRAF_SPEC.md` §27.7 and the milestone table.
3. Align Rust, spec, VS Code, and demo release version stamps to `0.81.0`; keep
   unpublished browser package pins on the latest verified npm version.
4. Run the validation commands listed above.
