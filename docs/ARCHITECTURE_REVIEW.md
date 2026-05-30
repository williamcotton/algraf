# Algraf Architecture Review

Status: Review of `v0.34.0` (`021f324`)
Date: 2026-05-30
Scope: full Rust workspace (`crates/`), the spec/plan/code contract, and the
three runtime consumers (CLI, LSP, WASM).
Method: source read of every crate plus `docs/ALGRAF_SPEC.md` §23–24.

> This is a point-in-time architectural assessment, not a normative document.
> Nothing here changes behavior; where it recommends a change, that change must
> still go through the spec/plan workflow described in `CLAUDE.md` before it is
> real. Line numbers reference `v0.34.0` and will drift.

---

## 1. Executive summary

Algraf is a mature, unusually disciplined codebase for its age. The workspace is
nine crates (~40k LOC of `src`, ~9k LOC of tests, 119 example `.ag` files, a
9,845-line normative spec) arranged as a strict downward dependency chain from
`algraf-core` to `algraf-cli`. The single most important architectural property
— **one analysis/render engine shared by CLI, LSP, and the browser** (spec §0,
§24.3) — is genuinely upheld in the code, not just aspirational. The `driver`
crate is the seam that makes this work, and the `wasm` crate (365 LOC) proves it
by running the exact native pipeline against an in-memory IO backend.

The design's load-bearing ideas are all sound and well-executed:

- A **lossless rowan CST** with a resilient, non-panicking parser.
- **Diagnostics as values** with a stable, test-enforced code registry.
- A **pure analyzer** producing a high-level `ChartIr` decoupled from both syntax
  and rendering.
- A **Planning/Emission boundary** in the renderer with a single `MarkSink`
  abstraction feeding three backends (SVG, draw-list, raster) that agree by
  construction.
- A **`Table` trait boundary** that keeps the concrete dataframe out of every
  upstream crate (spec §10.5).

The risks are not correctness risks — they are **scaling-of-maintenance** risks
concentrated in a few large modules, and a handful of **hand-maintained
registries** that lack compile-time consistency guarantees. The renderer
(`algraf-render`, 15.3k LOC, ~38% of the workspace) is where complexity is
accumulating fastest, with `stats.rs` (1,706 LOC) and `space.rs` (1,180 LOC) as
the two clearest god-file candidates.

Overall grade: **strong**. The architecture will carry the roadmap without a
rewrite; the work ahead is disciplined decomposition of a few hotspots and
automating the registries before they drift.

---

## 2. Workspace shape

| Crate | `src` LOC | Role | Notes |
| --- | ---: | --- | --- |
| `algraf-core` | 665 | `Span`, `Diagnostic`, `Severity` | 124 registered diagnostic codes |
| `algraf-syntax` | 3,770 | lexer, parser, AST/CST, formatter | logos + rowan; hand-written RD + Pratt |
| `algraf-data` | 3,009 | CSV/JSON/GeoJSON/TopoJSON/Shapefile/SQLite, inference | `Table` trait boundary |
| `algraf-semantics` | 7,699 | name resolution, validation, IR, registry | pure `analyze` |
| `algraf-driver` | 4,158 | IO, resolution, loading, caching, prepare | the shared seam |
| `algraf-render` | 15,346 | scales, layout, stats, geom, SVG/raster | largest crate by far |
| `algraf-lsp` | 3,851 | tower-lsp backend + features | reuses driver/semantics |
| `algraf-cli` | 1,694 | the `algraf` binary | command dispatch + I/O |
| `algraf-wasm` | 365 | browser runtime | in-memory `DriverIo` |

Dependency direction flows strictly downward (`core` → … → `cli`), with
`driver` depending on syntax/data/semantics and `cli`/`wasm`/`lsp` as leaf
consumers. No cycles. The `wasm` crate is new since the CLAUDE.md crate table was
written (the table lists eight crates; there are nine) — a minor doc-drift worth
fixing.

The supporting documentation is a real asset: 40 versioned plan files
(`V0_2`–`V0_41`), plus `CACHE_POLICY.md`, `PERFORMANCE_BASELINE.md`,
`WASM_AUDIT.md`, `WEBGL_FEASIBILITY.md`, and `GANTT_SPACE_ALGEBRA.md`. The
spec↔plan↔code contract is the governing discipline of the project and it is
visibly followed.

---

## 3. Layer-by-layer assessment

### 3.1 `algraf-core` — foundation (665 LOC)

Minimal and correct. `Span` (`span.rs`) is a half-open byte range with `cover`,
`contains`, `is_empty`; zero-length spans for recovery nodes are first-class.
`Diagnostic` (`diagnostic.rs:353`) is a value carrying a stable `code`,
`Severity`, message, primary span, optional related spans, and optional help,
with fluent builders (`error`, `warning`, `with_help`, `with_related`).

**The diagnostic registry is the standout.** 124 codes
(`diagnostic.rs:77`–`330`) across E0xxx (syntax), E1xxx (semantic), W2xxx
(warnings), H3xxx (hints), R0xxx (refactors). Three tests enforce discipline that
most projects never reach (`diagnostic.rs:419`–`476`):

1. `registered_codes_are_unique_and_well_formed` — shape + uniqueness.
2. `spec_diagnostic_catalog_is_registered` — bidirectional sync between
   `all_codes()` and spec §26 (parsed from the markdown). The code catalog and
   the normative spec **cannot** drift apart without a red test.
3. `production_sources_use_registered_constants` — scans every `crates/**/src`
   file and fails if any raw `"E1234"`-shaped string literal exists outside the
   registry. Codes must be constants.

This is exactly the right way to keep a growing diagnostic surface honest.

**Weakness — manual triple-entry.** Adding a code requires editing the `codes`
module constant, the `all_codes()` array, and spec §26. The tests catch a miss,
but the friction is real and grows linearly. A declarative
`register_codes! { E0001 "msg", ... }` macro generating both the constants and
the slice (and ideally a `code → message` map) would remove the busywork and the
class of copy-paste bugs entirely. Low effort, compounding payoff.

### 3.2 `algraf-syntax` — parser & CST (3,770 LOC)

A hand-written recursive-descent parser with a Pratt sub-parser for the algebra,
running over a `logos` lexer and emitting a **lossless rowan CST**. Trivia
(whitespace, comments) are tokens fed into the green tree, which is what makes the
formatter (`format.rs`) able to round-trip source verbatim and the LSP able to
offer semantic tokens over the same tree.

Strengths:

- **Resilience is real** (spec §12.1, §27.4). The parser never panics; it
  recovers via synchronization points (`recover_item`, `recover_arg`), inserts
  zero-width ERROR nodes for missing primaries, and even does edit-distance
  keyword-typo recovery (`Cahrt` → `Chart`). `tests/resilience.rs` exercises
  this.
- **Algebra precedence** is a clean Pratt parser: `/` (nest) > `*` (cross) > `+`
  (blend), left-associative via right-bp > left-bp.
- **Source-constructor registry** (`source.rs`, `SOURCE_CONSTRUCTORS`) is a
  single static source of truth for `GeoJson`/`TopoJson`/`Sqlite`/`Shapefile`
  names, their path-argument rules, doc text, and LSP completion snippets. Adding
  a constructor is one reviewable entry.

Weaknesses:

- **`parser.rs` is 1,188 LOC** mixing token-cursor logic, tree building,
  block/declaration/value/algebra parsing, *and* post-parse validation
  (`validate_source_header`, gated-constructor checks). Recovery strategy is
  scattered across ad-hoc functions rather than a recovery table. This is still
  navigable today but is the first crate-internal file that would benefit from
  splitting (`token_cursor` / `block_parser` / `value_parser` / `algebra_parser`
  / `validator`). The test suite is already partitioned this way
  (`algebra_parser.rs`, `block_parser.rs`, `lexer.rs`, `resilience.rs`,
  `formatter.rs`), so the seams are known.

### 3.3 `algraf-data` — data access (3,009 LOC)

The `Table` trait (`frame.rs`) is a three-method read-only boundary
(`schema`, `row_count`, `value`) and is the **only** data surface exposed
upward. `DataFrame` (columnar, `Column` enum of `Vec<Option<T>>`) is the private
implementation. Spec §10.5's "concrete dataframe must not leak" is upheld — no
upstream crate touches `Column` or the column index. A future Polars/DuckDB
backend could implement `Table` without disturbing semantics, render, or LSP.

Type inference (`infer.rs`) is one canonical pipeline (bool → int → float →
temporal → string) shared by every format loader, so CSV/JSON/NDJSON all agree on
how a column is typed. Temporal values are normalized to a UTC-equivalent
`DateTimeValue` with explicit precision, which is what lets temporal scales sort
and format deterministically across time zones.

The **`sql` feature flag** is handled cleanly: `lib.rs` swaps `sqlite.rs` (real
FFI, 545 LOC) for `sqlite_stub.rs` (33 LOC returning "unavailable") via
`#[cfg]` + `#[path]`, so WASM builds never link `libsqlite3-sys`. SQLite access
is constrained to `SELECT`/`WITH` with a mandatory top-level `ORDER BY` for
determinism (`sqlite.rs`).

Weaknesses / risks:

- **No compile-time parity between `sqlite.rs` and `sqlite_stub.rs`.** If the
  real module grows a function, the stub must be hand-updated or WASM breaks. A
  shared trait would guarantee parity, though it is arguably over-engineering for
  a single rarely-changing module — at minimum a comment cross-linking the two is
  warranted.
- **Silent `Mixed → String` fallback.** A mostly-numeric column with one stray
  string silently becomes categorical. The escape hatch (explicit `Parse(...)`)
  exists, but the inference outcome is invisible unless the user inspects the
  schema. LSP hover surfacing inferred type + sample values would mitigate.
- **Geometry loaders trust their input** — no NaN/Inf coordinate or
  winding/validity checks in `geojson.rs`/`topojson.rs`/`shapefile.rs`. Malformed
  geometry flows straight into spatial scale training.
- **Ambiguous date formats** under inference default to ISO-8601-first, which is
  the safe choice; `01/02/2020`-style ambiguity is resolved only by explicit
  `Parse`.

### 3.4 `algraf-semantics` — analyzer & IR (7,699 LOC)

`analyze` is **pure**: (parsed tree + schema) → (`ChartIr` + diagnostics), no I/O
or global state (`lib.rs`). The analyzer is split by concern, not by phase
— `frames.rs`, `scales.rs`, `stats.rs`, `properties.rs`, `guides.rs`,
`themes.rs`, `tables.rs`, `lowering.rs`, threaded through an `Analyzer` context
(`analyzer/context.rs`) that carries the primary schema, named-table schemas,
incrementally-built derived schemas, and the two-level `let` scope (chart vs.
space, with space bindings cleared after each space — spec §9.6). The split maps
cleanly onto spec sections and is easy to navigate.

`ChartIr` (`ir.rs`, 951 LOC) is a high-level, declarative representation: it says
*what* to draw, not *how*. Frames are a recursive enum
(`Vector`/`Cartesian`/`Nested`/`Union`/`Invalid`) that directly encodes the
algebra; aesthetic mappings carry fully-resolved `ColumnRef`s (name + dtype +
span); error recovery uses sentinels (`FrameIr::Invalid`,
`DataSourceIr::Missing`, `dtype: Unknown`) so a broken program still yields a
partial IR and *all* diagnostics in one pass.

`PropertyKey` (`ir.rs`) is a single enum whose `as_str()` is the one canonical
spelling shared by the registry, diagnostics, and debug JSON — the renderer
matches on variants, not strings. Stat **output schemas are computed without
executing the stat** (`planning.rs`), which is what lets the analyzer (and the
LSP) know the columns a `Derive`/`Histogram` will produce before any data is
loaded, and lets a topological sort detect derive-dependency cycles
(`stats.rs`, `E1501`).

Weaknesses / risks:

- **`lowering.rs` (1,039 LOC) is the complexity hotspot** and is duplicative.
  Histogram desugaring has three near-parallel implementations
  (`desugar_histogram`, `blended_histogram`, `grouped_histogram`), each
  hand-rolling the same "make a `Bin` `DeriveIr`, synthesize `bin_start`/`count`,
  build a `Rect`" pattern, repeated again across freq-poly / bin2d / density. A
  shared builder would cut this materially.
- **Registry boilerplate is hand-maintained in ~7 places** (the `GeometryKind`
  enum, its `display_name`/`css_class`, the `PropertyKey` enum + `as_str` +
  `PROPERTY_KEYS` array, and the `registry.rs` prop specs). Names are matched by
  string equality at runtime with **no compile-time check** that
  `display_name()` agrees with the registry array. This is the most likely place
  for a silent inconsistency as geometries are added. A derive/proc-macro over
  `PropertyKey`/`GeometryKind` would make the registry the single source.
- **Property validation is scattered** across `properties.rs` rather than
  centralized against the registry's `Accept` specs — the registry says what is
  valid, but the checking logic is partly re-implemented.
- **Schema asymmetry in the IR**: user `Table` declarations carry only
  name+path (schema loaded later, outside the analyzer), while derived tables
  embed their full schema. The renderer must therefore fetch the two kinds of
  schema differently, and the LSP can only offer table-column completion if it
  resolves schemas itself (which it does, via the driver cache).

### 3.5 `algraf-render` — the renderer (15,346 LOC)

The crate is organized around the **Planning/Emission boundary** documented at
`lib.rs:7`–`26` (spec §24.6), and the boundary holds in the code:

- **Planning** consumes IR + data through `Table` only, and resolves a complete
  scene — derived tables, trained scales (`scale.rs`, `domains.rs`, `space.rs`),
  layout rectangles (`layout.rs`), stats (`stats.rs`), guide measurements, and
  legends. It writes no bytes.
- **Emission** serializes that scene through one `MarkSink` trait
  (`sink.rs`) implemented by an SVG sink and a draw-list sink; a raster backend
  replays the draw list via `tiny-skia`. Geometry (`geom/`) and guide (`guide/`)
  emission describe primitives to the sink and make no scale/layout decisions.

This is the crate's best property: because all three backends observe identical
sink calls, **they agree on coordinates and colors by construction** — there is
no second code path to keep in sync. Domain hints (`domains.rs`) are an equally
clean sub-pattern: geometries declare the domain they need (e.g. "bars include
zero") without ever touching a scale.

Other strengths:

- `AxisScale` polymorphism (`space.rs`) hides continuous/temporal/band/nested
  scales behind `resolve_x/y` so every geometry is scale-agnostic.
- Polar layout iteratively shrinks the plot radius to fit perimeter labels while
  keeping a true circle (`space.rs`, `build_polar`).
- Interaction metadata (`render/metadata.rs`) is **inert data** built from the
  planned scene and serialized to deterministic JSON — the sidecar that drives
  browser tooltips/highlights without any runtime logic in the SVG.
- Determinism (spec §18.12) is respected in practice: stable layer ordering,
  locale-independent float formatting (`svg.rs` `num`), and stat outputs sorted
  via `f64::total_cmp` / `BTreeMap`.

Weaknesses / risks — this is where the workspace's debt concentrates:

- **`stats.rs` (1,706 LOC) is a god-module.** It holds 1D/2D/hex binning,
  temporal/calendar binning, count aggregation, Gaussian KDE, LOESS (~300 LOC of
  dense numeric code), and boxplot quantiles, with no sub-module structure. It is
  the single largest file in the workspace and the highest-value refactor target:
  `stats/bin/`, `stats/density/`, `stats/smooth/`, `stats/util/`.
- **Determinism is enforced by convention, not type.** Each stat must remember to
  sort (`stats.rs` uses inline `sort_by(f64::total_cmp)` in some places, a
  `BTreeMap` in others). A contributor adding a stat can silently break
  determinism, and the tests would not catch it — there is exactly one
  determinism test (LOESS) and none for bin/density/hexbin. Adding "same input →
  identical output twice" tests per stat is cheap insurance for a spec-mandated
  property.
- **`space.rs` (1,180 LOC)** bundles axis training, temporal tick/format logic,
  polar math, nested-band algebra, and perimeter-label estimation. Extracting
  `space/polar.rs` and `space/temporal.rs` would leave a focused axis-training
  core. Adding a new `AxisScale` variant today means touching ~10 match arms.
- **Geometry dispatch is one big match** in `geom/mod.rs` with a per-kind module
  and a shared 414-LOC `geom/common.rs` helper bag. For a *closed* geometry set
  (spec §24.6 forbids plugins) a match is defensible, but polar variants are
  re-implemented independently in `bar.rs`, `line.rs`, and `rect_tile.rs` rather
  than shared, and `common.rs` is itself becoming a grab-bag. As geometry count
  grows (already 18 kinds), a `Geometry` trait or shared polar helpers would
  reduce friction.

### 3.6 `algraf-driver` — the shared seam (4,158 LOC)

The driver is the architectural keystone and it is excellent. It is explicitly
non-UI (no arg parsing, no printing, no LSP, no PNG) and exposes parse,
resolution, loading, caching, and a one-call `prepare_chart` orchestration that
returns a `PreparedChart` containing data + analysis + IR. **All three consumers
go through it with zero duplication of core logic:**

- CLI: `prepare_chart` (one-shot).
- LSP: `prepare_chart` inside `spawn_blocking` + a per-backend
  `InMemorySchemaCache` for incremental schema resolution.
- WASM: `prepare_chart_with_io` against a `MemoryIo`.

The `DriverIo` trait (`io.rs`) is minimal and correctly scoped — local bytes and
metadata only, no env/process/network — which is exactly what makes the WASM
in-memory backend and the test in-memory IO trivial to write. `OsDriverIo` is
~25 LOC.

The **schema cache** (`cache.rs`, see `CACHE_POLICY.md`) is sound and
conservative: keyed on normalized path + explicit format + SQL query;
invalidated by a `SourceFingerprint` (len + mtime, content hash reserved but
unused); a cache hit requires *both* fingerprints present and equal, so it never
serves stale data on missing metadata. Errors are cached as `(code, message)` to
avoid re-thrashing on recurring failures. The behavior is covered by targeted
tests (reuse-unchanged, reload-on-change, never-serve-stale, distinct error
kinds).

Weaknesses / risks:

- **Sync/async duplication** (`loading.rs`): every loader exists twice
  (`load_data` / `load_data_with_async_io`, etc.), ~200 LOC of parallel surface.
  This is a historical artifact of "no `async fn` in traits"; notably **no caller
  currently uses the async path** (the LSP uses `spawn_blocking` over the sync
  path). This is the cleanest candidate for either consolidation (modern Rust
  supports `async fn` in traits) or, more pragmatically, **deletion of the unused
  async surface** until something needs it.
- **Policy-threaded parameters**: loaders take a `TemporalParsePolicy` through
  5–6 positional params because `prepare_chart` applies `Parse(...)` declarations
  during load. The coupling is semantically justified but argues for a small
  `DataLoadingContext` struct.

### 3.7 `algraf-lsp` — editor intelligence (3,851 LOC)

The LSP faithfully reuses the driver and semantics rather than re-implementing
the language: it parses (syntax), resolves schemas through the driver cache, and
runs `analyze_with_tables` (semantics), then serves completion/hover/navigation
from the cached parse+analysis in `DocumentState`. This is the spec §0 promise
("LSP and CLI diagnostics MUST derive from the same analysis engine") delivered.

Weaknesses:

- **`backend.rs` couples document management to analysis.** `upsert_document`
  parses, resolves schema, analyzes, inserts into the `DashMap`, and publishes
  diagnostics in one method, so there is no hook to skip analysis on a
  whitespace-only edit or to cache by content hash. Splitting `analyze` from
  `update_document` would enable incremental skipping and isolated testing.
- **Fallback-schema retention** (hold the previous schema if a reload fails)
  couples `DocumentState` to both current and prior state — pragmatic for UX,
  fragile on rename/delete.
- **First-chart-only table resolution**: named-table schemas are resolved for the
  document's first chart only, so completion/hover for a second chart's tables
  silently won't resolve. Likely intentional, but undocumented and untested.

### 3.8 `algraf-cli` — the binary (1,694 LOC)

A clean clap-based command enum (`render`/`check`/`format`/`schema`/`ast`/`ir`/
`lsp`) over `main.rs`. The dispatch itself is sound; the size lives in
`render_cmd` and the 108-LOC `prepare_render_inputs`, which is mostly legitimate
orchestration: driver call, six-way driver-error handling, diagnostic filtering
(strict mode), CLI overrides (width/height/theme), and three output backends
(SVG/draw-list/raster). It is not architecturally wrong, but it is monolithic;
extracting a `write_outputs` helper and a `RenderBackend`-style abstraction for
the three output forms would improve testability.

### 3.9 `algraf-wasm` — browser runtime (365 LOC)

Exemplary. `MemoryIo` (45 LOC) implements `DriverIo` over a host-supplied
`name → bytes` map, matching on the path's file-name component (correct: the
browser has no filesystem hierarchy, but the driver's relative-path resolution
still runs). `render_to_svg` is parse → `prepare_chart_with_io` → render — the
*same* path the CLI uses — so SVG is byte-identical to native for any chart not
needing an excluded capability (SQLite/shapefile/PNG/on-the-fly projection),
which are cleanly excluded via the `sql`-off feature wiring and capability
checks. A test asserts in-memory rendering produces real SVG. This 365-LOC crate
is the proof that the driver abstraction actually pays off.

---

## 4. Cross-cutting observations

**Determinism** is treated as a first-class requirement and is largely achieved,
but its *enforcement* is uneven: ironclad in emission (ordering, float
formatting), convention-based in stats. The asymmetry between a fully
test-enforced diagnostic registry and an almost-untested stat-determinism surface
is the clearest inconsistency in the project's otherwise high QA bar.

**Hand-maintained registries** are a recurring theme — diagnostics (3 sites),
the property/geometry vocabulary (~7 sites), and the sqlite stub/real pair. Each
is currently correct and mostly test-guarded, but each is a manual consistency
obligation. The project has the test infrastructure to make these safe; it would
benefit from making more of them *generated* rather than *checked*.

**File-size distribution** is the leading indicator of where to invest:
`stats.rs` (1,706), `main.rs` (1,398), `driver/lib.rs` (1,284), `space.rs`
(1,180), `lowering.rs` (1,039), `parser.rs` (1,188), `ir.rs` (951). None are
broken; all are above the point where one more feature makes them harder to
reason about.

**Spec/code drift** is low and self-policing (the diagnostic catalog test is the
best example), but two small drifts exist: the CLAUDE.md crate table predates
`algraf-wasm`, and the spec status line still reads "Draft 0.14.0" while the
workspace is at 0.34.0.

---

## 5. Prioritized recommendations

These are suggestions for future plan files, not committed work. Each must pass
through the spec/plan workflow. They have since been captured as concrete work
items in [`V0_35_PLAN.md`](V0_35_PLAN.md), an internal architecture-hardening
release sequenced before the ggplot2 feature roadmap (now v0.36–v0.42).

**Tier 1 — low effort, compounding payoff**

1. Add per-stat determinism tests (bin / density / hexbin / temporal-bin) so the
   spec §18.12 guarantee is enforced, not assumed (`render/stats.rs`).
2. Generate the diagnostic registry from one declarative source
   (`register_codes!`), collapsing the constant/array/`all_codes` triple-entry
   (`core/diagnostic.rs`).
3. Fix the two doc drifts: add `algraf-wasm` to the CLAUDE.md crate table; bump
   the spec status line.

**Tier 2 — medium effort, real structural relief**

4. Decompose `render/stats.rs` into `stats/{bin,density,smooth,util}` and encode
   the "sorted output" determinism contract at the module boundary.
5. Factor histogram/freq-poly/bin2d desugaring in `semantics/lowering.rs` behind
   a shared builder to remove the triplicated `Bin → Rect` pattern.
6. Split `syntax/parser.rs` along the lines the test suite already implies
   (cursor / block / value / algebra / validator).
7. Generate the `PropertyKey`/`GeometryKind` ↔ registry mapping (proc-macro or
   `build.rs`) so name agreement is a compile error, not a runtime hope.

**Tier 3 — opportunistic**

8. Extract `space/{polar,temporal}` from `render/space.rs`; share polar geometry
   helpers instead of re-implementing per geometry.
9. In the driver, either consolidate or delete the unused async loading surface;
   bundle the temporal-policy params into a context struct.
10. In the LSP, split `analyze` from `update_document` to enable incremental
    skipping; document (or remove) the first-chart-only table limitation.

---

## 6. Verdict

Algraf is architected the way a long-lived language toolchain should be: a strict
crate hierarchy, a single shared engine behind every consumer, diagnostics and
data behind stable value/trait boundaries, and a spec that the code is
*mechanically* kept honest against. The decisions that are hard to reverse — the
CST model, the pure analyzer + IR, the Planning/Emission split, the `DriverIo`
seam — are all the right ones and are implemented faithfully.

The remaining work is the pleasant kind: decomposing a few files that success has
made large, and converting a handful of hand-checked registries into generated
ones before they have a chance to drift. There is no architectural cul-de-sac
here and nothing that demands a rewrite. The codebase has earned its momentum.
