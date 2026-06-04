# algraf-wasm

Browser runtime loader and structural TypeScript ABI types for Algraf `0.63.x`.

During local source-mode development, build or copy `algraf.wasm` into the host
app's public assets and pass that URL explicitly:

```ts
import { loadAlgrafRuntime } from "algraf-wasm";

const runtime = await loadAlgrafRuntime({ wasmUrl: "/wasm/algraf.wasm" });
```

For package-surface validation, run `npm run build:wasm` and then
`npm run pack:local`; the generated tarball includes `dist/algraf.wasm`.
