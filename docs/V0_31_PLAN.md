# Algraf v0.31.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_30_PLAN.md`](V0_30_PLAN.md)
Follow-on plan: [`V0_32_PLAN.md`](V0_32_PLAN.md)

## Purpose

This document defines the intended v0.31.0 release shape: clearing the accumulated
language-surface polish that prior feature releases deliberately deferred. These
are smaller, mostly independent items that did not justify their own release but
that real charts keep wanting:

- the temporal "Should" items deferred from [`V0_28_PLAN.md`](V0_28_PLAN.md)
  (IANA timezones, temporal literals, off-axis temporal formatting, time-only
  values with an anchor, parse-failure severity);
- the polar "deferred" items from [`V0_26_PLAN.md`](V0_26_PLAN.md) (the
  `radial_bar` concentric-ring mode and configurable start angle/direction).

The long-standing reserved language items — nested `Space` blocks and
space-local scale/guide/annotation declarations (spec §4.2) — were originally
scoped here but are **promoted to their own release**,
[`V0_32_PLAN.md`](V0_32_PLAN.md): they are a single coherent grammar/scope
subsystem rather than independent polish, and conflating them with this release's
small, mostly-independent items would couple unrelated work. v0.31 keeps its
language-surface-polish thesis; nested spaces become v0.32's headline.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.31.0 is a **language-surface polish** release. It does not introduce a new
subsystem; it finishes families of features that earlier releases left
intentionally incomplete so they could ship the core first. Every item is scoped
to stay deterministic, locale-independent, and consistent with the existing
algebra, scale, and coordinate models.

The unifying decision is to round out *existing* capabilities rather than open
new ones: polar already exists, so finish its chart family; temporal parsing and
formatting already exist, so make timezones, literals, and off-axis labels
consistent. The grammar's reserved nested-space and space-local-annotation items
are a new scope subsystem rather than polish, so they move to
[`V0_32_PLAN.md`](V0_32_PLAN.md) and are out of scope here.

## Current Debt Surface

The plan/spec/code audit found:

- [`V0_28_PLAN.md`](V0_28_PLAN.md) "Should" items are all `Planned` and unshipped:
  IANA timezone names (`timezone: "America/Chicago"`), temporal display outside
  axes (`Format(...)` / `Text(..., timeFormat: ...)`), time-only values with an
  anchor date, temporal literals (`datetime("…")`, `date("…")` for reference
  marks and explicit domains), and parse-failure severity controls
  (`onError: "warn" | "error" | "missing"`).
- [`V0_26_PLAN.md`](V0_26_PLAN.md) shipped polar across the geometry family but
  deferred `radial_bar.ag`, which needs a per-category independent angular-bar
  mode (`theta: "y"` with a categorical radius), distinct from the cumulative pie
  path and the value→radius coxcomb path. v0.26 also fixed the angle origin at
  12-o'clock clockwise and deferred configurable `startAngle`/`direction`.
- Spec §4.2 says "Nested spaces are reserved for later versions" and the first
  implementation rejects them with a diagnostic; §4.2 also reserves space-local
  scale/guide/annotation declarations "in later versions." This release does not
  resolve that reservation — it is the subject of
  [`V0_32_PLAN.md`](V0_32_PLAN.md).
- Temporal formatting (spec §19.4) only applies to axis guides; there is no
  consistent way to format a temporal column used as a text label or categorical
  legend entry, which the v0.28 plan flagged as a candidate surface.

## Scope Rules

- Every item stays deterministic and locale-independent. English month/weekday
  names remain fixed strings, not host-locale output (spec §19.4, §28).
- Temporal storage stays UTC-equivalent microseconds (the v0.28 settled
  decision). IANA timezones only affect interpretation of explicitly declared
  naive datetimes; no timezone-aware scale arithmetic or DST-aware calendar
  math.
- Polar additions stay in the scaled-space layer; geometries ask the space for
  coordinates and arc parameters (spec §10.5, §16.16). The algebra grammar is
  untouched.
- New temporal literals are values, not a new algebra primitive; they appear only
  where a value or domain bound is accepted.
- Cartesian and existing polar output remain byte-for-byte unchanged where a new
  argument is absent.
- Each item is independently testable and may ship or slip on its own.

## Capstone Acceptance Target

The capstone is the deferred `radial_bar.ag` plus a temporal chart that uses an
IANA timezone, a temporal literal reference line, and a custom off-axis label —
each previously impossible:

```ag
Chart(data: "sales_by_rep.csv", width: 600, height: 600) {
    Space(amount, coords: "polar", theta: "y", startAngle: 90) {
        Bar(fill: rep, radius: rep)
    }
}
```

```ag
Chart(data: "events.csv", width: 820, height: 420) {
    Parse(column: started_at, as: "datetime", format: "%m/%d/%Y %H:%M", timezone: "America/Chicago")

    Space(started_at * latency_ms) {
        Line()
        VLine(x: datetime("2026-05-27T20:00:00Z"), stroke: "red", label: "deploy")
    }
}
```

The release must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

## Design Decisions (settled)

1. **Finish, don't expand.** Each item completes an existing family; none adds a
   new subsystem.
2. **`radial_bar` is a layout/mapping mode, not a geometry.** Per the v0.26
   no-new-geometries rule, concentric independent bars come from a categorical
   radius mapping under `theta: "y"`, reusing `BarLayout`.
3. **Temporal literals are typed values.** `datetime("…")` and `date("…")` parse
   with the same conservative rules as automatic inference (spec §10.3) and yield
   a UTC-equivalent instant usable as a reference-mark position or scale-domain
   bound.
4. **Off-axis temporal formatting reuses the §19.4 format model.** A `timeFormat`
   on a text/label surface uses the same named and custom patterns as
   `Guide(timeFormat: …)`; no second formatting engine.
5. **IANA zones are an interpretation lens only.** They resolve a declared naive
   datetime to a UTC-equivalent instant; scale spacing stays UTC.
6. **Reserved language items move to their own release.** Nested spaces and
   space-local annotations are a coherent grammar/scope subsystem, not polish, so
   they are sequenced into [`V0_32_PLAN.md`](V0_32_PLAN.md) rather than decided
   here. They still do not stay in indefinite "reserved" limbo — v0.32 ships a
   tested, specified design or records a formal rejection.

## v0.31.0 Must

### 1. `radial_bar` concentric-ring mode

Status: Planned.

Acceptance criteria:

- Support a per-category independent angular-bar mode under `theta: "y"` with a
  categorical radius, producing concentric ring segments (distinct from the
  cumulative pie path and the value→radius coxcomb path).
- Reuse `BarLayout` and the existing polar wedge/annular-segment emission
  (spec §16.16); add no new geometry.
- Specify how the radius category is selected (e.g. a `radius:`/grouping mapping)
  and reserve a diagnostic for an invalid or missing radius category.
- Add the deferred `radial_bar.ag` example and register it in
  `examples/generate.sh`.

### 2. Configurable polar start angle and direction

Status: Planned.

Acceptance criteria:

- Add `Space(...)` polar arguments for start angle and direction
  (e.g. `startAngle:` in degrees, `direction: "clockwise" | "counterclockwise"`),
  with documented defaults equal to the current fixed 12-o'clock clockwise
  behavior so existing polar examples do not drift.
- Validate values and reserve diagnostics for out-of-range/invalid inputs.
- The angular range mapping (spec §16.16) is parameterized by these args.

### 3. Temporal literals

Status: Planned.

Acceptance criteria:

- Add `datetime("…")` and `date("…")` value constructors usable where a value or
  domain bound is accepted — at least `HLine`/`VLine` positions and
  `Scale(domain: [...])` bounds.
- Literals parse with the conservative automatic rules (spec §10.3) and yield a
  UTC-equivalent instant / date.
- Invalid literal contents produce a targeted diagnostic; reserve codes in §26.
- Temporal literals are not algebra primitives and are rejected inside `Space`
  frames and stat inputs (consistent with §9.6).

### 4. IANA timezone names

Status: Planned.

Acceptance criteria:

- `Parse(..., timezone: "America/Chicago")` and other IANA names are accepted,
  applying only when the selected pattern produces a naive datetime, resolving to
  a UTC-equivalent instant.
- Add `chrono-tz` (or equivalent) only if it does not pull timezone-aware scale
  arithmetic into scope; document the dependency.
- Unknown zone names produce the existing invalid-timezone diagnostic with
  updated wording.
- No DST-aware calendar spacing; the v0.28 deferral of timezone-aware scale
  arithmetic stands.

### 5. Off-axis temporal formatting

Status: Planned.

Acceptance criteria:

- Add a way to format temporal values used outside axis guides — at minimum a
  `timeFormat:` argument on `Text` labels, and/or a small `Format(column: …,
  timeFormat: …)` declaration — reusing the §19.4 named/custom format model.
- Output is deterministic and locale-independent; English names are fixed
  strings.
- Misuse (a `timeFormat` on a non-temporal value) produces a targeted
  diagnostic, consistent with the §19.4 non-temporal-axis rule.

### 6. Parse-failure severity controls

Status: Planned.

Acceptance criteria:

- Add `onError: "warn" | "error" | "missing"` to `Parse(...)`, defaulting to the
  current aggregated-warning behavior (`warn`/`missing` equivalent).
- `"error"` turns per-column parse failure into a blocking diagnostic for stricter
  ETL; `"missing"` coerces failures to missing without a warning where the user
  opts in.
- Behavior is deterministic and the aggregated-warning wording stays stable for
  the default.

### 7. Examples, README, spec, and release hygiene

Status: Planned.

Acceptance criteria:

- Add examples for `radial_bar`, a configurable-start-angle polar chart, a
  temporal-literal reference line, and off-axis temporal formatting; regenerate
  with `./examples/generate.sh`.
- README gains sections in the temporal and coords/polar tutorial progressions.
- Spec updates cover §7 (literal/`Format` grammar),
  §10.3 (temporal literal parsing), §14 (`Text`/reference-mark `timeFormat`,
  `radial_bar` mapping), §16.16 (start angle/direction parameters), §19.4
  (off-axis formatting), and §26 (diagnostics).
- Workspace `Cargo.toml` and `editors/vscode/package.json` are bumped to
  `0.31.0`; LSP completion/hover and the VS Code grammar gain any new keywords.

## v0.31.0 Should

### Time-only values with an anchor date

Status: Planned.

Allow explicit parsing of time-only columns when the user supplies an anchor date
(or pairs the time with a date column). Automatic time-only inference stays
rejected because a temporal scale needs a date anchor.

### Polar label legibility follow-up

Status: Planned.

Continue the v0.26 "polar axis label legibility" item: rotate/anchor perimeter
labels sensibly and avoid overlap on dense theta axes, now including
configurable start-angle orientation.

## Explicitly Deferred Past v0.31.0

- Natural-language dates, two-digit-year inference, host-locale month/weekday
  names (the v0.28 standing deferrals).
- Timezone-aware scale spacing, DST-aware calendar arithmetic, fiscal/business
  calendars.
- 3D+ polar frames and combining polar with geographic projections (spec §16.15).
- Extensibility — plugins, custom stats/geometries, user-defined functions, and
  macros remain the scope of [`V0_25_PLAN.md`](V0_25_PLAN.md), still pending and
  not reopened here.

## Optional-Item Audit

### Promote In v0.31.0 (Must)

- `radial_bar` concentric-ring mode.
- Configurable polar start angle and direction.
- Temporal literals.
- IANA timezone names.
- Off-axis temporal formatting.
- Parse-failure severity controls.
- Examples, README, spec, and release hygiene.

### Consider If Capacity Allows (Should)

- Time-only values with an anchor date.
- Polar label legibility follow-up.

### Keep Deferred

- Natural-language dates, locale formatting, timezone-aware arithmetic,
  fiscal/business calendars, 3D/geographic polar, and extensibility (v0.25).

## Promotion Workflow

1. Reserve new diagnostics in spec §26 before coding.
2. Add the `radial_bar` mapping mode and example over the existing polar
   emission.
3. Parameterize the polar angular range with start-angle/direction args.
4. Add `datetime`/`date` literal constructors and thread them into
   reference-mark and scale-domain positions.
5. Add IANA timezone interpretation for declared naive datetimes.
6. Add off-axis `timeFormat` over the §19.4 format model.
7. Add `Parse(onError: …)` severity handling.
8. Add examples, README, LSP/grammar updates; bump versions; confirm no
   unintended example drift.

## A note on sequencing

The next release after this polish pass is [`V0_32_PLAN.md`](V0_32_PLAN.md),
which takes on the reserved **nested `Space` blocks and space-local
scale/guide/annotation declarations** (spec §4.2) as a focused grammar/scope
subsystem — work that was originally listed here but pulled out so it gets a
release of its own rather than riding alongside unrelated polish.

The big still-unimplemented subsystem beyond that is **extensibility** —
plugins, custom stats, custom geometries, user-defined functions, and macros —
which already has a written but unshipped plan in
[`V0_25_PLAN.md`](V0_25_PLAN.md). Implementation leapfrogged v0.25 (releases
0.26–0.28 shipped first), so that plan remains the plan-of-record for
extensibility and should be slotted in (and renumbered if desired) when the team
is ready to take it on, after the nested-spaces release.
