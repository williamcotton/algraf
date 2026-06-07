# Algraf v0.70.0 Plan

Status: Implemented
Target version: 0.70.0
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_69_PLAN.md`](V0_69_PLAN.md)
Neighboring PDL plan: [`V0_40_PLAN.md`](../../pdl/docs/V0_40_PLAN.md)
Roadmap theme: Demo site and README CLI documentation alignment.
Cross-repo coordination: `../pdl/` for the matching cross-tool pipe
documentation under PDL v0.40.

## Purpose

Algraf v0.70 is a documentation/demo-site backport. It surfaces the CLI/native
side of Algraf on the demo homepage and adds a cross-link callout to PDL in
the top-level README so visitors arriving through the browser path can see
what the standalone Rust binary already provides.

No language, parser, analyzer, renderer, LSP, CLI, or WASM behavior is
introduced or changed in this release. The spec's normative text is not
touched; only its `Status:` line tracks the workspace version.

## Must

- Add a "On the command line" section to the demo homepage.

  Status: Implemented.

  `demo/src/pages/HomePage.tsx` gains a new `cli-section` between the
  existing `language-highlights` feature cards and the `home-band`. The
  section includes render-target chips (`--output .svg`, `--output .png`,
  `--format svg+json`, `--format draw-list`, `--interactive`), subcommand
  chips (`render`, `check`, `format`, `schema`, `ast`, `ir`, `lsp`),
  accepted data-format chips (CSV, TSV, JSON, NDJSON, GeoJSON, Parquet,
  Arrow IPC stream, SQLite), and a cross-tool pipe snippet linking to PDL.
  The existing hero, both install strips, the feature cards, and the
  home band are left untouched.

- Add a PDL companion callout to `README.md`.

  Status: Implemented.

  The existing piped-data block under "`algraf render` accepts `--data -`"
  gains a one-sentence callout naming PDL as the recommended preparation
  companion, with a link to the PDL repository. No other README sections
  are restructured.

- Align demo bash snippets to a single light visual style.

  Status: Implemented.

  `demo/src/styles.css` carries new `.cli-section`, `.cli-chip-row`,
  `.cli-chip`, `.cli-subgroup`, and `.cli-snippet` rules. `.cli-snippet`
  matches the canonical `.install-strip pre` token set (`padding: 14px`,
  `border: 1px solid #e0e6ea`, `background: #fbfbf9`, `color: #1d2a30`,
  `font-size: 0.86rem`, `line-height: 1.55`) and a `.cli-snippet code`
  reset prevents the inline pill style from leaking into the snippet block.

- Bump release version stamps to 0.70.0.

  Status: Implemented.

  Updates: workspace `Cargo.toml`, `Cargo.lock` workspace member entries,
  `docs/ALGRAF_SPEC.md` `Status:` line, `editors/vscode/package.json`,
  `editors/vscode/package-lock.json`, `demo/package.json`, and
  `demo/package-lock.json`.

- Drop the README pointer to `docs/ALGRAF_SPEC.md`.

  Status: Implemented.

  The README opening stanza no longer asks readers to navigate to the
  normative spec. The "visual gallery lives in `examples/README.md`" line
  stays.

## Should

- Keep `algraf-wasm` and `algraf-editor` consumer pins unchanged in this
  release.

  Status: Implemented.

  No published browser packages are cut for v0.70.0. `demo/package.json`
  continues to depend on the previously published `algraf-wasm` and
  `algraf-editor` versions. Downstream consumers (including Datafarm Studio)
  should likewise keep their `algraf-wasm` / `algraf-editor` pins on the
  last-published versions until a browser-package release is cut.

- Leave normative spec text alone.

  Status: Implemented.

  `docs/ALGRAF_SPEC.md` gets only a `Status:` bump; no `MUST`/`SHOULD`/`MAY`
  text changes, no new diagnostic codes, and no new geometries, themes,
  scales, guides, render targets, or CLI flags are documented because none
  are added in this release.

## Validation

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
cd demo && npm install && npm run check && npm run build
```

The Rust workspace has no source changes in this release, so the existing
test suite remains the authority for runtime behavior. The demo build verifies
the new TSX section type-checks and bundles.

## Promotion Workflow

This release does not promote any deferred features. The render targets,
subcommands, data formats, and `--data -` streaming surface mentioned in the
new demo section all already exist as `MUST`/`SHOULD` items in
`docs/ALGRAF_SPEC.md`; the documentation simply makes them more visible.
