# Algraf v0.27.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_26_PLAN.md`](V0_26_PLAN.md)

## Purpose

This document defines the intended v0.27.0 release shape: making Algraf
straightforward to embed in a pipeline DSL or host application in the same
practical style as a ggplot-like plotting middleware.

The target host shape is a WebPipe-style step that receives structured pipeline
state, accepts inline Algraf source, applies request-time variables, and returns
SVG or PNG bytes without requiring temporary chart files or host filesystem
reads.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.27.0 is an **embedding and invocation ergonomics** release. Earlier releases
made the driver I/O boundary injectable, split data loading from semantics, and
kept render deterministic. This release turns those seams into a documented
product surface for CLI pipelines and Rust hosts.

The core decision is that `Chart(data: stdin)` remains the Algraf language
sentinel for caller-provided primary data. In the CLI, that caller is the
process standard input. In an embedded host, that caller is the host-provided
input bytes or structured JSON state. Algraf should not add WebPipe-specific
keywords such as `input` or `pipeline`; a host DSL may alias those to `stdin`
before calling Algraf if it wants a more domain-native spelling.

## Current Debt Surface

The plan/spec audit found:

- The CLI can read source from a file or stdin, but not from an inline string
  while leaving stdin free for piped data.
- `Chart(data: stdin)` and `--data -` are currently specified as CSV-only, even
  though JSON and NDJSON are supported for path-backed sources.
- The public crates expose the pieces needed for embedding, but hosts must wire
  parsing, data I/O, preparation, rendering, diagnostics, theme selection, and
  output formatting by hand.
- Algraf has `let` bindings in the source language, but no invocation-time
  variable layer comparable to `--var color=red` in pipeline plotting tools.
- The current `SourceInput` model only distinguishes path vs stdin source text;
  inline source needs a stable diagnostic label and base-directory rule.
- Host integrations need clear security defaults: no implicit filesystem,
  network, environment, or process access.

## Scope Rules

- Embedding must use the existing parser, analyzer, driver, and renderer. Do not
  fork language behavior for WebPipe.
- `stdin` remains the canonical Algraf sentinel for caller-provided data.
- The default format for `stdin` remains CSV unless an explicit invocation or
  source-level format override is promoted.
- Inline source is a source-input mode, not a new language construct.
- Variable expansion is an invocation preprocessing layer, not hygienic Algraf
  macros and not user-defined functions.
- Embedded hosts must be able to disable path reads entirely.
- No network fetching, jq evaluation, request context model, or HTTP routing is
  added to Algraf.
- Single-chart stdin data remains the default safe case; multi-chart sharing of
  one input stream remains rejected unless a deliberate buffered-data design is
  promoted.

## Capstone Acceptance Target

The capstone is a WebPipe-shaped weather chart rendered from JSON pipeline
state, without creating an intermediate `.ag` file:

```bash
printf '[{"time":"2026-05-27T00:00","temp":68.1},{"time":"2026-05-27T01:00","temp":67.4}]' \
  | algraf render \
      --eval 'Chart(data: stdin, width: 800, height: 400) { Space(time * temp) { Line(stroke: "$color", strokeWidth: $size) Point(fill: "$color", size: $size) } }' \
      --data - \
      --data-format json \
      --var color="#e74c3c" \
      --var size=3
```

The equivalent Rust embedding path must accept the same source string, the same
JSON bytes or `serde_json::Value`, the same variables, and produce the same SVG
as the CLI path.

The release must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

## Design Decisions (settled)

1. **Keep `stdin` in Algraf.** It is already the language sentinel for
   caller-provided data. WebPipe may present `input` or `pipeline` as aliases at
   its own layer, but core Algraf should not grow host-specific names.
2. **Do not make data implicit.** `Chart(data: ...)` remains required. Embedded
   hosts may provide templates that insert `data: stdin`, but the core language
   keeps the data source visible.
3. **Make stream format explicit.** JSON pipeline state cannot rely on filename
   extension inference. A `--data-format`/embedded format setting is required
   for JSON, NDJSON, TSV, GeoJSON, and TopoJSON bytes read from the caller input.
4. **Variables are raw invocation templates first.** v0.27 may add a small,
   deterministic `$name` or `${name}` substitution layer. Hygienic macros,
   typed expression functions, and reusable templates remain v0.25 extension
   territory unless explicitly promoted there.
5. **Expose a facade, not a second engine.** The embedded API should compose the
   existing syntax, driver, semantics, and render crates behind a stable helper
   so host applications do not duplicate CLI internals.

## v0.27.0 Must

### 1. Inline source input for CLI commands

Status: Planned.

Acceptance criteria:

- Add `--eval <source>` and `-e <source>` to `render`, and consider the same
  input mode for `check`, `schema`, `ir`, and `ast` so debugging inline charts
  does not require temporary files.
- `--eval` is mutually exclusive with positional source input.
- `--eval` source gets a stable diagnostic label such as `<eval>` or
  `<inline.ag>`.
- Inline source resolves relative data paths against `--base-dir` when present,
  otherwise the current working directory. It is not treated as source stdin, so
  `Chart(data: stdin)` and `--data -` remain available for data bytes.
- CLI integration tests cover inline render success, parse diagnostics with
  useful spans, `--eval` plus `--data -`, and conflict errors.

### 2. Caller-provided data format override

Status: Planned.

Acceptance criteria:

- Add an explicit primary-data format override for caller-provided bytes, exposed
  at least as `--data-format <csv|tsv|json|ndjson|geojson|topojson>` on CLI
  render/check/schema/ir paths that can load data.
- The override applies to `--data -` and `Chart(data: stdin)`. Decide and
  specify whether it may also override extension inference for `--data <path>`.
- Default behavior remains unchanged: stdin without a format override is CSV.
- Driver resolution carries the selected format into `DataLocation::Stdin` or an
  equivalent plan object.
- Loading from stdin uses the same byte-slice readers as path-backed CSV, TSV,
  JSON, NDJSON, GeoJSON, and TopoJSON.
- Error messages and diagnostics name the selected stream format; malformed JSON
  and NDJSON reuse existing data diagnostic codes.
- Tests cover JSON array-of-objects stdin, NDJSON stdin, TSV stdin, bad format
  values, and backwards-compatible CSV stdin.

### 3. Embedded rendering facade

Status: Planned.

Acceptance criteria:

- Add a documented Rust API for rendering from:
  - inline Algraf source text;
  - host-provided primary data bytes or structured JSON;
  - an explicit data format;
  - optional variables;
  - width, height, theme, output format, strictness, and base-directory policy.
- The facade returns a structured result containing output bytes/string,
  content type, diagnostics, data warnings, and render metadata where available.
- Hosts can supply a `DriverIo` implementation that serves stdin bytes and may
  deny all path reads by default.
- The API does not depend on `algraf-cli`, clap, terminal diagnostics, or process
  stdin/stdout.
- The facade renders through the same `prepare_chart_with_io` and
  `render_with_tables` path as CLI render.
- Tests prove the embedded facade and CLI produce equivalent SVG for the same
  inline source, data bytes, variables, and render options.

### 4. Invocation variable expansion

Status: Planned.

Acceptance criteria:

- Add repeated `--var key=value` support to CLI source-consuming commands where
  inline/source variables are useful.
- Add the same variable map to the embedded facade.
- Specify the placeholder syntax before implementation. The preferred syntax is
  `${name}`; `$name` MAY be accepted for compatibility with existing pipeline
  plotting tools if ambiguity rules are documented.
- Undefined variables produce deterministic diagnostics or usage errors before
  parsing. Duplicate variables have a specified precedence or are rejected.
- Expansion happens before parsing and diagnostics clearly indicate whether
  spans refer to original template source, expanded source, or both.
- Values are raw Algraf source fragments after CLI shell parsing. Docs must show
  safe examples for string values (`stroke: "$color"`) and numeric values
  (`strokeWidth: $size`).
- The expansion layer is deliberately small: no conditionals, loops, jq,
  expression evaluation, environment-variable reads, or file includes.

### 5. Output format and middleware ergonomics

Status: Planned.

Acceptance criteria:

- Align CLI and embedded output selection around SVG and PNG. SVG remains native;
  PNG uses the existing rasterization path or the render backend promoted by
  v0.24 if it exists.
- Embedded results expose content types (`image/svg+xml`, `image/png`) so a host
  middleware can replace pipeline state directly.
- Width, height, theme, PNG scale, and PNG DPI have one documented precedence
  order between source declarations, CLI flags, and embedded request options.
- Binary PNG output is returned as bytes by the Rust API; base64 encoding is a
  host decision, not an Algraf core requirement.

### 6. Security and host I/O policy

Status: Planned.

Acceptance criteria:

- Document the secure embedded default: stdin/input bytes are available, path
  reads are denied unless the host explicitly provides a filesystem policy.
- Relative path resolution for inline source is deterministic and cannot
  silently use a fake source path outside the configured base directory.
- Host-provided `DriverIo` examples include an input-only provider and an
  allowlisted in-memory provider.
- No embedded API reads process environment variables, runs commands, opens
  network connections, or uses process stdin implicitly.
- Error wording distinguishes denied host I/O from missing files.

### 7. WebPipe/ggplot-style integration example

Status: Planned.

Acceptance criteria:

- Add a docs/example section that shows a WebPipe-shaped integration:

```text
GET /svg/weather
|> jq: weatherData
|> jq: `
  .hourly as $h |
  [$h.time, $h.temperature_2m] | transpose | map({time: .[0], temp: .[1]})
`
|> algraf({
  "type": "svg",
  "width": 800,
  "height": 400,
  "dataFormat": "json",
  "variables": {
    "color": "#e74c3c",
    "size": $context.request.query.size // "3"
  }
}): `
  Chart(data: stdin, width: 800, height: 400) {
    Space(time * temp) {
      Line(stroke: "$color", strokeWidth: $size)
      Point(fill: "$color", size: $size)
    }
  }
`
```

- The example states that `stdin` is the Algraf-side caller-input sentinel, not
  necessarily OS stdin inside an embedded host.
- The example does not require Algraf to implement WebPipe, HTTP routing, fetch,
  jq, or request-context evaluation.

### 8. Spec, plan, and example hygiene

Status: Planned.

Acceptance criteria:

- Spec updates cover:
  - §7 Chart source-input and data-source notes if any source-level format
    syntax is promoted.
  - §10 Data Sources for stream format selection and non-CSV stdin.
  - §22 CLI for `--eval`, `--var`, and `--data-format`.
  - §23 crate boundaries for the embedded facade and the rule that it does not
    depend on `algraf-cli`.
  - §26 Diagnostics Catalog for any new variable, inline-source, or
    data-format diagnostics.
  - §29 Security for embedded host I/O policy.
- Workspace `Cargo.toml` and `editors/vscode/package.json` are bumped to
  `0.27.0` when the release branch is ready.
- README and examples are updated only with runnable, deterministic examples.
- Examples are regenerated with `./examples/generate.sh` if rendered artifacts
  change.

## v0.27.0 Should

### Source-level stream format syntax

Status: Planned.

Consider a source-level spelling such as `Chart(data: stdin, dataFormat: "json")`
only if it can be extracted by the driver before analysis without muddying the
source-expression model. If promoted, it must be specified alongside CLI
`--data-format` precedence.

### Direct `serde_json::Value` input

Status: Planned.

The embedded facade should accept `serde_json::Value` directly if that avoids
host-side serialization boilerplate while preserving the same dataframe loader
semantics as JSON bytes.

### Async host facade

Status: Planned.

Add an async variant if it can wrap or reuse `AsyncDriverIo` without forcing
async dependencies on the synchronous CLI path.

### Diagnostic source maps for variables

Status: Planned.

If raw variable expansion makes diagnostics confusing, add a small source-map
model so parse and semantic errors can point back to template source where
possible.

## Explicitly Deferred Past v0.27.0

- Adding `input`, `pipeline`, or `state` as Algraf language aliases for
  `stdin`.
- Implicit `Chart(data: stdin)` when a chart omits `data`.
- jq, request context evaluation, HTTP routing, fetch, or middleware execution
  inside Algraf.
- A general macro/template language, hygienic expansion, or user-defined
  functions beyond simple invocation variable substitution.
- Environment-variable interpolation.
- Hidden network, filesystem, or process access in embedded mode.
- Multi-chart sharing of one unbuffered stdin stream.
- Browser/WASM package surface beyond whatever the embedded facade naturally
  enables.

## Optional-Item Audit

### Promote In v0.27.0 (Must)

- Inline source input for CLI commands.
- Caller-provided data format override.
- Embedded rendering facade.
- Invocation variable expansion.
- Output format and middleware ergonomics.
- Security and host I/O policy.
- WebPipe/ggplot-style integration example.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- Source-level stream format syntax.
- Direct `serde_json::Value` input.
- Async host facade.
- Diagnostic source maps for variables.

### Keep Deferred

- Host-specific language aliases, implicit data sources, jq/request evaluation,
  general macros, environment interpolation, hidden I/O, multi-chart stdin
  sharing, and browser-specific packaging.

## Promotion Workflow

1. Specify `stdin` semantics for embedded callers and reserve any new
   diagnostics in spec §26 before coding.
2. Add inline `SourceInput` support and CLI `--eval`; verify stdin remains
   available for data bytes.
3. Add stream format selection through driver planning and loading; keep CSV as
   the default.
4. Add the small variable expansion layer with tests for spans, missing
   variables, duplicates, strings, and numeric fragments.
5. Add the embedded rendering facade over existing driver/render APIs.
6. Wire SVG/PNG output options and content types through the facade.
7. Add security-policy tests for input-only and denied-path embedded I/O.
8. Add the WebPipe-style docs/example and update README where appropriate.
9. Run formatter, clippy, workspace tests, regenerate examples, and review
   intentional diffs.
