import type { AnalyzeResult } from '@pondpilot/flowscope-core';

import type { ChatReference } from '@/features/librarian/utils/schema-identifiers';

export interface LineageNodeResolution {
  /** Node IDs in the lineage graph that should be highlighted. */
  nodeIds: string[];
  /** Table-like node IDs whose owning columns are in `nodeIds` and which therefore need to be expanded so the columns are visible. */
  tablesToExpand: string[];
  /**
   * Top-level node ID best suited to recenter the viewport on. Columns are not
   * top-level ReactFlow nodes (they render inside table nodes), so passing a
   * column id to `useNodeFocus` results in a no-op. For column refs we expose
   * the parent table id here; callers should prefer this over `nodeIds[0]` for
   * fitView/focus calls. Falls back to the first node id when no parent table
   * could be resolved.
   */
  primaryFocusId: string | null;
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

function buildTableQualifiedKey(canonicalName: GlobalNodeLike['canonicalName']): string | null {
  if (!canonicalName?.name) return null;
  const { catalog, schema, name } = canonicalName;
  const parts = [catalog, schema, name].filter(
    (p): p is string => typeof p === 'string' && p.length > 0
  );
  return parts.join('.').toLowerCase();
}

interface ColumnIdentity {
  /** The column's own name, or undefined if it cannot be derived. */
  columnName: string | undefined;
  /** The owning table's name (bare), or undefined if it cannot be derived. */
  parentName: string | undefined;
  /** Lowercased qualified key for the parent table (catalog.schema.name). */
  parentQualifiedKey: string | null;
}

const EMPTY_COLUMN_IDENTITY: ColumnIdentity = {
  columnName: undefined,
  parentName: undefined,
  parentQualifiedKey: null,
};

// Resolve a column node's identity in one place. Different emitters pack the
// column into either `canonicalName.column` (with the table in `.name`) or
// the trailing `canonicalName.name` (with the table in `.schema`). Picking
// the right split — and the matching parent-table parts — was previously
// duplicated across three helpers; co-locating them here keeps the rule a
// single fact instead of three independent opinions.
function splitColumnIdentity(node: GlobalNodeLike): ColumnIdentity {
  if (node.type !== 'column' || !node.canonicalName) {
    return EMPTY_COLUMN_IDENTITY;
  }
  const { catalog, schema, name, column } = node.canonicalName;
  const columnName = column ?? name;
  const parentName = column ? name : schema;
  const parentParts = (column ? [catalog, schema, name] : [catalog, schema]).filter(
    (p): p is string => typeof p === 'string' && p.length > 0
  );
  const parentQualifiedKey = parentParts.length > 0 ? parentParts.join('.').toLowerCase() : null;
  return { columnName, parentName, parentQualifiedKey };
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
  const identity = splitColumnIdentity(node);
  if (
    normalize(identity.parentName) === normalize(tableName) &&
    (normalize(node.label) === normalize(columnName) ||
      normalize(identity.columnName) === normalize(columnName))
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
  if (normalize(splitColumnIdentity(node).columnName) === target) return true;
  return false;
}

function findParentTableId(
  columnNode: GlobalNodeLike,
  tableIdsByQualifiedName: Map<string, string>,
  tableIdsByName: Map<string, string>
): string | null {
  // Prefer fully-qualified lookup (catalog.schema.name) so columns map to
  // their actual owning table when multiple table-like nodes share the same
  // bare name across schemas.
  const identity = splitColumnIdentity(columnNode);
  if (identity.parentQualifiedKey) {
    const id = tableIdsByQualifiedName.get(identity.parentQualifiedKey);
    if (id) return id;
  }
  const bareKey = normalize(identity.parentName);
  if (bareKey) {
    const id = tableIdsByName.get(bareKey);
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
  let primaryFocusId: string | null = null;

  if (!result || refs.length === 0) {
    return { nodeIds, tablesToExpand, primaryFocusId };
  }

  const allNodes = (result.nodes ?? []) as unknown as GlobalNodeLike[];

  // Index table-like nodes so column nodes can resolve their owning table id.
  // The qualified map (catalog.schema.name) keeps schemas with duplicate
  // table names distinct; the bare-name map is the fallback when a column
  // has no schema/catalog on its canonicalName.
  const tableIdsByQualifiedName = new Map<string, string>();
  const tableIdsByName = new Map<string, string>();
  // Type per table-like node id. Used to prefer real source tables over
  // views / CTEs when a bare column name matches column nodes in both: the
  // analyzer can attach transitive column-lineage nodes to view nodes even
  // when the column is not selected into the view's output.
  const tableTypeById = new Map<string, string>();
  for (const node of allNodes) {
    if (!isTableLike(node.type)) continue;
    tableTypeById.set(node.id, node.type);
    const qualifiedKey = buildTableQualifiedKey(node.canonicalName);
    if (qualifiedKey && !tableIdsByQualifiedName.has(qualifiedKey)) {
      tableIdsByQualifiedName.set(qualifiedKey, node.id);
    }
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

  // Resolve a column match to its parent table id, falling back to the ref's
  // explicit tableName lookup when the column's own canonicalName lacks a
  // useful pointer to its owning table. Returning null means the parent
  // could not be identified — callers should not promote the column id to
  // primaryFocusId in that case, since column ids are not top-level
  // ReactFlow nodes and would make `revealNodeInGraph` a no-op.
  const resolveColumnParentId = (
    columnNode: GlobalNodeLike,
    refTableName: string | undefined
  ): string | null => {
    const direct = findParentTableId(columnNode, tableIdsByQualifiedName, tableIdsByName);
    if (direct) return direct;
    if (refTableName) {
      const fallback = tableIdsByName.get(normalize(refTableName));
      if (fallback) return fallback;
    }
    return null;
  };

  // Rank a column match by the type of its resolved parent. Real source
  // tables come first so primaryFocusId lands on a base table when one
  // exists, instead of a view that just references the column transitively.
  const parentTypeRank = (columnNode: GlobalNodeLike, refTableName: string | undefined): number => {
    const parentId = resolveColumnParentId(columnNode, refTableName);
    if (!parentId) return 4;
    const type = tableTypeById.get(parentId);
    if (type === 'table') return 0;
    if (type === 'view') return 1;
    if (type === 'cte') return 2;
    return 3;
  };

  for (const ref of refs) {
    if (ref.tableName && ref.columnName) {
      const matches = allNodes.filter((n) =>
        matchesQualifiedColumn(n, ref.tableName!, ref.columnName!)
      );
      if (matches.length === 0) continue;
      const sorted = [...matches].sort(
        (a, b) => parentTypeRank(a, ref.tableName) - parentTypeRank(b, ref.tableName)
      );
      for (const match of sorted) {
        addNode(match.id);
        const parentId = resolveColumnParentId(match, ref.tableName);
        if (parentId) addExpand(parentId);
        if (primaryFocusId === null) primaryFocusId = parentId ?? match.id;
      }
      continue;
    }

    if (ref.tableName) {
      const matches = allNodes.filter((n) => matchesTable(n, ref.tableName!));
      if (matches.length === 0) continue;
      for (const match of matches) {
        addNode(match.id);
        if (primaryFocusId === null) primaryFocusId = match.id;
      }
      continue;
    }

    if (ref.columnName) {
      const matches = allNodes.filter((n) => matchesBareColumn(n, ref.columnName!));
      if (matches.length === 0) continue;
      const sorted = [...matches].sort(
        (a, b) => parentTypeRank(a, undefined) - parentTypeRank(b, undefined)
      );
      for (const match of sorted) {
        addNode(match.id);
        const parentId = resolveColumnParentId(match, undefined);
        if (parentId) addExpand(parentId);
        if (primaryFocusId === null) primaryFocusId = parentId ?? match.id;
      }
    }
  }

  return { nodeIds, tablesToExpand, primaryFocusId };
}
