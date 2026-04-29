import type { AnalyzeResult } from '@pondpilot/flowscope-core';

import type { ChatReference } from '@/features/librarian/utils/schema-identifiers';

export interface LineageNodeResolution {
  /** Node IDs in the lineage graph that should be highlighted. */
  nodeIds: string[];
  /** Table-like node IDs whose owning columns are in `nodeIds` and which therefore need to be expanded so the columns are visible. */
  tablesToExpand: string[];
}

interface GlobalNodeLike {
  id: string;
  type: string;
  label: string;
  canonicalName?: {
    catalog?: string;
    schema?: string;
    name?: string;
    column?: string;
  };
}

function normalize(value: string | undefined | null): string {
  return (value ?? '').toLowerCase();
}

function isTableLike(type: string): boolean {
  return type === 'table' || type === 'view' || type === 'cte';
}

function buildQualifiedName(node: GlobalNodeLike): string {
  if (!node.canonicalName) return node.label;
  const { catalog, schema, name, column } = node.canonicalName;
  const parts = [catalog, schema, name, column].filter(
    (p): p is string => typeof p === 'string' && p.length > 0
  );
  return parts.length > 0 ? parts.join('.') : node.label;
}

function matchesTable(node: GlobalNodeLike, tableName: string): boolean {
  if (!isTableLike(node.type)) return false;
  const target = normalize(tableName);
  if (!target) return false;
  if (normalize(node.label) === target) return true;
  if (normalize(node.canonicalName?.name) === target) return true;
  if (normalize(buildQualifiedName(node)) === target) return true;
  return false;
}

function matchesQualifiedColumn(
  node: GlobalNodeLike,
  tableName: string,
  columnName: string
): boolean {
  if (node.type !== 'column') return false;
  const target = `${normalize(tableName)}.${normalize(columnName)}`;
  const qualified = normalize(buildQualifiedName(node));
  if (qualified === target) return true;
  // Suffix match handles full canonical names like catalog.schema.tableName.columnName.
  if (qualified.endsWith(`.${target}`)) return true;
  // Fallback: parent table canonical name + column label.
  if (
    normalize(node.canonicalName?.name) === normalize(tableName) &&
    (normalize(node.label) === normalize(columnName) ||
      normalize(node.canonicalName?.column) === normalize(columnName))
  ) {
    return true;
  }
  return false;
}

function matchesBareColumn(node: GlobalNodeLike, columnName: string): boolean {
  if (node.type !== 'column') return false;
  const target = normalize(columnName);
  if (!target) return false;
  if (normalize(node.label) === target) return true;
  if (normalize(node.canonicalName?.column) === target) return true;
  return false;
}

function findParentTableId(
  columnNode: GlobalNodeLike,
  tableIdsByName: Map<string, string>
): string | null {
  const canonicalName = normalize(columnNode.canonicalName?.name);
  if (canonicalName) {
    const id = tableIdsByName.get(canonicalName);
    if (id) return id;
  }
  return null;
}

/**
 * Resolve a list of `ChatReference`s into concrete lineage node IDs.
 *
 * - Table refs map to every table-like global node whose label/canonical name matches.
 * - Qualified column refs map to the column node whose qualified name equals
 *   `${tableName}.${columnName}` (case-insensitive); a fallback compares the
 *   parent table's canonical name and the column's label.
 * - Bare column refs map to every column node with the matching label, and the
 *   parent table IDs are added to `tablesToExpand` so callers can ensure the
 *   columns become visible.
 *
 * Refs with zero matches are skipped silently. Returned IDs are deduplicated
 * while preserving first-seen order.
 */
export function resolveLineageNodeIds(
  result: AnalyzeResult | null,
  refs: ChatReference[]
): LineageNodeResolution {
  const nodeIds: string[] = [];
  const tablesToExpand: string[] = [];
  const seenIds = new Set<string>();
  const seenExpand = new Set<string>();

  if (!result || refs.length === 0) {
    return { nodeIds, tablesToExpand };
  }

  const allNodes = (result.globalLineage?.nodes ?? []) as unknown as GlobalNodeLike[];

  // Index table-like nodes by lowercased label and canonical name so column
  // nodes can resolve their owning table id.
  const tableIdsByName = new Map<string, string>();
  for (const node of allNodes) {
    if (!isTableLike(node.type)) continue;
    const labelKey = normalize(node.label);
    if (labelKey && !tableIdsByName.has(labelKey)) tableIdsByName.set(labelKey, node.id);
    const canonicalKey = normalize(node.canonicalName?.name);
    if (canonicalKey && !tableIdsByName.has(canonicalKey)) {
      tableIdsByName.set(canonicalKey, node.id);
    }
  }

  const addNode = (id: string) => {
    if (!id || seenIds.has(id)) return;
    seenIds.add(id);
    nodeIds.push(id);
  };
  const addExpand = (id: string) => {
    if (!id || seenExpand.has(id)) return;
    seenExpand.add(id);
    tablesToExpand.push(id);
  };

  for (const ref of refs) {
    if (ref.tableName && ref.columnName) {
      const matches = allNodes.filter((n) =>
        matchesQualifiedColumn(n, ref.tableName!, ref.columnName!)
      );
      if (matches.length === 0) continue;
      for (const match of matches) {
        addNode(match.id);
        const parentId = findParentTableId(match, tableIdsByName);
        if (parentId) addExpand(parentId);
      }
      continue;
    }

    if (ref.tableName) {
      const matches = allNodes.filter((n) => matchesTable(n, ref.tableName!));
      if (matches.length === 0) continue;
      for (const match of matches) addNode(match.id);
      continue;
    }

    if (ref.columnName) {
      const matches = allNodes.filter((n) => matchesBareColumn(n, ref.columnName!));
      if (matches.length === 0) continue;
      for (const match of matches) {
        addNode(match.id);
        const parentId = findParentTableId(match, tableIdsByName);
        if (parentId) addExpand(parentId);
      }
    }
  }

  return { nodeIds, tablesToExpand };
}
