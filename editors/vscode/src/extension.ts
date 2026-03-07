import * as vscode from "vscode";
import * as path from "path";
import * as fs from "fs";
import { execFile } from "child_process";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

/** Resolve the path to a gust binary (gust-lsp, gust, etc.) */
function resolveGustBinary(context: vscode.ExtensionContext, name: string): string {
  const exe = process.platform === "win32" ? `${name}.exe` : name;
  const bundled = context.asAbsolutePath(path.join("..", "..", "..", "target", "debug", exe));
  const configured = vscode.workspace.getConfiguration("gust").get<string>("lsp.path", "").trim();

  if (name === "gust-lsp" && configured.length > 0) {
    return configured;
  }

  if (name === "gust") {
    const cliConfigured = vscode.workspace.getConfiguration("gust").get<string>("cli.path", "").trim();
    if (cliConfigured.length > 0) {
      return cliConfigured;
    }
  }

  // If we have a configured LSP path, look for the CLI next to it
  if (name === "gust" && configured.length > 0) {
    const sibling = path.join(path.dirname(configured), exe);
    if (fs.existsSync(sibling)) {
      return sibling;
    }
  }

  if (fs.existsSync(bundled)) {
    return bundled;
  }

  return name; // fall back to PATH
}

/** Run a gust CLI command and return stdout/stderr */
function runGustCli(
  context: vscode.ExtensionContext,
  args: string[]
): Promise<{ stdout: string; stderr: string }> {
  const bin = resolveGustBinary(context, "gust");
  return new Promise((resolve, reject) => {
    execFile(bin, args, { maxBuffer: 4 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (err) {
        // Still resolve so callers can inspect stderr
        resolve({ stdout: stdout ?? "", stderr: stderr ?? err.message });
      } else {
        resolve({ stdout: stdout ?? "", stderr: stderr ?? "" });
      }
    });
  });
}

/** Build a simple HTML page that renders Mermaid markup */
function buildDiagramHtml(mermaidSource: string, mermaidJsUrl: string): string {
  const escaped = mermaidSource
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Gust State Diagram</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      background: var(--vscode-editor-background, #1e1e1e);
      color: var(--vscode-editor-foreground, #d4d4d4);
      width: 100vw;
      height: 100vh;
      overflow: hidden;
    }
    #controls {
      position: fixed;
      top: 8px;
      right: 8px;
      z-index: 10;
      display: flex;
      gap: 4px;
    }
    #controls button {
      background: var(--vscode-button-background, #0e639c);
      color: var(--vscode-button-foreground, #fff);
      border: none;
      border-radius: 4px;
      width: 28px;
      height: 28px;
      font-size: 16px;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    #controls button:hover {
      background: var(--vscode-button-hoverBackground, #1177bb);
    }
    #viewport {
      width: 100%;
      height: 100%;
      overflow: hidden;
      cursor: grab;
    }
    #viewport.dragging { cursor: grabbing; }
    #canvas {
      transform-origin: 0 0;
      padding: 2rem;
      display: inline-block;
    }
    .error {
      color: #f48771;
      white-space: pre-wrap;
      font-family: monospace;
    }
  </style>
</head>
<body>
  <div id="controls">
    <button id="zoomIn" title="Zoom in">+</button>
    <button id="zoomOut" title="Zoom out">&minus;</button>
    <button id="zoomFit" title="Fit to view">&#x2922;</button>
  </div>
  <div id="viewport">
    <div id="canvas">
      <pre class="mermaid">${escaped}</pre>
    </div>
  </div>
  <script src="${mermaidJsUrl}"></script>
  <script>
    mermaid.initialize({
      startOnLoad: true,
      theme: document.body.classList.contains('vscode-light') ? 'default' : 'dark'
    });

    // Pan & zoom
    const viewport = document.getElementById('viewport');
    const canvas = document.getElementById('canvas');
    let scale = 1, panX = 0, panY = 0;
    let dragging = false, startX = 0, startY = 0;

    function applyTransform() {
      canvas.style.transform = 'translate(' + panX + 'px,' + panY + 'px) scale(' + scale + ')';
    }

    // Mouse wheel zoom
    viewport.addEventListener('wheel', (e) => {
      e.preventDefault();
      const rect = viewport.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const delta = e.deltaY > 0 ? 0.9 : 1.1;
      const newScale = Math.min(5, Math.max(0.1, scale * delta));
      // Zoom toward cursor
      panX = mx - (mx - panX) * (newScale / scale);
      panY = my - (my - panY) * (newScale / scale);
      scale = newScale;
      applyTransform();
    }, { passive: false });

    // Pan with mouse drag
    viewport.addEventListener('mousedown', (e) => {
      dragging = true;
      startX = e.clientX - panX;
      startY = e.clientY - panY;
      viewport.classList.add('dragging');
    });
    window.addEventListener('mousemove', (e) => {
      if (!dragging) return;
      panX = e.clientX - startX;
      panY = e.clientY - startY;
      applyTransform();
    });
    window.addEventListener('mouseup', () => {
      dragging = false;
      viewport.classList.remove('dragging');
    });

    // Button controls
    document.getElementById('zoomIn').addEventListener('click', () => {
      scale = Math.min(5, scale * 1.2);
      applyTransform();
    });
    document.getElementById('zoomOut').addEventListener('click', () => {
      scale = Math.max(0.1, scale * 0.8);
      applyTransform();
    });
    document.getElementById('zoomFit').addEventListener('click', () => {
      scale = 1; panX = 0; panY = 0;
      applyTransform();
    });
  </script>
</body>
</html>`;
}

export function activate(context: vscode.ExtensionContext): void {
  // ── LSP Client ──────────────────────────────────────────────────────
  const lspCommand = resolveGustBinary(context, "gust-lsp");

  const serverOptions: ServerOptions = { command: lspCommand };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "gust" }]
  };

  client = new LanguageClient("gust-lsp", "Gust Language Server", serverOptions, clientOptions);
  client.start().catch((err) => {
    void vscode.window.showWarningMessage(
      `Unable to start gust-lsp (${String(err)}). Build workspace binary: cargo build -p gust-lsp`
    );
  });

  // ── Status Bar ──────────────────────────────────────────────────────
  const statusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  statusBar.text = "$(circuit-board) Gust";
  statusBar.tooltip = "Gust Language Server";
  statusBar.command = "gust.showDiagram";
  context.subscriptions.push(statusBar);

  function updateStatusBar(editor: vscode.TextEditor | undefined): void {
    if (editor && editor.document.languageId === "gust") {
      statusBar.show();
    } else {
      statusBar.hide();
    }
  }

  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor(updateStatusBar)
  );
  updateStatusBar(vscode.window.activeTextEditor);

  // ── Diagnostics Collection ──────────────────────────────────────────
  const diagnostics = vscode.languages.createDiagnosticCollection("gust");
  context.subscriptions.push(diagnostics);

  // ── Command: Show State Diagram ─────────────────────────────────────
  let diagramPanel: vscode.WebviewPanel | undefined;

  context.subscriptions.push(
    vscode.commands.registerCommand("gust.showDiagram", async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || editor.document.languageId !== "gust") {
        void vscode.window.showWarningMessage("Open a .gu file first.");
        return;
      }

      const filePath = editor.document.uri.fsPath;

      const { stdout, stderr } = await runGustCli(context, ["diagram", filePath]);
      const mermaidSource = stdout.trim();

      if (!mermaidSource) {
        void vscode.window.showErrorMessage(`gust diagram failed: ${stderr.trim() || "no output"}`);
        return;
      }

      const mermaidPath = path.join(context.extensionPath, "node_modules", "mermaid", "dist", "mermaid.min.js");
      const mermaidExists = fs.existsSync(mermaidPath);

      if (diagramPanel) {
        diagramPanel.reveal(vscode.ViewColumn.Beside);
      } else {
        diagramPanel = vscode.window.createWebviewPanel(
          "gustDiagram",
          "Gust: State Diagram",
          vscode.ViewColumn.Beside,
          {
            enableScripts: true,
            localResourceRoots: [vscode.Uri.file(path.join(context.extensionPath, "node_modules"))]
          }
        );
        diagramPanel.onDidDispose(() => {
          diagramPanel = undefined;
        });
      }

      const mermaidSrc = mermaidExists
        ? diagramPanel.webview.asWebviewUri(vscode.Uri.file(mermaidPath)).toString()
        : "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js";

      diagramPanel.webview.html = buildDiagramHtml(mermaidSource, mermaidSrc);
    })
  );

  // Re-render diagram on save of .gu files
  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument(async (doc) => {
      if (doc.languageId === "gust" && diagramPanel) {
        const { stdout } = await runGustCli(context, ["diagram", doc.uri.fsPath]);
        const mermaidSource = stdout.trim();
        if (mermaidSource) {
          const mermaidPath = path.join(context.extensionPath, "node_modules", "mermaid", "dist", "mermaid.min.js");
          const mermaidExists = fs.existsSync(mermaidPath);
          const mermaidSrc = mermaidExists
            ? diagramPanel.webview.asWebviewUri(vscode.Uri.file(mermaidPath)).toString()
            : "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js";
          diagramPanel.webview.html = buildDiagramHtml(mermaidSource, mermaidSrc);
        }
      }
    })
  );

  // ── Command: Check File ─────────────────────────────────────────────
  context.subscriptions.push(
    vscode.commands.registerCommand("gust.checkFile", async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || editor.document.languageId !== "gust") {
        void vscode.window.showWarningMessage("Open a .gu file first.");
        return;
      }

      const filePath = editor.document.uri.fsPath;
      const uri = editor.document.uri;

      const { stdout, stderr } = await runGustCli(context, ["check", filePath]);

      // Success message goes to stdout; errors/warnings go to stderr
      if (stdout.includes("Check passed")) {
        diagnostics.set(uri, []);
        void vscode.window.showInformationMessage("Gust: check passed.");
        return;
      }

      const output = stderr.trim();
      if (!output) {
        diagnostics.set(uri, []);
        void vscode.window.showInformationMessage("Gust: no issues found.");
        return;
      }

      // Parse multi-line error format:
      //   error: duplicate state name 'Foo'
      //     --> src/payment.gu:5:3
      const diags: vscode.Diagnostic[] = [];
      const lines = output.split("\n");

      for (let i = 0; i < lines.length; i++) {
        const sevMatch = /^(error|warning):\s*(.+)$/.exec(lines[i].trim());
        if (!sevMatch) {
          continue;
        }

        const severity = sevMatch[1] === "error"
          ? vscode.DiagnosticSeverity.Error
          : vscode.DiagnosticSeverity.Warning;
        const message = sevMatch[2];

        // Look for location on next few lines
        let lineNum = 0;
        let col = 0;
        for (let j = i + 1; j < Math.min(i + 3, lines.length); j++) {
          const locMatch = /^\s*-->\s*(.+):(\d+):(\d+)/.exec(lines[j]);
          if (locMatch) {
            lineNum = Math.max(0, parseInt(locMatch[2], 10) - 1);
            col = Math.max(0, parseInt(locMatch[3], 10) - 1);
            break;
          }
        }

        const range = new vscode.Range(lineNum, col, lineNum, col + 1);
        diags.push(new vscode.Diagnostic(range, message, severity));
      }

      diagnostics.set(uri, diags);

      if (diags.length === 0) {
        void vscode.window.showInformationMessage("Gust: no issues found.");
      }
    })
  );

  // ── Command: Format Document ────────────────────────────────────────
  context.subscriptions.push(
    vscode.commands.registerCommand("gust.formatDocument", async () => {
      await vscode.commands.executeCommand("editor.action.formatDocument");
    })
  );
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
