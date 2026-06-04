# Algraf Browser Package Development

Algraf v0.63 adds package-shaped browser integrations without requiring npm
publication during development.

## Source Mode

Use source mode for daily cross-repo work:

1. Build the local WASM artifact from `algraf/` or use the host app's copy
   command.
2. Copy `target/wasm32-unknown-unknown/release/algraf_wasm.wasm` into the
   host's public assets as `wasm/algraf.wasm`.
3. Install or alias the sibling packages with filesystem paths:
   - `algraf-wasm`: `file:../algraf/packages/wasm`
   - `algraf-editor`: `file:../algraf/editors/monaco`
4. Load the runtime with an explicit host URL:

```ts
import { loadAlgrafRuntime } from "algraf-wasm";

const runtime = await loadAlgrafRuntime({ wasmUrl: "/wasm/algraf.wasm" });
```

The Algraf demo and Studio use this mode in this working tree.

## Packed Mode

Use packed mode for package-surface validation before publishing:

1. From `packages/wasm`, run `npm run pack:local`.
2. From `editors/monaco`, run `npm run pack:local`.
3. Install the generated tarballs from `artifacts/` into the demo or Studio
   with `file:` paths.
4. Run the host app's normal type, build, and browser checks.

Generated `dist/` contents, local tarballs, and copied WASM artifacts are
ignored source outputs.
