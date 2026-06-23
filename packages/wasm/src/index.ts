export interface AlgrafDiagnostic {
  code: string;
  severity: "error" | "warning" | "information" | "hint";
  message: string;
  span: {
    start: number;
    end: number;
  };
  related?: Array<{
    span: {
      start: number;
      end: number;
    };
    message: string;
  }>;
  help?: string;
}

export interface AlgrafRenderResult {
  svg: string | null;
  sidecar: string | null;
  diagnostics: AlgrafDiagnostic[];
  error: string | null;
}

export interface AlgrafAstSpan {
  start: number;
  end: number;
}

export interface AlgrafAstNode {
  node: string;
  span: AlgrafAstSpan;
  children: AlgrafAstChild[];
}

export interface AlgrafAstToken {
  token: string;
  text: string;
  span: AlgrafAstSpan;
}

export type AlgrafAstChild = AlgrafAstNode | AlgrafAstToken;

export interface AlgrafAstResult {
  ast: AlgrafAstNode | null;
  diagnostics: AlgrafDiagnostic[];
  error: string | null;
}

export type AlgrafLanguageReferencePart = "language" | "tooling" | "full";

export interface AlgrafLanguageReferenceSource {
  part: Exclude<AlgrafLanguageReferencePart, "full">;
  path: string;
}

export interface AlgrafLanguageReferenceOptions {
  part?: AlgrafLanguageReferencePart;
}

export interface AlgrafLanguageReference {
  markdown: string;
  version: string;
  part: AlgrafLanguageReferencePart;
  source: string;
  sources: AlgrafLanguageReferenceSource[];
  error?: string | null;
}

export interface LspPosition {
  line: number;
  character: number;
}

export interface LspRange {
  start: LspPosition;
  end: LspPosition;
}

export interface LspDiagnostic {
  range: LspRange;
  severity?: number;
  code?: string | number;
  source?: string;
  message: string;
  relatedInformation?: Array<{
    location: {
      uri: string;
      range: LspRange;
    };
    message: string;
  }>;
}

export type AlgrafEditorFeatureRequest =
  | { kind: "diagnostics" }
  | { kind: "hover"; position: LspPosition }
  | { kind: "completion"; position: LspPosition }
  | { kind: "signatureHelp"; position: LspPosition }
  | { kind: "formatting" }
  | { kind: "rangeFormatting"; range: LspRange }
  | { kind: "semanticTokens" }
  | { kind: "codeActions"; range: LspRange; diagnostics: LspDiagnostic[] }
  | { kind: "definition"; position: LspPosition }
  | { kind: "references"; position: LspPosition; includeDeclaration: boolean }
  | { kind: "documentHighlights"; position: LspPosition }
  | { kind: "prepareRename"; position: LspPosition }
  | { kind: "rename"; position: LspPosition; newName: string }
  | { kind: "documentSymbols" };

export interface AlgrafEditorServiceResult<T = unknown> {
  diagnostics: LspDiagnostic[];
  result: T;
  error: string | null;
}

interface AlgrafWasmExports extends WebAssembly.Exports {
  memory: WebAssembly.Memory;
  algraf_alloc(len: number): number;
  algraf_dealloc(ptr: number, len: number): void;
  algraf_render_json(ptr: number, len: number): bigint;
  algraf_ast_json(ptr: number, len: number): bigint;
  algraf_language_reference_json(): bigint;
  algraf_language_reference_part_json(ptr: number, len: number): bigint;
  algraf_editor_service_json(ptr: number, len: number): bigint;
}

export interface AlgrafRuntime {
  render(source: string, files: Record<string, string>, variables?: Record<string, string>): AlgrafRenderResult;
  ast(source: string, variables?: Record<string, string>): AlgrafAstResult;
  languageReference(options?: AlgrafLanguageReferenceOptions): AlgrafLanguageReference;
  editorService<T = unknown>(
    source: string,
    files: Record<string, string>,
    request: AlgrafEditorFeatureRequest,
    uri?: string,
  ): AlgrafEditorServiceResult<T>;
}

export interface LoadAlgrafRuntimeOptions {
  wasmUrl?: string | URL;
  fetcher?: typeof fetch;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();
declare const __ALGRAF_WASM_MODULE_URL__: string | undefined;
const moduleBaseUrl = typeof __ALGRAF_WASM_MODULE_URL__ === "string" ? __ALGRAF_WASM_MODULE_URL__ : undefined;

export function defaultAlgrafWasmUrl(baseUrl: string | URL | undefined = moduleBaseUrl): string {
  if (baseUrl === undefined) {
    throw new Error("algraf-wasm could not infer a package-local WASM URL; pass loadAlgrafRuntime({ wasmUrl }) explicitly.");
  }
  return new URL("../dist/algraf.wasm", baseUrl).toString();
}

export async function loadAlgrafRuntime(options: LoadAlgrafRuntimeOptions | string | URL = {}): Promise<AlgrafRuntime> {
  const resolvedOptions = normalizeLoadOptions(options);
  const wasmUrl = resolvedOptions.wasmUrl ?? defaultAlgrafWasmUrl();
  const fetcher = resolvedOptions.fetcher ?? fetch;
  const response = await fetcher(wasmUrl);
  if (!response.ok) {
    throw new Error(`failed to fetch ${wasmUrl.toString()}: ${response.status}`);
  }

  const instance = await instantiateWasm(response);
  const exports = instance.exports as AlgrafWasmExports;
  assertAlgrafExports(exports);

  return {
    render(source, files, variables) {
      return renderWithExports(exports, source, files, variables);
    },
    ast(source, variables) {
      return astWithExports(exports, source, variables);
    },
    languageReference(options) {
      return languageReferenceWithExports(exports, options);
    },
    editorService<T = unknown>(source: string, files: Record<string, string>, request: AlgrafEditorFeatureRequest, uri = "inmemory://algraf/demo.ag") {
      return editorServiceWithExports<T>(exports, source, files, request, uri);
    },
  };
}

function normalizeLoadOptions(options: LoadAlgrafRuntimeOptions | string | URL): LoadAlgrafRuntimeOptions {
  return typeof options === "string" || options instanceof URL ? { wasmUrl: options } : options;
}

// proj4rs (pulled in by algraf-render for map projections) parses every
// proj-string number through `js_sys::parse_float`/`parse_int` on wasm32. Those
// bindings pass the Rust `&str` as a `(ptr, len)` pair into wasm memory and
// expect the host shim to decode it. The shims need the module's memory, which
// only exists after instantiation, so they read it through this holder.
interface MemoryHolder {
  memory: WebAssembly.Memory | null;
}

async function instantiateWasm(response: Response): Promise<WebAssembly.Instance> {
  const holder: MemoryHolder = { memory: null };
  const imports = wasmImports(holder);
  let instance: WebAssembly.Instance | null = null;

  if (WebAssembly.instantiateStreaming) {
    try {
      const result = await WebAssembly.instantiateStreaming(response.clone(), imports);
      instance = result.instance;
    } catch {
      // Local static servers sometimes serve .wasm with a generic MIME type.
    }
  }

  if (!instance) {
    const bytes = await response.arrayBuffer();
    const result = await WebAssembly.instantiate(bytes, imports);
    instance = result.instance;
  }

  holder.memory = (instance.exports as { memory?: WebAssembly.Memory }).memory ?? null;
  return instance;
}

function wasmImports(holder: MemoryHolder): WebAssembly.Imports {
  const readWasmString = (ptr: number, len: number): string => {
    if (!holder.memory) {
      return "";
    }
    return decoder.decode(new Uint8Array(holder.memory.buffer, ptr, len));
  };

  return {
    __wbindgen_placeholder__: {
      __wbindgen_object_drop_ref: () => undefined,
      __wbindgen_describe: () => undefined,
      __wbg_slice_742ea240b87540f5: (value: { slice?: (start?: number, end?: number) => unknown }, start: number, end: number) =>
        value?.slice?.(start, end) ?? null,
      // js-sys `parse_float(&str)`: the string arrives as (ptr, len), not a value
      // to stringify. Decoding the pointer integer instead corrupts every
      // projection parameter, which silently distorts maps (proj4rs aea).
      __wbg_parseFloat_c975dff06aab7294: (ptr: number, len: number) => Number.parseFloat(readWasmString(ptr, len)),
      __wbg_getInt32_1c64e9ae6cdf8387: (value: Int32Array, index: number) => value[index],
      __wbg_getUint32_d2df457b9b889ec3: (value: Uint32Array, index: number) => value[index],
      __wbg_getFloat32_8e834aa3204c9d65: (value: Float32Array, index: number) => value[index],
      __wbg_getFloat64_9c98e48df974a354: (value: Float64Array, index: number) => value[index],
      __wbg_buffer_297793a8f3a42542: (value: ArrayBufferView) => value.buffer,
      // js-sys `parse_int(&str, radix)`: same (ptr, len) string ABI, plus radix.
      __wbg_parseInt_dfc3421c30502d69: (ptr: number, len: number, radix: number) =>
        Number.parseInt(readWasmString(ptr, len), radix),
      __wbg___wbindgen_throw_9c31b086c2b26051: (ptr: number, len: number) => {
        throw new Error(`wasm-bindgen throw at ${ptr}:${len}`);
      },
      __wbg_Error_bce6d499ff0a4aff: () => new Error(),
      __wbg___wbindgen_string_get_d109740c0d18f4d7: () => 0,
    },
    __wbindgen_externref_xform__: {
      __wbindgen_externref_table_set_null: () => undefined,
      __wbindgen_externref_table_grow: () => -1,
    },
  };
}

function renderWithExports(
  exports: AlgrafWasmExports,
  source: string,
  files: Record<string, string>,
  variables: Record<string, string> = {},
): AlgrafRenderResult {
  const inputBytes = encoder.encode(JSON.stringify({ source, files, variables }));
  const inputPtr = exports.algraf_alloc(inputBytes.length);

  try {
    new Uint8Array(exports.memory.buffer, inputPtr, inputBytes.length).set(inputBytes);
    const packed = exports.algraf_render_json(inputPtr, inputBytes.length);
    const outputPtr = Number(packed & 0xffffffffn);
    const outputLen = Number(packed >> 32n);
    const output = new Uint8Array(exports.memory.buffer, outputPtr, outputLen).slice();
    exports.algraf_dealloc(outputPtr, outputLen);
    return JSON.parse(decoder.decode(output)) as AlgrafRenderResult;
  } finally {
    exports.algraf_dealloc(inputPtr, inputBytes.length);
  }
}

function astWithExports(
  exports: AlgrafWasmExports,
  source: string,
  variables: Record<string, string> = {},
): AlgrafAstResult {
  const inputBytes = encoder.encode(JSON.stringify({ source, variables }));
  const inputPtr = exports.algraf_alloc(inputBytes.length);

  try {
    new Uint8Array(exports.memory.buffer, inputPtr, inputBytes.length).set(inputBytes);
    const packed = exports.algraf_ast_json(inputPtr, inputBytes.length);
    const outputPtr = Number(packed & 0xffffffffn);
    const outputLen = Number(packed >> 32n);
    const output = new Uint8Array(exports.memory.buffer, outputPtr, outputLen).slice();
    exports.algraf_dealloc(outputPtr, outputLen);
    return JSON.parse(decoder.decode(output)) as AlgrafAstResult;
  } finally {
    exports.algraf_dealloc(inputPtr, inputBytes.length);
  }
}

function languageReferenceWithExports(
  exports: AlgrafWasmExports,
  options: AlgrafLanguageReferenceOptions = {},
): AlgrafLanguageReference {
  const part = options.part ?? "full";
  if (!isLanguageReferencePart(part)) {
    throw new Error(`unknown Algraf language-reference part: ${String(part)}`);
  }

  if (options.part === undefined) {
    const packed = exports.algraf_language_reference_json();
    return readPackedJson<AlgrafLanguageReference>(exports, packed);
  }

  const inputBytes = encoder.encode(JSON.stringify({ part }));
  const inputPtr = exports.algraf_alloc(inputBytes.length);

  try {
    new Uint8Array(exports.memory.buffer, inputPtr, inputBytes.length).set(inputBytes);
    const packed = exports.algraf_language_reference_part_json(inputPtr, inputBytes.length);
    return readPackedJson<AlgrafLanguageReference>(exports, packed);
  } finally {
    exports.algraf_dealloc(inputPtr, inputBytes.length);
  }
}

function isLanguageReferencePart(part: unknown): part is AlgrafLanguageReferencePart {
  return part === "language" || part === "tooling" || part === "full";
}

function readPackedJson<T>(exports: AlgrafWasmExports, packed: bigint): T {
  const outputPtr = Number(packed & 0xffffffffn);
  const outputLen = Number(packed >> 32n);
  const output = new Uint8Array(exports.memory.buffer, outputPtr, outputLen).slice();
  exports.algraf_dealloc(outputPtr, outputLen);
  return JSON.parse(decoder.decode(output)) as T;
}

function editorServiceWithExports<T>(
  exports: AlgrafWasmExports,
  source: string,
  files: Record<string, string>,
  request: AlgrafEditorFeatureRequest,
  uri: string,
): AlgrafEditorServiceResult<T> {
  const inputBytes = encoder.encode(JSON.stringify({ source, files, uri, request }));
  const inputPtr = exports.algraf_alloc(inputBytes.length);

  try {
    new Uint8Array(exports.memory.buffer, inputPtr, inputBytes.length).set(inputBytes);
    const packed = exports.algraf_editor_service_json(inputPtr, inputBytes.length);
    const outputPtr = Number(packed & 0xffffffffn);
    const outputLen = Number(packed >> 32n);
    const output = new Uint8Array(exports.memory.buffer, outputPtr, outputLen).slice();
    exports.algraf_dealloc(outputPtr, outputLen);
    return JSON.parse(decoder.decode(output)) as AlgrafEditorServiceResult<T>;
  } finally {
    exports.algraf_dealloc(inputPtr, inputBytes.length);
  }
}

function assertAlgrafExports(exports: WebAssembly.Exports): asserts exports is AlgrafWasmExports {
  if (
    !(exports.memory instanceof WebAssembly.Memory) ||
    typeof exports.algraf_alloc !== "function" ||
    typeof exports.algraf_dealloc !== "function" ||
    typeof exports.algraf_render_json !== "function" ||
    typeof exports.algraf_ast_json !== "function" ||
    typeof exports.algraf_language_reference_json !== "function" ||
    typeof exports.algraf_language_reference_part_json !== "function" ||
    typeof exports.algraf_editor_service_json !== "function"
  ) {
    throw new Error("algraf.wasm does not expose the expected browser ABI");
  }
}
