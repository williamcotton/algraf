# algraf-wasm

Browser runtime loader and structural TypeScript ABI types for Algraf `0.86.x`.

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

`runtime.ast(source, variables)` parses source without loading data or rendering
and returns the same lossless parse-tree JSON shape as `algraf ast --json`:

```ts
const parsed = runtime.ast(source);
console.log(parsed.ast?.node);
```

`runtime.languageReference()` returns the Algraf language-reference Markdown
embedded in the WASM binary, suitable for downstream tool or LLM context:

```ts
const reference = runtime.languageReference();
console.log(reference.version, reference.markdown);
```

For package-surface validation, run `npm run build:wasm` and
`npm pack --dry-run`; the generated tarball includes the JavaScript entrypoints,
TypeScript declarations, and `dist/algraf.wasm`.
