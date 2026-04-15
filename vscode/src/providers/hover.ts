import * as vscode from 'vscode';
import { analyzeSql, isWasmInitialized } from '../analysis';
import { getFiltersForStatement, scopeNodesToStatement } from '../statementScopedLineage';
import {
  edgesInStatement,
  type AnalyzeResult,
  type Dialect,
  type Edge,
  type Node,
  type StatementMeta,
} from '../types';

/**
 * Provides hover information showing table details, join types, and filters.
 */
export class FlowScopeHoverProvider implements vscode.HoverProvider {
  private cachedResults: Map<string, { result: AnalyzeResult; version: number }> = new Map();

  constructor() {
    // Invalidate cache when FlowScope config (e.g. dialect) changes, since
    // cached analysis results were produced with the old settings.
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration('flowscope')) {
        this.cachedResults.clear();
      }
    });
  }

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
    let result: AnalyzeResult;

    const cached = this.cachedResults.get(uri);
    if (cached && cached.version === document.version) {
      result = cached.result;
    } else {
      const dialect = config.get<Dialect>('dialect', 'generic');
      try {
        result = analyzeSql({ sql, dialect });
        this.cachedResults.set(uri, {
          result,
          version: document.version,
        });
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

    // Find matching node, scoped to the statement containing the cursor.
    for (const stmt of result.statements) {
      // Check if position is within statement
      if (stmt.span && (byteOffset < stmt.span.start || byteOffset > stmt.span.end)) {
        continue;
      }

      const stmtNodes = scopeNodesToStatement(result, stmt.statementIndex, stmt.sourceName);
      const stmtEdges = edgesInStatement(result, stmt.statementIndex);

      // Find matching table/view/CTE node
      const matchingNode = stmtNodes.find((node) => {
        if (node.type === 'column') {
          return false;
        }
        return node.label.toLowerCase() === word;
      });

      if (matchingNode) {
        return this.createHover(matchingNode, stmt, stmtNodes, stmtEdges);
      }
    }

    return null;
  }

  private createHover(
    node: Node,
    stmt: StatementMeta,
    stmtNodes: Node[],
    stmtEdges: Edge[]
  ): vscode.Hover {
    const lines: string[] = [];

    // Header
    const icon = node.type === 'cte' ? '📝' : '📋';
    lines.push(`**${icon} ${node.type.toUpperCase()}: ${node.label}**`);

    // Qualified name
    if (node.qualifiedName && node.qualifiedName !== node.label) {
      lines.push(`\n*${node.qualifiedName}*`);
    }

    // Join info (read from edges originating from this node)
    const joinEdge = stmtEdges.find((e) => e.from === node.id && e.joinType);
    if (joinEdge?.joinType) {
      lines.push(`\n**Join:** ${joinEdge.joinType}`);
      if (joinEdge.joinCondition) {
        lines.push(`\n\`\`\`sql\nON ${joinEdge.joinCondition}\n\`\`\``);
      }
    }

    // Filters
    const filters = getFiltersForStatement(node, stmt.statementIndex);
    if (filters.length > 0) {
      lines.push(`\n**Filters:**`);
      for (const filter of filters) {
        lines.push(`- \`${filter.expression}\` (${filter.clauseType})`);
      }
    }

    // Related columns
    const columns = stmtNodes.filter(
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
