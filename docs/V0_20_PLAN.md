# Algraf v0.20.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_19_PLAN.md`](V0_19_PLAN.md)
Follow-on plan: [`V0_21_PLAN.md`](V0_21_PLAN.md)

## Purpose

This document defines the intended v0.20.0 release shape: promoting the
language-surface features that have been mentioned as optional since the early
plans, after the refactor runway has made source compatibility easier to reason
about.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when syntax, diagnostics, tests,
docs, and examples land together.

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
- Version declarations and feature gates must be optional at first; unversioned
  files keep current behavior unless the spec deliberately changes that rule.
- Do not add SQL, network access, plugins, custom stats, output backends, or
  runtime interactivity in this release.
- Every new source form needs parser, formatter, semantic, LSP, VS Code grammar,
  diagnostic, and example coverage before it is considered done.
- Avoid property aliases unless an explicit alias policy is promoted.

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
explicitly chooses that migration. Otherwise `git diff -- examples` should be
empty except for new examples.

## Design Decisions (settled)

1. **Versioning comes before feature gates.** A file must have a way to declare
   its language expectations before feature-gated syntax expands.
2. **Style fragments are property bags, not functions.** They reuse constant
   value semantics where possible but do not introduce arbitrary computation.
3. **Temporal intervals are stat options.** Calendar bins belong to `Bin` and
   `Histogram`, not a new date arithmetic language.
4. **Gradient positions must stay deterministic.** Explicit positions augment
   existing stop arrays without changing default interpolation.
5. **Temporal formats are data/render contracts.** Adding an input format means
   extending data type inference; adding a display format means extending
   deterministic renderer label formatting, not adding source-language date
   arithmetic.

## v0.20.0 Must

### 1. Source-level language version declaration

Status: Planned.

Acceptance criteria:

- Add an optional top-level declaration such as `Algraf(version: "0.20")`.
- The parser accepts at most one version declaration before chart blocks and
  recovers from malformed declarations.
- Unversioned files remain valid and keep current behavior.
- CLI, LSP, formatter, semantic tokens, VS Code grammar, AST/JSON, and docs
  handle the declaration.
- Diagnostics explain unsupported future versions and malformed declarations.

### 2. Feature gate declaration model

Status: Planned.

Acceptance criteria:

- Define how a source file opts into feature-gated syntax or runtime behavior.
- Feature gates are parsed and reported even if no gated feature is enabled yet.
- Feature names are stable strings with completion and hover docs.
- Unknown gates produce a diagnostic with a suggestion when possible.
- Later plans can use gates for SQL, network, plugins, or experimental syntax
  without redesigning the source header.

### 3. Reusable style fragments

Status: Planned.

Acceptance criteria:

- Add a property-bag value such as `Style(...)` or an equivalent explicitly
  specified syntax.
- A style fragment can be bound with `let` and applied inside geometry or
  declaration calls without hiding individual property diagnostics.
- Fragment application preserves deterministic property precedence and duplicate
  diagnostics.
- Fragments cannot contain column mappings unless the spec deliberately allows
  them for the receiving property context.
- LSP rename, references, completion, hover, and formatter support the new form.

### 4. Calendar-aware temporal intervals and formats

Status: Planned.

Acceptance criteria:

- `Bin` and `Histogram` accept a temporal interval option such as
  `interval: "month"` for date/datetime inputs.
- Supported intervals are enumerated in the spec before implementation.
- The spec names the exact additional date/datetime input format promoted in
  v0.20 and whether it is accepted for date-only values, datetimes, or both.
- Temporal inference accepts the promoted input format deterministically without
  falling back to locale-dependent parsing or local system time.
- The spec names the exact temporal display format promoted in v0.20 and the
  axis/guide contexts where the renderer uses it.
- Temporal display formatting is deterministic across locales and time zones,
  and it preserves the date-only versus datetime distinction.
- Interval assignment is deterministic across time zones and does not depend on
  local system time.
- Numeric binning behavior remains unchanged.
- Tests cover date-only, naive datetime, and offset-aware datetime inputs.
- Tests cover accepted and rejected examples of the promoted input format plus
  snapshot coverage for the promoted display format.
- Temporal axes use nice calendar ticks and labels where the new interval model
  gives the renderer enough information to do so deterministically.

### 5. Gradient stop positions

Status: Planned.

Acceptance criteria:

- Continuous `fill`/`stroke` scales accept explicit stop positions while the
  existing evenly spaced array form keeps working.
- Positions are validated for arity, monotonic order, and domain range.
- Continuous legends reflect positioned stops deterministically.
- Invalid stop declarations emit targeted diagnostics.
- Existing gradient examples continue to render unchanged.

### 6. Escape syntax expansion

Status: Planned.

Acceptance criteria:

- Add Unicode escape syntax for double-quoted strings.
- Decide and document whether quoted identifiers support the same escapes or a
  narrower advanced escape mode.
- Invalid escapes produce precise diagnostics and recover.
- Formatter and AST/JSON preserve the intended escaped value.
- Tests cover non-ASCII strings and byte/UTF-16 range conversion.

### 7. Spec, plan, and example hygiene

Status: Planned.

Acceptance criteria:

- Workspace and VS Code versions are bumped to `0.20.0` when the release branch
  is ready.
- Spec §6, §7, §9, §10, §15, §16, §19, §20, §21, §26, and §30 are updated
  before shipped syntax is marked complete.
- README and examples demonstrate each promoted source feature.
- Examples are regenerated with `./examples/generate.sh`.

## v0.20.0 Should

### Color alias policy decision

Status: Planned.

Decide whether `color`/`colour` remain rejected forever or become explicit
aliases for a narrow context. Do not add aliases unless diagnostics, docs, and
examples can make the behavior unambiguous.

### On-type formatting pilot

Status: Planned.

If v0.18 finds safe cases, implement on-type formatting only for closing braces
or newlines. Otherwise keep it deferred.

### Nested `Space` and qualified-name design

Status: Planned.

Write a concrete design for nested `Space` blocks and future qualified names
using `.`, including scope, inheritance, diagnostics, and LSP navigation. Do not
implement either syntax unless the design can land with parser, semantic,
formatter, and editor support.

## Explicitly Deferred Past v0.20.0

- SQL, network sources, command sources, and environment-variable access.
- Plugins, custom stats, user-defined functions, and macros.
- Nested `Space` implementation and 3D Cartesian rendering.
- New render backends, interactivity, and animation.

## Optional-Item Audit

### Promote In v0.20.0 (Must)

- Source-level version declarations.
- Feature gate declaration model.
- Reusable style fragments.
- Calendar-aware temporal intervals and formats.
- Gradient stop positions.
- Unicode and quoted-identifier escape expansion.
- Spec, plan, and example hygiene.

### Consider If Capacity Allows (Should)

- Color alias policy decision.
- On-type formatting pilot.
- Nested `Space` and qualified-name design.

### Keep Deferred

- Data backend, plugin, render backend, and deep algebra features.

## Promotion Workflow

1. Add parser/formatter fixtures for the source header and style fragments.
2. Implement version declarations and feature gate parsing before gated features.
3. Add style fragments with semantic and LSP support.
4. Add temporal input parsing, temporal display formatting, interval bins, and
   gradient stop positions.
5. Expand escape handling.
6. Update specs, examples, README, and VS Code grammar.
7. Run formatter, clippy, workspace tests, regenerate examples, and review
   intentional example diffs.
