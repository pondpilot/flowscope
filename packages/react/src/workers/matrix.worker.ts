/**
 * Web Worker for matrix computation.
 * Builds table/script matrices and autocomplete data off the main thread.
 */
import type { StatementLineage } from '@pondpilot/flowscope-core';
import type { MatrixData, TableDependencyWithDetails, ScriptDependency, MatrixCellData } from '../utils/matrixUtils';
import {
  extractTableDependenciesWithDetails,
  extractScriptDependencies,
  extractAllColumnNames,
} from '../utils/matrixUtils';

export interface MatrixBuildRequest {
  type: 'build-matrix';
  requestId: string;
  statements: StatementLineage[];
  maxItems?: number;
}

export interface MatrixBuildResponse {
  type: 'build-result';
  requestId: string;
  tableMatrix: MatrixData;
  scriptMatrix: MatrixData;
  allColumnNames: string[];
  tableMetrics: MatrixMetrics;
  scriptMetrics: MatrixMetrics;
  tableItemCount: number;
  tableItemsRendered: number;
  scriptItemCount: number;
  scriptItemsRendered: number;
  error?: string;
}

export interface MatrixMetrics {
  rowCounts: Map<string, number>;
  colCounts: Map<string, number>;
  maxRow: number;
  maxCol: number;
  maxIntensity: number;
}

function computeMatrixMetrics(matrix: MatrixData, mode: 'tables' | 'scripts'): MatrixMetrics {
  const rowCounts = new Map<string, number>();
  const colCounts = new Map<string, number>();
  let maxRow = 0;
  let maxCol = 0;
  let maxIntensity = 1;

  for (const item of matrix.items) {
    rowCounts.set(item, 0);
    colCounts.set(item, 0);
  }

  for (const [rowId, rowCells] of matrix.cells) {
    for (const [colId, cell] of rowCells) {
      if (cell.type === 'write') {
        const rowCount = (rowCounts.get(rowId) || 0) + 1;
        rowCounts.set(rowId, rowCount);
        maxRow = Math.max(maxRow, rowCount);

        const colCount = (colCounts.get(colId) || 0) + 1;
        colCounts.set(colId, colCount);
        maxCol = Math.max(maxCol, colCount);
      }

      if (cell.type !== 'none' && cell.type !== 'self') {
        let intensity = 0;
        if (mode === 'tables') {
          intensity = (cell.details as { columnCount?: number } | undefined)?.columnCount || 0;
        } else {
          intensity = (cell.details as { sharedTables?: string[] } | undefined)?.sharedTables?.length || 0;
        }
        if (intensity > maxIntensity) {
          maxIntensity = intensity;
        }
      }
    }
  }

  return { rowCounts, colCounts, maxRow, maxCol, maxIntensity };
}

function buildTableMatrixWithItems(
  dependencies: TableDependencyWithDetails[],
  items: string[]
): MatrixData {
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

function buildScriptMatrixWithItems(
  dependencies: ScriptDependency[],
  items: string[]
): MatrixData {
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

function selectTopItems(
  items: string[],
  counts: Map<string, number>,
  maxItems: number
): { selected: string[]; rendered: number } {
  if (maxItems <= 0 || items.length <= maxItems) {
    return { selected: [...items].sort(), rendered: items.length };
  }

  const sortedByDegree = [...items].sort((a, b) => {
    const diff = (counts.get(b) || 0) - (counts.get(a) || 0);
    if (diff !== 0) return diff;
    return a.localeCompare(b);
  });

  const selected = sortedByDegree.slice(0, maxItems).sort();
  return { selected, rendered: selected.length };
}

console.log('[Matrix Worker] Worker initialized');

self.onmessage = (event: MessageEvent<MatrixBuildRequest>) => {
  const request = event.data;

  if (request.type !== 'build-matrix') {
    return;
  }

  const startTime = performance.now();
  const debug = !!(import.meta as { env?: { DEV?: boolean } }).env?.DEV;

  try {
    const maxItems = request.maxItems ?? 0;

    const tableDepsStart = performance.now();
    const tableDeps = extractTableDependenciesWithDetails(request.statements);
    const tableDepsMs = performance.now() - tableDepsStart;

    const tableCounts = new Map<string, number>();
    const tableItemsSet = new Set<string>();
    for (const dep of tableDeps) {
      tableItemsSet.add(dep.sourceTable);
      tableItemsSet.add(dep.targetTable);
      tableCounts.set(dep.sourceTable, (tableCounts.get(dep.sourceTable) || 0) + 1);
      tableCounts.set(dep.targetTable, (tableCounts.get(dep.targetTable) || 0) + 1);
    }
    const tableItemsAll = Array.from(tableItemsSet);
    const { selected: tableItems, rendered: tableItemsRendered } = selectTopItems(
      tableItemsAll,
      tableCounts,
      maxItems
    );
    const tableItemsSetSelected = new Set(tableItems);
    const limitedTableDeps = tableDeps.filter(
      (dep) => tableItemsSetSelected.has(dep.sourceTable) && tableItemsSetSelected.has(dep.targetTable)
    );

    const tableMatrixStart = performance.now();
    const tableMatrix = buildTableMatrixWithItems(limitedTableDeps, tableItems);
    const tableMatrixMs = performance.now() - tableMatrixStart;

    const tableMetricsStart = performance.now();
    const tableMetrics = computeMatrixMetrics(tableMatrix, 'tables');
    const tableMetricsMs = performance.now() - tableMetricsStart;

    const scriptDepsStart = performance.now();
    const scriptDeps = extractScriptDependencies(request.statements);
    const scriptDepsMs = performance.now() - scriptDepsStart;

    const scriptCounts = new Map<string, number>();
    for (const script of scriptDeps.allScripts) {
      scriptCounts.set(script, 0);
    }
    for (const dep of scriptDeps.dependencies) {
      scriptCounts.set(dep.sourceScript, (scriptCounts.get(dep.sourceScript) || 0) + 1);
      scriptCounts.set(dep.targetScript, (scriptCounts.get(dep.targetScript) || 0) + 1);
    }
    const { selected: scriptItems, rendered: scriptItemsRendered } = selectTopItems(
      scriptDeps.allScripts,
      scriptCounts,
      maxItems
    );
    const scriptItemsSetSelected = new Set(scriptItems);
    const limitedScriptDeps = scriptDeps.dependencies.filter(
      (dep) => scriptItemsSetSelected.has(dep.sourceScript) && scriptItemsSetSelected.has(dep.targetScript)
    );

    const scriptMatrixStart = performance.now();
    const scriptMatrix = buildScriptMatrixWithItems(limitedScriptDeps, scriptItems);
    const scriptMatrixMs = performance.now() - scriptMatrixStart;

    const scriptMetricsStart = performance.now();
    const scriptMetrics = computeMatrixMetrics(scriptMatrix, 'scripts');
    const scriptMetricsMs = performance.now() - scriptMetricsStart;

    const columnNamesStart = performance.now();
    const allColumnNames = extractAllColumnNames(request.statements);
    const columnNamesMs = performance.now() - columnNamesStart;

    const duration = performance.now() - startTime;
    if (debug) {
      console.log(
        `[Matrix Worker] tableDeps=${tableDeps.length}, tableItems=${tableItemsAll.length} -> ${tableItemsRendered} (${tableItemsRendered * tableItemsRendered} cells)`
      );
      console.log(
        `[Matrix Worker] scriptDeps=${scriptDeps.dependencies.length}, scriptItems=${scriptDeps.allScripts.length} -> ${scriptItemsRendered} (${scriptItemsRendered * scriptItemsRendered} cells)`
      );
      console.log(
        `[Matrix Worker] steps: tableDeps ${tableDepsMs.toFixed(1)}ms, tableMatrix ${tableMatrixMs.toFixed(1)}ms, tableMetrics ${tableMetricsMs.toFixed(1)}ms`
      );
      console.log(
        `[Matrix Worker] steps: scriptDeps ${scriptDepsMs.toFixed(1)}ms, scriptMatrix ${scriptMatrixMs.toFixed(1)}ms, scriptMetrics ${scriptMetricsMs.toFixed(1)}ms`
      );
      console.log(
        `[Matrix Worker] steps: columnNames ${columnNamesMs.toFixed(1)}ms`
      );
    }

    console.log(`[Matrix Worker] Build completed in ${duration.toFixed(2)}ms`);

    const response: MatrixBuildResponse = {
      type: 'build-result',
      requestId: request.requestId,
      tableMatrix,
      scriptMatrix,
      allColumnNames,
      tableMetrics,
      scriptMetrics,
      tableItemCount: tableItemsAll.length,
      tableItemsRendered,
      scriptItemCount: scriptDeps.allScripts.length,
      scriptItemsRendered,
    };

    self.postMessage(response);
  } catch (error) {
    console.error('[Matrix Worker] Error:', error);
    const response: MatrixBuildResponse = {
      type: 'build-result',
      requestId: request.requestId,
      tableMatrix: { items: [], cells: new Map() },
      scriptMatrix: { items: [], cells: new Map() },
      allColumnNames: [],
      tableMetrics: { rowCounts: new Map(), colCounts: new Map(), maxRow: 0, maxCol: 0, maxIntensity: 1 },
      scriptMetrics: { rowCounts: new Map(), colCounts: new Map(), maxRow: 0, maxCol: 0, maxIntensity: 1 },
      tableItemCount: 0,
      tableItemsRendered: 0,
      scriptItemCount: 0,
      scriptItemsRendered: 0,
      error: error instanceof Error ? error.message : 'Unknown error',
    };
    self.postMessage(response);
  }
};
