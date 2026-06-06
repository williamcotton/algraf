# Algraf Browser Package Development

Algraf v0.67 publishes package-shaped browser integrations for npm consumers.

## Published Package Mode

Use published packages for demo, Studio, and downstream package-surface checks:

1. Install the published browser packages:

   ```bash
   npm install algraf-wasm@0.68.0 algraf-editor@0.68.1
   ```

2. Use the package-local WASM asset or pass an explicit host URL. Vite hosts
   should also import Monaco's editor worker and Onigasm's WASM asset from app
   source and pass them to `algraf-editor` through `setupOptions`.

```ts
import { loadAlgrafRuntime } from "algraf-wasm";

const runtime = await loadAlgrafRuntime({ wasmUrl: "/wasm/algraf.wasm" });
```

The Algraf demo consumes these published package versions.

## Package Validation

Use packed mode for package-surface validation before publishing:

1. From `packages/wasm`, run `npm pack --dry-run`.
2. From `editors/monaco`, run `npm pack --dry-run`.
3. Run the host app's normal type, build, and browser checks against the
   published package versions.

Generated `dist/` contents, local tarballs, and copied WASM artifacts are
ignored source outputs.
