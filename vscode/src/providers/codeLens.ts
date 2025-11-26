import * as vscode from 'vscode';
import { analyzeSql, isWasmInitialized } from '../analysis';
import type { Dialect, StatementLineage } from '../types';

/**
 * Provides CodeLens annotations showing complexity, table count, and join count
 * above each SQL statement.
 */
export class FlowScopeCodeLensProvider implements vscode.CodeLensProvider {
  private _onDidChangeCodeLenses = new vscode.EventEmitter<void>();
  public readonly onDidChangeCodeLenses = this._onDidChangeCodeLenses.event;

  private cachedResults: Map<string, { statements: StatementLineage[]; version: number }> =
    new Map();

  constructor() {
    // Refresh CodeLenses when document changes
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (e.document.languageId === 'sql') {
        this.cachedResults.delete(e.document.uri.toString());
        this._onDidChangeCodeLenses.fire();
      }
    });
  }

  public refresh(): void {
    this.cachedResults.clear();
    this._onDidChangeCodeLenses.fire();
  }

  public provideCodeLenses(
    document: vscode.TextDocument,
    _token: vscode.CancellationToken
  ): vscode.CodeLens[] | Thenable<vscode.CodeLens[]> {
    const config = vscode.workspace.getConfiguration('flowscope');
    if (!config.get<boolean>('enableCodeLens', true)) {
      return [];
    }

    if (!isWasmInitialized()) {
      return [];
    }

    const uri = document.uri.toString();
    const cached = this.cachedResults.get(uri);

    if (cached && cached.version === document.version) {
      return this.createCodeLenses(document, cached.statements);
    }

    // Analyze the document
    const sql = document.getText();
    if (!sql.trim()) {
      return [];
    }

    const dialect = config.get<Dialect>('dialect', 'generic');

    try {
      const result = analyzeSql({ sql, dialect });

      // Cache the results
      this.cachedResults.set(uri, {
        statements: result.statements,
        version: document.version,
      });

      return this.createCodeLenses(document, result.statements);
    } catch (error) {
      console.error('FlowScope analysis error:', error);
      return [];
    }
  }

  private createCodeLenses(
    document: vscode.TextDocument,
    statements: StatementLineage[]
  ): vscode.CodeLens[] {
    const codeLenses: vscode.CodeLens[] = [];
    const text = document.getText();

    for (const stmt of statements) {
      if (!stmt.span) {
        continue;
      }

      // Convert byte offset to position
      const startPos = this.byteOffsetToPosition(text, stmt.span.start);
      const range = new vscode.Range(startPos, startPos);

      // Count tables/views/CTEs (excluding columns)
      const tableCount = stmt.nodes.filter((n) => n.type === 'table' || n.type === 'view' || n.type === 'cte').length;

      // Build the annotation text
      const parts: string[] = [];

      if (tableCount > 0) {
        parts.push(`${tableCount} table${tableCount !== 1 ? 's' : ''}`);
      }

      if (stmt.joinCount > 0) {
        parts.push(`${stmt.joinCount} join${stmt.joinCount !== 1 ? 's' : ''}`);
      }

      parts.push(`complexity: ${stmt.complexityScore}`);

      const title = `📊 ${parts.join(' | ')}`;

      codeLenses.push(
        new vscode.CodeLens(range, {
          title,
          command: 'flowscope.showLineage',
          arguments: [document.uri, stmt.statementIndex],
          tooltip: 'Click to show lineage graph',
        })
      );
    }

    return codeLenses;
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
}
