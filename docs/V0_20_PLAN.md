# Algraf v0.20.0 Plan

Status: Complete
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_19_PLAN.md`](V0_19_PLAN.md)
Follow-on plan: [`V0_21_PLAN.md`](V0_21_PLAN.md)

## Purpose

This document defines the final intended v0.20.0 release shape: promoting the
language-surface features that have been mentioned as optional since the early
plans, after the refactor runway has made source compatibility easier to reason
about.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to implement the settled surface below; an item ships only when
syntax, diagnostics, tests, docs, and examples land together.

This plan is intentionally complete enough to implement without further product
decisions. Implementation may still discover bugs or force mechanical
adjustments, but the source-level behavior, naming, and deferred decisions are
settled here.

## Release Thesis

v0.20.0 is a **language versioning and reuse** release: give source files a
version declaration, introduce a disciplined feature-gate mechanism, and finish
small expressive gaps that have been explicitly deferred: reusable style
fragments, calendar-aware temporal bins, gradient stop positions, and richer
escape handling. It also turns temporal parsing and display formats from loose
implementation choices into explicit, testable contracts.

The release is intentionally about source-language ergonomics, not data
backends or rendering engines.

## Current Debt Surface

The plan/spec audit found:

- v0.5 explicitly deferred the optional `Algraf(version: "...")` declaration.
- v0.5 deferred reusable style fragments because constant `let` values were the
  first cut and property bags needed a separate design.
- v0.2/v0.3 deferred calendar-aware bin intervals such as
  `interval: "month"`.
- Temporal inference currently recognizes only the original RFC3339/ISO-shaped
  inputs, and temporal label formats are examples rather than a settled renderer
  contract.
- v0.3 implemented evenly spaced color gradients but deferred gradient stop
  positions.
- The spec mentions Unicode string escapes and advanced quoted-identifier escape
  modes as later additions.
- Feature gates are listed in the spec but no plan currently promotes the
  mechanism.

## Scope Rules

- New syntax must be backwards compatible for existing `.ag` files.
- Version declarations and feature gates are optional in v0.20; unversioned
  files keep current behavior.
- Do not add SQL, network access, plugins, custom stats, output backends, or
  runtime interactivity in this release.
- Every new source form needs parser, formatter, semantic, LSP, VS Code grammar,
  diagnostic, and example coverage before it is considered done.
- Do not add `color` or `colour` aliases in this release.
- Do not register LSP on-type formatting in this release.

## Capstone Acceptance Target

The capstone is a source-compatible language polish release:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

Existing examples may gain optional version declarations only if the release
explicitly chooses that migration during implementation. Otherwise
`git diff -- examples` should be empty except for new examples and intentional
renderer changes caused by temporal display formatting.

## Design Decisions (settled)

1. **Versioning comes before feature gates.** A file must have a way to declare
   its language expectations before feature-gated syntax expands.
2. **The source header is a call-like top-level declaration.**
   `Algraf(version: "0.20", features: [...])` appears before any `Chart`.
3. **Feature gates are named strings.** They are parsed, offered in completion,
   documented, and diagnosed in v0.20 even when no gated feature ships yet.
4. **Style fragments are property bags, not functions.** They reuse `let`
   binding and `CallValue` syntax but do not introduce arbitrary computation.
5. **Style application uses a `style:` argument.** This avoids a new spread
   operator and keeps the parser close to the existing argument-list model.
6. **Temporal intervals are stat options.** Calendar bins belong to `Bin` and
   `Histogram`, not a new date arithmetic language.
7. **Temporal formats are data/render contracts.** Adding an input format means
   extending data type inference; adding a display format means extending
   deterministic renderer label formatting and guide options.
8. **Gradient positions are explicit stop values.**
   `Stop(value: ..., color: "...")` augments the existing string-array gradient
   form without changing default interpolation.
9. **Unicode escapes use one syntax.** Strings and quoted identifiers both use
   `\u{...}` for Unicode scalar values.
10. **Potentially confusing aliases stay rejected.** `color` and `colour` remain
    diagnostics that tell the user to choose `fill` or `stroke`.

## Final v0.20 Surface

The final source-visible target is:

```ag
Algraf(version: "0.20")

Chart(data: "events.csv") {
    let muted = Style(fill: "#6b7280", alpha: 0.55)

    Derive months = Bin(time, interval: "month")

    Scale(
        fill: value,
        gradient: [
            Stop(value: 0, color: "#3366cc"),
            Stop(value: 50, color: "#f7f7f7"),
            Stop(value: 100, color: "#cc3333"),
        ],
    )

    Guide(axis: x, timeFormat: "iso-minute")

    Space(time * value) {
        Point(style: muted, stroke: "#111827")
    }
}
```

The promoted temporal input format is the strict minute-precision naive datetime
form `YYYY-MM-DD HH:MM`, for example `2026-05-25 14:30`.

The promoted temporal display format is named `iso-minute` and renders datetime
labels as `YYYY-MM-DD HH:MM`. Date-only values continue to render as
`YYYY-MM-DD` when an ISO-style date label is required.

## v0.20.0 Must

### 1. Source-level language version declaration

Status: Implemented in v0.20. The final syntax is a single optional
top-level declaration:

```ag
Algraf(version: "0.20")
```

It may also carry feature gates:

```ag
Algraf(version: "0.20", features: ["sql"])
```

Acceptance criteria:

- Add optional `Algraf(...)` before the first `Chart` block.
- The parser accepts at most one `Algraf(...)` declaration per file.
- The declaration has no body and is not a chart.
- `version` is a required string when the declaration is present.
- The canonical v0.20 spelling is `"0.20"`; `"0.20.0"` is accepted and
  normalized in AST/JSON metadata.
- Unsupported future versions produce a diagnostic but still parse the rest of
  the file for editor recovery.
- Malformed declarations recover before the first `Chart` where possible.
- Unversioned files remain valid and keep current behavior.
- CLI, LSP, formatter, semantic tokens, VS Code grammar, AST/JSON, and docs
  handle the declaration.
- Diagnostics explain duplicate declarations, missing `version`, malformed
  version strings, and unsupported future versions.

### 2. Feature gate declaration model

Status: Implemented in v0.20. Feature gates are declared inside the
`Algraf(...)` header with `features: [...]`.

Acceptance criteria:

- `features` is an optional array of string literals.
- Feature names use stable lowercase slug strings.
- The v0.20 registry reserves `sql`, `network`, `plugins`, and `experimental`.
  These gates are recognized for diagnostics, completion, and hover; they do not
  enable SQL, network access, plugins, or experimental syntax in v0.20.
- Unknown gates produce a diagnostic with a suggestion when possible.
- Duplicate gates produce a diagnostic and are ignored after the first
  occurrence.
- Feature gates are reported in AST/JSON and LSP document metadata.
- Completion and hover document known gates and their v0.20 availability.
- Later plans can attach SQL, network, plugins, or experimental syntax to this
  mechanism without redesigning the source header.

### 3. Reusable style fragments

Status: Implemented in v0.20. The final syntax is:

```ag
let muted = Style(fill: "#6b7280", alpha: 0.55)

Point(style: muted, stroke: "#111827")
```

Acceptance criteria:

- `Style(...)` is a property-bag value that can be bound with `let`.
- A `style:` argument applies a style fragment inside geometry calls and
  declaration calls that accept styleable properties.
- `style:` values must resolve to a `Style(...)` fragment; other values produce
  a targeted diagnostic.
- Fragment application expands at the `style:` argument position.
- Later explicit properties override earlier fragment properties.
- A later fragment overrides earlier explicit properties only if the later
  `style:` argument appears later in the argument list.
- Duplicate diagnostics remain deterministic and point to the fragment key,
  the `style:` use site, and the overriding property where useful.
- Fragment keys are validated against the receiving call. Unknown or
  inapplicable keys are reported at the fragment definition and use site when
  enough context exists.
- A fragment cannot contain `style:` to avoid recursive expansion.
- Fragments cannot contain column mappings unless the receiving property
  context allows that mapping.
- Cyclic style references are impossible in v0.20 because `Style(...)` cannot
  refer to another variable; if implementation later permits nested fragments,
  cycles must be diagnosed before shipping.
- LSP rename, references, completion, hover, and formatter support both the
  `let` binding and `style:` use sites.
- Add an example that uses one chart-scope fragment and one space-scope
  fragment to prove shadowing remains the existing `let` shadowing model.

### 4. Calendar-aware temporal intervals and formats

Status: Implemented in v0.20. Calendar-aware binning uses
`interval: "<unit>"`; deterministic ISO-style temporal display uses
`Guide(..., timeFormat: "iso-minute")`.

Acceptance criteria:

- `Bin` and `Histogram` accept `interval` for temporal inputs.
- Supported interval strings are `minute`, `hour`, `day`, `week`, `month`,
  `quarter`, and `year`.
- Date-only inputs accept `day`, `week`, `month`, `quarter`, and `year`.
- Datetime inputs accept all supported intervals.
- `interval` is mutually exclusive with `bins`, `binWidth`, and `boundary`.
- `closed` remains supported and keeps its existing left/right semantics.
- Weeks start on Monday.
- Quarters start on January 1, April 1, July 1, and October 1.
- Calendar interval assignment is deterministic across time zones and does not
  depend on local system time.
- Naive datetimes use the existing UTC-equivalent interpretation.
- Offset-aware RFC3339 datetimes are normalized to UTC before interval
  assignment.
- Calendar bins are half-open by default: `[start, end)`.
- For date-only inputs, `bin_start` and `bin_end` remain date-only; `bin_center`
  is the lower midpoint date by ordinal day.
- For datetime inputs, `bin_start`, `bin_end`, and `bin_center` remain
  datetimes; `bin_center` is the exact UTC-equivalent midpoint instant.
- Numeric binning behavior remains unchanged.
- Temporal inference accepts the strict `YYYY-MM-DD HH:MM` datetime format in
  addition to existing v0.1 formats.
- The promoted input format is accepted for datetimes only, not date-only
  columns.
- Invalid calendar dates or times in the promoted input format remain strings
  during inference and produce the existing late-invalid-value warning when a
  temporal scale has already been selected.
- The promoted display format is named `iso-minute` and renders datetime labels
  as `YYYY-MM-DD HH:MM`.
- `Guide(axis: x, timeFormat: "iso-minute")` and
  `Guide(axis: y, timeFormat: "iso-minute")` apply only to temporal axes.
- `timeFormat: "iso-date"` is accepted for temporal axes and renders date-only
  labels as `YYYY-MM-DD`; datetime values using `iso-date` render the UTC date
  portion.
- Unknown `timeFormat` values produce a targeted diagnostic.
- Temporal display formatting is deterministic across locales and time zones.
- Temporal axes use nice calendar ticks and labels where the interval model
  gives the renderer enough information to do so deterministically.
- Tests cover date-only, naive datetime, and offset-aware datetime inputs.
- Tests cover accepted and rejected examples of `YYYY-MM-DD HH:MM`.
- Snapshot tests cover `iso-minute` and `iso-date` axis labels.
- Add or update examples for a monthly temporal histogram and an `iso-minute`
  time-series axis.

### 5. Gradient stop positions

Status: Implemented in v0.20. The existing evenly spaced string-array form
continues to work:

```ag
Scale(fill: value, gradient: ["#3366cc", "#cc3333"])
```

The positioned form uses `Stop(...)` call values:

```ag
Scale(
    fill: value,
    gradient: [
        Stop(value: 0, color: "#3366cc"),
        Stop(value: 50, color: "#f7f7f7"),
        Stop(value: 100, color: "#cc3333"),
    ],
)
```

Acceptance criteria:

- Continuous `fill` and `stroke` scales accept positioned gradient stops.
- The existing array of two or more color strings keeps its current evenly
  spaced behavior.
- A positioned gradient is an array of two or more
  `Stop(value: ..., color: "...")` values.
- String stops and `Stop(...)` values cannot be mixed in the same gradient.
- Stop `value` is a domain value, not a normalized 0-1 position.
- Stop values must be strictly increasing after type conversion.
- Stop values must be inside the trained domain when the domain is known.
- If the first or last stop is inside the domain instead of exactly at the
  domain boundary, the renderer extends the edge color to the boundary.
- Colors use the existing color validation rules.
- Continuous legends reflect positioned stops deterministically.
- Invalid stop declarations emit targeted diagnostics for arity, unknown keys,
  missing keys, non-monotonic values, out-of-domain values, and invalid colors.
- Existing gradient examples continue to render unchanged.
- Add a positioned-gradient example or update `examples/gradient.ag` only if the
  output change is intentional and documented.

### 6. Escape syntax expansion

Status: Implemented in v0.20. Strings and quoted identifiers gain Unicode
scalar escapes:

```ag
let label = "Revenue \u{2014} forecast"
Space(`city\u{20}name` * value) {
    Point()
}
```

Acceptance criteria:

- Add `\u{...}` Unicode escape syntax for double-quoted strings.
- The escape body accepts one to six ASCII hex digits.
- The decoded value must be a valid Unicode scalar value; surrogate code points
  and values above `U+10FFFF` are invalid.
- Quoted identifiers support `\u{...}`, `\\`, and escaped backticks.
- Quoted identifiers do not gain string-only control escapes such as `\n` or
  `\t`; column names that contain actual newlines remain out of scope for
  v0.20 examples and diagnostics.
- Invalid escapes produce precise diagnostics and recover.
- Unterminated Unicode escapes recover without swallowing the rest of the file.
- Formatter and AST/JSON preserve the intended decoded value; formatter output
  may preserve the original escape spelling or use a canonical `\u{...}`
  spelling, but must round-trip through parse and format.
- Tests cover non-ASCII strings, quoted identifiers, invalid scalar values,
  invalid hex digits, unterminated escapes, and byte/UTF-16 range conversion.

### 7. Spec, plan, and example hygiene

Status: Implemented in v0.20.

Acceptance criteria:

- Workspace and VS Code versions are bumped to `0.20.0` when the release branch
  is ready.
- Spec §6, §7, §9, §10, §15, §16, §19, §20, §21, §26, and §30 are updated
  before shipped syntax is marked complete.
- Diagnostic codes are reserved before implementation for malformed headers,
  unsupported versions, unknown feature gates, invalid style fragments, invalid
  temporal intervals/formats, invalid gradient stops, and invalid Unicode
  escapes.
- README and examples demonstrate each promoted source feature.
- Examples are regenerated with `./examples/generate.sh`.
- This plan is updated to `Status: Complete` only after implementation, tests,
  docs, and examples land.

## v0.20.0 Should

### Color alias policy decision

Status: Settled for v0.20. `color` and `colour` remain rejected.

`fill` and `stroke` have different semantics. A generic color alias would make
code shorter but less precise, especially for geometries that accept both. The
existing diagnostic should remain: choose `fill` or `stroke`; do not add
aliases in v0.20.

Acceptance criteria:

- Keep rejecting `color` and `colour` as ordinary geometry/declaration
  properties.
- Diagnostics mention `fill` and `stroke` as the intentional alternatives.
- No README or example uses `color` or `colour`.

### On-type formatting pilot

Status: Rejected for v0.20.

Do not implement `textDocument/onTypeFormatting` in this release. The formatter
is holistic, and v0.20 adds source-header, style-fragment, and escape-syntax
surface area that should not be reformatted opportunistically while a document
is syntactically incomplete. Document/range formatting remains the supported
editor formatting surface.

Acceptance criteria:

- No on-type formatting capability is registered.
- Existing document and range formatting behavior remains unchanged except for
  formatting the new v0.20 syntax.
- Spec §21 keeps the on-type formatting rationale aligned with this decision.

### Nested `Space` and qualified-name design

Status: Design-only in v0.20; implementation remains deferred.

The future design is:

- A nested `Space` creates a child space scope.
- Child spaces inherit the active data source, theme, guide defaults, and scale
  defaults unless they override them locally.
- Child spaces do not inherit geometry properties from parent geometries.
- Qualified names use `.` in future source syntax, for example
  `outer.inner`.
- Qualified names resolve only named language entities, not arbitrary columns.
- Column names containing `.` remain ordinary column names and must be quoted
  when the grammar would otherwise parse the dot as qualification.
- LSP navigation must treat each qualified segment as a resolvable symbol.

No parser support for nested `Space` blocks or qualified names ships in v0.20.
The design reserves `.` for later work and prevents v0.20 from introducing
syntax that would conflict with it.

## Explicitly Deferred Past v0.20.0

- SQL, network sources, command sources, and environment-variable access.
- Plugins, custom stats, user-defined functions, and macros.
- Nested `Space` implementation and qualified-name implementation.
- 3D Cartesian rendering.
- New render backends, interactivity, and animation.
- On-type formatting.
- `color`/`colour` aliases.
- User-configurable temporal format strings beyond the named formats
  `iso-date` and `iso-minute`.

## Optional-Item Audit

### Promote In v0.20.0 (Must)

- Source-level version declarations.
- Feature gate declaration model.
- Reusable style fragments.
- Calendar-aware temporal intervals and formats.
- Gradient stop positions.
- Unicode string and quoted-identifier escape expansion.
- Spec, plan, and example hygiene.

### Settle In v0.20.0 Without Promoting

- Color alias policy: rejected.
- On-type formatting pilot: rejected.
- Nested `Space` and qualified-name design: documented only.

### Keep Deferred

- Data backend, plugin, render backend, and deep algebra features.

## Promotion Workflow

1. Add parser/formatter fixtures for `Algraf(...)`, `features`, `Style(...)`,
   `style:`, `Stop(...)`, `timeFormat`, and Unicode escapes.
2. Implement version declarations and feature gate parsing before other gated or
   version-sensitive features.
3. Add style fragments with semantic and LSP support.
4. Add temporal input parsing, named temporal display formats, calendar interval
   bins, and temporal guide diagnostics.
5. Add positioned gradient stops and continuous legend coverage.
6. Expand string and quoted-identifier escape handling.
7. Reserve diagnostics and update spec sections before marking behavior
   complete.
8. Update README, examples, and VS Code grammar.
9. Run formatter, clippy, workspace tests, regenerate examples, and review
   intentional example diffs.
