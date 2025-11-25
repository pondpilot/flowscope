import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import { analyzeSql, isWasmInitialized } from '../analysis';
import type { AnalyzeResult, Dialect, IssueCount } from '../types';

/**
 * Manages the lineage visualization webview panel using the React-based flowscope-react package.
 */
export class LineagePanel {
  public static currentPanel: LineagePanel | undefined;
  public static readonly viewType = 'flowscopeLineage';

  private readonly _panel: vscode.WebviewPanel;
  private readonly _extensionUri: vscode.Uri;
  private readonly _extensionPath: string;
  private _disposables: vscode.Disposable[] = [];
  private _currentDocument: vscode.TextDocument | undefined;
  private _currentStatementIndex: number | undefined;

  public static createOrShow(
    extensionUri: vscode.Uri,
    extensionPath: string,
    document?: vscode.TextDocument,
    statementIndex?: number
  ): void {
    const column = vscode.ViewColumn.Beside;

    if (LineagePanel.currentPanel) {
      LineagePanel.currentPanel._panel.reveal(column);
      if (document) {
        LineagePanel.currentPanel.update(document, statementIndex);
      }
      return;
    }

    const panel = vscode.window.createWebviewPanel(LineagePanel.viewType, 'SQL Lineage', column, {
      enableScripts: true,
      retainContextWhenHidden: true,
      localResourceRoots: [vscode.Uri.joinPath(extensionUri, 'dist')],
    });

    LineagePanel.currentPanel = new LineagePanel(panel, extensionUri, extensionPath);

    if (document) {
      LineagePanel.currentPanel.update(document, statementIndex);
    }
  }

  private constructor(
    panel: vscode.WebviewPanel,
    extensionUri: vscode.Uri,
    extensionPath: string
  ) {
    this._panel = panel;
    this._extensionUri = extensionUri;
    this._extensionPath = extensionPath;

    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);

    // Set initial HTML
    this._panel.webview.html = this._getHtmlContent();

    // Handle messages from the webview
    this._panel.webview.onDidReceiveMessage(
      (message) => {
        switch (message.type) {
          case 'ready':
            // Webview is ready, send current data if available
            if (this._currentDocument) {
              this.update(this._currentDocument, this._currentStatementIndex);
            }
            break;
          case 'nodeClick':
            // Handle node click - could navigate to source
            console.log('Node clicked:', message.node);
            break;
        }
      },
      null,
      this._disposables
    );

    // Listen to document changes
    this._disposables.push(
      vscode.workspace.onDidChangeTextDocument((e) => {
        if (
          this._currentDocument &&
          e.document.uri.toString() === this._currentDocument.uri.toString()
        ) {
          this.update(e.document, this._currentStatementIndex);
        }
      })
    );
  }

  public update(document: vscode.TextDocument, statementIndex?: number): void {
    this._currentDocument = document;
    this._currentStatementIndex = statementIndex;

    if (!isWasmInitialized()) {
      this._panel.webview.postMessage({ type: 'error', message: 'WASM not initialized' });
      return;
    }

    const sql = document.getText();
    if (!sql.trim()) {
      this._panel.webview.postMessage({ type: 'empty' });
      return;
    }

    const config = vscode.workspace.getConfiguration('flowscope');
    const dialect = config.get<Dialect>('dialect', 'generic');

    try {
      const result = analyzeSql({ sql, dialect });

      // If a specific statement is selected, filter to just that statement
      let filteredResult: AnalyzeResult = result;
      if (statementIndex !== undefined) {
        const scopedResult = this.getStatementScopedResult(result, statementIndex);
        if (scopedResult) {
          filteredResult = scopedResult;
        }
      }

      this._panel.webview.postMessage({
        type: 'update',
        data: {
          result: filteredResult,
          sql,
        },
      });
    } catch (error) {
      this._panel.webview.postMessage({
        type: 'error',
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  private getStatementScopedResult(
    result: AnalyzeResult,
    statementIndex: number
  ): AnalyzeResult | undefined {
    const statement = result.statements[statementIndex];
    if (!statement) {
      return undefined;
    }

    const targetStatementIndex = statement.statementIndex;
    const globalNodes = result.globalLineage.nodes.filter((node) =>
      node.statementRefs.some((ref) => ref.statementIndex === targetStatementIndex)
    );
    const relevantNodeIds = new Set(globalNodes.map((node) => node.id));
    const globalEdges = result.globalLineage.edges.filter(
      (edge) => relevantNodeIds.has(edge.from) && relevantNodeIds.has(edge.to)
    );
    const filteredIssues = result.issues.filter(
      (issue) =>
        issue.statementIndex === undefined || issue.statementIndex === targetStatementIndex
    );
    const issueCount = filteredIssues.reduce<IssueCount>(
      (acc, issue) => {
        switch (issue.severity) {
          case 'error':
            acc.errors += 1;
            break;
          case 'warning':
            acc.warnings += 1;
            break;
          case 'info':
            acc.infos += 1;
            break;
        }
        return acc;
      },
      { errors: 0, warnings: 0, infos: 0 }
    );
    const tableCount = statement.nodes.filter(
      (node) => node.type === 'table' || node.type === 'cte'
    ).length;
    const columnCount = statement.nodes.filter((node) => node.type === 'column').length;

    return {
      ...result,
      statements: [statement],
      globalLineage: {
        nodes: globalNodes,
        edges: globalEdges,
      },
      summary: {
        statementCount: 1,
        tableCount,
        columnCount,
        joinCount: statement.joinCount,
        complexityScore: statement.complexityScore,
        issueCount,
        hasErrors: issueCount.errors > 0,
      },
      issues: filteredIssues,
    };
  }

  private _getHtmlContent(): string {
    const webview = this._panel.webview;

    // Get URIs for the webview bundle
    const webviewDistPath = path.join(this._extensionPath, 'dist', 'webview');
    const scriptUri = webview.asWebviewUri(
      vscode.Uri.file(path.join(webviewDistPath, 'webview.js'))
    );

    // Check if CSS file exists (Vite might inline it)
    const cssPath = path.join(webviewDistPath, 'style.css');
    const cssUri = fs.existsSync(cssPath)
      ? webview.asWebviewUri(vscode.Uri.file(cssPath))
      : null;

    // Use a nonce for security
    const nonce = getNonce();

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}'; img-src ${webview.cspSource} data:; font-src ${webview.cspSource};">
  <title>SQL Lineage</title>
  ${cssUri ? `<link rel="stylesheet" href="${cssUri}">` : ''}
  <style>
    html, body, #root {
      height: 100%;
      margin: 0;
      padding: 0;
      overflow: hidden;
      background-color: var(--vscode-editor-background);
      color: var(--vscode-foreground);
    }
  </style>
</head>
<body>
  <div id="root"></div>
  <script nonce="${nonce}" src="${scriptUri}"></script>
</body>
</html>`;
  }

  public dispose(): void {
    LineagePanel.currentPanel = undefined;

    this._panel.dispose();

    while (this._disposables.length) {
      const d = this._disposables.pop();
      if (d) {
        d.dispose();
      }
    }
  }
}

function getNonce(): string {
  let text = '';
  const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  for (let i = 0; i < 32; i++) {
    text += possible.charAt(Math.floor(Math.random() * possible.length));
  }
  return text;
}
