import { useCallback, type JSX } from 'react';
import { toPng } from 'html-to-image';
import { toast } from 'sonner';
import {
  Download,
  Image,
  FileJson,
  FileSpreadsheet,
  FileCode,
  FileText,
  FileDown,
} from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
  DropdownMenuLabel,
} from './ui/dropdown-menu';
import { Button } from './ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from './ui/tooltip';
import type { AnalyzeResult, Issue, ResolvedSchemaMetadata } from '@pondpilot/flowscope-core';
import {
  extractScriptInfo,
  extractTableInfo,
  extractColumnMappings,
  type ScriptInfo,
  type TableInfo,
  type ColumnMapping,
} from '@pondpilot/flowscope-react';
import * as XLSX from 'xlsx';
import { useIsDarkMode } from '@pondpilot/flowscope-react';
import { getShortcutDisplay } from '@/lib/shortcuts';

// ============================================================================
// Types
// ============================================================================

export interface ExportDialogProps {
  result: AnalyzeResult | null;
  projectName: string;
  graphRef?: React.RefObject<HTMLDivElement | null>;
}

interface IssueExport {
  severity: string;
  code: string;
  message: string;
  sourceName?: string;
  line?: number;
}

interface SchemaColumnExport {
  tableName: string;
  columnName: string;
  dataType: string;
  isPrimaryKey: boolean;
  foreignKeyTable?: string;
  foreignKeyColumn?: string;
  origin: string;
}

// ============================================================================
// Helpers
// ============================================================================

function sanitizeFilename(name: string): string {
  return name.replace(/[^a-zA-Z0-9_-]/g, '_').toLowerCase();
}

function getTimestamp(): string {
  return new Date().toISOString().split('T')[0];
}

function generateFilename(projectName: string, extension: string): string {
  const safeName = sanitizeFilename(projectName);
  const timestamp = getTimestamp();
  return `${safeName}-export-${timestamp}.${extension}`;
}

/**
 * Sanitize a string value for safe inclusion in Excel cells.
 * Prevents formula injection by prefixing dangerous characters.
 */
function sanitizeXlsxValue(value: string): string {
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
 * Extract issues in export-friendly format
 */
function extractIssues(result: AnalyzeResult): IssueExport[] {
  if (!result.issues) return [];

  return result.issues.map((issue: Issue) => ({
    severity: issue.severity,
    code: issue.code,
    message: issue.message,
    sourceName: issue.span ? `Statement ${issue.statementIndex ?? 'unknown'}` : undefined,
    line: issue.span?.start,
  }));
}

/**
 * Extract schema columns in export-friendly format
 */
function extractSchemaColumns(resolvedSchema?: ResolvedSchemaMetadata): SchemaColumnExport[] {
  if (!resolvedSchema?.tables) return [];

  const columns: SchemaColumnExport[] = [];

  for (const table of resolvedSchema.tables) {
    const tableName = [table.catalog, table.schema, table.name]
      .filter(Boolean)
      .join('.');

    for (const col of table.columns) {
      columns.push({
        tableName,
        columnName: col.name,
        dataType: col.dataType || 'unknown',
        isPrimaryKey: col.isPrimaryKey || false,
        foreignKeyTable: col.foreignKey?.table,
        foreignKeyColumn: col.foreignKey?.column,
        origin: table.origin,
      });
    }
  }

  return columns;
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
 * Sanitize node ID for Mermaid
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

// ============================================================================
// Export Functions
// ============================================================================

function generateXlsxWorkbook(
  result: AnalyzeResult,
  projectName: string
): XLSX.WorkBook {
  const workbook = XLSX.utils.book_new();

  // Scripts sheet
  const scripts = extractScriptInfo(result.statements);
  const scriptsData = scripts.map((s: ScriptInfo) => ({
    'Script Name': sanitizeXlsxValue(s.sourceName),
    'Statement Count': s.statementCount,
    'Tables Read': sanitizeXlsxValue(s.tablesRead.join(', ')),
    'Tables Written': sanitizeXlsxValue(s.tablesWritten.join(', ')),
  }));
  const scriptsSheet = XLSX.utils.json_to_sheet(scriptsData);
  XLSX.utils.book_append_sheet(workbook, scriptsSheet, 'Scripts');

  // Tables sheet (enhanced with schema info)
  const tables = extractTableInfo(result.statements);
  const tablesData = tables.map((t: TableInfo) => ({
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
  const mappingsData = mappings.map((m: ColumnMapping) => ({
    'Source Table': sanitizeXlsxValue(m.sourceTable),
    'Source Column': sanitizeXlsxValue(m.sourceColumn),
    'Target Table': sanitizeXlsxValue(m.targetTable),
    'Target Column': sanitizeXlsxValue(m.targetColumn),
    'Expression': sanitizeXlsxValue(m.expression || ''),
    'Edge Type': m.edgeType,
  }));
  const mappingsSheet = XLSX.utils.json_to_sheet(mappingsData);
  XLSX.utils.book_append_sheet(workbook, mappingsSheet, 'Column Mappings');

  // Schema sheet (with column types, keys)
  const schemaColumns = extractSchemaColumns(result.resolvedSchema);
  if (schemaColumns.length > 0) {
    const schemaData = schemaColumns.map((c) => ({
      'Table': sanitizeXlsxValue(c.tableName),
      'Column': sanitizeXlsxValue(c.columnName),
      'Data Type': sanitizeXlsxValue(c.dataType),
      'Primary Key': c.isPrimaryKey ? 'Yes' : '',
      'FK Table': sanitizeXlsxValue(c.foreignKeyTable || ''),
      'FK Column': sanitizeXlsxValue(c.foreignKeyColumn || ''),
      'Origin': c.origin,
    }));
    const schemaSheet = XLSX.utils.json_to_sheet(schemaData);
    XLSX.utils.book_append_sheet(workbook, schemaSheet, 'Schema');
  }

  // Issues sheet
  const issues = extractIssues(result);
  if (issues.length > 0) {
    const issuesData = issues.map((i) => ({
      'Severity': i.severity.toUpperCase(),
      'Code': sanitizeXlsxValue(i.code),
      'Message': sanitizeXlsxValue(i.message),
      'Location': sanitizeXlsxValue(i.sourceName || ''),
    }));
    const issuesSheet = XLSX.utils.json_to_sheet(issuesData);
    XLSX.utils.book_append_sheet(workbook, issuesSheet, 'Issues');
  }

  // Summary sheet
  const summaryData = [
    { Metric: 'Project', Value: projectName },
    { Metric: 'Export Date', Value: new Date().toISOString() },
    { Metric: '', Value: '' },
    { Metric: 'Total Statements', Value: result.summary.statementCount },
    { Metric: 'Total Tables', Value: result.summary.tableCount },
    { Metric: 'Total Columns', Value: result.summary.columnCount },
    { Metric: 'Total Joins', Value: result.summary.joinCount },
    { Metric: 'Complexity Score', Value: result.summary.complexityScore },
    { Metric: '', Value: '' },
    { Metric: 'Errors', Value: result.summary.issueCount.errors },
    { Metric: 'Warnings', Value: result.summary.issueCount.warnings },
    { Metric: 'Info', Value: result.summary.issueCount.infos },
  ];
  const summarySheet = XLSX.utils.json_to_sheet(summaryData);
  XLSX.utils.book_append_sheet(workbook, summarySheet, 'Summary');

  // Dependency Matrix sheet
  const dependencies = extractTableDependencies(result);
  const depMatrixSheet = generateDependencyMatrixSheet(dependencies);
  XLSX.utils.book_append_sheet(workbook, depMatrixSheet, 'Dependency Matrix');

  return workbook;
}

interface TableDependency {
  sourceTable: string;
  targetTable: string;
}

function extractTableDependencies(result: AnalyzeResult): TableDependency[] {
  const dependencies: TableDependency[] = [];
  const seen = new Set<string>();

  for (const stmt of result.statements) {
    const tableNodes = stmt.nodes.filter((n) =>
      n.type === 'table' || n.type === 'view' || n.type === 'cte'
    );

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

function generateDependencyMatrixSheet(dependencies: TableDependency[]): XLSX.WorkSheet {
  const allTables = new Set<string>();
  for (const dep of dependencies) {
    allTables.add(dep.sourceTable);
    allTables.add(dep.targetTable);
  }

  const tableList = Array.from(allTables).sort();
  const depSet = new Set(dependencies.map((d) => `${d.sourceTable}->${d.targetTable}`));

  const matrixData: (string | number)[][] = [];
  const headerRow: string[] = ['', ...tableList.map((t) => sanitizeXlsxValue(t))];
  matrixData.push(headerRow);

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

  matrixData.push([]);
  matrixData.push(['Legend:']);
  matrixData.push(['w', 'Row table writes to column table']);
  matrixData.push(['r', 'Row table reads from column table']);
  matrixData.push(['-', 'Self (same table)']);

  return XLSX.utils.aoa_to_sheet(matrixData);
}

function generateStructuredJson(result: AnalyzeResult, projectName: string) {
  return {
    version: '1.0',
    projectName,
    exportedAt: new Date().toISOString(),
    summary: result.summary,
    scripts: extractScriptInfo(result.statements),
    tables: extractTableInfo(result.statements),
    columnMappings: extractColumnMappings(result.statements),
    schema: extractSchemaColumns(result.resolvedSchema),
    issues: extractIssues(result),
    rawResult: result,
  };
}

function generateCsv(result: AnalyzeResult): string {
  const mappings = extractColumnMappings(result.statements);

  const headers = ['Source Table', 'Source Column', 'Target Table', 'Target Column', 'Expression', 'Edge Type'];
  const rows = mappings.map((m: ColumnMapping) => [
    `"${m.sourceTable.replace(/"/g, '""')}"`,
    `"${m.sourceColumn.replace(/"/g, '""')}"`,
    `"${m.targetTable.replace(/"/g, '""')}"`,
    `"${m.targetColumn.replace(/"/g, '""')}"`,
    `"${(m.expression || '').replace(/"/g, '""')}"`,
    `"${m.edgeType}"`,
  ]);

  return [headers.join(','), ...rows.map((r) => r.join(','))].join('\n');
}

function generateMermaidScriptView(result: AnalyzeResult): string {
  const scripts = extractScriptInfo(result.statements);
  const lines: string[] = ['flowchart LR'];

  for (const script of scripts) {
    const id = sanitizeMermaidId(script.sourceName);
    const label = escapeMermaidLabel(script.sourceName);
    lines.push(`    ${id}["${label}"]`);
  }

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

function generateMermaidTableView(result: AnalyzeResult): string {
  const lines: string[] = ['flowchart LR'];
  const tableIds = new Map<string, string>();
  const edges = new Set<string>();

  for (const stmt of result.statements) {
    const tableNodes = stmt.nodes.filter((n) =>
      n.type === 'table' || n.type === 'view' || n.type === 'cte'
    );

    for (const node of tableNodes) {
      const key = node.qualifiedName || node.label;
      if (!tableIds.has(key)) {
        const id = sanitizeMermaidId(key);
        tableIds.set(key, id);
        const escapedLabel = escapeMermaidLabel(node.label);
        let shape: string;
        switch (node.type) {
          case 'cte':
            shape = `(["${escapedLabel}"])`;
            break;
          case 'view':
            shape = `[/"${escapedLabel}"/]`;
            break;
          default:
            shape = `["${escapedLabel}"]`;
        }
        lines.push(`    ${id}${shape}`);
      }
    }

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

function generateMermaidColumnView(result: AnalyzeResult): string {
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

function generateMermaidHybridView(result: AnalyzeResult): string {
  const lines: string[] = ['flowchart LR'];
  const scripts = extractScriptInfo(result.statements);
  const allTables = new Set<string>();

  for (const script of scripts) {
    script.tablesRead.forEach((t) => allTables.add(t));
    script.tablesWritten.forEach((t) => allTables.add(t));
  }

  for (const script of scripts) {
    const id = sanitizeMermaidId(`script_${script.sourceName}`);
    lines.push(`    ${id}{{"${escapeMermaidLabel(script.sourceName)}"}}`);
  }

  for (const table of allTables) {
    const id = sanitizeMermaidId(`table_${table}`);
    const shortName = table.split('.').pop() || table;
    lines.push(`    ${id}["${escapeMermaidLabel(shortName)}"]`);
  }

  for (const script of scripts) {
    const scriptId = sanitizeMermaidId(`script_${script.sourceName}`);
    for (const table of script.tablesWritten) {
      const tableId = sanitizeMermaidId(`table_${table}`);
      lines.push(`    ${scriptId} --> ${tableId}`);
    }
  }

  for (const script of scripts) {
    const scriptId = sanitizeMermaidId(`script_${script.sourceName}`);
    for (const table of script.tablesRead) {
      const tableId = sanitizeMermaidId(`table_${table}`);
      lines.push(`    ${tableId} --> ${scriptId}`);
    }
  }

  return lines.join('\n');
}

function generateAllMermaidDiagrams(result: AnalyzeResult): string {
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

function generateHtmlExport(result: AnalyzeResult, projectName: string): string {
  const scriptView = generateMermaidScriptView(result);
  const hybridView = generateMermaidHybridView(result);
  const tableView = generateMermaidTableView(result);
  const columnView = generateMermaidColumnView(result);

  const scripts = extractScriptInfo(result.statements);
  const tables = extractTableInfo(result.statements);
  const mappings = extractColumnMappings(result.statements);
  const issues = extractIssues(result);

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>${escapeHtml(projectName)} - Lineage Export</title>
  <script src="https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js"></script>
  <style>
    :root {
      --bg-primary: #ffffff;
      --bg-secondary: #f8fafc;
      --text-primary: #1e293b;
      --text-secondary: #64748b;
      --border-color: #e2e8f0;
      --accent-color: #3b82f6;
      --error-color: #ef4444;
      --warning-color: #f59e0b;
    }

    * { box-sizing: border-box; margin: 0; padding: 0; }

    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
      background-color: var(--bg-secondary);
      color: var(--text-primary);
      line-height: 1.6;
    }

    .container { max-width: 1400px; margin: 0 auto; padding: 2rem; }

    header {
      background: var(--bg-primary);
      border-bottom: 1px solid var(--border-color);
      padding: 1.5rem 2rem;
      margin-bottom: 2rem;
    }

    h1 { font-size: 1.75rem; font-weight: 600; }
    .export-date { color: var(--text-secondary); font-size: 0.875rem; margin-top: 0.5rem; }

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

    .card-label { font-size: 0.75rem; text-transform: uppercase; color: var(--text-secondary); }
    .card-value { font-size: 1.5rem; font-weight: 600; }
    .card-value.error { color: var(--error-color); }
    .card-value.warning { color: var(--warning-color); }

    .tabs { display: flex; gap: 0.5rem; margin-bottom: 1rem; border-bottom: 1px solid var(--border-color); padding-bottom: 0.5rem; }
    .tab { padding: 0.5rem 1rem; border: none; background: transparent; cursor: pointer; font-size: 0.875rem; color: var(--text-secondary); border-radius: 4px; }
    .tab:hover { background: var(--bg-secondary); }
    .tab.active { background: var(--accent-color); color: white; }

    .tab-content { display: none; }
    .tab-content.active { display: block; }

    .diagram-container {
      background: var(--bg-primary);
      border: 1px solid var(--border-color);
      border-radius: 8px;
      padding: 2rem;
      margin-bottom: 2rem;
      overflow-x: auto;
    }

    .mermaid { text-align: center; }

    table { width: 100%; border-collapse: collapse; background: var(--bg-primary); border: 1px solid var(--border-color); border-radius: 8px; overflow: hidden; margin-bottom: 2rem; }
    th, td { padding: 0.75rem 1rem; text-align: left; border-bottom: 1px solid var(--border-color); }
    th { background: var(--bg-secondary); font-weight: 600; font-size: 0.75rem; text-transform: uppercase; color: var(--text-secondary); }
    tr:last-child td { border-bottom: none; }
    tr:hover td { background: var(--bg-secondary); }

    .section-title { font-size: 1.25rem; font-weight: 600; margin-bottom: 1rem; }

    .badge { display: inline-block; padding: 0.125rem 0.5rem; border-radius: 9999px; font-size: 0.75rem; font-weight: 500; }
    .badge-table { background: #dbeafe; color: #1d4ed8; }
    .badge-view { background: #dcfce7; color: #16a34a; }
    .badge-cte { background: #fef3c7; color: #d97706; }
    .badge-error { background: #fee2e2; color: #dc2626; }
    .badge-warning { background: #fef3c7; color: #d97706; }
    .badge-info { background: #dbeafe; color: #2563eb; }
  </style>
</head>
<body>
  <header>
    <h1>${escapeHtml(projectName)}</h1>
    <div class="export-date">Exported on ${new Date().toLocaleString()}</div>
  </header>

  <div class="container">
    <div class="summary-cards">
      <div class="card"><div class="card-label">Statements</div><div class="card-value">${result.summary.statementCount}</div></div>
      <div class="card"><div class="card-label">Tables</div><div class="card-value">${result.summary.tableCount}</div></div>
      <div class="card"><div class="card-label">Columns</div><div class="card-value">${result.summary.columnCount}</div></div>
      <div class="card"><div class="card-label">Joins</div><div class="card-value">${result.summary.joinCount}</div></div>
      <div class="card"><div class="card-label">Errors</div><div class="card-value${result.summary.issueCount.errors > 0 ? ' error' : ''}">${result.summary.issueCount.errors}</div></div>
      <div class="card"><div class="card-label">Warnings</div><div class="card-value${result.summary.issueCount.warnings > 0 ? ' warning' : ''}">${result.summary.issueCount.warnings}</div></div>
    </div>

    <div class="section-title">Diagrams</div>
    <div class="tabs">
      <button class="tab active" data-tab="script">Script View</button>
      <button class="tab" data-tab="hybrid">Hybrid View</button>
      <button class="tab" data-tab="table">Table View</button>
      <button class="tab" data-tab="column">Column View</button>
    </div>

    <div class="diagram-container">
      <div id="script" class="tab-content active"><div class="mermaid">${scriptView}</div></div>
      <div id="hybrid" class="tab-content"><div class="mermaid">${hybridView}</div></div>
      <div id="table" class="tab-content"><div class="mermaid">${tableView}</div></div>
      <div id="column" class="tab-content"><div class="mermaid">${columnView}</div></div>
    </div>

    ${issues.length > 0 ? `
    <div class="section-title">Issues</div>
    <table>
      <thead><tr><th>Severity</th><th>Code</th><th>Message</th></tr></thead>
      <tbody>
        ${issues.map((i) => `<tr><td><span class="badge badge-${i.severity}">${i.severity.toUpperCase()}</span></td><td>${escapeHtml(i.code)}</td><td>${escapeHtml(i.message)}</td></tr>`).join('')}
      </tbody>
    </table>
    ` : ''}

    <div class="section-title">Scripts</div>
    <table>
      <thead><tr><th>Script Name</th><th>Statements</th><th>Tables Read</th><th>Tables Written</th></tr></thead>
      <tbody>
        ${scripts.map((s: ScriptInfo) => `<tr><td>${escapeHtml(s.sourceName)}</td><td>${s.statementCount}</td><td>${escapeHtml(s.tablesRead.join(', '))}</td><td>${escapeHtml(s.tablesWritten.join(', '))}</td></tr>`).join('')}
      </tbody>
    </table>

    <div class="section-title">Tables</div>
    <table>
      <thead><tr><th>Name</th><th>Qualified Name</th><th>Type</th><th>Columns</th></tr></thead>
      <tbody>
        ${tables.map((t: TableInfo) => `<tr><td>${escapeHtml(t.name)}</td><td>${escapeHtml(t.qualifiedName)}</td><td><span class="badge badge-${t.type}">${t.type.toUpperCase()}</span></td><td>${escapeHtml(t.columns.join(', '))}</td></tr>`).join('')}
      </tbody>
    </table>

    <div class="section-title">Column Mappings</div>
    <table>
      <thead><tr><th>Source Table</th><th>Source Column</th><th>Target Table</th><th>Target Column</th><th>Expression</th></tr></thead>
      <tbody>
        ${mappings.map((m: ColumnMapping) => `<tr><td>${escapeHtml(m.sourceTable)}</td><td>${escapeHtml(m.sourceColumn)}</td><td>${escapeHtml(m.targetTable)}</td><td>${escapeHtml(m.targetColumn)}</td><td>${escapeHtml(m.expression || '')}</td></tr>`).join('')}
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
        mermaid.init(undefined, document.querySelector('#' + targetId + ' .mermaid'));
      });
    });
  </script>
</body>
</html>`;
}

// ============================================================================
// Download Helpers
// ============================================================================

function downloadBlob(content: string | ArrayBuffer, filename: string, mimeType: string): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.download = filename;
  link.href = url;
  link.click();
  URL.revokeObjectURL(url);
}

// ============================================================================
// Component
// ============================================================================

export function ExportDialog({ result, projectName, graphRef }: ExportDialogProps): JSX.Element | null {
  const isDarkMode = useIsDarkMode();

  const handleDownloadXlsx = useCallback(() => {
    if (!result) return;
    try {
      const workbook = generateXlsxWorkbook(result, projectName);
      XLSX.writeFile(workbook, generateFilename(projectName, 'xlsx'));
      toast.success('Excel export downloaded');
    } catch (err) {
      console.error('Failed to export Excel:', err);
      toast.error('Failed to export Excel');
    }
  }, [result, projectName]);

  const handleDownloadJson = useCallback(() => {
    if (!result) return;
    try {
      const data = generateStructuredJson(result, projectName);
      const jsonString = JSON.stringify(data, null, 2);
      downloadBlob(jsonString, generateFilename(projectName, 'json'), 'application/json');
      toast.success('JSON export downloaded');
    } catch (err) {
      console.error('Failed to export JSON:', err);
      toast.error('Failed to export JSON');
    }
  }, [result, projectName]);

  const handleDownloadCsv = useCallback(() => {
    if (!result) return;
    try {
      const csv = generateCsv(result);
      downloadBlob(csv, generateFilename(projectName, 'csv'), 'text/csv');
      toast.success('CSV export downloaded');
    } catch (err) {
      console.error('Failed to export CSV:', err);
      toast.error('Failed to export CSV');
    }
  }, [result, projectName]);

  const handleDownloadPng = useCallback(async () => {
    if (!graphRef?.current) {
      toast.error('Graph not available for export');
      return;
    }

    try {
      const backgroundColor = isDarkMode ? '#1e293b' : '#ffffff';
      const dataUrl = await toPng(graphRef.current, { backgroundColor });
      const link = document.createElement('a');
      link.download = generateFilename(projectName, 'png');
      link.href = dataUrl;
      link.click();
      toast.success('PNG export downloaded');
    } catch (err) {
      console.error('Failed to export image:', err);
      toast.error('Failed to export PNG');
    }
  }, [graphRef, projectName, isDarkMode]);

  const handleDownloadMermaid = useCallback(() => {
    if (!result) return;
    try {
      const content = generateAllMermaidDiagrams(result);
      downloadBlob(content, generateFilename(projectName, 'md'), 'text/markdown');
      toast.success('Mermaid export downloaded');
    } catch (err) {
      console.error('Failed to export Mermaid:', err);
      toast.error('Failed to export Mermaid');
    }
  }, [result, projectName]);

  const handleDownloadHtml = useCallback(() => {
    if (!result) return;
    try {
      const content = generateHtmlExport(result, projectName);
      downloadBlob(content, generateFilename(projectName, 'html'), 'text/html');
      toast.success('HTML export downloaded');
    } catch (err) {
      console.error('Failed to export HTML:', err);
      toast.error('Failed to export HTML');
    }
  }, [result, projectName]);

  if (!result) {
    return null;
  }

  return (
    <TooltipProvider>
      <DropdownMenu>
        <Tooltip>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon" className="h-8 w-8">
                <Download className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
          </TooltipTrigger>
          <TooltipContent>
            <p className="flex items-center gap-2">
              Export lineage data
              <kbd className="px-1.5 py-0.5 text-xs bg-muted rounded border font-mono">{getShortcutDisplay('export')}</kbd>
            </p>
          </TooltipContent>
        </Tooltip>
        <DropdownMenuContent align="end" className="w-52">
          <DropdownMenuLabel>Data Formats</DropdownMenuLabel>
          <DropdownMenuItem onClick={handleDownloadXlsx}>
            <FileSpreadsheet className="size-4 mr-2" />
            Excel (.xlsx)
          </DropdownMenuItem>
          <DropdownMenuItem onClick={handleDownloadJson}>
            <FileJson className="size-4 mr-2" />
            JSON
          </DropdownMenuItem>
          <DropdownMenuItem onClick={handleDownloadCsv}>
            <FileDown className="size-4 mr-2" />
            CSV (Column Mappings)
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuLabel>Visual Formats</DropdownMenuLabel>
          <DropdownMenuItem onClick={handleDownloadPng}>
            <Image className="size-4 mr-2" />
            PNG Image
          </DropdownMenuItem>
          <DropdownMenuItem onClick={handleDownloadMermaid}>
            <FileCode className="size-4 mr-2" />
            Mermaid (.md)
          </DropdownMenuItem>
          <DropdownMenuItem onClick={handleDownloadHtml}>
            <FileText className="size-4 mr-2" />
            HTML Report
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </TooltipProvider>
  );
}
