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

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    outputChannel = vscode.window.createOutputChannel("Algraf Language Server");
    context.subscriptions.push(outputChannel);

    context.subscriptions.push(
        vscode.commands.registerCommand("algraf.restartServer", async () => {
            await restartClient(context);
        }),
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
