import React from "react";
import * as monaco from "monaco-editor/esm/vs/editor/editor.api";
import "monaco-editor/min/vs/editor/editor.main.css";
import "monaco-editor/esm/vs/editor/contrib/hover/browser/hoverContribution";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import { wireTmGrammars } from "monaco-editor-textmate";
import { Registry } from "monaco-textmate";
import { loadWASM as loadOnigasm } from "onigasm";
import onigasmWasmUrl from "onigasm/lib/onigasm.wasm?url";

import algrafGrammar from "../assets/algraf.tmLanguage.json";
import algrafLanguageConfiguration from "../assets/language-configuration.json";
import { registerAlgrafEditorProviders } from "./providers";

export { registerAlgrafEditorProviders } from "./providers";

export const ALGRAF_LANGUAGE_ID = "algraf";
export const ALGRAF_SCOPE_NAME = "source.algraf";
export const ALGRAF_THEME_NAME = "algraf-playground";
export const ALGRAF_MARKER_OWNER = "algraf-wasm";
export const ALGRAF_DEFAULT_MODEL_URI = "inmemory://algraf/demo.ag";

const encoder = new TextEncoder();
let setupPromise: Promise<void> | null = null;
let onigasmPromise: Promise<void> | null = null;
let providerDisposable: monaco.IDisposable | null = null;

interface EditorContext {
  runtime: () => AlgrafRuntime | null;
  files: () => Record<string, string>;
}

const editorContexts = new Map<string, EditorContext>();

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

export interface AlgrafRuntime {
  editorService<T = unknown>(
    source: string,
    files: Record<string, string>,
    request: AlgrafEditorFeatureRequest,
    uri?: string,
  ): AlgrafEditorServiceResult<T>;
}

export interface AlgrafEditorProps {
  value: string;
  files: Record<string, string>;
  diagnostics: AlgrafDiagnostic[];
  runtime: AlgrafRuntime | null;
  onChange: (value: string) => void;
  modelUri?: string;
  languageId?: string;
  themeName?: string;
  theme?: monaco.editor.IStandaloneThemeData;
  className?: string;
  editorClassName?: string;
  options?: monaco.editor.IStandaloneEditorConstructionOptions;
  setupOptions?: SetupAlgrafMonacoOptions;
}

export interface SetupAlgrafMonacoOptions {
  languageId?: string;
  aliases?: string[];
  extensions?: string[];
  scopeName?: string;
  themeName?: string;
  theme?: monaco.editor.IStandaloneThemeData;
  grammar?: unknown;
  languageConfiguration?: monaco.languages.LanguageConfiguration;
  onigasmWasmUrl?: string;
  configureWorker?: boolean;
}

export function AlgrafEditor({
  value,
  files,
  diagnostics,
  runtime,
  onChange,
  modelUri,
  languageId = ALGRAF_LANGUAGE_ID,
  themeName = ALGRAF_THEME_NAME,
  theme,
  className = "algraf-editor-shell",
  editorClassName = "algraf-editor",
  options,
  setupOptions,
}: AlgrafEditorProps): React.ReactElement {
  const hostRef = React.useRef<HTMLDivElement | null>(null);
  const editorRef = React.useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const modelRef = React.useRef<monaco.editor.ITextModel | null>(null);
  const onChangeRef = React.useRef(onChange);
  const diagnosticsRef = React.useRef(diagnostics);
  const filesRef = React.useRef(files);
  const runtimeRef = React.useRef(runtime);
  const [setupError, setSetupError] = React.useState<string | null>(null);
  const resolvedModelUri = React.useMemo(() => monaco.Uri.parse(modelUri ?? ALGRAF_DEFAULT_MODEL_URI), [modelUri]);

  React.useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  React.useEffect(() => {
    filesRef.current = files;
  }, [files]);

  React.useEffect(() => {
    runtimeRef.current = runtime;
  }, [runtime]);

  React.useEffect(() => {
    diagnosticsRef.current = diagnostics;
    const model = modelRef.current;
    if (model) {
      setAlgrafMarkers(model, diagnostics);
    }
  }, [diagnostics]);

  React.useEffect(() => {
    const model = modelRef.current;
    if (model && model.getValue() !== value) {
      const selection = editorRef.current?.getSelection() ?? null;
      model.setValue(value);
      if (selection) {
        editorRef.current?.setSelection(selection);
      }
      setAlgrafMarkers(model, diagnosticsRef.current);
    }
  }, [value]);

  React.useEffect(() => {
    let cancelled = false;
    let editor: monaco.editor.IStandaloneCodeEditor | null = null;
    let model: monaco.editor.ITextModel | null = null;
    let contentDisposable: monaco.IDisposable | null = null;
    let contextKey: string | null = null;

    setupAlgrafMonaco({ ...setupOptions, languageId, themeName, theme })
      .then(() => {
        if (cancelled || !hostRef.current) {
          return;
        }

        ensureAlgrafProviders(languageId);
        model = monaco.editor.createModel(value, languageId, resolvedModelUri);
        contextKey = model.uri.toString();
        editorContexts.set(contextKey, {
          runtime: () => runtimeRef.current,
          files: () => filesRef.current,
        });
        editor = monaco.editor.create(hostRef.current, {
          model,
          theme: themeName,
          automaticLayout: true,
          bracketPairColorization: { enabled: true },
          cursorBlinking: "smooth",
          fixedOverflowWidgets: true,
          fontFamily: '"SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace',
          fontSize: 13,
          lineHeight: 20,
          minimap: { enabled: false },
          overviewRulerBorder: false,
          padding: { top: 12, bottom: 12 },
          renderLineHighlight: "line",
          scrollBeyondLastLine: false,
          smoothScrolling: true,
          tabSize: 4,
          wordWrap: "off",
          ...options,
        });

        modelRef.current = model;
        editorRef.current = editor;
        setAlgrafMarkers(model, diagnosticsRef.current);

        contentDisposable = model.onDidChangeContent(() => {
          onChangeRef.current(model?.getValue() ?? "");
        });
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setSetupError(err instanceof Error ? err.message : String(err));
        }
      });

    return () => {
      cancelled = true;
      contentDisposable?.dispose();
      if (contextKey) {
        editorContexts.delete(contextKey);
      }
      if (model) {
        monaco.editor.setModelMarkers(model, ALGRAF_MARKER_OWNER, []);
      }
      editor?.dispose();
      model?.dispose();
      if (editorRef.current === editor) {
        editorRef.current = null;
      }
      if (modelRef.current === model) {
        modelRef.current = null;
      }
    };
  }, [languageId, options, resolvedModelUri, setupOptions, theme, themeName]);

  return (
    <div className={className}>
      <div aria-label="Algraf source" className={editorClassName} ref={hostRef} />
      {setupError ? <div className="algraf-editor-error">Editor failed to load: {setupError}</div> : null}
    </div>
  );
}

function ensureAlgrafProviders(languageId: string): void {
  providerDisposable ??= registerAlgrafEditorProviders(
    languageId,
    (model) => editorContexts.get(model.uri.toString())?.runtime() ?? null,
    (model) => editorContexts.get(model.uri.toString())?.files() ?? {},
  );
}

export function setupAlgrafMonaco(options: SetupAlgrafMonacoOptions = {}): Promise<void> {
  setupPromise ??= setupAlgrafMonacoOnce(options).catch((error: unknown) => {
    setupPromise = null;
    throw error;
  });
  return setupPromise;
}

export function registerAlgrafLanguage(options: SetupAlgrafMonacoOptions = {}): void {
  const languageId = options.languageId ?? ALGRAF_LANGUAGE_ID;
  if (monaco.languages.getLanguages().some((language) => language.id === languageId)) {
    return;
  }
  monaco.languages.register({
    id: languageId,
    aliases: options.aliases ?? ["Algraf", "algraf"],
    extensions: options.extensions ?? [".ag"],
  });
  monaco.languages.setLanguageConfiguration(
    languageId,
    (options.languageConfiguration ?? algrafLanguageConfiguration) as monaco.languages.LanguageConfiguration,
  );
}

async function setupAlgrafMonacoOnce(options: SetupAlgrafMonacoOptions): Promise<void> {
  if (options.configureWorker !== false) {
    configureMonacoWorker();
  }
  registerAlgrafLanguage(options);
  defineAlgrafTheme(options.themeName ?? ALGRAF_THEME_NAME, options.theme ?? defaultAlgrafTheme());

  await loadOnigasmOnce(options.onigasmWasmUrl ?? onigasmWasmUrl);
  const registry = new Registry({
    getGrammarDefinition: async () => ({
      format: "json",
      content: options.grammar ?? algrafGrammar,
    }),
  });
  await wireTmGrammars(
    monaco as unknown as Parameters<typeof wireTmGrammars>[0],
    registry,
    new Map([[options.languageId ?? ALGRAF_LANGUAGE_ID, options.scopeName ?? ALGRAF_SCOPE_NAME]]),
  );
}

function configureMonacoWorker(): void {
  const target = globalThis as typeof globalThis & {
    MonacoEnvironment?: monaco.Environment;
  };

  target.MonacoEnvironment ??= {
    getWorker: () => new EditorWorker(),
  };
}

function loadOnigasmOnce(url: string): Promise<void> {
  onigasmPromise ??= loadOnigasm(url).catch((error: unknown) => {
    onigasmPromise = null;
    throw error;
  });
  return onigasmPromise;
}

export function defineAlgrafTheme(
  themeName = ALGRAF_THEME_NAME,
  theme: monaco.editor.IStandaloneThemeData = defaultAlgrafTheme(),
): void {
  monaco.editor.defineTheme(themeName, theme);
}

export function defaultAlgrafTheme(): monaco.editor.IStandaloneThemeData {
  return {
    base: "vs",
    inherit: true,
    rules: [
      { token: "comment", foreground: "6b7280", fontStyle: "italic" },
      { token: "string", foreground: "7a4a10" },
      { token: "number", foreground: "b42318" },
      { token: "keyword", foreground: "166f5c", fontStyle: "bold" },
      { token: "function", foreground: "0f5f8f", fontStyle: "bold" },
      { token: "property", foreground: "9a5512" },
      { token: "variable", foreground: "355f8c" },
      { token: "operator", foreground: "4f5b63" },
      { token: "constant.character.escape", foreground: "9f5b00", fontStyle: "bold" },
      { token: "constant.numeric", foreground: "b42318" },
      { token: "constant.language", foreground: "6f42c1" },
      { token: "invalid.illegal", foreground: "b42318", fontStyle: "underline" },
      { token: "keyword.control", foreground: "166f5c", fontStyle: "bold" },
      { token: "keyword.declaration", foreground: "166f5c", fontStyle: "bold" },
      { token: "keyword.operator.frame", foreground: "7a3f98", fontStyle: "bold" },
      { token: "keyword.operator", foreground: "4f5b63" },
      { token: "support.function", foreground: "0f5f8f", fontStyle: "bold" },
      { token: "support.function.aggregate", foreground: "0f5f8f", fontStyle: "bold" },
      { token: "support.function.aggregate.pdl", foreground: "0f5f8f", fontStyle: "bold" },
      { token: "support.function.scalar", foreground: "0f5f8f", fontStyle: "bold" },
      { token: "support.function.scalar.pdl", foreground: "0f5f8f", fontStyle: "bold" },
      { token: "support.function.window", foreground: "0f5f8f", fontStyle: "bold" },
      { token: "support.function.window.pdl", foreground: "0f5f8f", fontStyle: "bold" },
      { token: "entity.name.function.geometry", foreground: "0f5f8f", fontStyle: "bold" },
      { token: "entity.name.function.stat", foreground: "7a3f98", fontStyle: "bold" },
      { token: "entity.name.function.source", foreground: "3c6b22", fontStyle: "bold" },
      { token: "entity.name.function.literal", foreground: "6f42c1" },
      { token: "entity.name.function", foreground: "315f7d" },
      { token: "variable.parameter.property.unknown", foreground: "a33d2d" },
      { token: "variable.parameter.property", foreground: "9a5512" },
      { token: "variable.other.declaration", foreground: "145f52", fontStyle: "bold" },
      { token: "variable.other.quoted", foreground: "385f70" },
      { token: "variable.other.column", foreground: "355f8c" },
      { token: "punctuation", foreground: "68757d" },
    ],
    colors: {
      "editor.background": "#ffffff",
      "editor.foreground": "#171f24",
      "editor.lineHighlightBackground": "#f4f7f6",
      "editorLineNumber.foreground": "#9aa6ac",
      "editorLineNumber.activeForeground": "#21695d",
      "editorCursor.foreground": "#1f6f62",
      "editor.selectionBackground": "#cfe8df",
      "editor.inactiveSelectionBackground": "#e8f2ee",
      "editorIndentGuide.background1": "#edf1f3",
      "editorIndentGuide.activeBackground1": "#c9d8d3",
    },
  };
}

export function setAlgrafMarkers(model: monaco.editor.ITextModel, diagnostics: AlgrafDiagnostic[]): void {
  monaco.editor.setModelMarkers(
    model,
    ALGRAF_MARKER_OWNER,
    diagnostics.map((diagnostic) => diagnosticToAlgrafMarker(model, diagnostic)),
  );
}

export function diagnosticToAlgrafMarker(
  model: monaco.editor.ITextModel,
  diagnostic: AlgrafDiagnostic,
): monaco.editor.IMarkerData {
  const start = byteOffsetToPosition(model.getValue(), diagnostic.span.start);
  const end = normalizeEndPosition(model, start, byteOffsetToPosition(model.getValue(), diagnostic.span.end));

  return {
    code: diagnostic.code,
    severity: severityToMarkerSeverity(diagnostic.severity),
    source: "Algraf",
    message: diagnostic.help ? `${diagnostic.message}\n\n${diagnostic.help}` : diagnostic.message,
    startLineNumber: start.lineNumber,
    startColumn: start.column,
    endLineNumber: end.lineNumber,
    endColumn: end.column,
    relatedInformation: diagnostic.related?.map((related) => {
      const relatedStart = byteOffsetToPosition(model.getValue(), related.span.start);
      const relatedEnd = normalizeEndPosition(model, relatedStart, byteOffsetToPosition(model.getValue(), related.span.end));
      return {
        resource: model.uri,
        message: related.message,
        startLineNumber: relatedStart.lineNumber,
        startColumn: relatedStart.column,
        endLineNumber: relatedEnd.lineNumber,
        endColumn: relatedEnd.column,
      };
    }),
  };
}

function severityToMarkerSeverity(severity: AlgrafDiagnostic["severity"]): monaco.MarkerSeverity {
  switch (severity) {
    case "error":
      return monaco.MarkerSeverity.Error;
    case "warning":
      return monaco.MarkerSeverity.Warning;
    case "information":
      return monaco.MarkerSeverity.Info;
    case "hint":
      return monaco.MarkerSeverity.Hint;
  }
}

function byteOffsetToPosition(source: string, targetByteOffset: number): monaco.IPosition {
  const byteOffset = Math.max(0, targetByteOffset);
  let bytesSeen = 0;
  let lineNumber = 1;
  let column = 1;

  for (let index = 0; index < source.length && bytesSeen < byteOffset; ) {
    const codePoint = source.codePointAt(index);
    if (codePoint === undefined) {
      break;
    }

    const char = String.fromCodePoint(codePoint);
    const charBytes = encoder.encode(char).length;
    if (bytesSeen + charBytes > byteOffset) {
      break;
    }

    bytesSeen += charBytes;
    index += char.length;

    if (char === "\n") {
      lineNumber += 1;
      column = 1;
    } else {
      column += char.length;
    }
  }

  return { lineNumber, column };
}

function normalizeEndPosition(
  model: monaco.editor.ITextModel,
  start: monaco.IPosition,
  end: monaco.IPosition,
): monaco.IPosition {
  if (end.lineNumber !== start.lineNumber || end.column !== start.column) {
    return end;
  }

  const maxColumn = model.getLineMaxColumn(start.lineNumber);
  if (start.column < maxColumn) {
    return {
      lineNumber: start.lineNumber,
      column: start.column + 1,
    };
  }

  return end;
}
