# Algraf WASM Playground

Root-level Vite + React playground for `algraf.wasm`.

```bash
cd demo
npm install
npm run dev
```

`npm run dev` builds `crates/algraf-wasm` for `wasm32-unknown-unknown`, copies
the generated binary to `public/wasm/algraf.wasm`, then starts Vite. The app
fetches `public/data/penguins.json` and passes that JSON text to the WASM
runtime as an in-memory Algraf data source named `penguins.json`.
