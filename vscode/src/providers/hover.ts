import * as vscode from 'vscode';
import { analyzeSql, isWasmInitialized } from '../analysis';
import type { Dialect, Node, StatementLineage } from '../types';

/**
 * Provides hover information showing table details, join types, and filters.
 */
export class FlowScopeHoverProvider implements vscode.HoverProvider {
  private cachedResults: Map<string, { statements: StatementLineage[]; version: number }> =
    new Map();

  public provideHover(
    document: vscode.TextDocument,
    position: vscode.Position,
    _token: vscode.CancellationToken
  ): vscode.Hover | null {
    const config = vscode.workspace.getConfiguration('flowscope');
    if (!config.get<boolean>('enableHover', true)) {
      return null;
    }

    if (!isWasmInitialized()) {
      return null;
    }

    const sql = document.getText();
    if (!sql.trim()) {
      return null;
    }

    // Get or compute analysis
    const uri = document.uri.toString();
    let statements: StatementLineage[];

    const cached = this.cachedResults.get(uri);
    if (cached && cached.version === document.version) {
      statements = cached.statements;
    } else {
      const dialect = config.get<Dialect>('dialect', 'generic');
      try {
        const result = analyzeSql({ sql, dialect });
        this.cachedResults.set(uri, {
          statements: result.statements,
          version: document.version,
        });
        statements = result.statements;
      } catch {
        return null;
      }
    }

    // Find the word at the hover position
    const wordRange = document.getWordRangeAtPosition(position, /[a-zA-Z_][a-zA-Z0-9_]*/);
    if (!wordRange) {
      return null;
    }

    const word = document.getText(wordRange).toLowerCase();
    const byteOffset = this.positionToByteOffset(sql, position);

    // Find matching node
    for (const stmt of statements) {
      // Check if position is within statement
      if (stmt.span && (byteOffset < stmt.span.start || byteOffset > stmt.span.end)) {
        continue;
      }

      // Find matching table/CTE node
      const matchingNode = stmt.nodes.find((node) => {
        if (node.type === 'column') {
          return false;
        }
        return node.label.toLowerCase() === word;
      });

      if (matchingNode) {
        return this.createHover(matchingNode, stmt);
      }
    }

    return null;
  }

  private createHover(node: Node, stmt: StatementLineage): vscode.Hover {
    const lines: string[] = [];

    // Header
    const icon = node.type === 'cte' ? '📝' : '📋';
    lines.push(`**${icon} ${node.type.toUpperCase()}: ${node.label}**`);

    // Qualified name
    if (node.qualifiedName && node.qualifiedName !== node.label) {
      lines.push(`\n*${node.qualifiedName}*`);
    }

    // Join info
    if (node.joinType) {
      lines.push(`\n**Join:** ${node.joinType}`);
      if (node.joinCondition) {
        lines.push(`\n\`\`\`sql\nON ${node.joinCondition}\n\`\`\``);
      }
    }

    // Filters
    if (node.filters && node.filters.length > 0) {
      lines.push(`\n**Filters:**`);
      for (const filter of node.filters) {
        lines.push(`- \`${filter.expression}\` (${filter.clauseType})`);
      }
    }

    // Related columns
    const columns = stmt.nodes.filter(
      (n) =>
        n.type === 'column' &&
        (n.qualifiedName?.toLowerCase().startsWith(node.label.toLowerCase() + '.') ||
          n.label.toLowerCase().startsWith(node.label.toLowerCase() + '.'))
    );

    if (columns.length > 0) {
      lines.push(`\n**Columns used:** ${columns.length}`);
      const columnNames = columns.slice(0, 5).map((c) => {
        const name = c.label.includes('.') ? c.label.split('.').pop() : c.label;
        if (c.aggregation) {
          if (c.aggregation.isGroupingKey) {
            return `\`${name}\` (GROUP BY)`;
          }
          return `\`${name}\` (${c.aggregation.function ?? 'AGG'})`;
        }
        return `\`${name}\``;
      });
      lines.push(columnNames.join(', '));
      if (columns.length > 5) {
        lines.push(`*...and ${columns.length - 5} more*`);
      }
    }

    // Statement complexity
    lines.push(`\n---\n*Statement complexity: ${stmt.complexityScore}*`);

    const markdown = new vscode.MarkdownString(lines.join('\n'));
    markdown.isTrusted = true;

    return new vscode.Hover(markdown);
  }

  private positionToByteOffset(text: string, position: vscode.Position): number {
    const encoder = new TextEncoder();
    let offset = 0;

    const lines = text.split('\n');
    for (let i = 0; i < position.line && i < lines.length; i++) {
      offset += encoder.encode(lines[i] + '\n').length;
    }

    if (position.line < lines.length) {
      const lineText = lines[position.line].substring(0, position.character);
      offset += encoder.encode(lineText).length;
    }

    return offset;
  }
}
