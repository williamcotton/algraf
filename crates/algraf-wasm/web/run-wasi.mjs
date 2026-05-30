// Run the algraf-wasm WASI demo command and capture its SVG stdout.
// Usage: node --experimental-wasi-unstable-preview1 run-wasi.mjs <path-to.wasm>
import { readFile } from "node:fs/promises";
import { WASI } from "node:wasi";
import { argv } from "node:process";

const wasmPath = argv[2];
const wasi = new WASI({ version: "preview1", args: [], env: {}, returnOnExit: true });
const bytes = await readFile(wasmPath);
const module = await WebAssembly.compile(bytes);
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
wasi.start(instance);
