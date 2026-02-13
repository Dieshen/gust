import * as vscode from "vscode";
import * as path from "path";
import * as fs from "fs";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const binary = process.platform === "win32" ? "gust-lsp.exe" : "gust-lsp";
  const bundled = context.asAbsolutePath(path.join("..", "..", "..", "target", "debug", binary));
  const configured = vscode.workspace.getConfiguration("gust").get<string>("lsp.path", "").trim();
  const command = configured.length > 0 ? configured : (fs.existsSync(bundled) ? bundled : "gust-lsp");

  const serverOptions: ServerOptions = {
    command
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "gust" }]
  };

  client = new LanguageClient("gust-lsp", "Gust Language Server", serverOptions, clientOptions);
  client.start().catch((err) => {
    void vscode.window.showWarningMessage(
      `Unable to start gust-lsp (${String(err)}). Build workspace binary: cargo build -p gust-lsp`
    );
  });
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
