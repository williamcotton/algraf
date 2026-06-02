import React from "react";
import * as monaco from "monaco-editor/esm/vs/editor/editor.api";
import "monaco-editor/min/vs/editor/editor.main.css";
import "monaco-editor/esm/vs/editor/contrib/hover/browser/hoverContribution";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import { wireTmGrammars } from "monaco-editor-textmate";
import { Registry } from "monaco-textmate";
import { loadWASM as loadOnigasm } from "onigasm";
import onigasmWasmUrl from "onigasm/lib/onigasm.wasm?url";

import algrafGrammar from "../../editors/vscode/syntaxes/algraf.tmLanguage.json";
import type { AlgrafDiagnostic, AlgrafRuntime } from "./algrafWasm";
import { registerAlgrafEditorProviders } from "./editorProviders";

const LANGUAGE_ID = "algraf";
const SCOPE_NAME = "source.algraf";
const THEME_NAME = "algraf-playground";
const MARKER_OWNER = "algraf-wasm";
const DEFAULT_MODEL_URI = "inmemory://algraf/demo.ag";

const encoder = new TextEncoder();
let setupPromise: Promise<void> | null = null;
let providerDisposable: monaco.IDisposable | null = null;

interface EditorContext {
  runtime: () => AlgrafRuntime | null;
  files: () => Record<string, string>;
}

const editorContexts = new Map<string, EditorContext>();

export interface AlgrafEditorProps {
  value: string;
  files: Record<string, string>;
  diagnostics: AlgrafDiagnostic[];
  runtime: AlgrafRuntime | null;
  onChange: (value: string) => void;
  modelUri?: string;
}

export function AlgrafEditor({ value, files, diagnostics, runtime, onChange, modelUri }: AlgrafEditorProps): React.ReactElement {
  const hostRef = React.useRef<HTMLDivElement | null>(null);
  const editorRef = React.useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const modelRef = React.useRef<monaco.editor.ITextModel | null>(null);
  const onChangeRef = React.useRef(onChange);
  const diagnosticsRef = React.useRef(diagnostics);
  const filesRef = React.useRef(files);
  const runtimeRef = React.useRef(runtime);
  const [setupError, setSetupError] = React.useState<string | null>(null);
  const resolvedModelUri = React.useMemo(() => monaco.Uri.parse(modelUri ?? DEFAULT_MODEL_URI), [modelUri]);

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

    setupAlgrafMonaco()
      .then(() => {
        if (cancelled || !hostRef.current) {
          return;
        }

        ensureAlgrafProviders();
        model = monaco.editor.createModel(value, LANGUAGE_ID, resolvedModelUri);
        contextKey = model.uri.toString();
        editorContexts.set(contextKey, {
          runtime: () => runtimeRef.current,
          files: () => filesRef.current,
        });
        editor = monaco.editor.create(hostRef.current, {
          model,
          theme: THEME_NAME,
          automaticLayout: true,
          bracketPairColorization: { enabled: true },
          cursorBlinking: "smooth",
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
        monaco.editor.setModelMarkers(model, MARKER_OWNER, []);
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
  }, [resolvedModelUri]);

  return (
    <div className="algraf-editor-shell">
      <div aria-label="Algraf source" className="algraf-editor" ref={hostRef} />
      {setupError ? <div className="algraf-editor-error">Editor failed to load: {setupError}</div> : null}
    </div>
  );
}

function ensureAlgrafProviders(): void {
  providerDisposable ??= registerAlgrafEditorProviders(
    LANGUAGE_ID,
    (model) => editorContexts.get(model.uri.toString())?.runtime() ?? null,
    (model) => editorContexts.get(model.uri.toString())?.files() ?? {},
  );
}

function setupAlgrafMonaco(): Promise<void> {
  setupPromise ??= (async () => {
    configureMonacoWorker();

    if (!monaco.languages.getLanguages().some((language) => language.id === LANGUAGE_ID)) {
      monaco.languages.register({
        id: LANGUAGE_ID,
        aliases: ["Algraf", "algraf"],
        extensions: [".ag"],
      });
      monaco.languages.setLanguageConfiguration(LANGUAGE_ID, {
        comments: {
          lineComment: "//",
        },
        brackets: [
          ["{", "}"],
          ["[", "]"],
          ["(", ")"],
        ],
        autoClosingPairs: [
          { open: "{", close: "}" },
          { open: "[", close: "]" },
          { open: "(", close: ")" },
          { open: '"', close: '"' },
          { open: "`", close: "`" },
        ],
        surroundingPairs: [
          { open: "{", close: "}" },
          { open: "[", close: "]" },
          { open: "(", close: ")" },
          { open: '"', close: '"' },
          { open: "`", close: "`" },
        ],
      });
    }

    defineAlgrafTheme();

    await loadOnigasm(onigasmWasmUrl);
    const registry = new Registry({
      getGrammarDefinition: async () => ({
        format: "json",
        content: algrafGrammar,
      }),
    });
    await wireTmGrammars(monaco as unknown as Parameters<typeof wireTmGrammars>[0], registry, new Map([[LANGUAGE_ID, SCOPE_NAME]]));
  })();

  return setupPromise;
}

function configureMonacoWorker(): void {
  const target = globalThis as typeof globalThis & {
    MonacoEnvironment?: monaco.Environment;
  };

  target.MonacoEnvironment ??= {
    getWorker: () => new EditorWorker(),
  };
}

function defineAlgrafTheme(): void {
  monaco.editor.defineTheme(THEME_NAME, {
    base: "vs",
    inherit: true,
    rules: [
      { token: "comment", foreground: "6c757d", fontStyle: "italic" },
      { token: "string", foreground: "8a4b08" },
      { token: "constant.character.escape", foreground: "9f5b00", fontStyle: "bold" },
      { token: "constant.numeric", foreground: "b23b2a" },
      { token: "constant.language", foreground: "6f42c1" },
      { token: "invalid.illegal", foreground: "b42318", fontStyle: "underline" },
      { token: "keyword.declaration", foreground: "166f5c", fontStyle: "bold" },
      { token: "keyword.operator.frame", foreground: "7a3f98", fontStyle: "bold" },
      { token: "keyword.operator", foreground: "4f5b63" },
      { token: "entity.name.function.geometry", foreground: "0f5f8f", fontStyle: "bold" },
      { token: "entity.name.function.stat", foreground: "7a3f98", fontStyle: "bold" },
      { token: "entity.name.function.source", foreground: "3c6b22", fontStyle: "bold" },
      { token: "entity.name.function.literal", foreground: "6f42c1" },
      { token: "entity.name.function", foreground: "315f7d" },
      { token: "variable.parameter.property.unknown", foreground: "a33d2d" },
      { token: "variable.parameter.property", foreground: "9a5512" },
      { token: "variable.other.declaration", foreground: "145f52", fontStyle: "bold" },
      { token: "variable.other.quoted", foreground: "385f70" },
      { token: "variable.other.column", foreground: "263a40" },
      { token: "punctuation", foreground: "68757d" },
    ],
    colors: {
      "editor.background": "#ffffff",
      "editor.foreground": "#182025",
      "editor.lineHighlightBackground": "#f4f8f7",
      "editorLineNumber.foreground": "#9aa6ac",
      "editorLineNumber.activeForeground": "#2f7868",
      "editorCursor.foreground": "#1f6f62",
      "editor.selectionBackground": "#cfe8df",
      "editor.inactiveSelectionBackground": "#e8f2ee",
      "editorIndentGuide.background1": "#edf1f3",
      "editorIndentGuide.activeBackground1": "#c9d8d3",
    },
  });
}

function setAlgrafMarkers(model: monaco.editor.ITextModel, diagnostics: AlgrafDiagnostic[]): void {
  monaco.editor.setModelMarkers(
    model,
    MARKER_OWNER,
    diagnostics.map((diagnostic) => diagnosticToMarker(model, diagnostic)),
  );
}

function diagnosticToMarker(
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
