import * as vscode from 'vscode';
import { analyzeSql, isWasmInitialized } from '../analysis';
import type { Dialect, Issue, Severity } from '../types';

/**
 * Provides diagnostics (errors, warnings) based on SQL analysis.
 */
export class FlowScopeDiagnosticsProvider {
  private diagnosticCollection: vscode.DiagnosticCollection;
  private disposables: vscode.Disposable[] = [];

  constructor() {
    this.diagnosticCollection = vscode.languages.createDiagnosticCollection('flowscope');

    // Update diagnostics when document changes
    this.disposables.push(
      vscode.workspace.onDidChangeTextDocument((e) => {
        if (e.document.languageId === 'sql') {
          this.updateDiagnostics(e.document);
        }
      })
    );

    // Update diagnostics when document opens
    this.disposables.push(
      vscode.workspace.onDidOpenTextDocument((doc) => {
        if (doc.languageId === 'sql') {
          this.updateDiagnostics(doc);
        }
      })
    );

    // Clear diagnostics when document closes
    this.disposables.push(
      vscode.workspace.onDidCloseTextDocument((doc) => {
        this.diagnosticCollection.delete(doc.uri);
      })
    );

    // Re-run analysis when relevant configuration changes
    this.disposables.push(
      vscode.workspace.onDidChangeConfiguration((event) => {
        if (event.affectsConfiguration('flowscope')) {
          this.refreshAllOpenSqlDocuments();
        }
      })
    );

    // Analyze all open SQL documents
    this.refreshAllOpenSqlDocuments();
  }

  public updateDiagnostics(document: vscode.TextDocument): void {
    if (!isWasmInitialized()) {
      return;
    }

    const sql = document.getText();
    if (!sql.trim()) {
      this.diagnosticCollection.set(document.uri, []);
      return;
    }

    const config = vscode.workspace.getConfiguration('flowscope');
    const dialect = config.get<Dialect>('dialect', 'generic');

    try {
      const result = analyzeSql({ sql, dialect });
      const diagnostics = this.issuesToDiagnostics(document, result.issues);
      this.diagnosticCollection.set(document.uri, diagnostics);
    } catch (error) {
      console.error('FlowScope diagnostics error:', error);
      this.diagnosticCollection.set(document.uri, []);
    }
  }

  private refreshAllOpenSqlDocuments(): void {
    for (const doc of vscode.workspace.textDocuments) {
      if (doc.languageId === 'sql') {
        this.updateDiagnostics(doc);
      }
    }
  }

  private issuesToDiagnostics(document: vscode.TextDocument, issues: Issue[]): vscode.Diagnostic[] {
    const text = document.getText();

    return issues.map((issue) => {
      let range: vscode.Range;

      if (issue.span) {
        const start = this.byteOffsetToPosition(text, issue.span.start);
        const end = this.byteOffsetToPosition(text, issue.span.end);
        range = new vscode.Range(start, end);
      } else {
        // Default to first line if no span
        range = new vscode.Range(0, 0, 0, 0);
      }

      const severity = this.mapSeverity(issue.severity);

      const diagnostic = new vscode.Diagnostic(range, issue.message, severity);

      diagnostic.code = issue.code;
      diagnostic.source = 'FlowScope';

      return diagnostic;
    });
  }

  private mapSeverity(severity: Severity): vscode.DiagnosticSeverity {
    switch (severity) {
      case 'error':
        return vscode.DiagnosticSeverity.Error;
      case 'warning':
        return vscode.DiagnosticSeverity.Warning;
      case 'info':
        return vscode.DiagnosticSeverity.Information;
      default:
        return vscode.DiagnosticSeverity.Information;
    }
  }

  private byteOffsetToPosition(text: string, byteOffset: number): vscode.Position {
    const encoder = new TextEncoder();
    let byteCount = 0;
    let line = 0;
    let character = 0;

    for (let i = 0; i < text.length; i++) {
      if (byteCount >= byteOffset) {
        break;
      }

      const char = text[i];
      const charBytes = encoder.encode(char).length;
      byteCount += charBytes;

      if (char === '\n') {
        line++;
        character = 0;
      } else {
        character++;
      }
    }

    return new vscode.Position(line, character);
  }

  public dispose(): void {
    this.diagnosticCollection.dispose();
    for (const d of this.disposables) {
      d.dispose();
    }
  }
}
