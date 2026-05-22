import * as path from "path";
import * as vscode from "vscode";
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    Trace,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;
let outputChannel: vscode.OutputChannel | undefined;

// Preview pane state. The webview renders SVG returned by the server's
// `algraf/preview` custom request; all rendering happens in the `algraf`
// binary, never in the extension.
let previewPanel: vscode.WebviewPanel | undefined;
let previewUri: vscode.Uri | undefined;
let refreshTimer: ReturnType<typeof setTimeout> | undefined;
let dataWatchers: vscode.FileSystemWatcher[] = [];
let watchedPaths: string[] = [];

interface PreviewResult {
    svg: string | null;
    message: string | null;
    superseded: boolean;
    generation: number;
    dataPaths: string[];
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    outputChannel = vscode.window.createOutputChannel("Algraf Language Server");
    context.subscriptions.push(outputChannel);

    context.subscriptions.push(
        vscode.commands.registerCommand("algraf.restartServer", async () => {
            await restartClient(context);
        }),
    );

    context.subscriptions.push(
        vscode.commands.registerCommand("algraf.showPreview", () => showPreview(context)),
    );

    context.subscriptions.push(
        vscode.commands.registerCommand("algraf.refreshPreview", () => {
            if (previewPanel) {
                void refreshPreview();
            }
        }),
    );

    context.subscriptions.push(
        vscode.workspace.onDidChangeTextDocument((event) => onDocumentChanged(event.document)),
        vscode.workspace.onDidSaveTextDocument((document) => onDocumentChanged(document)),
    );

    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration(async (event) => {
            if (event.affectsConfiguration("algraf.server")) {
                await restartClient(context);
            } else if (event.affectsConfiguration("algraf.trace.server")) {
                await updateTrace();
            }
        }),
    );

    await startClient(context);
}

export async function deactivate(): Promise<void> {
    await stopClient();
}

async function restartClient(context: vscode.ExtensionContext): Promise<void> {
    await stopClient();
    await startClient(context);
}

async function startClient(context: vscode.ExtensionContext): Promise<void> {
    if (client) {
        return;
    }

    const serverOptions = buildServerOptions();
    const clientOptions: LanguageClientOptions = {
        documentSelector: [
            { scheme: "file", language: "algraf" },
            { scheme: "untitled", language: "algraf" },
        ],
        outputChannel,
    };

    client = new LanguageClient(
        "algraf",
        "Algraf Language Server",
        serverOptions,
        clientOptions,
    );

    context.subscriptions.push(client);
    await updateTrace();

    try {
        await client.start();
    } catch (error) {
        client = undefined;
        const message = error instanceof Error ? error.message : String(error);
        outputChannel?.appendLine(`Failed to start Algraf language server: ${message}`);
        void vscode.window.showErrorMessage(
            `Failed to start Algraf language server. Check algraf.server.path.`,
        );
    }
}

async function stopClient(): Promise<void> {
    const active = client;
    client = undefined;
    if (active) {
        await active.stop();
    }
}

function buildServerOptions(): ServerOptions {
    const config = vscode.workspace.getConfiguration("algraf");
    const command = config.get<string>("server.path", "algraf");
    const args = config.get<string[]>("server.args", ["lsp"]);
    const cwdSetting = config.get<string>("server.cwd", "");
    const extraEnv = config.get<Record<string, string>>("server.env", {});
    const cwd = resolveWorkspacePath(cwdSetting);

    return {
        command,
        args,
        options: {
            cwd,
            env: {
                ...process.env,
                ...extraEnv,
            },
        },
    };
}

function resolveWorkspacePath(value: string): string | undefined {
    if (!value.trim()) {
        return undefined;
    }

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    const expanded = workspaceFolder
        ? value.replace(/\$\{workspaceFolder\}/g, workspaceFolder)
        : value;

    if (path.isAbsolute(expanded)) {
        return expanded;
    }

    return workspaceFolder
        ? path.resolve(workspaceFolder, expanded)
        : path.resolve(expanded);
}

async function updateTrace(): Promise<void> {
    if (!client) {
        return;
    }

    const value = vscode.workspace
        .getConfiguration("algraf")
        .get<string>("trace.server", "off");
    await client.setTrace(traceFromSetting(value));
}

function traceFromSetting(value: string): Trace {
    switch (value) {
        case "messages":
            return Trace.Messages;
        case "verbose":
            return Trace.Verbose;
        default:
            return Trace.Off;
    }
}

// --- Preview pane -----------------------------------------------------------

async function showPreview(context: vscode.ExtensionContext): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== "algraf") {
        void vscode.window.showInformationMessage("Open an Algraf (.ag) file to preview it.");
        return;
    }

    previewUri = editor.document.uri;
    if (!previewPanel) {
        previewPanel = vscode.window.createWebviewPanel(
            "algrafPreview",
            "Algraf Preview",
            { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },
            { enableScripts: false, retainContextWhenHidden: true },
        );
        previewPanel.onDidDispose(
            () => {
                previewPanel = undefined;
                previewUri = undefined;
                disposeDataWatchers();
            },
            null,
            context.subscriptions,
        );
    }

    previewPanel.title = `Algraf Preview — ${path.basename(previewUri.fsPath)}`;
    previewPanel.reveal(vscode.ViewColumn.Beside, true);
    previewPanel.webview.html = renderMessage("Rendering…");
    await refreshPreview();
}

function onDocumentChanged(document: vscode.TextDocument): void {
    if (!previewPanel || !previewUri) {
        return;
    }
    if (document.uri.toString() !== previewUri.toString()) {
        return;
    }
    scheduleRefresh();
}

// Debounce so rapid edits or data-file writes coalesce into one render request.
function scheduleRefresh(): void {
    if (refreshTimer) {
        clearTimeout(refreshTimer);
    }
    refreshTimer = setTimeout(() => {
        void refreshPreview();
    }, 250);
}

// Watch the data files the document depends on (reported by the server) and
// refresh when they change. File globs are resolved on the server, watched on
// the client, so this works in remote workspaces too.
function updateDataWatchers(paths: string[]): void {
    const unchanged =
        paths.length === watchedPaths.length && paths.every((p, i) => p === watchedPaths[i]);
    if (unchanged) {
        return;
    }
    disposeDataWatchers();
    watchedPaths = [...paths];
    for (const filePath of paths) {
        const pattern = new vscode.RelativePattern(
            vscode.Uri.file(path.dirname(filePath)),
            path.basename(filePath),
        );
        const watcher = vscode.workspace.createFileSystemWatcher(pattern);
        watcher.onDidChange(scheduleRefresh);
        watcher.onDidCreate(scheduleRefresh);
        watcher.onDidDelete(scheduleRefresh);
        dataWatchers.push(watcher);
    }
}

function disposeDataWatchers(): void {
    for (const watcher of dataWatchers) {
        watcher.dispose();
    }
    dataWatchers = [];
    watchedPaths = [];
}

async function refreshPreview(): Promise<void> {
    if (!previewPanel || !previewUri || !client) {
        return;
    }
    const uri = client.code2ProtocolConverter.asUri(previewUri);
    try {
        const result = await client.sendRequest<PreviewResult>("algraf/preview", { uri });
        if (!previewPanel) {
            return;
        }
        // A newer request superseded this one; its result is stale.
        if (result.superseded) {
            return;
        }
        updateDataWatchers(result.dataPaths ?? []);
        previewPanel.webview.html = renderHtml(result);
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        if (previewPanel) {
            previewPanel.webview.html = renderMessage(
                `Preview request failed: ${escapeHtml(message)}`,
            );
        }
    }
}

function renderHtml(result: PreviewResult): string {
    if (result.svg) {
        return wrapHtml(`<div class="canvas">${result.svg}</div>`);
    }
    return renderMessage(escapeHtml(result.message ?? "No preview available."));
}

function renderMessage(message: string): string {
    return wrapHtml(`<div class="message">${message}</div>`);
}

function wrapHtml(body: string): string {
    // Inline SVG is part of the document DOM, so a strict CSP that forbids
    // external resources still renders it. Only inline styles are allowed.
    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta http-equiv="Content-Security-Policy"
          content="default-src 'none'; style-src 'unsafe-inline'; img-src data:;" />
    <style>
        body {
            margin: 0;
            padding: 16px;
            background: var(--vscode-editor-background);
            color: var(--vscode-editor-foreground);
            font-family: var(--vscode-font-family);
        }
        .canvas { display: flex; justify-content: center; }
        .canvas svg { max-width: 100%; height: auto; }
        .message { opacity: 0.8; font-size: 13px; white-space: pre-wrap; }
    </style>
</head>
<body>${body}</body>
</html>`;
}

function escapeHtml(value: string): string {
    return value
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
}
