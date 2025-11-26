/**
 * Export utilities for lineage data.
 * Supports XLSX, JSON, Mermaid, and HTML export formats.
 */

import * as XLSX from 'xlsx';
import type {
  AnalyzeResult,
  StatementLineage,
  Node,
} from '@pondpilot/flowscope-core';
import { isTableLikeType } from '@pondpilot/flowscope-core';

// ============================================================================
// Types
// ============================================================================

export interface ScriptInfo {
  sourceName: string;
  statementCount: number;
  tablesRead: string[];
  tablesWritten: string[];
}

export interface TableInfo {
  name: string;
  qualifiedName: string;
  type: 'table' | 'view' | 'cte';
  columns: string[];
  sourceName?: string;
}

export interface ColumnMapping {
  sourceTable: string;
  sourceColumn: string;
  targetTable: string;
  targetColumn: string;
  expression?: string;
  edgeType: string;
}

export type MermaidGraphType = 'script' | 'table' | 'column' | 'hybrid';

// ============================================================================
// Data Extraction Helpers
// ============================================================================

/**
 * Extract script information from statements
 */
export function extractScriptInfo(statements: StatementLineage[]): ScriptInfo[] {
  const scriptMap = new Map<string, ScriptInfo>();

  for (const stmt of statements) {
    const sourceName = stmt.sourceName || 'default';
    const existing = scriptMap.get(sourceName);

    const tablesRead = new Set<string>(existing?.tablesRead || []);
    const tablesWritten = new Set<string>(existing?.tablesWritten || []);

    for (const node of stmt.nodes) {
      if (node.type === 'table' || node.type === 'view') {
        // A table/view is written to if it has incoming data_flow edges (data flows INTO it)
        const isWritten = stmt.edges.some((e) => e.to === node.id && e.type === 'data_flow');
        // A table/view is read from if it has outgoing data_flow edges (data flows FROM it)
        const isRead = stmt.edges.some((e) => e.from === node.id && e.type === 'data_flow');

        if (isWritten) {
          tablesWritten.add(node.qualifiedName || node.label);
        }
        if (isRead || (!isWritten && !isRead)) {
          tablesRead.add(node.qualifiedName || node.label);
        }
      }
    }

    scriptMap.set(sourceName, {
      sourceName,
      statementCount: (existing?.statementCount || 0) + 1,
      tablesRead: Array.from(tablesRead),
      tablesWritten: Array.from(tablesWritten),
    });
  }

  return Array.from(scriptMap.values());
}

/**
 * Extract table information from statements
 */
export function extractTableInfo(statements: StatementLineage[]): TableInfo[] {
  const tableMap = new Map<string, TableInfo>();

  for (const stmt of statements) {
    const tableNodes = stmt.nodes.filter((n) => isTableLikeType(n.type));
    const columnNodes = stmt.nodes.filter((n) => n.type === 'column');

    for (const tableNode of tableNodes) {
      const key = tableNode.qualifiedName || tableNode.label;
      const existing = tableMap.get(key);

      // Find columns owned by this table/view
      const ownedColumnIds = stmt.edges
        .filter((e) => e.type === 'ownership' && e.from === tableNode.id)
        .map((e) => e.to);

      const columns = columnNodes
        .filter((c) => ownedColumnIds.includes(c.id))
        .map((c) => c.label);

      const existingColumns = new Set(existing?.columns || []);
      columns.forEach((c) => existingColumns.add(c));

      tableMap.set(key, {
        name: tableNode.label,
        qualifiedName: tableNode.qualifiedName || tableNode.label,
        type: tableNode.type as 'table' | 'view' | 'cte',
        columns: Array.from(existingColumns),
        sourceName: stmt.sourceName,
      });
    }
  }

  return Array.from(tableMap.values());
}

/**
 * Extract column-level lineage mappings
 */
export function extractColumnMappings(statements: StatementLineage[]): ColumnMapping[] {
  const mappings: ColumnMapping[] = [];

  for (const stmt of statements) {
    const tableNodes = stmt.nodes.filter((n) => isTableLikeType(n.type));
    const columnNodes = stmt.nodes.filter((n) => n.type === 'column');

    // Build column-to-table lookup
    const columnToTable = new Map<string, Node>();
    for (const edge of stmt.edges) {
      if (edge.type === 'ownership') {
        const tableNode = tableNodes.find((t) => t.id === edge.from);
        if (tableNode) {
          columnToTable.set(edge.to, tableNode);
        }
      }
    }

    // Find derivation/data_flow edges between columns
    for (const edge of stmt.edges) {
      if (edge.type === 'derivation' || edge.type === 'data_flow') {
        const sourceCol = columnNodes.find((c) => c.id === edge.from);
        const targetCol = columnNodes.find((c) => c.id === edge.to);

        if (sourceCol && targetCol) {
          const sourceTable = columnToTable.get(edge.from);
          const targetTable = columnToTable.get(edge.to);

          mappings.push({
            sourceTable: sourceTable?.qualifiedName || sourceTable?.label || 'Output',
            sourceColumn: sourceCol.label,
            targetTable: targetTable?.qualifiedName || targetTable?.label || 'Output',
            targetColumn: targetCol.label,
            expression: edge.expression || targetCol.expression,
            edgeType: edge.type,
          });
        }
      }
    }
  }

  return mappings;
}

/**
 * Represents a dependency from one table to another
 */
export interface TableDependency {
  sourceTable: string;
  targetTable: string;
}

/**
 * Extract table-to-table dependencies from statements.
 * A dependency exists when data flows from one table to another.
 */
export function extractTableDependencies(statements: StatementLineage[]): TableDependency[] {
  const dependencies: TableDependency[] = [];
  const seen = new Set<string>();

  for (const stmt of statements) {
    const tableNodes = stmt.nodes.filter((n) => isTableLikeType(n.type));

    // Find data_flow edges between tables
    for (const edge of stmt.edges) {
      if (edge.type === 'data_flow') {
        const sourceNode = tableNodes.find((n) => n.id === edge.from);
        const targetNode = tableNodes.find((n) => n.id === edge.to);

        if (sourceNode && targetNode) {
          const sourceKey = sourceNode.qualifiedName || sourceNode.label;
          const targetKey = targetNode.qualifiedName || targetNode.label;
          const depKey = `${sourceKey}->${targetKey}`;

          if (!seen.has(depKey) && sourceKey !== targetKey) {
            seen.add(depKey);
            dependencies.push({
              sourceTable: sourceKey,
              targetTable: targetKey,
            });
          }
        }
      }
    }
  }

  return dependencies;
}

/**
 * Generate a dependency matrix sheet for xlsx export.
 * Creates a matrix where rows and columns are tables, and cells indicate dependencies.
 * Uses letters: 'w' = row writes to column, 'r' = row reads from column
 */
export function generateDependencyMatrixSheet(
  dependencies: TableDependency[]
): XLSX.WorkSheet {
  // Collect all unique tables
  const allTables = new Set<string>();
  for (const dep of dependencies) {
    allTables.add(dep.sourceTable);
    allTables.add(dep.targetTable);
  }

  const tableList = Array.from(allTables).sort();

  // Build dependency lookup for quick access
  const depSet = new Set(dependencies.map((d) => `${d.sourceTable}->${d.targetTable}`));

  // Create matrix data
  // First row is header: empty cell + all table names
  const matrixData: (string | number)[][] = [];
  const headerRow: string[] = ['', ...tableList.map((t) => sanitizeXlsxValue(t))];
  matrixData.push(headerRow);

  // Each subsequent row: table name in first column, then dependency indicators
  // 'w' = row writes to column (data flows from row to column)
  // 'r' = row reads from column (data flows from column to row)
  for (const rowTable of tableList) {
    const row: (string | number)[] = [sanitizeXlsxValue(rowTable)];
    for (const colTable of tableList) {
      if (rowTable === colTable) {
        row.push('-');
      } else if (depSet.has(`${rowTable}->${colTable}`)) {
        row.push('w');
      } else if (depSet.has(`${colTable}->${rowTable}`)) {
        row.push('r');
      } else {
        row.push('');
      }
    }
    matrixData.push(row);
  }

  // Add empty row before legend
  matrixData.push([]);

  // Add legend
  matrixData.push(['Legend:']);
  matrixData.push(['w', 'Row table writes to column table (data flows row → column)']);
  matrixData.push(['r', 'Row table reads from column table (data flows column → row)']);
  matrixData.push(['-', 'Self (same table)']);

  return XLSX.utils.aoa_to_sheet(matrixData);
}

// ============================================================================
// XLSX Export
// ============================================================================

/**
 * Sanitize a string value for safe inclusion in Excel cells.
 * Prevents formula injection by prefixing dangerous characters with a single quote.
 * @see https://owasp.org/www-community/attacks/CSV_Injection
 */
export function sanitizeXlsxValue(value: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    return value;
  }
  const firstChar = value.charAt(0);
  if (firstChar === '=' || firstChar === '+' || firstChar === '-' || firstChar === '@') {
    return `'${value}`;
  }
  return value;
}

/**
 * Generate XLSX workbook with sheets for scripts, tables, and column mappings
 */
export function generateXlsxWorkbook(result: AnalyzeResult): XLSX.WorkBook {
  const workbook = XLSX.utils.book_new();

  // Scripts sheet
  const scripts = extractScriptInfo(result.statements);
  const scriptsData = scripts.map((s) => ({
    'Script Name': sanitizeXlsxValue(s.sourceName),
    'Statement Count': s.statementCount,
    'Tables Read': sanitizeXlsxValue(s.tablesRead.join(', ')),
    'Tables Written': sanitizeXlsxValue(s.tablesWritten.join(', ')),
  }));
  const scriptsSheet = XLSX.utils.json_to_sheet(scriptsData);
  XLSX.utils.book_append_sheet(workbook, scriptsSheet, 'Scripts');

  // Tables sheet
  const tables = extractTableInfo(result.statements);
  const tablesData = tables.map((t) => ({
    'Table Name': sanitizeXlsxValue(t.name),
    'Qualified Name': sanitizeXlsxValue(t.qualifiedName),
    'Type': t.type,
    'Columns': sanitizeXlsxValue(t.columns.join(', ')),
    'Source': sanitizeXlsxValue(t.sourceName || ''),
  }));
  const tablesSheet = XLSX.utils.json_to_sheet(tablesData);
  XLSX.utils.book_append_sheet(workbook, tablesSheet, 'Tables');

  // Column Mappings sheet
  const mappings = extractColumnMappings(result.statements);
  const mappingsData = mappings.map((m) => ({
    'Source Table': sanitizeXlsxValue(m.sourceTable),
    'Source Column': sanitizeXlsxValue(m.sourceColumn),
    'Target Table': sanitizeXlsxValue(m.targetTable),
    'Target Column': sanitizeXlsxValue(m.targetColumn),
    'Expression': sanitizeXlsxValue(m.expression || ''),
    'Edge Type': m.edgeType,
  }));
  const mappingsSheet = XLSX.utils.json_to_sheet(mappingsData);
  XLSX.utils.book_append_sheet(workbook, mappingsSheet, 'Column Mappings');

  // Summary sheet
  const summaryData = [
    { Metric: 'Total Statements', Value: result.summary.statementCount },
    { Metric: 'Total Tables', Value: result.summary.tableCount },
    { Metric: 'Total Columns', Value: result.summary.columnCount },
    { Metric: 'Total Joins', Value: result.summary.joinCount },
    { Metric: 'Complexity Score', Value: result.summary.complexityScore },
    { Metric: 'Errors', Value: result.summary.issueCount.errors },
    { Metric: 'Warnings', Value: result.summary.issueCount.warnings },
    { Metric: 'Info', Value: result.summary.issueCount.infos },
  ];
  const summarySheet = XLSX.utils.json_to_sheet(summaryData);
  XLSX.utils.book_append_sheet(workbook, summarySheet, 'Summary');

  // Dependency Matrix sheet
  const dependencies = extractTableDependencies(result.statements);
  const depMatrixSheet = generateDependencyMatrixSheet(dependencies);
  XLSX.utils.book_append_sheet(workbook, depMatrixSheet, 'Dependency Matrix');

  return workbook;
}

/**
 * Download XLSX file
 */
export function downloadXlsx(result: AnalyzeResult, filename = 'lineage-export.xlsx'): void {
  const workbook = generateXlsxWorkbook(result);
  XLSX.writeFile(workbook, filename);
}

// ============================================================================
// JSON Export
// ============================================================================

export interface StructuredLineageJson {
  version: string;
  exportedAt: string;
  summary: AnalyzeResult['summary'];
  scripts: ScriptInfo[];
  tables: TableInfo[];
  columnMappings: ColumnMapping[];
  rawResult: AnalyzeResult;
}

/**
 * Generate structured JSON export
 */
export function generateStructuredJson(result: AnalyzeResult): StructuredLineageJson {
  return {
    version: '1.0',
    exportedAt: new Date().toISOString(),
    summary: result.summary,
    scripts: extractScriptInfo(result.statements),
    tables: extractTableInfo(result.statements),
    columnMappings: extractColumnMappings(result.statements),
    rawResult: result,
  };
}

/**
 * Download JSON file
 */
export function downloadJson(result: AnalyzeResult, filename = 'lineage-export.json'): void {
  const data = generateStructuredJson(result);
  const jsonString = JSON.stringify(data, null, 2);
  const blob = new Blob([jsonString], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.download = filename;
  link.href = url;
  link.click();
  URL.revokeObjectURL(url);
}

// ============================================================================
// Mermaid Export
// ============================================================================

/**
 * Sanitize node ID for Mermaid (remove special chars)
 */
function sanitizeMermaidId(id: string): string {
  return id.replace(/[^a-zA-Z0-9_]/g, '_');
}

/**
 * Escape label for Mermaid
 */
function escapeMermaidLabel(label: string): string {
  return label.replace(/"/g, '\\"').replace(/\n/g, ' ');
}

/**
 * Generate Mermaid diagram for script-level view
 */
export function generateMermaidScriptView(result: AnalyzeResult): string {
  const scripts = extractScriptInfo(result.statements);
  const lines: string[] = ['flowchart LR'];

  // Create script nodes
  for (const script of scripts) {
    const id = sanitizeMermaidId(script.sourceName);
    const label = escapeMermaidLabel(script.sourceName);
    lines.push(`    ${id}["${label}"]`);
  }

  // Create edges based on shared tables (script A writes → script B reads)
  for (const producer of scripts) {
    for (const consumer of scripts) {
      if (producer.sourceName === consumer.sourceName) continue;

      const sharedTables = producer.tablesWritten.filter((t) =>
        consumer.tablesRead.includes(t)
      );

      if (sharedTables.length > 0) {
        const producerId = sanitizeMermaidId(producer.sourceName);
        const consumerId = sanitizeMermaidId(consumer.sourceName);
        const label = sharedTables.slice(0, 3).join(', ') + (sharedTables.length > 3 ? '...' : '');
        lines.push(`    ${producerId} -->|"${escapeMermaidLabel(label)}"| ${consumerId}`);
      }
    }
  }

  return lines.join('\n');
}

/**
 * Generate Mermaid diagram for table-level view
 */
export function generateMermaidTableView(result: AnalyzeResult): string {
  const lines: string[] = ['flowchart LR'];
  const tableIds = new Map<string, string>();
  const edges = new Set<string>();

  for (const stmt of result.statements) {
    const tableNodes = stmt.nodes.filter((n) => isTableLikeType(n.type));

    // Add table nodes
    for (const node of tableNodes) {
      const key = node.qualifiedName || node.label;
      if (!tableIds.has(key)) {
        const id = sanitizeMermaidId(key);
        tableIds.set(key, id);
        let shape: string;
        if (node.type === 'cte') {
          shape = `(["${escapeMermaidLabel(node.label)}"])`;
        } else if (node.type === 'view') {
          shape = `[/"${escapeMermaidLabel(node.label)}"/]`;
        } else {
          shape = `["${escapeMermaidLabel(node.label)}"]`;
        }
        lines.push(`    ${id}${shape}`);
      }
    }

    // Add edges
    for (const edge of stmt.edges) {
      if (edge.type === 'data_flow' || edge.type === 'derivation') {
        const sourceNode = tableNodes.find((n) => n.id === edge.from);
        const targetNode = tableNodes.find((n) => n.id === edge.to);

        if (sourceNode && targetNode) {
          const sourceKey = sourceNode.qualifiedName || sourceNode.label;
          const targetKey = targetNode.qualifiedName || targetNode.label;
          const edgeKey = `${sourceKey}->${targetKey}`;

          if (!edges.has(edgeKey) && sourceKey !== targetKey) {
            edges.add(edgeKey);
            const sourceId = tableIds.get(sourceKey);
            const targetId = tableIds.get(targetKey);
            lines.push(`    ${sourceId} --> ${targetId}`);
          }
        }
      }
    }
  }

  return lines.join('\n');
}

/**
 * Generate Mermaid diagram for column-level view
 */
export function generateMermaidColumnView(result: AnalyzeResult): string {
  const lines: string[] = ['flowchart LR'];
  const mappings = extractColumnMappings(result.statements);
  const nodes = new Set<string>();
  const edges = new Set<string>();

  for (const mapping of mappings) {
    const sourceId = sanitizeMermaidId(`${mapping.sourceTable}_${mapping.sourceColumn}`);
    const targetId = sanitizeMermaidId(`${mapping.targetTable}_${mapping.targetColumn}`);
    const sourceLabel = `${mapping.sourceTable}.${mapping.sourceColumn}`;
    const targetLabel = `${mapping.targetTable}.${mapping.targetColumn}`;

    if (!nodes.has(sourceId)) {
      nodes.add(sourceId);
      lines.push(`    ${sourceId}["${escapeMermaidLabel(sourceLabel)}"]`);
    }

    if (!nodes.has(targetId)) {
      nodes.add(targetId);
      lines.push(`    ${targetId}["${escapeMermaidLabel(targetLabel)}"]`);
    }

    const edgeKey = `${sourceId}->${targetId}`;
    if (!edges.has(edgeKey)) {
      edges.add(edgeKey);
      const style = mapping.edgeType === 'derivation' ? '-.->' : '-->';
      lines.push(`    ${sourceId} ${style} ${targetId}`);
    }
  }

  return lines.join('\n');
}

/**
 * Generate Mermaid diagram for hybrid view (scripts + tables)
 */
export function generateMermaidHybridView(result: AnalyzeResult): string {
  const lines: string[] = ['flowchart LR'];
  const scripts = extractScriptInfo(result.statements);
  const allTables = new Set<string>();

  // Collect all tables
  for (const script of scripts) {
    script.tablesRead.forEach((t) => allTables.add(t));
    script.tablesWritten.forEach((t) => allTables.add(t));
  }

  // Add script nodes
  for (const script of scripts) {
    const id = sanitizeMermaidId(`script_${script.sourceName}`);
    lines.push(`    ${id}{{"${escapeMermaidLabel(script.sourceName)}"}}`);
  }

  // Add table nodes
  for (const table of allTables) {
    const id = sanitizeMermaidId(`table_${table}`);
    const shortName = table.split('.').pop() || table;
    lines.push(`    ${id}["${escapeMermaidLabel(shortName)}"]`);
  }

  // Add edges: script -> written tables
  for (const script of scripts) {
    const scriptId = sanitizeMermaidId(`script_${script.sourceName}`);
    for (const table of script.tablesWritten) {
      const tableId = sanitizeMermaidId(`table_${table}`);
      lines.push(`    ${scriptId} --> ${tableId}`);
    }
  }

  // Add edges: read tables -> script
  for (const script of scripts) {
    const scriptId = sanitizeMermaidId(`script_${script.sourceName}`);
    for (const table of script.tablesRead) {
      const tableId = sanitizeMermaidId(`table_${table}`);
      lines.push(`    ${tableId} --> ${scriptId}`);
    }
  }

  return lines.join('\n');
}

/**
 * Generate Mermaid diagram based on graph type
 */
export function generateMermaid(result: AnalyzeResult, graphType: MermaidGraphType): string {
  switch (graphType) {
    case 'script':
      return generateMermaidScriptView(result);
    case 'table':
      return generateMermaidTableView(result);
    case 'column':
      return generateMermaidColumnView(result);
    case 'hybrid':
      return generateMermaidHybridView(result);
    default:
      return generateMermaidTableView(result);
  }
}

/**
 * Generate all Mermaid diagrams
 */
export function generateAllMermaidDiagrams(result: AnalyzeResult): string {
  const sections: string[] = [];

  sections.push('# Lineage Diagrams\n');

  sections.push('## Script View\n');
  sections.push('```mermaid');
  sections.push(generateMermaidScriptView(result));
  sections.push('```\n');

  sections.push('## Hybrid View (Scripts + Tables)\n');
  sections.push('```mermaid');
  sections.push(generateMermaidHybridView(result));
  sections.push('```\n');

  sections.push('## Table View\n');
  sections.push('```mermaid');
  sections.push(generateMermaidTableView(result));
  sections.push('```\n');

  sections.push('## Column View\n');
  sections.push('```mermaid');
  sections.push(generateMermaidColumnView(result));
  sections.push('```\n');

  return sections.join('\n');
}

/**
 * Download Mermaid markdown file
 */
export function downloadMermaid(result: AnalyzeResult, filename = 'lineage-diagrams.md'): void {
  const content = generateAllMermaidDiagrams(result);
  const blob = new Blob([content], { type: 'text/markdown' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.download = filename;
  link.href = url;
  link.click();
  URL.revokeObjectURL(url);
}

// ============================================================================
// HTML Export
// ============================================================================

/**
 * Generate self-contained HTML with embedded Mermaid diagrams
 */
export function generateHtmlExport(result: AnalyzeResult): string {
  const scriptView = generateMermaidScriptView(result);
  const hybridView = generateMermaidHybridView(result);
  const tableView = generateMermaidTableView(result);
  const columnView = generateMermaidColumnView(result);

  const scripts = extractScriptInfo(result.statements);
  const tables = extractTableInfo(result.statements);
  const mappings = extractColumnMappings(result.statements);

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Lineage Export</title>
  <script src="https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js"></script>
  <style>
    :root {
      --bg-primary: #ffffff;
      --bg-secondary: #f8fafc;
      --text-primary: #1e293b;
      --text-secondary: #64748b;
      --border-color: #e2e8f0;
      --accent-color: #3b82f6;
    }

    * {
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }

    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
      background-color: var(--bg-secondary);
      color: var(--text-primary);
      line-height: 1.6;
    }

    .container {
      max-width: 1400px;
      margin: 0 auto;
      padding: 2rem;
    }

    header {
      background: var(--bg-primary);
      border-bottom: 1px solid var(--border-color);
      padding: 1.5rem 2rem;
      margin-bottom: 2rem;
    }

    h1 {
      font-size: 1.75rem;
      font-weight: 600;
      color: var(--text-primary);
    }

    .export-date {
      color: var(--text-secondary);
      font-size: 0.875rem;
      margin-top: 0.5rem;
    }

    .summary-cards {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
      gap: 1rem;
      margin-bottom: 2rem;
    }

    .card {
      background: var(--bg-primary);
      border: 1px solid var(--border-color);
      border-radius: 8px;
      padding: 1rem;
    }

    .card-label {
      font-size: 0.75rem;
      text-transform: uppercase;
      color: var(--text-secondary);
      margin-bottom: 0.25rem;
    }

    .card-value {
      font-size: 1.5rem;
      font-weight: 600;
      color: var(--text-primary);
    }

    .tabs {
      display: flex;
      gap: 0.5rem;
      margin-bottom: 1rem;
      border-bottom: 1px solid var(--border-color);
      padding-bottom: 0.5rem;
    }

    .tab {
      padding: 0.5rem 1rem;
      border: none;
      background: transparent;
      cursor: pointer;
      font-size: 0.875rem;
      color: var(--text-secondary);
      border-radius: 4px;
      transition: all 0.2s;
    }

    .tab:hover {
      background: var(--bg-secondary);
    }

    .tab.active {
      background: var(--accent-color);
      color: white;
    }

    .tab-content {
      display: none;
    }

    .tab-content.active {
      display: block;
    }

    .diagram-container {
      background: var(--bg-primary);
      border: 1px solid var(--border-color);
      border-radius: 8px;
      padding: 2rem;
      margin-bottom: 2rem;
      overflow-x: auto;
    }

    .mermaid {
      text-align: center;
    }

    table {
      width: 100%;
      border-collapse: collapse;
      background: var(--bg-primary);
      border: 1px solid var(--border-color);
      border-radius: 8px;
      overflow: hidden;
      margin-bottom: 2rem;
    }

    th, td {
      padding: 0.75rem 1rem;
      text-align: left;
      border-bottom: 1px solid var(--border-color);
    }

    th {
      background: var(--bg-secondary);
      font-weight: 600;
      font-size: 0.75rem;
      text-transform: uppercase;
      color: var(--text-secondary);
    }

    tr:last-child td {
      border-bottom: none;
    }

    tr:hover td {
      background: var(--bg-secondary);
    }

    .section-title {
      font-size: 1.25rem;
      font-weight: 600;
      margin-bottom: 1rem;
      color: var(--text-primary);
    }

    .badge {
      display: inline-block;
      padding: 0.125rem 0.5rem;
      border-radius: 9999px;
      font-size: 0.75rem;
      font-weight: 500;
    }

    .badge-table {
      background: #dbeafe;
      color: #1d4ed8;
    }

    .badge-cte {
      background: #fef3c7;
      color: #d97706;
    }
  </style>
</head>
<body>
  <header>
    <h1>Lineage Export</h1>
    <div class="export-date">Exported on ${new Date().toLocaleString()}</div>
  </header>

  <div class="container">
    <div class="summary-cards">
      <div class="card">
        <div class="card-label">Statements</div>
        <div class="card-value">${result.summary.statementCount}</div>
      </div>
      <div class="card">
        <div class="card-label">Tables</div>
        <div class="card-value">${result.summary.tableCount}</div>
      </div>
      <div class="card">
        <div class="card-label">Columns</div>
        <div class="card-value">${result.summary.columnCount}</div>
      </div>
      <div class="card">
        <div class="card-label">Errors</div>
        <div class="card-value">${result.summary.issueCount.errors}</div>
      </div>
      <div class="card">
        <div class="card-label">Warnings</div>
        <div class="card-value">${result.summary.issueCount.warnings}</div>
      </div>
    </div>

    <div class="section-title">Diagrams</div>
    <div class="tabs">
      <button class="tab active" data-tab="script">Script View</button>
      <button class="tab" data-tab="hybrid">Hybrid View</button>
      <button class="tab" data-tab="table">Table View</button>
      <button class="tab" data-tab="column">Column View</button>
    </div>

    <div class="diagram-container">
      <div id="script" class="tab-content active">
        <div class="mermaid">${scriptView}</div>
      </div>
      <div id="hybrid" class="tab-content">
        <div class="mermaid">${hybridView}</div>
      </div>
      <div id="table" class="tab-content">
        <div class="mermaid">${tableView}</div>
      </div>
      <div id="column" class="tab-content">
        <div class="mermaid">${columnView}</div>
      </div>
    </div>

    <div class="section-title">Scripts</div>
    <table>
      <thead>
        <tr>
          <th>Script Name</th>
          <th>Statements</th>
          <th>Tables Read</th>
          <th>Tables Written</th>
        </tr>
      </thead>
      <tbody>
        ${scripts
          .map(
            (s) => `
          <tr>
            <td>${escapeHtml(s.sourceName)}</td>
            <td>${s.statementCount}</td>
            <td>${escapeHtml(s.tablesRead.join(', '))}</td>
            <td>${escapeHtml(s.tablesWritten.join(', '))}</td>
          </tr>
        `
          )
          .join('')}
      </tbody>
    </table>

    <div class="section-title">Tables</div>
    <table>
      <thead>
        <tr>
          <th>Name</th>
          <th>Qualified Name</th>
          <th>Type</th>
          <th>Columns</th>
        </tr>
      </thead>
      <tbody>
        ${tables
          .map(
            (t) => `
          <tr>
            <td>${escapeHtml(t.name)}</td>
            <td>${escapeHtml(t.qualifiedName)}</td>
            <td><span class="badge badge-${t.type}">${t.type.toUpperCase()}</span></td>
            <td>${escapeHtml(t.columns.join(', '))}</td>
          </tr>
        `
          )
          .join('')}
      </tbody>
    </table>

    <div class="section-title">Column Mappings</div>
    <table>
      <thead>
        <tr>
          <th>Source Table</th>
          <th>Source Column</th>
          <th>Target Table</th>
          <th>Target Column</th>
          <th>Expression</th>
        </tr>
      </thead>
      <tbody>
        ${mappings
          .map(
            (m) => `
          <tr>
            <td>${escapeHtml(m.sourceTable)}</td>
            <td>${escapeHtml(m.sourceColumn)}</td>
            <td>${escapeHtml(m.targetTable)}</td>
            <td>${escapeHtml(m.targetColumn)}</td>
            <td>${escapeHtml(m.expression || '')}</td>
          </tr>
        `
          )
          .join('')}
      </tbody>
    </table>
  </div>

  <script>
    mermaid.initialize({ startOnLoad: true, theme: 'neutral' });

    document.querySelectorAll('.tab').forEach(tab => {
      tab.addEventListener('click', () => {
        document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
        document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));

        tab.classList.add('active');
        const targetId = tab.dataset.tab;
        document.getElementById(targetId).classList.add('active');

        // Re-render mermaid for the newly visible diagram
        mermaid.init(undefined, document.querySelector('#' + targetId + ' .mermaid'));
      });
    });
  </script>
</body>
</html>`;
}

/**
 * Escape HTML entities
 */
function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

/**
 * Download HTML file
 */
export function downloadHtml(result: AnalyzeResult, filename = 'lineage-export.html'): void {
  const content = generateHtmlExport(result);
  const blob = new Blob([content], { type: 'text/html' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.download = filename;
  link.href = url;
  link.click();
  URL.revokeObjectURL(url);
}
