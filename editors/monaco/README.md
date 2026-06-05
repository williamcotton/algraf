# algraf-editor

Reusable Monaco and React editor integration for Algraf `0.67.x` browser hosts.

The package owns editor wiring only: language registration, TextMate grammar
setup, the default light theme, marker conversion, Monaco providers, structural
editor-service runtime types, and a thin `<AlgrafEditor />` component. Hosts
keep runtime loading, render buttons, preview panels, routing, and application
state.

Published packages expose `dist/index.mjs`, `dist/index.cjs`, and
`dist/index.d.ts`, while static TextMate and language-configuration assets stay
available through package subpath exports. Use source mode during local
cross-repo development:

```ts
import { AlgrafEditor } from "algraf-editor";
import { loadAlgrafRuntime } from "algraf-wasm";

const runtime = await loadAlgrafRuntime({ wasmUrl: "/wasm/algraf.wasm" });
```

Use packed mode before publishing by running `npm pack --dry-run` in
`packages/wasm` and `editors/monaco`, then inspecting the file lists for
`dist/`, declarations, README, package metadata, and editor assets.
