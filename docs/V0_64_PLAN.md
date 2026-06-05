# Algraf v0.64.0 Plan

Status: Implemented
Target version: 0.64.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_63_PLAN.md`](V0_63_PLAN.md)
Cross-repo coordination: `../pdl/docs/V0_29_PLAN.md`,
`../studio/docs/V0_4_PLAN.md`

## Purpose

Algraf v0.64 extends the existing interaction sidecar story with explicit,
read-only event emitters for host applications. The release goal is to let an
Algraf chart declare that a mark can emit a data field for a named event, while
keeping Algraf stateless and data-agnostic. Studio or another host captures the
emitted value and decides whether it updates PDL state, UI state, or nothing at
all.

The intended split is:

- Algraf parses, validates, renders, and emits inert SVG plus sidecar metadata.
- Algraf does not track selections, mutate values, evaluate PDL variables, or
  know what a host will do with emitted events.
- Studio consumes the sidecar, performs pointer snapping, and routes emitted
  values into its own orchestration state.
- PDL consumes host-supplied parameter/state context in a separate release.

This plan is not normative until [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) promotes the
source syntax, diagnostics, and sidecar schema changes with concrete MUST/SHOULD
language. Existing tooltip/highlight metadata from earlier releases should
remain compatible unless the spec explicitly supersedes it.

## Scope

### Event Emitter Syntax

Status: Implemented.

Acceptance criteria:

- Add a declarative event-emitter form, tentatively:

  ```algraf
  Chart(data: "zone_overview.csv", width: 350, height: 300) {
      Theme(name: "minimal")
      Scale(fill: zone, palette: "accent")

      Space(zone * total_revenue) {
          Bar(fill: zone)
          On(event: "click", emit: zone)
      }
  }
  ```

- `On(...)` declares event metadata only. It MUST NOT embed callbacks, scripts,
  state mutation, URLs, fetches, or host-specific routing names.
- `event` is a string event name accepted by the sidecar contract. The first
  release should require `"click"` unless hover/brush are explicitly specified
  and tested.
- `emit` names a column whose per-row value the host can read from the sidecar.
- Parser, formatter, semantic registry metadata, completions, hover, signature
  help, TextMate grammar, and examples are updated for the promoted syntax.

### Read-Only Semantics

Status: Implemented.

Acceptance criteria:

- Algraf remains a pure presenter. There are no `$` or `@` sigils, no PDL state
  references, no selection registry, and no cross-chart state engine in Algraf.
- The semantic analyzer validates that `emit` references an existing column and
  that the `On` block is placed where per-mark data can be resolved.
- Event declarations do not affect scale training, geometry layout, derived
  tables, mark ordering, or static SVG rendering except for emitted metadata.
- Charts without `On(...)` preserve existing SVG and sidecar behavior.

### Interaction Sidecar Extension

Status: Implemented.

Acceptance criteria:

- The versioned sidecar carries enough data for a host to map browser pointer
  coordinates to a mark and extract the emitted field value.
- Each event-capable mark record includes stable mark identity, plot identity,
  pixel position or bounds, grouping values, and an interaction object such as:

  ```json
  {
    "id": "plot0:g0:r2",
    "plot": "plot0",
    "x_px": 120.4,
    "y_px": 195.0,
    "groups": { "zone": "Riverfront" },
    "interaction": { "event": "click", "emit_field": "zone" }
  }
  ```

- The host can resolve `mark.groups[mark.interaction.emit_field]` to the emitted
  value without evaluating Algraf source or scraping SVG attributes.
- Sidecar key ordering, numeric formatting, mark ordering, and grouping value
  formatting remain deterministic.
- Existing sidecar fields such as `plot_rect`, axes, marks, groups, and plots
  stay stable unless a versioned migration is explicitly documented.

### Geometry And Coordinate Coverage

Status: Implemented.

Acceptance criteria:

- The first implementation MUST support ordinary per-row Cartesian marks needed
  by the Studio integration, at minimum `Bar` and `Point` if their current mark
  sinks expose stable coordinates.
- If additional geometries are easy to support through the same mark metadata
  path, include them only with tests.
- Unsupported placements or geometries produce targeted diagnostics rather than
  partial or misleading metadata.
- Pixel records should include bounds where the renderer already knows them;
  otherwise nearest-point snapping may be the documented initial host behavior.

### CLI, WASM, And Demo Surfaces

Status: Implemented.

Acceptance criteria:

- CLI sidecar emission through `--metadata` and `--format svg+json` includes the
  new event metadata.
- `algraf-wasm` render responses include the updated sidecar string when SVG is
  produced.
- Existing demos continue to render charts and may add a small example that logs
  or displays emitted field/value pairs, but Algraf does not ship Studio-specific
  state routing.
- Native, WASM, SVG, draw-list, and sidecar outputs remain derived from the same
  planned scene.

## Non-Goals

- PDL parameters, PDL states, or `$`/`@` syntax.
- Host routing rules such as "click zone updates `selected_zone`".
- Embedded JavaScript, callbacks, URL-valued interactions, network behavior, or
  mutation inside SVG output.
- A built-in selection engine, brush registry, or cross-chart coordination model.
- Arbitrary event handlers beyond the named inert events specified for v0.64.
- Changing existing tooltip/highlight semantics except where necessary to share
  sidecar infrastructure.

## Validation

Required checks before this plan can be marked landed:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Additional validation:

- Parser, formatter, semantic, and diagnostic tests for `On(event: ..., emit:
  ...)`.
- Render and sidecar snapshot tests for a `Bar` selector chart and a `Point`
  selector chart.
- Tests proving charts without `On(...)` do not change static SVG output.
- WASM ABI tests confirming `render` returns sidecar JSON with event metadata.
- CLI tests for `--metadata` and `--format svg+json` output.
- Editor-service/LSP metadata tests for completion, hover, signature help,
  semantic tokens where relevant, and static grammar highlighting.
- Browser review of the demo if a demo example or chart wrapper changes.

## Deferred

- Brush/range events and multi-value emissions.
- Host-side selection persistence or cross-chart coordination.
- Framework-specific React/Vue/Svelte packages beyond existing demo/reference
  code.
- Sidecar schema version 2 unless the v1 shape cannot represent event emitters
  safely.
