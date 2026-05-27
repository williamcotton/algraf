# Algraf v0.28.0 Plan

Status: Planned
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_27_PLAN.md`](V0_27_PLAN.md)

## Purpose

This document defines the intended v0.28.0 release shape: making Algraf's
datetime ingestion and datetime label output broad enough for real CSV, JSON,
database, and pipeline data while preserving deterministic rendering.

The audit for this plan confirms that datetime support is currently real but
narrow. Algraf already has temporal data type inference, temporal scales,
calendar binning, and two named axis label formats. It does not yet have broad
automatic parsing, explicit parse declarations for ambiguous or numeric inputs,
custom temporal output formats, or span-aware datetime ticks below date
precision.

As with prior releases, items here are planning guidance. A feature becomes
normative only when the relevant section of [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md) is
updated with concrete `MUST`, `SHOULD`, or `MUST NOT` language. Inclusion is a
commitment to *attempt*; an item ships only when code, tests, docs, and examples
remain synchronized.

## Release Thesis

v0.28.0 is a **temporal I/O ergonomics** release. Earlier releases made temporal
columns usable for line charts, histograms, calendar bins, and simple ISO axis
labels. This release turns temporal parsing and formatting into a dependable
surface instead of a small set of lucky input strings.

The central design is conservative by default and explicit when ambiguity is
unavoidable. Algraf should automatically infer unambiguous ISO/RFC and
English-month temporal strings. It should not silently guess whether
`01/02/2026` is January 2 or February 1. Ambiguous date orders, numeric epochs,
and source-specific conventions need source-level parse declarations.

Output should follow the same rule: built-in names cover common deterministic
formats, and custom `strftime`-style patterns cover project-specific labels.
Rendering must remain independent of the user's locale, local timezone, and
wall clock.

## Current Implementation Audit

The plan/spec/code audit found:

- `crates/algraf-data/src/temporal.rs` currently accepts RFC3339 timestamps with
  offsets, naive `YYYY-MM-DDTHH:MM:SS`, naive `YYYY-MM-DD HH:MM:SS`, naive
  `YYYY-MM-DD HH:MM`, and ISO dates in `YYYY-MM-DD`.
- `crates/algraf-data/src/infer.rs` classifies cells as boolean, integer,
  float, temporal, then string. Numeric epoch-like values therefore infer as
  numeric today, not temporal.
- A column becomes temporal only when every non-missing value parses as one of
  the currently accepted temporal forms. A single non-missing out-of-format date
  forces `Mixed` or `String` unless a later selected scale handles it as a late
  invalid value.
- CSV/TSV loading and JSON/NDJSON loading normalize cells to strings and run the
  same inference pipeline. GeoJSON properties and shapefile dBASE fields also
  route date-like values through string inference.
- Shapefile `Date` fields are converted to `YYYY-MM-DD`, so they already land on
  the current date parser. Shapefile datetime-like variants are currently
  treated as missing.
- `TemporalPrecision` only distinguishes `Date` from `DateTime`; there is no
  preserved precision for seconds, milliseconds, microseconds, or timezone
  offset presence after inference.
- `crates/algraf-semantics/src/ir.rs` currently has only
  `TemporalFormatIr::IsoDate` and `TemporalFormatIr::IsoMinute`.
- `Guide(timeFormat: ...)` currently accepts only `"iso-date"` and
  `"iso-minute"`. Unknown names produce a targeted diagnostic, but applying a
  temporal format to a non-temporal axis is not yet fully validated before
  render.
- `crates/algraf-render/src/space.rs` defaults temporal labels to
  `YYYY-MM-DD` for date-only values and `YYYY-MM-DD HH:MM` for datetime values.
  It does not expose custom format strings.
- Temporal tick planning has date-only daily/monthly helpers, but datetime
  ticks otherwise fall back to six equally spaced instants. That can create
  fractional clock boundaries and duplicate labels once finer output formats are
  added.
- The spec matches the narrow behavior: spec section 10.3 explicitly says values
  outside the listed temporal formats remain strings unless an explicit
  temporal parsing declaration is added in a later version.

## Scope Rules

- Keep existing accepted temporal inputs and output labels backward compatible.
- Automatic inference remains deterministic and conservative.
- Ambiguous localized dates require an explicit parse declaration.
- Numeric epoch values require an explicit parse declaration because numeric
  columns already infer before temporal columns.
- Naive datetimes remain timezone-free UTC-equivalent instants for scale
  mapping. They are not interpreted in the user's local timezone.
- Offset-aware inputs continue to normalize to UTC-equivalent instants.
- Formatting is deterministic across locale and timezone. Built-in English
  month/day labels, if added, are fixed strings, not host locale output.
- Use the existing `chrono` dependency unless a specific feature, such as IANA
  timezone names, justifies a small additional crate.
- Do not add a natural-language date parser.
- Do not add timezone-aware calendar scale arithmetic in v0.28.
- Do not change chart semantics for non-temporal numeric or string columns.

## Capstone Acceptance Target

The capstone is a single chart that mixes common real-world timestamp spellings,
declares the ambiguous parts explicitly, and renders with a custom temporal axis
label:

```ag
Chart(data: "events.csv", width: 820, height: 420) {
    Parse(
        column: started_at,
        as: "datetime",
        formats: [
            "%m/%d/%Y %I:%M %p",
            "%Y-%m-%dT%H:%M:%S%.f%:z",
            "%d %b %Y %H:%M",
        ],
        timezone: "UTC",
    )
    Parse(column: observed_epoch_ms, as: "datetime", unit: "milliseconds")

    Guide(axis: x, label: "Start time", timeFormat: "%b %-d, %Y %H:%M")

    Space(started_at * latency_ms) {
        Line()
        Point()
    }
}
```

Example data:

```csv
started_at,observed_epoch_ms,latency_ms
05/27/2026 2:30 PM,1780065000000,82
2026-05-27T20:45:15.250Z,1780087515250,91
27 May 2026 21:10,1780089000000,77
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

1. **Automatic parsing is broad but unambiguous.** ISO-like, RFC, compact
   year-first, and English-month forms may infer automatically. Numeric epochs,
   two-digit years, and day/month slash forms do not.
2. **Explicit parsing is a chart-body declaration.** The preferred source syntax
   is `Parse(column: ts, as: "datetime", ...)`, with optional `table: name` for a
   named `Table`. The declaration affects data loading, so the driver must
   extract parse policy before semantic analysis.
3. **Custom formatting uses `chrono`/`strftime` directives.** Existing
   `Guide(timeFormat: "iso-minute")` remains valid. A string beginning with `%`
   or containing formatting directives is treated as a custom pattern once
   validated.
4. **Internal storage remains UTC-equivalent microseconds.** v0.28 may parse
   nanosecond inputs, but it must either reject unsupported precision or round or
   truncate deterministically with documented behavior.
5. **Temporal parse policy is data-layer input, not renderer magic.** Once a
   column is loaded, downstream parser, semantics, LSP, stats, scales, and
   renderer continue to see a normal `DataType::Temporal` column.
6. **Formatting validation is semantic.** Unknown named formats, malformed custom
   patterns, and `timeFormat` on non-temporal axes should produce targeted
   diagnostics rather than silent fallback.

## v0.28.0 Must

### 1. Spec-first temporal audit reconciliation

Status: Planned.

Acceptance criteria:

- Update spec section 10.3 with the exact pre-v0.28 accepted temporal forms and
  the v0.28 promoted forms before changing parsing behavior.
- Update spec section 16.4 for temporal precision, UTC-equivalent mapping, and
  any new tick planning requirements.
- Update spec section 19.4 for named and custom temporal output formats.
- Reserve new diagnostics in spec section 26 before emitting them from parser,
  analyzer, driver, data loading, or render paths.
- Preserve compatibility tests for every currently accepted temporal input:
  RFC3339 with offset, `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM:SS`,
  `YYYY-MM-DD HH:MM:SS`, and `YYYY-MM-DD HH:MM`.
- Add a concise test matrix in code or docs listing accepted automatic forms,
  accepted explicit forms, and intentionally rejected ambiguous forms.

### 2. Broader automatic temporal inference

Status: Planned.

Acceptance criteria:

- Refactor `algraf-data::temporal::parse_temporal` into a table-driven parser
  with deterministic priority and tests for each accepted pattern.
- Keep all existing accepted formats and their current UTC/naive semantics.
- Add automatic support for unambiguous ISO-like datetime forms:
  - `YYYY-MM-DDTHH:MM`
  - `YYYY-MM-DDTHH:MM:SS`
  - `YYYY-MM-DDTHH:MM:SS.sss`
  - `YYYY-MM-DD HH:MM`
  - `YYYY-MM-DD HH:MM:SS`
  - `YYYY-MM-DD HH:MM:SS.sss`
  - the same datetime forms with `Z` or `+/-HH:MM` offsets where the syntax is
    RFC3339-compatible.
- Add automatic support for unambiguous date forms:
  - `YYYY-MM-DD`
  - `YYYY/MM/DD`
  - `YYYYMMDD`
- Add automatic support for RFC2822 timestamps such as
  `Wed, 27 May 2026 14:30:00 -0500`.
- Add automatic support for English-month forms that are not day/month
  ambiguous, such as `May 27, 2026`, `27 May 2026`, and those forms with
  24-hour time.
- Continue rejecting ambiguous automatic forms such as `01/02/2026`,
  `02-01-2026`, two-digit years, bare years, bare year-month values, and
  time-only values.
- Invalid dates and times, such as `2026-02-30` or `2026-05-27 25:00`, remain
  strings unless a matching explicit parse declaration is present.
- Missing-token behavior is unchanged.
- Mixed date-only and datetime columns continue to infer as datetime and lift
  date-only values to midnight.
- Columns mixing naive and offset-aware values continue to emit the existing
  warning, with wording updated if the broader parser adds more offset-aware
  input forms.

### 3. Explicit temporal parse declarations

Status: Planned.

Acceptance criteria:

- Add a chart-body declaration for temporal parse policy:

```ag
Parse(column: started_at, as: "datetime", format: "%m/%d/%Y %I:%M %p", timezone: "UTC")
Parse(column: settled_on, as: "date", format: "%d/%m/%Y")
Parse(column: epoch_ms, as: "datetime", unit: "milliseconds")
Parse(table: trades, column: executed_at, as: "datetime", formats: ["%FT%T%:z", "%F %T"])
```

- `column:` is a bare column identifier. `table:` is an optional bare table
  identifier naming a chart-scoped `Table`; absent `table:` means the primary
  chart data source.
- `as:` accepts `"date"` or `"datetime"`.
- `format:` accepts one custom pattern. `formats:` accepts an ordered list of
  patterns. `format:` and `formats:` are mutually exclusive.
- `unit:` accepts `"seconds"`, `"milliseconds"`, `"microseconds"`, and
  `"nanoseconds"` for numeric epoch columns. `unit:` is mutually exclusive with
  `format:` and `formats:`.
- `timezone:` applies only when the selected input pattern produces a naive
  datetime. v0.28 MUST support `"UTC"` and fixed offsets such as `"+02:00"` and
  `"-05:30"`.
- Explicit parse declarations are extracted by the driver before data loading so
  schema inference, semantic analysis, LSP schema previews, and render agree.
- Declared temporal columns stay temporal even when some non-missing cells fail
  to parse. Failed cells become missing and produce one aggregated data warning
  with the column name, failure count, and representative examples.
- If every non-missing value in a declared temporal column fails to parse, emit a
  targeted diagnostic while preserving deterministic downstream behavior.
- Duplicate parse declarations for the same table/column are rejected.
- Unknown table names, unknown column names, malformed format strings, unknown
  units, unknown `as:` values, and invalid `timezone:` values produce targeted
  diagnostics.
- Explicit parse declarations are not allowed for derived tables in v0.28.
- Schema-only loading uses the same parse policy as full loading.
- Tests cover primary data, named `Table` data, CSV/TSV, JSON/NDJSON, GeoJSON
  properties, SQLite query output where available, and shapefile date fields.

### 4. Data loader and cache plumbing for parse policy

Status: Planned.

Acceptance criteria:

- Add a `TemporalParsePolicy` or equivalent type in `algraf-data` that can be
  supplied to every loader that currently funnels cells through type inference.
- Keep the parser, semantics, render, and LSP crates independent from concrete
  dataframe internals; only the driver/data boundary should know how parse
  policies alter loading.
- Include parse policy in any data cache key or source fingerprint that can
  otherwise reuse a stale schema/frame.
- Preserve current behavior when no parse policy is supplied.
- Loading bytes, paths, stdin data, and named tables all use the same parse
  policy representation.
- Aggregated parse warnings are stable in order and wording.
- Tests prove that the same source bytes loaded from path and bytes produce the
  same temporal schema and values when given the same parse policy.

### 5. Named and custom temporal output formats

Status: Planned.

Acceptance criteria:

- Extend the temporal format IR beyond `IsoDate` and `IsoMinute` while keeping
  those names unchanged.
- Add named formats at least for:
  - `iso-date` -> `YYYY-MM-DD`
  - `iso-minute` -> `YYYY-MM-DD HH:MM`
  - `iso-second` -> `YYYY-MM-DD HH:MM:SS`
  - `iso-millis` -> `YYYY-MM-DD HH:MM:SS.sss`
  - `rfc3339` -> UTC `YYYY-MM-DDTHH:MM:SSZ` or a documented fractional variant
  - `year` -> `YYYY`
  - `month` -> `YYYY-MM`
  - `month-day` -> fixed English month/day or numeric `MM-DD`
  - `time-minute` -> `HH:MM`
  - `time-second` -> `HH:MM:SS`
- Accept custom `chrono`/`strftime`-style patterns in `Guide(timeFormat: "...")`,
  for example `"%b %-d, %Y"` and `"%Y-%m-%d %H:%M:%S"`.
- Unknown named formats and malformed custom patterns produce targeted semantic
  diagnostics.
- `Guide(timeFormat: ...)` without `axis:` remains an error.
- `Guide(timeFormat: ...)` on a known non-temporal axis produces a targeted
  diagnostic instead of silently doing nothing.
- Custom output is deterministic across system locale and timezone. Any English
  month or weekday output uses fixed English names.
- Tests cover named formats, custom formats, invalid formats, y-axis temporal
  formatting, and non-temporal misuse.

### 6. Span-aware temporal tick planning

Status: Planned.

Acceptance criteria:

- Replace datetime equal-spacing fallback ticks with a deterministic calendar
  and clock interval ladder.
- The ladder should cover year, quarter, month, week, day, hour, minute, second,
  and millisecond-level ticks where the internal microsecond representation
  supports them.
- Ticks should land on human boundary instants whenever practical: start of
  year, month, week, day, hour, minute, or second.
- Date-only daily/monthly behavior remains compatible unless the new ladder
  deliberately improves labels and tests document the change.
- Default temporal label choice adapts to domain span and tick interval. For
  example, multi-year axes may default to year/month labels, hourly axes to
  date+hour labels, and second-scale axes to time+seconds labels.
- If a requested `timeFormat` would produce duplicate adjacent tick labels, the
  renderer should either add enough context by default or emit a deterministic
  warning when the user requested the duplicate-prone custom format.
- Tick counts remain bounded and deterministic.
- Tests cover sub-minute, hourly, daily, monthly, yearly, and mixed date/datetime
  domains.

### 7. Diagnostics, LSP, and editor metadata

Status: Planned.

Acceptance criteria:

- Reserve and implement diagnostics for:
  - unknown `Parse` argument;
  - duplicate parse declaration;
  - unknown parse target table or column;
  - invalid temporal parse format;
  - invalid temporal output format;
  - invalid epoch unit;
  - invalid timezone;
  - explicit parse with all non-missing values failing;
  - `timeFormat` used on a non-temporal axis.
- LSP completion includes `Parse`, `format`, `formats`, `unit`, `timezone`, and
  new temporal format names where appropriate.
- Hover/help text states that automatic parsing avoids ambiguous date orders and
  points users to `Parse(...)` for localized formats.
- Semantic tokens and the VS Code TextMate grammar are updated if `Parse` or new
  argument names become source keywords.
- Diagnostics use source spans for declaration errors and data-source context
  for row/value parse warnings.

### 8. Examples and README coverage

Status: Planned.

Acceptance criteria:

- Add at least one new example demonstrating automatic broad parsing with
  unambiguous input strings.
- Add at least one new example demonstrating `Parse(...)` for ambiguous
  `MM/DD/YYYY` or `DD/MM/YYYY` data.
- Add at least one new example demonstrating a custom `Guide(timeFormat: "...")`.
- Regenerate SVG/PNG outputs with `./examples/generate.sh`.
- Add README sections for the new examples in the tutorial progression near the
  existing temporal line chart and temporal histogram sections.
- Example data must be small, deterministic, and must not rely on the current
  date, local timezone, or locale.

### 9. Spec, plan, and release hygiene

Status: Planned.

Acceptance criteria:

- Spec updates cover:
  - section 7 chart-body declaration grammar for `Parse(...)`;
  - section 10 data loading, type inference, missing values, explicit parse policy, and
    cache keys;
  - sections 13 and 14 temporal stat behavior if parse policy affects derived tables;
  - section 16 temporal scales, precision, and tick generation;
  - section 19 `Guide(timeFormat: ...)` named and custom formats;
  - section 21 LSP completion/hover changes;
  - section 22 CLI behavior if schema/check/render output surfaces parse warnings;
  - section 26 diagnostics catalog;
  - section 27 tests.
- Workspace `Cargo.toml` and `editors/vscode/package.json` are bumped to
  `0.28.0` when the release branch is ready.
- README, examples, and rendered artifacts stay synchronized.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
  `cargo test --workspace`, and `./examples/generate.sh` pass before marking
  items implemented.

## v0.28.0 Should

### IANA timezone names

Status: Planned.

Support `timezone: "America/Chicago"` and other IANA names if a dependency such
as `chrono-tz` can be added without making timezone-aware scale arithmetic part
of v0.28. IANA zones would apply only when interpreting explicitly declared
naive datetimes.

### Temporal display outside axes

Status: Planned.

Consider a small output-format declaration or geometry argument for temporal
values used as labels, categorical domains, or legends. Candidate surfaces:

```ag
Format(column: started_at, timeFormat: "%H:%M")
Text(label: started_at, timeFormat: "%H:%M")
```

This should ship only if the scope stays narrow and does not confuse scale
labels, guide labels, and text geometry labels.

### Time-only values with an anchor date

Status: Planned.

Consider explicit parsing for time-only columns when the user supplies an anchor
date or pairs the time with another date column. Automatic time-only inference
should remain rejected because a temporal scale needs a date anchor.

### Temporal literals

Status: Planned.

Consider source-level temporal literal syntax for reference marks and explicit
domains, such as:

```ag
VLine(x: datetime("2026-05-27T12:00:00Z"))
Scale(axis: x, domain: [date("2026-01-01"), date("2026-12-31")])
```

Only promote this if parsing, validation, and formatting diagnostics remain
small and testable.

### Parse failure severity controls

Status: Planned.

Consider `onError: "warn" | "error" | "missing"` for explicit parse
declarations if real examples need stricter ETL behavior than the default
aggregated warning.

## Explicitly Deferred Past v0.28.0

- Natural-language dates such as `next Tuesday`, `today`, or `end of month`.
- Silent automatic parsing of ambiguous localized dates such as `01/02/2026`.
- Two-digit year inference.
- Host-locale-dependent month or weekday names.
- Timezone-aware scale spacing or daylight-saving-aware calendar arithmetic.
- A new internal temporal storage type beyond UTC-equivalent microseconds.
- Leap-second semantics beyond what the selected parser library can represent
  deterministically.
- Fiscal calendars, custom week starts beyond the existing Monday week rule, and
  business-day calendars.
- A general data schema language unrelated to temporal parsing.
- User-defined parsing functions or arbitrary code execution during data load.

## Optional-Item Audit

### Promote In v0.28.0 (Must)

- Spec-first temporal audit reconciliation.
- Broader automatic temporal inference for unambiguous formats.
- Explicit `Parse(...)` declarations for ambiguous/local/numeric inputs.
- Data loader and cache plumbing for parse policy.
- Named and custom temporal axis output formats.
- Span-aware temporal tick planning.
- Diagnostics, LSP, and editor metadata.
- Examples, README, and release hygiene.

### Consider If Capacity Allows (Should)

- IANA timezone names.
- Temporal display formatting outside axes.
- Time-only values with an anchor date.
- Temporal literals.
- Parse failure severity controls.

### Keep Deferred

- Natural-language parsing, ambiguous automatic localized parsing, host locale
  formatting, timezone-aware scale arithmetic, new internal temporal storage,
  fiscal/business calendars, and arbitrary user-defined parsing code.

## Promotion Workflow

1. Update spec section 10.3, section 16.4, section 19.4, section 26, and grammar sections with the intended
   temporal parsing/formatting surface and diagnostic reservations.
2. Add parser and AST support for chart-level `Parse(...)` declarations.
3. Add a syntax/driver pre-analysis extractor for parse policies so schema
   loading sees the declarations before semantic analysis.
4. Implement `TemporalParsePolicy` in `algraf-data` and thread it through path,
   bytes, stdin, and schema-only loaders.
5. Expand automatic `parse_temporal` with a table-driven parser and focused
   tests for every accepted and rejected form.
6. Add explicit parse handling for custom patterns, ordered pattern lists,
   numeric epochs, UTC/fixed-offset timezone interpretation, and aggregate parse
   warnings.
7. Extend `TemporalFormatIr`, guide analysis, and renderer formatting for named
   and custom output formats.
8. Replace datetime equal-spaced fallback ticks with a span-aware temporal tick
   ladder.
9. Add LSP completions, hover text, semantic tokens, and VS Code grammar updates
   for new source syntax.
10. Add examples and README sections, regenerate rendered artifacts, and review
    intentional diffs.
11. Run formatter, clippy, workspace tests, and example generation before
    changing item statuses from Planned.
