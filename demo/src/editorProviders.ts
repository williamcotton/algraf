import * as monaco from "monaco-editor/esm/vs/editor/editor.api";

import type {
  AlgrafEditorFeatureRequest,
  AlgrafRuntime,
  LspDiagnostic,
  LspPosition,
  LspRange,
} from "./algrafWasm";

const SEMANTIC_TOKEN_TYPES = [
  "keyword",
  "function",
  "property",
  "variable",
  "operator",
  "string",
  "number",
  "comment",
];

interface LspTextEdit {
  range: LspRange;
  newText: string;
}

interface LspMarkupContent {
  kind: string;
  value: string;
}

interface LspHover {
  contents: LspMarkupContent | string | Array<string | LspMarkupContent>;
  range?: LspRange;
}

interface LspCompletionItem {
  label: string;
  kind?: number;
  detail?: string;
  documentation?: string | LspMarkupContent;
  insertText?: string;
  insertTextFormat?: number;
}

interface LspSignatureHelp {
  signatures: LspSignatureInformation[];
  activeSignature?: number;
  activeParameter?: number;
}

interface LspSignatureInformation {
  label: string;
  documentation?: string | LspMarkupContent;
  parameters?: LspParameterInformation[];
  activeParameter?: number;
}

interface LspParameterInformation {
  label: string | [number, number];
  documentation?: string | LspMarkupContent;
}

interface LspLocation {
  uri: string;
  range: LspRange;
}

interface LspDocumentHighlight {
  range: LspRange;
  kind?: number;
}

interface LspWorkspaceEdit {
  changes?: Record<string, LspTextEdit[]>;
}

interface LspCodeAction {
  title: string;
  kind?: string;
  diagnostics?: LspDiagnostic[];
  edit?: LspWorkspaceEdit;
}

interface LspDocumentSymbol {
  name: string;
  detail?: string;
  kind: number;
  range: LspRange;
  selectionRange: LspRange;
  children?: LspDocumentSymbol[];
}

interface LspSemanticTokenObject {
  deltaLine: number;
  deltaStart: number;
  length: number;
  tokenType: number;
  tokenModifiersBitset: number;
}

interface LspSemanticTokens {
  data: number[] | LspSemanticTokenObject[];
}

export function registerAlgrafEditorProviders(
  languageId: string,
  getRuntime: () => AlgrafRuntime | null,
  getFiles: () => Record<string, string>,
): monaco.IDisposable {
  const disposables: monaco.IDisposable[] = [
    monaco.languages.registerHoverProvider(languageId, {
      provideHover(model, position) {
        const hover = requestFeature<LspHover | null>(model, getRuntime, getFiles, {
          kind: "hover",
          position: toLspPosition(position),
        });
        if (!hover) {
          return null;
        }
        return {
          contents: hoverContents(hover.contents),
          range: hover.range ? fromLspRange(hover.range) : undefined,
        };
      },
    }),
    monaco.languages.registerCompletionItemProvider(languageId, {
      triggerCharacters: [":", "*", "/", "+", "("],
      provideCompletionItems(model, position) {
        const response = requestFeature<LspCompletionItem[] | { items: LspCompletionItem[] } | null>(
          model,
          getRuntime,
          getFiles,
          {
            kind: "completion",
            position: toLspPosition(position),
          },
        );
        const items = Array.isArray(response) ? response : response?.items ?? [];
        return {
          suggestions: items.map((item) => completionItem(item, position)),
        };
      },
    }),
    monaco.languages.registerSignatureHelpProvider(languageId, {
      signatureHelpTriggerCharacters: ["(", ","],
      signatureHelpRetriggerCharacters: [","],
      provideSignatureHelp(model, position) {
        const help = requestFeature<LspSignatureHelp | null>(model, getRuntime, getFiles, {
          kind: "signatureHelp",
          position: toLspPosition(position),
        });
        if (!help) {
          return null;
        }
        return {
          value: {
            activeSignature: help.activeSignature ?? 0,
            activeParameter: help.activeParameter ?? 0,
            signatures: help.signatures.map((signature) => ({
              label: signature.label,
              documentation: markdownString(signature.documentation),
              parameters: (signature.parameters ?? []).map((parameter) => ({
                label: parameter.label,
                documentation: markdownString(parameter.documentation),
              })),
            })),
          },
          dispose: () => undefined,
        };
      },
    }),
    monaco.languages.registerDocumentFormattingEditProvider(languageId, {
      provideDocumentFormattingEdits(model) {
        const edits = requestFeature<LspTextEdit[] | null>(model, getRuntime, getFiles, {
          kind: "formatting",
        });
        return (edits ?? []).map(textEdit);
      },
    }),
    monaco.languages.registerDocumentRangeFormattingEditProvider(languageId, {
      provideDocumentRangeFormattingEdits(model, range) {
        const edits = requestFeature<LspTextEdit[] | null>(model, getRuntime, getFiles, {
          kind: "rangeFormatting",
          range: toLspRange(range),
        });
        return (edits ?? []).map(textEdit);
      },
    }),
    monaco.languages.registerDocumentSemanticTokensProvider(languageId, {
      getLegend() {
        return {
          tokenTypes: SEMANTIC_TOKEN_TYPES,
          tokenModifiers: [],
        };
      },
      provideDocumentSemanticTokens(model) {
        const result = requestFeature<LspSemanticTokens | { data?: LspSemanticTokens["data"] } | null>(
          model,
          getRuntime,
          getFiles,
          {
            kind: "semanticTokens",
          },
        );
        return {
          data: new Uint32Array(flattenSemanticTokens(result?.data ?? [])),
          resultId: undefined,
        };
      },
      releaseDocumentSemanticTokens() {
        return undefined;
      },
    }),
    monaco.languages.registerCodeActionProvider(languageId, {
      provideCodeActions(model, range, context) {
        const actions = requestFeature<LspCodeAction[] | null>(model, getRuntime, getFiles, {
          kind: "codeActions",
          range: toLspRange(range),
          diagnostics: context.markers.map(markerToDiagnostic),
        });
        return {
          actions: (actions ?? []).map((action) => codeAction(action, model.uri)),
          dispose: () => undefined,
        };
      },
    }),
    monaco.languages.registerDefinitionProvider(languageId, {
      provideDefinition(model, position) {
        const result = requestFeature<LspLocation | LspLocation[] | { uri: string; range: LspRange } | null>(
          model,
          getRuntime,
          getFiles,
          {
            kind: "definition",
            position: toLspPosition(position),
          },
        );
        return locations(result);
      },
    }),
    monaco.languages.registerReferenceProvider(languageId, {
      provideReferences(model, position, context) {
        const result = requestFeature<LspLocation[] | null>(model, getRuntime, getFiles, {
          kind: "references",
          position: toLspPosition(position),
          includeDeclaration: context.includeDeclaration,
        });
        return locations(result);
      },
    }),
    monaco.languages.registerDocumentHighlightProvider(languageId, {
      provideDocumentHighlights(model, position) {
        const result = requestFeature<LspDocumentHighlight[] | null>(model, getRuntime, getFiles, {
          kind: "documentHighlights",
          position: toLspPosition(position),
        });
        return (result ?? []).map((highlight) => ({
          range: fromLspRange(highlight.range),
          kind: documentHighlightKind(highlight.kind),
        }));
      },
    }),
    monaco.languages.registerRenameProvider(languageId, {
      resolveRenameLocation(model, position) {
        const response = requestFeature<{ range?: LspRange } | LspRange | null>(model, getRuntime, getFiles, {
          kind: "prepareRename",
          position: toLspPosition(position),
        });
        const range = "range" in (response ?? {}) ? (response as { range?: LspRange }).range : (response as LspRange | null);
        if (!range) {
          return {
            range: new monaco.Range(position.lineNumber, position.column, position.lineNumber, position.column),
            text: "",
            rejectReason: "This symbol cannot be renamed.",
          };
        }
        return {
          range: fromLspRange(range),
          text: model.getValueInRange(fromLspRange(range)),
        };
      },
      provideRenameEdits(model, position, newName) {
        const edit = requestFeature<LspWorkspaceEdit | null>(model, getRuntime, getFiles, {
          kind: "rename",
          position: toLspPosition(position),
          newName,
        });
        if (!edit) {
          return { edits: [] };
        }
        return workspaceEdit(edit);
      },
    }),
    monaco.languages.registerDocumentSymbolProvider(languageId, {
      provideDocumentSymbols(model) {
        const result = requestFeature<LspDocumentSymbol[] | null>(model, getRuntime, getFiles, {
          kind: "documentSymbols",
        });
        return (result ?? []).map(documentSymbol);
      },
    }),
  ];

  return {
    dispose() {
      for (const disposable of disposables) {
        disposable.dispose();
      }
    },
  };
}

function requestFeature<T>(
  model: monaco.editor.ITextModel,
  getRuntime: () => AlgrafRuntime | null,
  getFiles: () => Record<string, string>,
  request: AlgrafEditorFeatureRequest,
): T | null {
  const runtime = getRuntime();
  if (!runtime) {
    return null;
  }
  const response = runtime.editorService<T>(model.getValue(), getFiles(), request, model.uri.toString());
  if (response.error) {
    console.warn(`Algraf editor service failed: ${response.error}`);
    return null;
  }
  return response.result;
}

function toLspPosition(position: monaco.IPosition): LspPosition {
  return {
    line: Math.max(0, position.lineNumber - 1),
    character: Math.max(0, position.column - 1),
  };
}

function fromLspPosition(position: LspPosition): monaco.IPosition {
  return {
    lineNumber: position.line + 1,
    column: position.character + 1,
  };
}

function toLspRange(range: monaco.IRange): LspRange {
  return {
    start: toLspPosition({ lineNumber: range.startLineNumber, column: range.startColumn }),
    end: toLspPosition({ lineNumber: range.endLineNumber, column: range.endColumn }),
  };
}

function fromLspRange(range: LspRange): monaco.Range {
  const start = fromLspPosition(range.start);
  const end = fromLspPosition(range.end);
  return new monaco.Range(start.lineNumber, start.column, end.lineNumber, end.column);
}

function hoverContents(contents: LspHover["contents"]): monaco.IMarkdownString[] {
  const values = Array.isArray(contents) ? contents : [contents];
  return values.map((value) => ({
    value: typeof value === "string" ? value : value.value,
    isTrusted: false,
    supportThemeIcons: false,
    supportHtml: false,
  }));
}

function markdownString(value: string | LspMarkupContent | undefined): monaco.IMarkdownString | string | undefined {
  if (!value) {
    return undefined;
  }
  if (typeof value === "string") {
    return value;
  }
  return {
    value: value.value,
    isTrusted: false,
    supportHtml: false,
  };
}

function completionItem(item: LspCompletionItem, position: monaco.IPosition): monaco.languages.CompletionItem {
  return {
    label: item.label,
    kind: completionKind(item.kind),
    detail: item.detail,
    documentation: markdownString(item.documentation),
    insertText: item.insertText ?? item.label,
    insertTextRules:
      item.insertTextFormat === 2 ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet : undefined,
    range: new monaco.Range(position.lineNumber, position.column, position.lineNumber, position.column),
  };
}

function completionKind(kind: number | undefined): monaco.languages.CompletionItemKind {
  switch (kind) {
    case 3:
      return monaco.languages.CompletionItemKind.Function;
    case 5:
      return monaco.languages.CompletionItemKind.Field;
    case 10:
      return monaco.languages.CompletionItemKind.Property;
    case 12:
      return monaco.languages.CompletionItemKind.Value;
    case 14:
      return monaco.languages.CompletionItemKind.Keyword;
    case 15:
      return monaco.languages.CompletionItemKind.Snippet;
    case 16:
      return monaco.languages.CompletionItemKind.Color;
    case 24:
      return monaco.languages.CompletionItemKind.Operator;
    default:
      return monaco.languages.CompletionItemKind.Text;
  }
}

function textEdit(edit: LspTextEdit): monaco.languages.TextEdit {
  return {
    range: fromLspRange(edit.range),
    text: edit.newText,
  };
}

function flattenSemanticTokens(data: number[] | LspSemanticTokenObject[]): number[] {
  if (data.length === 0 || typeof data[0] === "number") {
    return data as number[];
  }
  return (data as LspSemanticTokenObject[]).flatMap((token) => [
    token.deltaLine,
    token.deltaStart,
    token.length,
    token.tokenType,
    token.tokenModifiersBitset,
  ]);
}

function markerToDiagnostic(marker: monaco.editor.IMarkerData): LspDiagnostic {
  return {
    range: toLspRange(marker),
    severity: markerSeverity(marker.severity),
    code: markerCode(marker.code),
    source: marker.source,
    message: marker.message,
  };
}

function markerSeverity(severity: monaco.MarkerSeverity): number {
  switch (severity) {
    case monaco.MarkerSeverity.Error:
      return 1;
    case monaco.MarkerSeverity.Warning:
      return 2;
    case monaco.MarkerSeverity.Info:
      return 3;
    case monaco.MarkerSeverity.Hint:
      return 4;
  }
}

function markerCode(code: monaco.editor.IMarkerData["code"]): string | number | undefined {
  if (typeof code === "string" || typeof code === "number") {
    return code;
  }
  return code?.value;
}

function codeAction(action: LspCodeAction, currentUri: monaco.Uri): monaco.languages.CodeAction {
  return {
    title: action.title,
    kind: action.kind,
    diagnostics: action.diagnostics?.map((diagnostic) => ({
      severity: diagnosticSeverity(diagnostic.severity),
      message: diagnostic.message,
      startLineNumber: diagnostic.range.start.line + 1,
      startColumn: diagnostic.range.start.character + 1,
      endLineNumber: diagnostic.range.end.line + 1,
      endColumn: diagnostic.range.end.character + 1,
    })),
    edit: action.edit ? workspaceEdit(action.edit, currentUri) : undefined,
  };
}

function diagnosticSeverity(severity: number | undefined): monaco.MarkerSeverity {
  switch (severity) {
    case 1:
      return monaco.MarkerSeverity.Error;
    case 2:
      return monaco.MarkerSeverity.Warning;
    case 3:
      return monaco.MarkerSeverity.Info;
    case 4:
      return monaco.MarkerSeverity.Hint;
    default:
      return monaco.MarkerSeverity.Info;
  }
}

function workspaceEdit(edit: LspWorkspaceEdit, fallbackUri?: monaco.Uri): monaco.languages.WorkspaceEdit {
  const edits: monaco.languages.IWorkspaceTextEdit[] = [];
  for (const [uri, textEdits] of Object.entries(edit.changes ?? {})) {
    const resource = uri ? monaco.Uri.parse(uri) : fallbackUri;
    if (!resource) {
      continue;
    }
    for (const text of textEdits) {
      edits.push({
        resource,
        textEdit: {
          range: fromLspRange(text.range),
          text: text.newText,
        },
        versionId: undefined,
      });
    }
  }
  return { edits };
}

function locations(result: LspLocation | LspLocation[] | null | undefined): monaco.languages.Location[] {
  const values = Array.isArray(result) ? result : result ? [result] : [];
  return values.map((location) => ({
    uri: monaco.Uri.parse(location.uri),
    range: fromLspRange(location.range),
  }));
}

function documentHighlightKind(kind: number | undefined): monaco.languages.DocumentHighlightKind {
  switch (kind) {
    case 3:
      return monaco.languages.DocumentHighlightKind.Write;
    case 2:
      return monaco.languages.DocumentHighlightKind.Read;
    default:
      return monaco.languages.DocumentHighlightKind.Text;
  }
}

function documentSymbol(symbol: LspDocumentSymbol): monaco.languages.DocumentSymbol {
  return {
    name: symbol.name,
    detail: symbol.detail ?? "",
    kind: symbolKind(symbol.kind),
    range: fromLspRange(symbol.range),
    selectionRange: fromLspRange(symbol.selectionRange),
    tags: [],
    children: symbol.children?.map(documentSymbol),
  };
}

function symbolKind(kind: number): monaco.languages.SymbolKind {
  switch (kind) {
    case 7:
      return monaco.languages.SymbolKind.Property;
    case 12:
      return monaco.languages.SymbolKind.Function;
    case 13:
      return monaco.languages.SymbolKind.Variable;
    case 19:
      return monaco.languages.SymbolKind.Object;
    default:
      return monaco.languages.SymbolKind.Object;
  }
}
