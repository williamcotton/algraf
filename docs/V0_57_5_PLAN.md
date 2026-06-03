# Algraf v0.57.5 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Related PDL release: PDL v0.13 stream-interoperability plan.
Predecessor plan: [`V0_57_PLAN.md`](V0_57_PLAN.md)
Roadmap theme: PDL and Unix-pipe interop for caller-provided data streams.

## Purpose

v0.57.5 makes Algraf ready to consume tabular output from a separate PDL process
without a disk handoff.

PDL's preferred interop format is Arrow IPC streaming:

```bash
pdl run prep.pdl --stdout-format arrow-stream | algraf render chart.ag --data - --data-format arrow-stream --output chart.svg
```

Algraf's side of the contract is intentionally narrower than PDL's v0.13
source/sink work: Algraf consumes a table stream produced by PDL or another
tool, but it does not parse `.pdl`, run PDL stages, or add a second
`--stdin-format` CLI spelling. The existing Algraf caller-data boundary remains
`Chart(data: input)` plus `--data -` and `--data-format`.

Algraf already has the right source-language shape for this workflow:

```ag
Chart(data: input) {
    Space(region * revenue) {
        Bar(fill: region)
    }
}
```

The missing Algraf pieces are native Arrow IPC stream input, stdin format
sniffing with a CSV fallback, CLI/driver/embedded consistency, and tests that
prove piped bytes are consumed once and interpreted correctly.

## Scope

### Arrow IPC Stream Caller Data

Status: Implemented.

Algraf supports Arrow IPC stream bytes as caller-provided data for
`Chart(data: input)`, `Chart(data: stdin)`, and `--data -`.

Acceptance criteria:

- `algraf-data` MUST load Arrow IPC stream bytes into the existing dataframe
  abstraction without exposing Arrow internals to parser, semantics, LSP, or
  render crates.
- Arrow scalar and null mapping MUST follow the Parquet/Arrow logical type
  policy already used for native Parquet sources where practical.
- Unsupported Arrow physical or logical types MUST produce registered
  diagnostics before rendering.
- Arrow stream loading MUST work from in-memory byte providers as well as native
  stdin-backed CLI input.
- Tests MUST cover numeric, string, boolean, temporal, and null-containing Arrow
  stream inputs.

### Stdin Format Selection And Sniffing

Status: Implemented.

Caller-provided bytes from stdin use explicit format selection first, then
sniffing, then CSV fallback.

Acceptance criteria:

- `--data-format` MUST accept `arrow-stream`.
- `--data-format arrow` MAY be accepted as a compatibility alias for
  `arrow-stream`.
- Explicit `--data-format` MUST override sniffing.
- Without `--data-format`, caller-provided stdin bytes MUST be sniffed for
  Arrow IPC stream and Parquet magic bytes before falling back to CSV.
- Sniffed Arrow IPC file bytes SHOULD produce a deterministic unsupported-format
  diagnostic rather than being treated as CSV.
- Text sniffing for JSON/NDJSON MAY remain deferred; those formats continue to
  work through explicit `--data-format`.
- Sniffing MUST preserve peeked bytes so the selected loader receives the full
  stream.
- Existing CSV stdin workflows MUST continue to work without `--data-format`.
- The command MUST continue to reject using stdin for both Algraf source and
  caller-provided data in the same invocation.

### Driver And CLI Integration

Status: Implemented.

The existing driver source-resolution seam remains the single integration point
for CLI, LSP, tests, embedded callers, and future hosts.

Acceptance criteria:

- `algraf-driver` MUST represent `arrow-stream` as a first-class explicit caller
  data format.
- Driver/data error mapping MUST assign stable diagnostics for Arrow stream
  parse errors, unsupported Arrow types, and unsupported sniffed stream formats.
- `algraf render --help` and relevant CLI diagnostics MUST list `arrow-stream`
  in the accepted `--data-format` values.
- `algraf schema` MUST be able to read caller-provided Arrow stream bytes when
  supplied through `--data - --data-format arrow-stream`.
- JSON diagnostic output MUST report the same diagnostic codes as human output.

### Embedded And WASM Boundaries

Status: Implemented.

Embedded callers should be able to supply Arrow stream bytes through the same
driver-facing data boundary, while browser support remains explicit about its
feature availability.

Acceptance criteria:

- The native embedded render facade MUST accept explicit `arrow-stream`
  caller-provided bytes.
- WASM MAY defer Arrow stream loading if the Arrow dependency footprint is too
  large for the browser build.
- If WASM defers Arrow streams, it MUST fail through a registered data/driver
  diagnostic rather than panic.
- Browser hosts that do not supply caller input MUST continue to avoid reading
  process stdin; an unavailable stream is a host data issue, not a browser I/O
  operation.
- Browser editor-service requests MUST NOT try to read process stdin.

### LSP And Editor Behavior

Status: Implemented.

Editor behavior stays schema-aware when caller-provided schemas are available
and conservative when stdin data is unavailable.

Acceptance criteria:

- `Chart(data: input)` and `Chart(data: stdin)` MUST continue to avoid hard
  unknown-column diagnostics when no caller-provided schema is available.
- Hover text for `input`/`stdin` SHOULD describe caller-provided data and mention
  CLI format override/sniffing behavior.
- Completion and hover MAY use in-memory Arrow stream bytes when supplied by an
  editor-service host.
- LSP tests MUST cover non-ASCII span conversion for diagnostics produced around
  caller-data source expressions.

### Documentation And Examples

Status: Implemented.

The repository documents the PDL-to-Algraf Unix pipe workflow.

Acceptance criteria:

- `docs/ALGRAF_SPEC.md` MUST document `arrow-stream` in the caller-provided data
  format list before implementation is marked complete.
- `docs/ALGRAF_SPEC.md` MUST reserve any new Arrow stream diagnostics before
  code emits them.
- The README or docs MUST include a PDL-to-Algraf pipe example using
  `Chart(data: input)`.
- At least one test fixture MUST model PDL output by piping or supplying Arrow
  IPC stream bytes to Algraf.

### Release Version Alignment

Status: Implemented.

Workspace, extension, demo, lockfile, and specification version stamps are
aligned to `0.57.5` for this active plan.

Acceptance criteria:

- `Cargo.toml` and `Cargo.lock` record workspace crates at `0.57.5`.
- `docs/ALGRAF_SPEC.md` records `0.57.5` as the working-copy specification and
  lists this plan in the release-planning milestone table.
- `editors/vscode/package.json`, `editors/vscode/package-lock.json`,
  `demo/package.json`, and `demo/package-lock.json` record `0.57.5`.

## Non-Goals

- No PDL implementation in the Algraf repository.
- No `.pdl` parser or PDL runtime inside Algraf.
- No integrated PDL+Algraf runner.
- No mutation or generation of `.ag` source from PDL.
- No new `.ag` source syntax such as `stdin(format: "...")` in this release.
- No browser/WASM Arrow support requirement if the dependency footprint is too
  large; native CLI support is the required target.
- No change to path-backed source extension inference except where
  caller-provided `--data <path>` already uses `--data-format` as an override.

## Validation

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Feature-specific tests covered during implementation:

- `algraf-data` Arrow IPC stream fixture loading.
- Driver format sniffing preserves peeked bytes.
- CLI render from `--data - --data-format arrow-stream`.
- CLI render from `--data -` with sniffed Arrow stream bytes.
- CLI schema from `--data - --data-format arrow-stream`.
- CSV stdin remains the fallback when no Arrow/Parquet magic is detected.
- LSP caller-data diagnostics remain non-fatal when stdin schema is unavailable.
