# Algraf v0.68.1 Plan

Status: Implemented
Target version: 0.68.1
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_68_PLAN.md`](V0_68_PLAN.md)

## Purpose

Algraf v0.68.1 is a browser editor package patch. It fixes the published
`algraf-editor` package so Vite-specific Monaco worker and Onigasm asset query
imports are owned by host applications rather than emitted from the package
`dist/` entrypoint.

This patch does not change `.ag` syntax, rendering behavior, editor-service
behavior, `algraf-wasm`, or the browser JSON ABI.

## Scope

### Editor Package Asset Contract

Status: Implemented.

Acceptance criteria:

- `algraf-editor` published `dist/index.mjs` and `dist/index.cjs` do not import
  `monaco-editor` workers with `?worker` or Onigasm WASM with `?url`.
- `setupAlgrafMonaco` and `<AlgrafEditor />` accept host-provided
  `createEditorWorker` and `onigasmWasmUrl` setup options.
- Hosts can still opt out of package worker setup with `configureWorker: false`.
- The first-party Vite demo imports the worker and Onigasm WASM asset from app
  source and passes them through `setupOptions`.

## Validation

- `npm run build` from `editors/monaco`
- Inspect generated `dist/` for absence of `?worker` and `?url` imports.
