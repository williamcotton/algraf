# algraf-wasm

Browser runtime loader and structural TypeScript ABI types for Algraf `0.67.x`.

Published packages expose `dist/index.mjs`, `dist/index.cjs`, and
`dist/index.d.ts`, and package tarballs include `dist/algraf.wasm`. During
local source-mode development, build or copy `algraf.wasm` into the host app's
public assets and pass that URL explicitly:

```ts
import { loadAlgrafRuntime } from "algraf-wasm";

const runtime = await loadAlgrafRuntime({ wasmUrl: "/wasm/algraf.wasm" });
```

`runtime.render(source, files, variables)` accepts an optional third argument
for invocation-time `$name` and `${name}` source-fragment variables.

For package-surface validation, run `npm run build:wasm` and
`npm pack --dry-run`; the generated tarball includes the JavaScript entrypoints,
TypeScript declarations, and `dist/algraf.wasm`.
