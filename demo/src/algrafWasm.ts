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
  algraf_editor_service_json(ptr: number, len: number): bigint;
}

export interface AlgrafRuntime {
  render(source: string, files: Record<string, string>): AlgrafRenderResult;
  editorService<T = unknown>(
    source: string,
    files: Record<string, string>,
    request: AlgrafEditorFeatureRequest,
    uri?: string,
  ): AlgrafEditorServiceResult<T>;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export async function loadAlgrafRuntime(url = "/wasm/algraf.wasm"): Promise<AlgrafRuntime> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`failed to fetch ${url}: ${response.status}`);
  }

  const instance = await instantiateWasm(response);
  const exports = instance.exports as AlgrafWasmExports;
  assertAlgrafExports(exports);

  return {
    render(source, files) {
      return renderWithExports(exports, source, files);
    },
    editorService<T = unknown>(source: string, files: Record<string, string>, request: AlgrafEditorFeatureRequest, uri = "inmemory://algraf/demo.ag") {
      return editorServiceWithExports<T>(exports, source, files, request, uri);
    },
  };
}

async function instantiateWasm(response: Response): Promise<WebAssembly.Instance> {
  if (WebAssembly.instantiateStreaming) {
    try {
      const result = await WebAssembly.instantiateStreaming(response.clone(), wasmImports());
      return result.instance;
    } catch {
      // Local static servers sometimes serve .wasm with a generic MIME type.
    }
  }

  const bytes = await response.arrayBuffer();
  const result = await WebAssembly.instantiate(bytes, wasmImports());
  return result.instance;
}

function wasmImports(): WebAssembly.Imports {
  return {
    __wbindgen_placeholder__: {
      __wbindgen_object_drop_ref: () => undefined,
      __wbindgen_describe: () => undefined,
      __wbg_slice_742ea240b87540f5: (value: { slice?: (start?: number, end?: number) => unknown }, start: number, end: number) =>
        value?.slice?.(start, end) ?? null,
      __wbg_parseFloat_c975dff06aab7294: (value: unknown) => Number.parseFloat(String(value)),
      __wbg_getInt32_1c64e9ae6cdf8387: (value: Int32Array, index: number) => value[index],
      __wbg_getUint32_d2df457b9b889ec3: (value: Uint32Array, index: number) => value[index],
      __wbg_getFloat32_8e834aa3204c9d65: (value: Float32Array, index: number) => value[index],
      __wbg_getFloat64_9c98e48df974a354: (value: Float64Array, index: number) => value[index],
      __wbg_buffer_297793a8f3a42542: (value: ArrayBufferView) => value.buffer,
      __wbg_parseInt_dfc3421c30502d69: (value: unknown, radix: number) => Number.parseInt(String(value), radix),
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
): AlgrafRenderResult {
  const inputBytes = encoder.encode(JSON.stringify({ source, files }));
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
    typeof exports.algraf_editor_service_json !== "function"
  ) {
    throw new Error("algraf.wasm does not expose the expected browser ABI");
  }
}
