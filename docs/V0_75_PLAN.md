# Algraf v0.75.0 Plan

Status: Implemented (shipped as 0.75.0; npm packages algraf-wasm@0.75.0 and
algraf-editor@0.75.0 published, demo updated)
Target version: 0.75.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_74_PLAN.md`](V0_74_PLAN.md)
Roadmap theme: Comprehensive date/time visualization support; robust temporal
parsing, explicit temporal axis declarations, linear timeseries spacing,
calendar-aware tick intervals, and deterministic temporal tick labels.
Cross-repo coordination: none required to ship 0.75.0.

## Purpose

Algraf v0.75 is a broad temporal visualization release. It makes date and
datetime columns easier to ingest, easier to validate, easier to control, and
easier to render correctly as continuous position dimensions.

Algraf already has temporal data types, temporal literals, explicit `Parse(...)`
support, temporal position mapping, temporal domains, and `Guide(timeFormat:
...)`. This release builds on that foundation rather than introducing temporal
support from zero. The goal is to make the whole temporal path feel complete:
authors should be able to load ordinary date/time data, map it directly to an
axis, see missing dates represented as real elapsed time, choose calendar-aware
tick cadence, and format tick labels without preprocessing their data outside
Algraf.

Sequencing note: v0.75's scale IR additions landed in the existing
`crates/algraf-cli/src/ir_json.rs` serializer module. v0.74's proposed CLI
module split remains independent maintenance; rebase it over this work when
it starts.

The most important user-facing behavior is linear timeseries spacing. If a table
has rows for `2024-01-01`, `2024-01-02`, and `2024-01-10`, the distance between
January 2 and January 10 must be eight times the distance between January 1 and
January 2. Missing rows must not collapse elapsed time the way a categorical
axis would.

## Release Thesis

Dates are not categories. A temporal axis is a continuous numeric mapping over
normalized instants, with calendar-specific guide behavior layered on top.

This creates two related requirements:

1. **Position mapping must be mathematical.** Date and datetime values map
   linearly through the trained temporal domain. Missing dates, weekends, quiet
   months, and sparse event periods remain visible as proportional gaps unless
   the author explicitly chooses a categorical axis.
2. **Guide generation must be calendar-aware.** Numeric tick ladders cannot
   faithfully express weeks, months, quarters, years, leap days, or variable
   month lengths. v0.75 adds explicit guide interval controls so authors can ask
   for labels every day, week, month, quarter, or year while retaining
   deterministic output.

This release also clarifies terminology and property placement. `timeFormat`
remains the canonical Algraf property for temporal tick-label formatting and
stays on `Guide`, which owns label presentation. Tick *positions* are scale
concerns in Algraf — exact `breaks:` already lives on `Scale` — so generated
tick cadence joins it there. New DSL surface follows existing Algraf
declaration style: `Scale(axis: x, type: "temporal")` and
`Scale(axis: x, tickInterval: "1 month")`, not ggplot-style helper calls or
snake_case aliases.

## Current Baseline

The existing implementation and spec already include substantial temporal
behavior:

- `Temporal` data type and temporal column inference.
- `date("...")` and `datetime("...")` literals.
- Explicit parse declarations through `Parse(...)`.
- CSV, JSON, Arrow, and native data paths that can produce temporal columns.
- Continuous temporal position mapping in the renderer.
- Temporal domain bounds and exact temporal `breaks:`.
- Named and custom temporal formatting through `Guide(timeFormat: ...)`.
- Calendar-aware temporal bin intervals for histogram/bin-derived data.
- Tick-label rotation and row dodging through `tickLabelAngle` and
  `tickLabelRows`.

v0.75 should audit, extend, and harden these behaviors into one coherent date
axis system.

## Proposed Spec Changes

`ALGRAF_SPEC.md` should be updated in the same implementation change that lands
the code. The spec changes are normative; this plan is sequencing guidance.

- Add an explicit temporal position scale type:

  ```ag
  Scale(axis: x, type: "temporal")
  Scale(axis: y, type: "temporal")
  ```

  `type: "temporal"` is axis-only. Aesthetic scales must reject it. It is valid
  for temporal columns and unknown columns that later resolve to temporal data.
  It must not silently coerce ordinary strings into dates at render time.

- Add a temporal scale tick interval property:

  ```ag
  Scale(axis: x, tickInterval: "1 week")
  Scale(axis: x, tickInterval: "1 month")
  Scale(axis: x, tickInterval: "1 year")
  ```

  `tickInterval` is axis-only and applies to temporal axes. It lives on
  `Scale` next to `breaks:` because both control tick positions; `Guide`
  keeps the presentation properties (`timeFormat`, `tickLabelAngle`,
  `tickLabelRows`). It controls tick generation, not data binning. Histogram
  and derive binning continue to use their existing `interval:` controls.
  Declaring both `tickInterval` and explicit `breaks:` on the same axis is a
  diagnosed conflict; `breaks:` wins.

- Keep `timeFormat` as the temporal tick-label formatting property:

  ```ag
  Guide(axis: x, timeFormat: "%Y-%m-%d")
  Guide(axis: x, timeFormat: "%b %-d, %Y")
  Scale(axis: x, tickInterval: "1 month")
  Guide(axis: x, timeFormat: "%b %Y")
  ```

  Do not introduce `date_format` in v0.75. Do not add snake_case aliases.

- Clarify precedence among temporal guide controls:
  - `Scale(breaks: [...], labels: [...])` supplies exact tick values and exact
    labels.
  - `Scale(breaks: [...])` supplies exact tick values, then `timeFormat`
    controls the generated labels.
  - `Scale(tickInterval: ...)` supplies generated calendar ticks when explicit
    breaks are absent. Declaring both on one axis is a diagnosed conflict.
  - Automatic temporal ticks are used when neither explicit breaks nor
    `tickInterval` is declared.
  - `tickLabelAngle` and `tickLabelRows` affect label layout only.

- Clarify date/time determinism requirements:
  - Mapping must be independent of host locale, wall clock, and local timezone.
  - Default and custom formatting must be deterministic.
  - Calendar ticks must use documented UTC-equivalent calendar boundaries.

## Scope

The implementation spans the data, driver, semantics, render, editor, examples,
and documentation surfaces.

### Data Ingestion and Parse Policy

Temporal ingestion should become more predictable across CSV and JSON, with
native/Arrow behavior preserved.

- Audit CSV and JSON inference for date-only, datetime, RFC3339 offset datetime,
  and common ISO-like strings.
- Preserve missing values as null temporal cells when the surrounding column is
  temporal.
- Avoid degrading a mostly temporal column to string solely because it contains
  blanks, explicit null values, or configured missing markers.
- For malformed non-missing cells in an otherwise temporal column, prefer a
  non-fatal diagnostic plus null cell when the current data path can report that
  row-level issue deterministically.
- Keep explicit `Parse(...)` declarations authoritative over inference.
- Preserve date-only precision where all non-missing values are dates.
- Lift date-only values to midnight when a column intentionally mixes dates and
  datetimes, preserving datetime scale behavior.
- Preserve RFC3339 offset normalization to UTC-equivalent instants.
- Ensure Arrow date and timestamp columns continue to become `Temporal` columns.
- Add focused data tests for blank values, invalid cells, mixed date/datetime
  values, offset-aware datetimes, and explicit parse policies.

### PDL Interoperability

PDL temporal scalar functions currently return existing value classes:
normalized dates and datetimes are strings, calendar fields are numbers, and
parse failures are null. Algraf v0.75 should make the PDL-to-Algraf workflow
work without requiring PDL to introduce primitive date/datetime output types.

- Treat PDL `date(value)` output (`YYYY-MM-DD`) as ordinary temporal input when
  loaded through CSV, JSON, or Arrow string columns. This is the main path for
  daily timeseries charts with missing dates.
- Treat PDL `datetime(value)` output (normalized RFC3339 strings) as ordinary
  temporal input and normalize it through Algraf's existing UTC-equivalent
  temporal inference path.
- Treat PDL `date_floor(value, "day")`, `"month"`, and `"year"` outputs as
  temporal when they render as full dates or RFC3339 datetimes.
- Keep partial bucket keys categorical unless Algraf explicitly adds parsing for
  them later. Examples:
  - `date_format(value, "%H")` is an hour-of-day key, not a timeline.
  - `date_format(value, "%G-W%V")` is an ISO week label, not a full date.
  - `date_format(value, "%Y-%m")` is a month label, not a full date.
  - `date_format(value, "%:z")` is timezone-offset metadata.
- Document the recommended PDL pattern for Algraf consumers: emit one full
  temporal coordinate column such as `author_day = date(author_date)` or
  `author_instant = datetime(author_date)`, and emit separate label/grouping
  columns such as `author_month`, `author_week`, or `tz_offset`.
- Do not require cross-repo PDL changes for v0.75. This release only commits
  Algraf to infer and render the full date/datetime strings PDL already emits.

### Scale Semantics and Analysis

Temporal axes should be automatic when the data type is temporal and explicit
when the author asks for it.

- Add `ScaleTypeIr::Temporal` with canonical source spelling `"temporal"`.
- Accept `Scale(axis: x, type: "temporal")` and
  `Scale(axis: y, type: "temporal")`.
- Reject `type: "temporal"` on aesthetic scale targets such as `fill`, `stroke`,
  `size`, `alpha`, `shape`, and `strokeWidth`.
- Reject or diagnose temporal scale declarations on known non-temporal scalar
  columns, using the existing unsupported-scale diagnostic style.
- Preserve existing automatic temporal training when a mapped position column is
  `Temporal` and no overriding scale type is declared.
- Preserve `type: "categorical"` as the explicit opt-out for authors who really
  want date strings or temporal values treated as ordered categories.
- Keep `domain: [date(...), date(...)]` and `domain: [datetime(...),
  datetime(...)]` working on temporal axes.
- Ensure `reverse: true`, `expand:`, exact `breaks:`, and exact `labels:` remain
  valid with temporal axes where already supported.
- Diagnose incompatible combinations clearly, especially `integer: true` on a
  temporal axis and numeric-only domain forms on a temporal-only declaration.

### Scale IR and Tick Interval Model

Temporal tick interval control needs its own IR instead of reusing binning
types directly. Axis ticks and statistical bins have different semantics even
when they use the same calendar words.

- Add an axis tick interval IR model for temporal ticks, carried on the scale
  IR alongside `breaks:`.
- Store independent x and y tick intervals through the existing chart-scope and
  space-scope `Scale` declaration rules (spec §16.11).
- Support these interval units:
  - millisecond
  - second
  - minute
  - hour
  - day
  - week
  - month
  - quarter
  - year
- Support positive integer step counts for each unit:
  - `"day"` and `"1 day"` are equivalent.
  - `"2 weeks"`, `"6 months"`, and `"5 years"` are valid.
  - Singular and plural unit names are valid.
- Reject malformed values with `E1204`:
  - zero counts: `"0 days"`
  - negative counts: `"-1 month"`
  - fractional counts: `"1.5 hours"`
  - unknown units: `"fortnight"` unless explicitly promoted later
  - extra tokens: `"every 2 weeks"`
  - non-string values
- Require `axis: x` or `axis: y` whenever `tickInterval` is present.
- Treat `tickInterval` on a non-temporal axis as a diagnostic, not a silent
  no-op.
- Keep `timeFormat` validation unchanged except where tests reveal gaps.

### Diagnostics and Code Reservations

Reserve new codes in `ALGRAF_SPEC.md` before implementation, per the standard
promotion workflow:

- Malformed `tickInterval` values (zero, negative, or fractional counts,
  unknown units, extra tokens, non-string values) and `tickInterval` without
  `axis: x`/`axis: y` reuse `E1204`, matching `tickLabelAngle` validation.
- `tickInterval` on a non-temporal axis: reserve `E1608`. (`E1601`–`E1604` and
  `E1606`–`E1607` are taken; `E1605` is already reserved for null bounds.)
- `tickInterval` and explicit `breaks:` on the same axis: reserve `E1609` as a
  warning; `breaks:` wins per the precedence rules.
- `type: "temporal"` on aesthetic scales or on known non-temporal columns
  reuses the existing scale-type diagnostic style (`E1204`/`E1602` family);
  settle the exact codes during spec promotion.
- Row-level malformed temporal cells surface through the existing data-warning
  path (plain warnings without source spans), not new E-codes.

### Temporal Tick Generation

Explicit tick intervals must produce deterministic calendar/clock boundaries
inside the trained axis domain.

- Generate fixed-duration ticks for millisecond, second, minute, hour, day, and
  week intervals.
- Generate calendar ticks for month, quarter, and year intervals.
- Month ticks land on the first day of the month at `00:00:00`.
- Quarter ticks land on January 1, April 1, July 1, or October 1 at `00:00:00`.
- Year ticks land on January 1 at `00:00:00`.
- Week ticks use a documented UTC week anchor. Recommendation: Monday
  `00:00:00` UTC-equivalent boundaries.
- Multi-step intervals anchor to fixed unit grids rather than the domain
  start, so the same interval yields the same calendar phase on any domain:
  - month steps count from the January month grid, so `"3 months"` lands on
    Jan/Apr/Jul/Oct (identical to `"1 quarter"`) and `"6 months"` lands on
    Jan/Jul;
  - year steps land on years divisible by the step count;
  - week steps count from the ISO Monday grid;
  - day steps count from the Unix epoch day grid;
  - hour, minute, second, and millisecond steps count from midnight of each
    UTC-equivalent day.
- Ticks outside the trained domain are not emitted.
- Ticks on exact domain boundaries are emitted.
- Generated ticks must remain bounded. If the requested interval would produce
  too many labels for the panel, the renderer should deterministically thin the
  generated tick set rather than emitting unbounded guide output.
- The thinning rule must preserve calendar alignment by promoting the step
  count within the same unit grid — monthly ticks thin to every 2nd, 3rd, 6th,
  or 12th month boundary on the January-anchored grid — rather than keeping
  every Nth generated tick by index (which drifts phase) or switching to
  numeric interpolation.
- Automatic temporal ticks remain available when no explicit interval is
  declared.
- Extend the automatic tick ladder between its monthly and yearly rungs with
  2-, 3-, and 6-month strides, and add a Monday-anchored weekly rung between
  the daily and monthly rungs. Today `temporal_ticks` in
  `crates/algraf-render/src/space/temporal.rs` offers only 1-month strides for
  spans of 45–400 days and then jumps to year strides, so spans of roughly 13
  to 24 months can fall through monthly (too many ticks) and yearly (too few)
  to the equal-spaced numeric fallback, which labels fractional-month
  instants, and multi-year spans get only sparse year ticks.
- After the ladder extension, the numeric-interpolation fallback should be
  unreachable for domains spanning at least two calendar days; keep it only as
  a final safety net.
- Existing interval-center hints from temporal histogram/bin output should
  continue to work when explicit tick intervals are absent.

### Tick Labels and Formatting

Tick labels should be useful by default and controllable when authors need a
specific convention.

- Keep `Guide(axis: ..., timeFormat: "...")` as the canonical formatting API.
- Continue supporting named formats such as `iso-date`, `iso-minute`,
  `iso-second`, `iso-millis`, `rfc3339`, `year`, `month`, `month-day`,
  `time-minute`, and `time-second`.
- Continue supporting validated chrono/strftime-style custom format strings.
- Ensure custom formatting works with automatic ticks, explicit `tickInterval`,
  and explicit temporal `breaks:`.
- Preserve exact user labels from `Scale(labels: [...])` when paired with
  `breaks:`.
- Improve default label choice where needed so date-only columns default to
  date labels and datetime columns default to date/time labels.
- Keep formatting independent of host locale and local timezone.
- Document the supported custom-format subset precisely enough that authors do
  not need to infer behavior from chrono internals.

### Linear Timeseries Rendering

The core chart behavior must prove that time remains continuous even when data
is sparse.

- Ensure all position geometries that already support temporal coordinates map
  them through temporal scales:
  - `Point`
  - `Line`
  - `Path`
  - `Area` and filled area variants
  - `Rect` and interval-derived rectangles
  - annotation marks that accept x/y positions
  - text labels with temporal x/y positions
- Add regression coverage for missing dates:
  - rows at `2024-01-01`, `2024-01-02`, and `2024-01-10`
  - rendered x coordinate gap from Jan 2 to Jan 10 is eight times the gap from
    Jan 1 to Jan 2, within deterministic floating-point tolerance
- Confirm categorical override still collapses to bands only when the author
  explicitly declares `Scale(axis: x, type: "categorical")`.
- Confirm date-only values and datetimes can share a single temporal axis
  without panics or `NaN` output.

### Editor, LSP, and Documentation Support

The language-facing tooling should know the new temporal scale and guide
controls.

- Add registry metadata for `type: "temporal"` and `tickInterval`.
- Add completions for `tickInterval` values on `Scale(axis: ...)`.
- Update hover/help text for temporal scales, `timeFormat`, and `tickInterval`.
- Update semantic tests if registry output changes.
- Update TextMate grammar only if new syntax categories require it. Ordinary
  argument names and string values should not require grammar changes.

### Examples

Add examples that specifically teach temporal behavior rather than merely using
dates incidentally.

- `examples/timeseries_gaps.ag`
  - Demonstrates sparse daily data with a visible multi-day gap.
  - Uses a line and points so the proportional spacing is obvious.
  - Uses `Guide(axis: x, timeFormat: "%b %-d")`.

- `examples/temporal_tick_interval.ag`
  - Demonstrates monthly ticks with `Scale(axis: x, tickInterval: "1 month")`
    plus `Guide(axis: x, timeFormat: "%b %Y")`.
  - Should make monthly or quarterly cadence obvious.

- `examples/temporal_weekly_ticks.ag`
  - Demonstrates weekly ticks on a daily or event series.
  - Confirms documented week anchor behavior.

- `examples/temporal_year_ticks.ag`
  - Demonstrates yearly or multi-year labels across a longer span.

- Update the top-level `README.md` tutorial with a section for each new
  example, placed in the tutorial progression per the repository guidelines,
  and `examples/README.md` where applicable.

## Non-Goals

- Do not add ggplot-style helper declarations such as `scale_x_time()` or
  `scale_y_time()` in v0.75.
- Do not add snake_case aliases such as `date_format` or `tick_interval`.
- Do not add locale-dependent month/day names. Output remains deterministic and
  locale-independent.
- Do not add render-time display timezone controls such as "show this axis in
  America/New_York". Existing parse-time timezone behavior should remain, but
  axis rendering stays UTC-equivalent and deterministic.
- Do not add trading-day, business-day, holiday, or market-calendar scales.
  Friday to Monday remains a three-day gap on a continuous temporal axis.
- Do not add discontinuous temporal axes that remove weekends or missing
  periods.
- Do not add fractional guide intervals such as `"1.5 hours"`.
- Do not make invalid date strings silently parse through ambiguous
  locale-specific conventions such as `01/02/2024`.
- Do not require browser/WASM support for native-only formats beyond existing
  supported behavior.

## Must

- Add explicit temporal position scale declarations.

  Status: Implemented. `ScaleTypeIr::Temporal`, analysis acceptance/rejection,
  IR JSON serialization, and the render-time `R0004` fallback diagnostic for
  known non-temporal columns landed with semantics and render tests.

  `Scale(axis: x, type: "temporal")` and `Scale(axis: y, type: "temporal")`
  must parse, analyze, serialize in IR, and render as continuous temporal axes.
  They must be rejected on aesthetic scales and incompatible known non-temporal
  axes.

- Add `Scale(axis: ..., tickInterval: "...")` for temporal axes.

  Status: Implemented. Interval parsing (E1204), axis-only and
  non-temporal-axis validation (E1608), breaks-conflict warning (E1609),
  grid-anchored generation with step promotion and an `R0004` promotion
  warning, and the byte-equivalence fixture against explicit `breaks:` all
  landed. Explicit temporal `breaks:` are now exact (no index thinning).

  The property must support positive integer millisecond, second, minute, hour,
  day, week, month, quarter, and year intervals. It must be validated during
  analysis and carried through chart-scope and space-scope scale declarations
  the same way `breaks:` is.

- Extend the automatic temporal tick ladder.

  Status: Implemented. Monday-anchored weekly rung, 2/3/6-month strides on the
  epoch month grid, extended year strides through 1000, and
  granularity-adaptive default labels (`%Y` / `%Y-%m` / `%Y-%m-%d`) landed;
  the numeric fallback is unreachable for ordinary calendar domains. Seven
  existing examples re-rendered with improved ticks/labels and were visually
  inspected.

  Add 2-, 3-, and 6-month strides between the monthly and yearly rungs and a
  Monday-anchored weekly rung between the daily and monthly rungs, so charts
  without explicit `tickInterval` get sensible cadence on one-to-several-year
  spans and the equal-spaced numeric fallback becomes unreachable for ordinary
  calendar domains. Regenerate and visually inspect any example fixtures whose
  default ticks change.

- Preserve and harden `Guide(axis: ..., timeFormat: "...")`.

  Status: Implemented. Render tests cover `timeFormat` with automatic ticks,
  `tickInterval`, and explicit temporal `breaks:`.

  `timeFormat` remains the canonical temporal tick-label formatting property.
  It must work with automatic ticks, `tickInterval`, and explicit temporal
  `breaks:`.

- Guarantee linear spacing for temporal position axes.

  Status: Implemented. A render regression test asserts the Jan 2 → Jan 10
  pixel gap is exactly eight times the Jan 1 → Jan 2 gap from emitted SVG
  coordinates; the `timeseries_gaps` example shows the behavior.

  Missing dates in input data must produce proportional spatial gaps. Temporal
  axes must not silently fall back to categorical spacing.

- Harden temporal ingestion around missing and malformed values.

  Status: Implemented. Mostly-temporal columns (at most 10% unparseable
  non-missing cells) now infer Temporal with nulls plus one aggregated
  warning instead of degrading to `Mixed`; blanks never count against
  inference. Covered by data tests and spec §10.3 text.

  Blank/null cells in otherwise temporal columns must remain missing temporal
  cells. Malformed non-missing temporal cells should be diagnosed without
  degrading valid temporal rows where the data path can support that behavior.

- Update `ALGRAF_SPEC.md`.

  Status: Implemented. §16.4 (ladder + adaptive labels), §16.11
  (`type: "temporal"`, `tickInterval`, exact breaks), §10.3 (mostly-temporal
  inference), and the §26 diagnostic catalog (E1608, E1609) are updated; the
  catalog-sync test passes.

  Document explicit temporal scale type, `tickInterval`, precedence with
  `breaks` and `labels`, deterministic calendar boundaries, formatting behavior,
  diagnostics, and non-goals.

- Add examples and README tutorial coverage.

  Status: Implemented. `timeseries_gaps`, `temporal_tick_interval`,
  `temporal_weekly_ticks`, `temporal_year_ticks`, and
  `temporal_subsecond_ticks` landed with CSV fixtures, generate.sh entries,
  rendered SVG/PNG output, and `examples/README.md` tutorial sections placed
  in the temporal progression.

  At minimum, add examples for sparse timeseries gaps and explicit monthly or
  weekly tick intervals, then update `examples/README.md` in the same change.

- Preserve existing temporal behavior and examples.

  Status: Implemented. The full workspace suite passes. One spec-backed
  expectation changed intentionally: month-start ticks now label `%Y-%m`
  (`test_temporal_axis_uses_calendar_month_ticks` updated), and seven
  examples re-rendered with the improved ladder/labels.

  Existing temporal parsing, temporal literals, temporal histogram intervals,
  off-axis `Text(timeFormat: ...)`, temporal domains, and temporal `breaks:`
  must continue to pass their current tests unless a spec-backed update
  intentionally changes output.

## Should

- Add robust render assertions for proportional temporal spacing.

  Status: Implemented. The gap-ratio test parses emitted SVG `cx` positions.

  Tests should inspect emitted SVG positions or renderer data structures rather
  than relying only on image review.

- Add y-axis temporal tick coverage.

  Status: Implemented. A render test exercises `Scale(axis: y, tickInterval: "1 year")`.

  Most examples use time on x, but `Scale(axis: y, tickInterval: ...)` and
  `Scale(axis: y, type: "temporal")` should be tested.

- Add automatic interval fallback tests.

  Status: Implemented. Ladder unit tests cover 18-month, 3-year, two-month, and century spans plus the weekly Monday rung.

  Existing automatic tick generation should remain sensible when no
  `tickInterval` is present.

- Add a `tickInterval`-versus-`breaks` equivalence fixture.

  Status: Implemented. `tick_interval_matches_equivalent_explicit_breaks_byte_for_byte` renders both forms and asserts identical SVG bytes.

  Following the v0.40 equivalence-baseline convention, a chart using
  `Scale(axis: x, tickInterval: "3 months")` must render byte-for-byte
  identical SVG to the same chart with an explicit `breaks:` array listing
  the same calendar instants. The explicit array is the test oracle; the
  interval is the user-facing feature.

- Document dense label strategies.

  Status: Implemented. The `temporal_tick_interval` example combines `tickInterval` with `tickLabelAngle`, and spec §16.11 cross-references guide label thinning.

  The examples and spec should show that authors can combine `tickInterval` with
  `tickLabelAngle` or `tickLabelRows` when labels are dense.

- Add editor completions and hover entries.

  Status: Implemented. `"temporal"` joined `SCALE_TYPE_NAMES`, `tickInterval` has an ArgDoc hover entry and nine common-value completions.

  `type: "temporal"` and common `tickInterval` values should appear in editor
  completions where the existing registry/completion system can provide them.

- Add diagnostics for ambiguous temporal inference where practical.

  Status: Implemented. Mostly-temporal columns warn with offending examples; mixed naive/offset warnings were already present.

  If a column contains competing date formats or mixes parseable values with
  malformed values, users should get actionable diagnostics instead of a
  surprising categorical axis.

## Could

- Support named aliases for common `tickInterval` values in completions only,
  such as suggesting `"1 week"`, `"1 month"`, and `"1 year"`.

  Status: Implemented in editor completions.

  These are not new syntax; they are editor conveniences.

- Add sub-second interval examples if millisecond/second ticks are promoted.

  Status: Implemented. `temporal_subsecond_ticks` shows 500-millisecond ticks with millisecond labels.

  This is useful for telemetry and event streams but less important than day,
  week, month, quarter, and year axes.

- Add a warning when explicit `tickInterval` produces aggressive thinning.

  Status: Implemented. Step promotion past the interval budget emits an `R0004` warning naming the requested and effective cadence.

  This may help authors understand why not every requested tick was rendered,
  but deterministic thinning without a warning is acceptable for v0.75.

- Add a dedicated temporal-axis example using explicit `Scale(axis: x, type:
  "temporal")`.

  Status: Implemented. `timeseries_gaps` declares the explicit temporal scale.

  Automatic temporal training should normally be enough, but the explicit scale
  example is useful for documentation.

## Validation

Per standard workflow, the workspace checks are authoritative:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./examples/generate.sh
git diff -- examples
```

Additional v0.75-specific validation:

- Render the sparse timeseries example and verify that Jan 2 to Jan 10 occupies
  eight times the horizontal distance of Jan 1 to Jan 2.
- Render or unit-test a PDL-style CSV containing `author_day` values emitted as
  `YYYY-MM-DD` strings and confirm Algraf infers a temporal axis rather than a
  categorical axis.
- Render weekly, monthly, quarterly, and yearly interval examples and verify
  that tick labels land on documented calendar boundaries.
- Confirm custom `timeFormat` changes labels without changing tick positions.
- Confirm exact `breaks` plus `labels` still override generated temporal labels.
- Confirm `tickInterval: "3 months"` renders byte-identically to the
  equivalent explicit `breaks:` array on the same domain.
- Render a chart with an 18-month domain and confirm the automatic ladder
  produces calendar-boundary ticks rather than the equal-spaced numeric
  fallback.
- Confirm the same chart renders deterministically across repeated runs.

## Open Questions

1. **Week anchor.** Should week ticks anchor to Monday `00:00:00` UTC-equivalent
   boundaries?

   Recommendation: yes. Monday is ISO-compatible, deterministic, and avoids
   locale-dependent week starts.

2. **Tick overflow limit.** What maximum generated tick count should trigger
   thinning?

   Recommendation: use the existing axis tick budget where possible. If the
   existing renderer uses different budgets by orientation or panel size,
   preserve that behavior and thin calendar ticks before label measurement
   explodes.

3. **Malformed cells in inferred temporal columns.** Should invalid non-missing
   cells become nulls plus warnings, or should the whole column remain string?

   Recommendation: null plus warning for mostly temporal columns when row-level
   diagnostics are available. If a data path cannot report row-level parse
   warnings cleanly, prefer deterministic inference behavior and document the
   limitation.

4. **`"time"` as a scale type alias.** Should `type: "time"` be accepted as an
   alias for `type: "temporal"`?

   Recommendation: no for v0.75. Use one canonical spelling in the DSL and
   documentation.

5. **Sub-second guide intervals.** Should v0.75 support millisecond ticks in
   addition to second and larger units?

   Recommendation: yes if the existing temporal representation and formatter
   path already make this straightforward. If implementation risk rises,
   defer millisecond intervals while keeping second through year intervals.

## Promotion Workflow

This plan introduces and hardens spec-level behavior across data, scales,
guides, rendering, editor services, examples, and docs. When implemented:

1. Update `ALGRAF_SPEC.md` to formally document `type: "temporal"`,
   `tickInterval`, temporal tick precedence, temporal formatting behavior, and
   diagnostics.
2. Update `Status:` lines in this document as each item lands.
3. Add or update examples and regenerate example artifacts.
4. Run the validation commands listed above.
5. Bump release version stamps to `0.75.0` only when this release plan ships,
   following the repository's version-promotion requirements.
6. Begin `V0_76_PLAN.md` for the next release scope.
