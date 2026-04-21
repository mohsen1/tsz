import * as cp from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  context.subscriptions.push(
    vscode.commands.registerCommand("tsz.restartLanguageServer", async () => {
      await restartClient(context, true);
    }),
  );

  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration("tsz.lsp.path") || event.affectsConfiguration("tsz.lsp.args")) {
        await restartClient(context, false);
      }
    }),
  );

  await startClient(context, true);
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

async function restartClient(context: vscode.ExtensionContext, userRequested: boolean): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }

  await startClient(context, userRequested);
}

async function startClient(context: vscode.ExtensionContext, userRequested: boolean): Promise<void> {
  const resolution = resolveServerCommand(context);
  const resolvedCommand = resolution.command;
  if (!resolvedCommand) {
    const message = [
      "Could not find tsz-lsp.",
      "Build it with: cargo build -p tsz-cli --bin tsz-lsp, or set tsz.lsp.path.",
      `Checked: ${resolution.checkedPaths.join(", ")}`,
    ].join(" ");
    if (userRequested) {
      void vscode.window.showErrorMessage(message);
    }
    return;
  }

  const args = vscode.workspace.getConfiguration("tsz").get<string[]>("lsp.args", []);
  const outputChannel = vscode.window.createOutputChannel("TSZ LSP");
  const traceOutputChannel = vscode.window.createOutputChannel("TSZ LSP Trace");

  const serverOptions: ServerOptions = () => {
    const serverArgs = ensureStdioMode(args);
    const serverProcess = cp.spawn(resolvedCommand, serverArgs, {
      cwd: resolution.workingDirectory,
      stdio: "pipe",
    });

    return Promise.resolve(serverProcess);
  };

  const watcher = vscode.workspace.createFileSystemWatcher("**/*.{ts,tsx,js,jsx,mjs,cjs}");
  context.subscriptions.push(watcher);

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "typescript" },
      { scheme: "file", language: "typescriptreact" },
      { scheme: "file", language: "javascript" },
      { scheme: "file", language: "javascriptreact" },
    ],
    outputChannel,
    traceOutputChannel,
    synchronize: {
      fileEvents: watcher,
    },
  };

  client = new LanguageClient("tsz-lsp", "TSZ LSP", serverOptions, clientOptions);
  context.subscriptions.push(client);
  await client.start();
}

function resolveServerCommand(context: vscode.ExtensionContext): {
  command: string | undefined;
  checkedPaths: string[];
  workingDirectory: string;
} {
  const configuredPath = vscode.workspace.getConfiguration("tsz").get<string>("lsp.path", "").trim();
  const workspaceFolder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const extensionRepoRoot = path.resolve(context.extensionPath, "..", "..");
  const workingDirectory = workspaceFolder ?? extensionRepoRoot;

  if (configuredPath) {
    return {
      command: fs.existsSync(configuredPath) ? configuredPath : undefined,
      checkedPaths: [configuredPath],
      workingDirectory,
    };
  }

  const roots = new Set<string>();
  if (workspaceFolder) {
    roots.add(workspaceFolder);
  }

  roots.add(extensionRepoRoot);

  const checkedPaths: string[] = [];
  for (const root of roots) {
    for (const candidate of binaryCandidates(root)) {
      checkedPaths.push(candidate);
      if (fs.existsSync(candidate)) {
        return { command: candidate, checkedPaths, workingDirectory };
      }
    }
  }

  return { command: undefined, checkedPaths, workingDirectory };
}

function binaryName(): string {
  return process.platform === "win32" ? "tsz-lsp.exe" : "tsz-lsp";
}

function binaryCandidates(root: string): string[] {
  return [
    path.join(root, ".target", "debug", binaryName()),
    path.join(root, ".target", "release", binaryName()),
    path.join(root, "target", "debug", binaryName()),
    path.join(root, "target", "release", binaryName()),
  ];
}

function ensureStdioMode(args: string[]): string[] {
  if (args.includes("--mode")) {
    return args;
  }

  return ["--mode", "stdio", ...args];
}