# Algraf v0.49.0 Plan

Status: Implemented
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_48_PLAN.md`](V0_48_PLAN.md)
Roadmap theme: embedded host parity for interactive SVG output.

## Purpose

v0.49.0 closes the gap between the CLI/LSP interactive SVG path and the
embedded Rust rendering facade used by host runtimes. Hosts can opt into the
same fixed Algraf-shipped interactive runtime without copying JavaScript or
introducing a source-authored script surface.

## Scope

### Embedded Interactive SVG Option

Status: Implemented.

`EmbeddedRenderOptions` gains an `interactive: bool` option. The default remains
`false`, preserving script-free SVG output for existing embedded callers.

When `interactive` is `true` and the output format is SVG, embedded rendering
MUST emit the same fixed, audited interactive runtime as CLI `--interactive`.
The chart body remains identical to the static SVG body; the only additional
output is the single Algraf-shipped `<script>` appended before `</svg>`.

The option does not accept script text, script URLs, callbacks, or runtime
configuration from chart source or host configuration. It selects only Algraf's
owned runtime.

### PNG Behavior

Status: Implemented.

`interactive` does not change PNG output. PNG is raster image output, so embedded
rendering continues to rasterize static SVG bytes.

## Non-Goals

- No new `.ag` syntax or source feature gate.
- No custom JavaScript, URL, callback, or plugin runtime API.
- No change to CLI `--interactive`, LSP preview interactivity, or static
  `<title>`/`data-algraf-highlight` metadata.
- No Webpipe-specific configuration surface in this repository.

## Validation

- Embedded-render tests cover the default script-free behavior and the
  `interactive: true` SVG path.
- Interactive embedded output is checked to share the static chart body prefix
  before the appended script.
- The full workspace checks remain clean.
