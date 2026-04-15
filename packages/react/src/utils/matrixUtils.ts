/**
 * Matrix view utilities for extracting table and script dependencies.
 * These functions analyze lineage data to build dependency matrices.
 */

import type { AnalyzeResult, Span } from '@pondpilot/flowscope-core';
import { isTableLikeType, nodesInStatement, edgesInStatement } from '@pondpilot/flowscope-core';
import {
  getCreatedRelationNodeIds,
  OUTPUT_NODE_TYPE,
  JOIN_DEPENDENCY_EDGE_TYPE,
  buildColumnOwnershipMap,
} from './lineageHelpers';
import { getOccurrenceForStatement } from './nodeOccurrences';

// ============================================================================
// Types
// ============================================================================

export interface TableDependencyWithDetails {
  sourceTable: string;
  targetTable: string;
  columnCount: number;
  columns: Array<{ source: string; target: string; expression?: string }>;
  spans: Span[];
  locations: Array<{ span: Span; sourceName?: string; statementIndex: number }>;
}

export interface ScriptDependency {
  sourceScript: string;
  targetScript: string;
  sharedTables: string[];
}

export interface MatrixCellData {
  type: 'self' | 'write' | 'read' | 'none';
  details?: TableDependencyWithDetails | ScriptDependency;
}

export interface MatrixData {
  items: string[];
  cells: Map<string, Map<string, MatrixCellData>>;
}

// ============================================================================
// Data Extraction
// ============================================================================

/**
 * Extracts all unique column names from the lineage graph.
 * Used for search autocomplete.
 */
export function extractAllColumnNames(result: AnalyzeResult): string[] {
  const columnNames = new Set<string>();
  for (const node of result.nodes) {
    if (node.type === 'column') {
      columnNames.add(node.label);
    }
  }
  return Array.from(columnNames).sort();
}

/**
 * Extracts table-to-table dependencies with column-level details from lineage statements.
 * Tracks which columns flow between tables and captures source spans for navigation.
 */
export function extractTableDependenciesWithDetails(
  result: AnalyzeResult
): TableDependencyWithDetails[] {
  const depMap = new Map<string, TableDependencyWithDetails>();

  for (const stmt of result.statements) {
    const stmtNodes = nodesInStatement(result, stmt.statementIndex);
    const stmtEdges = edgesInStatement(result, stmt.statementIndex);
    const tableNodes = stmtNodes.filter((n) => isTableLikeType(n.type));
    const outputNodes = stmtNodes.filter((n) => n.type === OUTPUT_NODE_TYPE);
    const relationNodes = [...tableNodes, ...outputNodes];
    const columnNodes = stmtNodes.filter((n) => n.type === 'column');

    const columnToTable = buildColumnOwnershipMap(
      stmtEdges,
      relationNodes,
      (n) => n.qualifiedName || n.label
    );

    for (const edge of stmtEdges) {
      if (edge.type === 'data_flow' || edge.type === JOIN_DEPENDENCY_EDGE_TYPE) {
        const sourceNode = relationNodes.find((n) => n.id === edge.from);
        const targetNode = relationNodes.find((n) => n.id === edge.to);

        if (sourceNode && targetNode) {
          const sourceKey = sourceNode.qualifiedName || sourceNode.label;
          const targetKey = targetNode.qualifiedName || targetNode.label;
          const depKey = `${sourceKey}->${targetKey}`;

          if (sourceKey !== targetKey) {
            if (!depMap.has(depKey)) {
              depMap.set(depKey, {
                sourceTable: sourceKey,
                targetTable: targetKey,
                columnCount: 0,
                columns: [],
                spans: [],
                locations: [],
              });
            }
            const dep = depMap.get(depKey)!;
            const occurrences = getOccurrenceForStatement(sourceNode, stmt.statementIndex);
            if (occurrences.spans.length > 0) {
              occurrences.spans.forEach((span, index) => {
                dep.spans.push(span);
                dep.locations.push({
                  span,
                  sourceName: occurrences.sourceNames[index] ?? undefined,
                  statementIndex: stmt.statementIndex,
                });
              });
            } else if (sourceNode.span) {
              dep.spans.push(sourceNode.span);
              dep.locations.push({ span: sourceNode.span, statementIndex: stmt.statementIndex });
            }
          }
        }
      }

      if (edge.type === 'derivation' || edge.type === 'data_flow') {
        const sourceCol = columnNodes.find((c) => c.id === edge.from);
        const targetCol = columnNodes.find((c) => c.id === edge.to);

        if (sourceCol && targetCol) {
          const sourceTable = columnToTable.get(edge.from);
          const targetTable = columnToTable.get(edge.to);

          if (sourceTable && targetTable && sourceTable !== targetTable) {
            const depKey = `${sourceTable}->${targetTable}`;
            if (!depMap.has(depKey)) {
              depMap.set(depKey, {
                sourceTable,
                targetTable,
                columnCount: 0,
                columns: [],
                spans: [],
                locations: [],
              });
            }
            const dep = depMap.get(depKey)!;
            dep.columnCount++;
            dep.columns.push({
              source: sourceCol.label,
              target: targetCol.label,
              expression: edge.expression || targetCol.expression,
            });
          }
        }
      }
    }
  }

  return Array.from(depMap.values());
}

export interface ScriptDependencyResult {
  dependencies: ScriptDependency[];
  allScripts: string[];
}

/**
 * Extracts script-to-script dependencies based on shared tables.
 * A dependency exists when one script writes to a table that another script reads.
 * Returns both dependencies and all script names (for showing scripts with no dependencies).
 */
export function extractScriptDependencies(result: AnalyzeResult): ScriptDependencyResult {
  const scriptMap = new Map<string, { tablesRead: Set<string>; tablesWritten: Set<string> }>();

  for (const stmt of result.statements) {
    const sourceName = stmt.sourceName || 'default';
    if (!scriptMap.has(sourceName)) {
      scriptMap.set(sourceName, { tablesRead: new Set(), tablesWritten: new Set() });
    }
    const scriptData = scriptMap.get(sourceName)!;

    const stmtNodes = nodesInStatement(result, stmt.statementIndex);
    const stmtEdges = edgesInStatement(result, stmt.statementIndex);
    const tableNodes = stmtNodes.filter((n) => n.type === 'table' || n.type === 'view');
    const outputNodes = stmtNodes.filter((n) => n.type === OUTPUT_NODE_TYPE);
    const createdRelationIds = getCreatedRelationNodeIds(stmt.statementType, stmtNodes, stmtEdges);

    for (const node of outputNodes) {
      scriptData.tablesWritten.add(node.qualifiedName || node.label);
    }

    for (const node of tableNodes) {
      const tableName = node.qualifiedName || node.label;
      const isWritten =
        stmtEdges.some((e) => e.to === node.id && e.type === 'data_flow') ||
        createdRelationIds.has(node.id);
      const isRead = stmtEdges.some((e) => e.from === node.id && e.type === 'data_flow');

      if (isWritten) {
        scriptData.tablesWritten.add(tableName);
      }
      if (isRead || (!isWritten && !isRead)) {
        scriptData.tablesRead.add(tableName);
      }
    }
  }

  const dependencies: ScriptDependency[] = [];
  const allScripts = Array.from(scriptMap.keys());

  for (const producerScript of allScripts) {
    const producer = scriptMap.get(producerScript)!;
    for (const consumerScript of allScripts) {
      if (producerScript === consumerScript) continue;
      const consumer = scriptMap.get(consumerScript)!;

      const sharedTables = Array.from(producer.tablesWritten).filter((t) =>
        consumer.tablesRead.has(t)
      );

      if (sharedTables.length > 0) {
        dependencies.push({
          sourceScript: producerScript,
          targetScript: consumerScript,
          sharedTables,
        });
      }
    }
  }

  return { dependencies, allScripts };
}

// ============================================================================
// Matrix Building
// ============================================================================

/**
 * Builds a matrix data structure from table dependencies.
 */
export function buildTableMatrix(dependencies: TableDependencyWithDetails[]): MatrixData {
  const allTables = new Set<string>();
  for (const dep of dependencies) {
    allTables.add(dep.sourceTable);
    allTables.add(dep.targetTable);
  }
  const items = Array.from(allTables).sort();

  const depLookup = new Map<string, TableDependencyWithDetails>();
  for (const dep of dependencies) {
    depLookup.set(`${dep.sourceTable}->${dep.targetTable}`, dep);
  }

  const cells = new Map<string, Map<string, MatrixCellData>>();
  for (const rowItem of items) {
    const row = new Map<string, MatrixCellData>();
    for (const colItem of items) {
      if (rowItem === colItem) {
        row.set(colItem, { type: 'self' });
      } else {
        const writeKey = `${rowItem}->${colItem}`;
        const readKey = `${colItem}->${rowItem}`;

        if (depLookup.has(writeKey)) {
          row.set(colItem, { type: 'write', details: depLookup.get(writeKey) });
        } else if (depLookup.has(readKey)) {
          row.set(colItem, { type: 'read', details: depLookup.get(readKey) });
        } else {
          row.set(colItem, { type: 'none' });
        }
      }
    }
    cells.set(rowItem, row);
  }

  return { items, cells };
}

/**
 * Builds a matrix data structure from script dependencies.
 * Takes allScripts to include scripts with no dependencies in the matrix.
 */
export function buildScriptMatrix(
  dependencies: ScriptDependency[],
  allScripts: string[]
): MatrixData {
  const items = [...allScripts].sort();

  const depLookup = new Map<string, ScriptDependency>();
  for (const dep of dependencies) {
    depLookup.set(`${dep.sourceScript}->${dep.targetScript}`, dep);
  }

  const cells = new Map<string, Map<string, MatrixCellData>>();
  for (const rowItem of items) {
    const row = new Map<string, MatrixCellData>();
    for (const colItem of items) {
      if (rowItem === colItem) {
        row.set(colItem, { type: 'self' });
      } else {
        const writeKey = `${rowItem}->${colItem}`;
        const readKey = `${colItem}->${rowItem}`;

        if (depLookup.has(writeKey)) {
          row.set(colItem, { type: 'write', details: depLookup.get(writeKey) });
        } else if (depLookup.has(readKey)) {
          row.set(colItem, { type: 'read', details: depLookup.get(readKey) });
        } else {
          row.set(colItem, { type: 'none' });
        }
      }
    }
    cells.set(rowItem, row);
  }

  return { items, cells };
}
