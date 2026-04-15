/**
 * Web Worker for graph building computation.
 * Runs buildFlowNodes and buildFlowEdges off the main thread to prevent UI blocking.
 *
 * This worker handles the CPU-intensive task of transforming lineage data into
 * React Flow nodes and edges, which can take several seconds for large SQL files.
 */
import type {
  AnalyzeResult,
  Node,
  Edge,
  ResolvedSchemaMetadata,
  StatementMeta,
  FilterPredicate,
  AggregationInfo,
} from '@pondpilot/flowscope-core';
import { isTableLikeType, nodesInStatement, edgesInStatement } from '@pondpilot/flowscope-core';
import { GRAPH_CONFIG } from '../constants';
import {
  buildJoinedTableIds,
  formatJoinType,
  getCreatedRelationNodeIds,
  groupOutputColumns,
  resolveOutputMapping,
  edgePairKey,
  syntheticEdgeId,
  isNodeHighlighted,
  createStatementScope,
  withStatementScope,
} from '../utils/lineageHelpers';
import { mergeNodesForNavigation, scopeNodeToStatement } from '../utils/nodeOccurrences';

// =============================================================================
// Types for worker communication (all serializable - no Sets, no functions)
// =============================================================================

/**
 * Column information for serialization.
 */
export interface SerializedColumnInfo {
  id: string;
  name: string;
  expression?: string;
  isHighlighted?: boolean;
  sourceName?: string;
  aggregation?: AggregationInfo;
}

/**
 * Serializable node data for table nodes.
 * Extends Record<string, unknown> for React Flow compatibility.
 */
export interface SerializedTableNodeData extends Record<string, unknown> {
  label: string;
  nodeType: 'table' | 'view' | 'cte' | 'virtualOutput';
  isRecursive?: boolean;
  columns: SerializedColumnInfo[];
  isSelected: boolean;
  isCollapsed: boolean;
  isHighlighted: boolean;
  isBaseTable?: boolean;
  sourceName?: string;
  hiddenColumnCount?: number;
  filters?: FilterPredicate[];
  qualifiedName?: string;
  schema?: string;
  database?: string;
}

/**
 * Serializable node data for script nodes.
 * Extends Record<string, unknown> for React Flow compatibility.
 */
export interface SerializedScriptNodeData extends Record<string, unknown> {
  label: string;
  sourceName: string;
  tablesRead: string[];
  tablesWritten: string[];
  statementCount: number;
  isSelected: boolean;
  isHighlighted: boolean;
}

/**
 * Serializable Flow Node.
 */
export interface SerializedFlowNode {
  id: string;
  type: string;
  position: { x: number; y: number };
  data: SerializedTableNodeData | SerializedScriptNodeData;
}

/**
 * Serializable edge data.
 * Extends Record<string, unknown> for React Flow compatibility.
 */
export interface SerializedEdgeData extends Record<string, unknown> {
  type?: string;
  joinType?: string;
  joinCondition?: string;
  expression?: string;
  sourceColumn?: string;
  targetColumn?: string;
  isDerived?: boolean;
  isHighlighted?: boolean;
}

/**
 * Serializable Flow Edge.
 */
export interface SerializedFlowEdge {
  id: string;
  source: string;
  target: string;
  sourceHandle?: string;
  targetHandle?: string;
  type?: string;
  label?: string;
  animated?: boolean;
  zIndex?: number;
  data?: SerializedEdgeData;
  style?: { strokeDasharray?: string };
}

/**
 * Request message to the worker for table view graph building.
 */
export interface GraphBuildRequest {
  type: 'build-table-graph';
  requestId: string;
  result: AnalyzeResult;
  selectedNodeId: string | null;
  searchTerm: string;
  collapsedNodeIds: string[]; // Array instead of Set for serialization
  expandedTableIds: string[]; // Array instead of Set for serialization
  resolvedSchema: ResolvedSchemaMetadata | null;
  defaultCollapsed: boolean;
  showColumnEdges: boolean;
}

/**
 * Request message for script view graph building.
 */
export interface ScriptGraphBuildRequest {
  type: 'build-script-graph';
  requestId: string;
  result: AnalyzeResult;
  selectedNodeId: string | null;
  searchTerm: string;
  showTables: boolean;
}

/**
 * Response message from the worker.
 */
export interface GraphBuildResponse {
  type: 'build-result';
  requestId: string;
  nodes: SerializedFlowNode[];
  edges: SerializedFlowEdge[];
  lineageNodes?: Node[];
  error?: string;
}

// =============================================================================
// Constants and helpers (copied from graphBuilders.ts to avoid import issues)
// =============================================================================

const SELECT_STATEMENT_TYPES = new Set([
  'SELECT',
  'WITH',
  'UNION',
  'INTERSECT',
  'EXCEPT',
  'VALUES',
]);

const OUTPUT_NODE_TYPE = 'output';
const JOIN_DEPENDENCY_EDGE_TYPE = 'join_dependency';

/**
 * Determine if a node should be collapsed based on the default state and overrides.
 */
function computeIsCollapsed(
  nodeId: string,
  defaultCollapsed: boolean,
  overrideIds: Set<string>
): boolean {
  return defaultCollapsed ? !overrideIds.has(nodeId) : overrideIds.has(nodeId);
}

/**
 * Helper to find table in resolved schema by matching label/qualified name
 */
function findSchemaTable(
  tableLabel: string,
  qualifiedName: string | undefined,
  resolvedSchema: ResolvedSchemaMetadata | null | undefined
) {
  if (!resolvedSchema?.tables) return null;

  if (qualifiedName) {
    const table = resolvedSchema.tables.find((t) => {
      const schemaQualified = [t.catalog, t.schema, t.name].filter(Boolean).join('.');
      return schemaQualified === qualifiedName;
    });
    if (table) return table;
  }

  const table = resolvedSchema.tables.find((t) => t.name === tableLabel);
  return table || null;
}

/**
 * Process table columns by injecting missing schema columns when expanded.
 */
function processTableColumns(
  tableLabel: string,
  qualifiedName: string | undefined,
  nodeId: string,
  existingColumns: SerializedColumnInfo[],
  isExpanded: boolean,
  resolvedSchema: ResolvedSchemaMetadata | null | undefined
): { columns: SerializedColumnInfo[]; hiddenColumnCount: number } {
  const schemaTable = findSchemaTable(tableLabel, qualifiedName, resolvedSchema);

  if (!schemaTable) {
    return { columns: existingColumns, hiddenColumnCount: 0 };
  }

  const existingColumnNames = new Set(existingColumns.map((col) => col.name.toLowerCase()));
  const schemaColumns = schemaTable.columns || [];
  const missingColumns = schemaColumns.filter(
    (col) => !existingColumnNames.has(col.name.toLowerCase())
  );

  const hiddenColumnCount = missingColumns.length;

  if (isExpanded && missingColumns.length > 0) {
    const injectedColumns: SerializedColumnInfo[] = missingColumns.map((col) => ({
      id: `${nodeId}__schema_${col.name}`,
      name: col.name,
      expression: col.dataType,
    }));
    return {
      columns: [...existingColumns, ...injectedColumns],
      hiddenColumnCount,
    };
  }

  return { columns: existingColumns, hiddenColumnCount };
}

/**
 * Merged lineage view used by the worker's graph builders. Operates over the
 * union of nodes/edges across all statements in an `AnalyzeResult` while
 * preserving per-statement scope metadata required by output resolution.
 */
interface MergedLineage {
  nodes: Node[];
  edges: Edge[];
  isSelect: boolean;
}

function mergeAnalyzeResult(result: AnalyzeResult): MergedLineage {
  const mergedNodes = new Map<string, Node>();
  const mergedEdges = new Map<string, Edge>();
  let anySelect = false;

  for (const stmt of result.statements) {
    if (SELECT_STATEMENT_TYPES.has((stmt.statementType || '').toUpperCase())) {
      anySelect = true;
    }
    const sourceName = stmt.sourceName;
    const statementScope = createStatementScope(stmt.statementIndex, sourceName);

    for (const node of nodesInStatement(result, stmt.statementIndex)) {
      const scopedNode = scopeNodeToStatement(node, stmt.statementIndex, sourceName);
      const nodeWithSource = sourceName
        ? {
            ...scopedNode,
            metadata: {
              ...(scopedNode.metadata || {}),
              sourceName,
            },
          }
        : scopedNode;
      const nodeWithScope =
        scopedNode.statementIds.length === 1
          ? withStatementScope(nodeWithSource, statementScope)
          : nodeWithSource;
      const mergedNode = mergeNodesForNavigation(
        mergedNodes.get(node.id) ?? null,
        nodeWithScope,
        sourceName
      );
      mergedNodes.set(node.id, mergedNode);
    }

    for (const edge of edgesInStatement(result, stmt.statementIndex)) {
      if (!mergedEdges.has(edge.id)) {
        mergedEdges.set(
          edge.id,
          edge.statementIds.length === 1 ? withStatementScope(edge, statementScope) : edge
        );
      }
    }
  }

  return {
    nodes: Array.from(mergedNodes.values()),
    edges: Array.from(mergedEdges.values()),
    isSelect: anySelect,
  };
}

/**
 * Build column ownership map: column ID -> table ID.
 */
function buildColumnOwnershipMap(
  edges: Edge[],
  tableNodes: Node[],
  getNodeId: (n: Node) => string
): Map<string, string> {
  const columnToTableMap = new Map<string, string>();
  const tableIds = new Set(tableNodes.map(getNodeId));

  for (const edge of edges) {
    if (edge.type === 'ownership' && tableIds.has(edge.from)) {
      columnToTableMap.set(edge.to, edge.from);
    }
  }

  return columnToTableMap;
}

// =============================================================================
// Graph Building Functions
// =============================================================================

interface NodeBuilderOptions {
  selectedNodeId: string | null;
  searchTerm: string;
  isCollapsed: boolean;
}

interface TableNodeBuilderOptions extends NodeBuilderOptions {
  hiddenColumnCount?: number;
  isRecursive?: boolean;
  isBaseTable?: boolean;
}

/**
 * Build TableNodeData for a table/CTE node.
 */
function buildTableNodeData(
  node: Node,
  columns: SerializedColumnInfo[],
  options: TableNodeBuilderOptions
): SerializedTableNodeData {
  let nodeType: 'table' | 'view' | 'cte' | 'virtualOutput' = 'table';
  if (node.type === 'cte') {
    nodeType = 'cte';
  } else if (node.type === 'view') {
    nodeType = 'view';
  }

  // Canonical info is carried on the node itself in the flat model.
  const canonical = node.canonicalName;

  const qualifiedName = canonical
    ? [canonical.catalog, canonical.schema, canonical.name].filter(Boolean).join('.')
    : node.label;

  return {
    label: node.label,
    nodeType,
    columns,
    isSelected: node.id === options.selectedNodeId,
    isHighlighted: isNodeHighlighted(options.searchTerm, columns, node.label),
    isCollapsed: options.isCollapsed,
    hiddenColumnCount: options.hiddenColumnCount,
    isRecursive: options.isRecursive,
    isBaseTable: options.isBaseTable,
    filters: node.filters,
    qualifiedName,
    schema: canonical?.schema,
    database: canonical?.catalog,
  };
}

/**
 * Build TableNodeData for the virtual Output node.
 */
function buildOutputNodeData(
  nodeId: string,
  label: string,
  outputColumns: SerializedColumnInfo[],
  options: NodeBuilderOptions
): SerializedTableNodeData {
  return {
    label,
    nodeType: 'virtualOutput',
    columns: outputColumns,
    isSelected: nodeId === options.selectedNodeId,
    isHighlighted: isNodeHighlighted(options.searchTerm, outputColumns, label),
    isCollapsed: options.isCollapsed,
  };
}

/**
 * Build table-level flow nodes with columns.
 */
function buildFlowNodes(
  merged: MergedLineage,
  selectedNodeId: string | null,
  searchTerm: string,
  collapsedNodeIds: Set<string>,
  expandedTableIds: Set<string>,
  resolvedSchema: ResolvedSchemaMetadata | null | undefined,
  defaultCollapsed: boolean
): SerializedFlowNode[] {
  const tableNodes = merged.nodes.filter((n) => isTableLikeType(n.type));
  const columnNodes = merged.nodes.filter((n) => n.type === 'column');
  const outputNodes = merged.nodes.filter((n) => n.type === OUTPUT_NODE_TYPE);
  const isSelect = merged.isSelect;
  // Identify tables introduced via JOIN (base tables are those NOT in this set)
  const joinedTableIds = buildJoinedTableIds(merged.edges, merged.nodes);
  const hasJoinNodes = joinedTableIds.size > 0;

  const baseTableIds = new Set<string>();
  if (hasJoinNodes) {
    tableNodes.forEach((node) => {
      if (node.type !== 'table') return;
      if (!joinedTableIds.has(node.id)) {
        baseTableIds.add(node.id);
      }
    });
  }

  const recursiveNodeIds = new Set(
    merged.edges.filter((e) => e.type === 'data_flow' && e.from === e.to).map((e) => e.from)
  );

  const tableColumnMap = new Map<string, SerializedColumnInfo[]>();
  const ownedColumnIds = new Set<string>();

  for (const edge of merged.edges) {
    if (edge.type === 'ownership') {
      const parentNode = tableNodes.find((n) => n.id === edge.from);
      const childNode = columnNodes.find((n) => n.id === edge.to);
      if (parentNode && childNode) {
        const cols = tableColumnMap.get(parentNode.id) || [];
        cols.push({
          id: childNode.id,
          name: childNode.label,
          expression: childNode.expression,
          aggregation: childNode.aggregation,
        });
        tableColumnMap.set(parentNode.id, cols);
        ownedColumnIds.add(childNode.id);
      }
    }
  }

  // Sort nodes: tables first, then CTEs
  const sortedTableNodes = [...tableNodes].sort((a, b) => {
    if (a.type === 'cte' && b.type !== 'cte') return 1;
    if (a.type !== 'cte' && b.type === 'cte') return -1;
    return 0;
  });

  const flowNodes: SerializedFlowNode[] = [];

  for (const node of sortedTableNodes) {
    const existingColumns = tableColumnMap.get(node.id) || [];
    const isExpanded = expandedTableIds.has(node.id);

    const { columns, hiddenColumnCount } = processTableColumns(
      node.label,
      node.qualifiedName,
      node.id,
      existingColumns,
      isExpanded,
      resolvedSchema
    );

    flowNodes.push({
      id: node.id,
      type: 'tableNode',
      position: { x: 0, y: 0 },
      data: buildTableNodeData(node, columns, {
        selectedNodeId,
        searchTerm,
        isCollapsed: computeIsCollapsed(node.id, defaultCollapsed, collapsedNodeIds),
        hiddenColumnCount,
        isRecursive: recursiveNodeIds.has(node.id),
        isBaseTable: baseTableIds.has(node.id),
      }),
    });
  }

  const outputColumnsByNodeId = groupOutputColumns(
    outputNodes,
    merged.edges,
    columnNodes,
    ownedColumnIds,
    isSelect,
    GRAPH_CONFIG.VIRTUAL_OUTPUT_NODE_ID
  );

  if (isSelect) {
    outputNodes.forEach((outputNode) => {
      flowNodes.push({
        id: outputNode.id,
        type: 'tableNode',
        position: { x: 0, y: 0 },
        data: buildOutputNodeData(
          outputNode.id,
          outputNode.label,
          outputColumnsByNodeId.get(outputNode.id) || [],
          {
            selectedNodeId,
            searchTerm,
            isCollapsed: computeIsCollapsed(outputNode.id, defaultCollapsed, collapsedNodeIds),
          }
        ),
      });
    });

    const virtualOutputColumns =
      outputColumnsByNodeId.get(GRAPH_CONFIG.VIRTUAL_OUTPUT_NODE_ID) || [];
    if (virtualOutputColumns.length > 0) {
      flowNodes.push({
        id: GRAPH_CONFIG.VIRTUAL_OUTPUT_NODE_ID,
        type: 'tableNode',
        position: { x: 0, y: 0 },
        data: buildOutputNodeData(
          GRAPH_CONFIG.VIRTUAL_OUTPUT_NODE_ID,
          'Output',
          virtualOutputColumns,
          {
            selectedNodeId,
            searchTerm,
            isCollapsed: computeIsCollapsed(
              GRAPH_CONFIG.VIRTUAL_OUTPUT_NODE_ID,
              defaultCollapsed,
              collapsedNodeIds
            ),
          }
        ),
      });
    }
  }

  return flowNodes;
}

/**
 * Build React Flow edges from a merged lineage view.
 */
function buildFlowEdges(
  merged: MergedLineage,
  showColumnEdges: boolean,
  defaultCollapsed: boolean,
  collapsedNodeIds: Set<string>
): SerializedFlowEdge[] {
  const tableNodes = merged.nodes.filter((n) => isTableLikeType(n.type));
  const columnNodes = merged.nodes.filter((n) => n.type === 'column');
  const outputNodes = merged.nodes.filter((n) => n.type === OUTPUT_NODE_TYPE);
  const isSelect = merged.isSelect;

  const tableNodeMap = new Map<string, Node>();
  for (const node of tableNodes) {
    tableNodeMap.set(node.id, node);
  }

  const columnToTableMap = buildColumnOwnershipMap(merged.edges, tableNodes, (n) => n.id);

  const { outputNodeIds, outputColumnIds } = resolveOutputMapping(
    merged.edges,
    outputNodes,
    columnNodes,
    columnToTableMap,
    isSelect,
    GRAPH_CONFIG.VIRTUAL_OUTPUT_NODE_ID
  );

  // Column-level edges
  if (showColumnEdges) {
    const flowEdges: SerializedFlowEdge[] = [];
    const tableEdgeKeys = new Set<string>();
    const tablePairsFromColumns = new Set<string>();

    const isTableCollapsed = (tableId: string) => {
      if (!tableNodeMap.has(tableId)) {
        return false;
      }
      return computeIsCollapsed(tableId, defaultCollapsed, collapsedNodeIds);
    };

    const pushTableEdge = (
      sourceTableId: string,
      targetTableId: string,
      edgeType: string,
      sourceEdge?: Edge
    ) => {
      if (sourceTableId === targetTableId) {
        return;
      }

      const dedupKey = edgePairKey(sourceTableId, targetTableId);
      if (tableEdgeKeys.has(dedupKey)) {
        return;
      }

      tableEdgeKeys.add(dedupKey);
      const joinType = formatJoinType(sourceEdge?.joinType);

      const uiEdgeType = edgeType === JOIN_DEPENDENCY_EDGE_TYPE ? 'joinDependency' : edgeType;

      flowEdges.push({
        id: syntheticEdgeId('relation', sourceTableId, targetTableId),
        source: sourceTableId,
        target: targetTableId,
        type: 'animated',
        label: joinType,
        data: {
          type: uiEdgeType,
          joinType: sourceEdge?.joinType,
          joinCondition: sourceEdge?.joinCondition,
        },
      });
    };

    const columnNodeMap = new Map<string, Node>();
    for (const col of columnNodes) {
      columnNodeMap.set(col.id, col);
    }

    merged.edges
      .filter((e) => e.type === 'derivation' || e.type === 'data_flow')
      .forEach((edge) => {
        const sourceCol = columnNodeMap.get(edge.from);
        const targetCol = columnNodeMap.get(edge.to);
        const sourceRelationId = tableNodeMap.has(edge.from) ? edge.from : undefined;
        const targetRelationId = columnToTableMap.get(edge.to);

        if (sourceCol && targetCol) {
          const sourceTableId = columnToTableMap.get(edge.from);
          const targetTableId = columnToTableMap.get(edge.to);

          if (sourceTableId && targetTableId && sourceTableId !== targetTableId) {
            tablePairsFromColumns.add(edgePairKey(sourceTableId, targetTableId));
            const hasExpression = !!(edge.expression || targetCol.expression);
            const isDerivedColumn = edge.type === 'derivation' || hasExpression;

            const isSourceCollapsed = isTableCollapsed(sourceTableId);
            const isTargetCollapsed = isTableCollapsed(targetTableId);

            if (isSourceCollapsed || isTargetCollapsed) {
              pushTableEdge(sourceTableId, targetTableId, edge.type, edge);
              return;
            }

            flowEdges.push({
              id: edge.id,
              source: sourceTableId,
              target: targetTableId,
              sourceHandle: edge.from,
              targetHandle: edge.to,
              type: 'animated',
              data: {
                type: edge.type,
                expression: edge.expression || targetCol.expression,
                sourceColumn: sourceCol.label,
                targetColumn: targetCol.label,
                isDerived: isDerivedColumn,
              },
              style: {
                strokeDasharray: isDerivedColumn ? '5,5' : undefined,
              },
            });
          }
        } else if (sourceRelationId && targetRelationId && sourceRelationId !== targetRelationId) {
          // Handle relation-to-column edges (e.g., base table → COUNT(*) output column)
          // as table-level edges. Self-referential edges (same relation) are excluded
          // since they don't represent cross-table data flow.
          tablePairsFromColumns.add(edgePairKey(sourceRelationId, targetRelationId));
          pushTableEdge(sourceRelationId, targetRelationId, edge.type, edge);
        }
      });

    const relationNodeIds = new Set(tableNodes.map((node) => node.id));
    outputNodeIds.forEach((nodeId) => relationNodeIds.add(nodeId));

    merged.edges
      .filter(
        (edge) =>
          edge.type === 'data_flow' ||
          edge.type === 'derivation' ||
          edge.type === JOIN_DEPENDENCY_EDGE_TYPE
      )
      .forEach((edge) => {
        if (!relationNodeIds.has(edge.from) || !relationNodeIds.has(edge.to)) {
          return;
        }

        if (tablePairsFromColumns.has(edgePairKey(edge.from, edge.to))) {
          return;
        }

        pushTableEdge(edge.from, edge.to, edge.type, edge);
      });

    return flowEdges;
  }

  // Table-level edges
  const flowEdges: SerializedFlowEdge[] = [];
  const seenEdges = new Set<string>();
  const selectOutputPairs = new Map<
    string,
    { sourceId: string; targetId: string; joinType?: string; joinCondition?: string }
  >();

  for (const edge of merged.edges) {
    if (edge.type === 'data_flow' || edge.type === 'derivation') {
      if (isSelect && outputColumnIds.has(edge.to)) {
        const sourceTableId =
          columnToTableMap.get(edge.from) || (tableNodeMap.has(edge.from) ? edge.from : undefined);
        const targetOutputId = columnToTableMap.get(edge.to);
        if (sourceTableId && targetOutputId && sourceTableId !== targetOutputId) {
          const pairKey = edgePairKey(sourceTableId, targetOutputId);
          if (!selectOutputPairs.has(pairKey)) {
            selectOutputPairs.set(pairKey, {
              sourceId: sourceTableId,
              targetId: targetOutputId,
              joinType: edge.joinType,
              joinCondition: edge.joinCondition,
            });
          }
        }
        continue;
      }

      const sourceTableId = columnToTableMap.get(edge.from);
      const targetTableId = columnToTableMap.get(edge.to);

      if (sourceTableId && targetTableId && sourceTableId !== targetTableId) {
        const dedupKey = edgePairKey(sourceTableId, targetTableId);
        if (!seenEdges.has(dedupKey)) {
          seenEdges.add(dedupKey);

          const joinType = formatJoinType(edge.joinType);

          flowEdges.push({
            id: syntheticEdgeId('relation', sourceTableId, targetTableId),
            source: sourceTableId,
            target: targetTableId,
            type: 'animated',
            label: joinType,
            data: {
              type: edge.type,
              joinType: edge.joinType,
              joinCondition: edge.joinCondition,
            },
          });
        }
      } else {
        const sourceFromColumn = columnToTableMap.get(edge.from);
        const targetFromColumn = columnToTableMap.get(edge.to);
        const sourceTable = tableNodeMap.get(edge.from);
        const targetTable = tableNodeMap.get(edge.to);

        const resolvedSourceId = sourceFromColumn || (sourceTable ? sourceTable.id : null);
        const resolvedTargetId = targetFromColumn || (targetTable ? targetTable.id : null);

        if (resolvedSourceId && resolvedTargetId && resolvedSourceId !== resolvedTargetId) {
          const dedupKey = edgePairKey(resolvedSourceId, resolvedTargetId);
          if (!seenEdges.has(dedupKey)) {
            seenEdges.add(dedupKey);

            const joinType = formatJoinType(edge.joinType);

            flowEdges.push({
              id: syntheticEdgeId('relation', resolvedSourceId, resolvedTargetId),
              source: resolvedSourceId,
              target: resolvedTargetId,
              type: 'animated',
              label: joinType,
              data: {
                type: edge.type,
                joinType: edge.joinType,
                joinCondition: edge.joinCondition,
              },
            });
          }
        }
      }
    }
  }

  merged.edges
    .filter((edge) => edge.type === JOIN_DEPENDENCY_EDGE_TYPE)
    .forEach((edge) => {
      const sourceId = edge.from;
      const targetId = edge.to;

      if (sourceId === targetId) {
        return;
      }

      const dedupKey = edgePairKey(sourceId, targetId);
      if (seenEdges.has(dedupKey)) {
        return;
      }

      seenEdges.add(dedupKey);

      const joinType = formatJoinType(edge.joinType);

      flowEdges.push({
        id: edge.id,
        source: sourceId,
        target: targetId,
        type: 'animated',
        label: joinType,
        data: {
          type: 'joinDependency',
          joinType: edge.joinType,
          joinCondition: edge.joinCondition,
        },
      });
    });

  if (isSelect && selectOutputPairs.size > 0) {
    selectOutputPairs.forEach(({ sourceId, targetId, joinType, joinCondition }) => {
      const label = formatJoinType(joinType);

      flowEdges.push({
        id: syntheticEdgeId('select-output', sourceId, targetId),
        source: sourceId,
        target: targetId,
        type: 'animated',
        label,
        data: {
          type: 'data_flow',
          joinType,
          joinCondition,
        },
      });
    });
  }

  return flowEdges;
}

// =============================================================================
// Script-Level Graph Building
// =============================================================================

interface StatementSlice {
  meta: StatementMeta;
  nodes: Node[];
  edges: Edge[];
}

const UI_CONSTANTS = {
  MAX_EDGE_LABEL_TABLES: 3,
};

function sliceStatements(result: AnalyzeResult): StatementSlice[] {
  return result.statements.map((meta) => ({
    meta,
    nodes: nodesInStatement(result, meta.statementIndex),
    edges: edgesInStatement(result, meta.statementIndex),
  }));
}

function getScriptIO(slices: StatementSlice[]) {
  const reads = new Set<string>();
  const writes = new Set<string>();
  const readQualified = new Set<string>();
  const writeQualified = new Set<string>();

  slices.forEach((slice) => {
    const createdRelationIds = getCreatedRelationNodeIds(
      slice.meta.statementType,
      slice.nodes,
      slice.edges
    );
    slice.nodes.forEach((node) => {
      if (node.type === OUTPUT_NODE_TYPE) {
        writes.add(node.label);
        writeQualified.add(node.qualifiedName || node.label);
        return;
      }

      if (node.type === 'table' || node.type === 'view') {
        const isWritten =
          slice.edges.some((e) => e.to === node.id && e.type === 'data_flow') ||
          createdRelationIds.has(node.id);
        const isRead = slice.edges.some((e) => e.from === node.id && e.type === 'data_flow');

        if (isWritten) {
          writes.add(node.label);
          writeQualified.add(node.qualifiedName || node.label);
        }
        if (isRead || (!isWritten && !isRead)) {
          reads.add(node.label);
          readQualified.add(node.qualifiedName || node.label);
        }
      }
    });
  });
  return { reads, writes, readQualified, writeQualified };
}

function groupStatementsByScript(slices: StatementSlice[]): Map<string, StatementSlice[]> {
  const scriptMap = new Map<string, StatementSlice[]>();
  slices.forEach((slice) => {
    const sourceName = slice.meta.sourceName || 'unknown';
    const existing = scriptMap.get(sourceName) || [];
    existing.push(slice);
    scriptMap.set(sourceName, existing);
  });
  return scriptMap;
}

function createScriptNodes(
  scriptMap: Map<string, StatementSlice[]>,
  selectedNodeId: string | null,
  searchTerm: string
): SerializedFlowNode[] {
  const lowerCaseSearchTerm = searchTerm.toLowerCase();
  const nodes: SerializedFlowNode[] = [];

  scriptMap.forEach((slices, sourceName) => {
    const { reads, writes } = getScriptIO(slices);
    const isHighlighted = !!(
      lowerCaseSearchTerm && sourceName.toLowerCase().includes(lowerCaseSearchTerm)
    );

    nodes.push({
      id: `script:${sourceName}`,
      type: 'scriptNode',
      position: { x: 0, y: 0 },
      data: {
        label: sourceName,
        sourceName,
        tablesRead: Array.from(reads),
        tablesWritten: Array.from(writes),
        statementCount: slices.length,
        isSelected: `script:${sourceName}` === selectedNodeId,
        isHighlighted,
      } as SerializedScriptNodeData,
    });
  });

  return nodes;
}

function buildHybridGraph(
  scriptMap: Map<string, StatementSlice[]>,
  selectedNodeId: string | null,
  searchTerm: string
): { nodes: SerializedFlowNode[]; edges: SerializedFlowEdge[] } {
  const lowerCaseSearchTerm = searchTerm.toLowerCase();
  const nodes: SerializedFlowNode[] = [];
  const edges: SerializedFlowEdge[] = [];
  const uniqueTables = new Map<string, { label: string; sourceName?: string }>();

  scriptMap.forEach((slices) => {
    const { readQualified, writeQualified } = getScriptIO(slices);

    slices.forEach((slice) => {
      const createdRelationIds = getCreatedRelationNodeIds(
        slice.meta.statementType,
        slice.nodes,
        slice.edges
      );
      slice.nodes.forEach((node) => {
        if (node.type === OUTPUT_NODE_TYPE) {
          const qName = node.qualifiedName || node.label;
          uniqueTables.set(qName, { label: node.label, sourceName: slice.meta.sourceName });
          return;
        }

        if (node.type === 'table' || node.type === 'view') {
          const qName = node.qualifiedName || node.label;
          const isWritten =
            slice.edges.some((e) => e.to === node.id && e.type === 'data_flow') ||
            createdRelationIds.has(node.id);

          if (isWritten) {
            uniqueTables.set(qName, { label: node.label, sourceName: slice.meta.sourceName });
          } else if (!uniqueTables.has(qName)) {
            uniqueTables.set(qName, { label: node.label });
          }
        }
      });
    });

    const sourceId = `script:${slices[0].meta.sourceName || 'unknown'}`;

    writeQualified.forEach((qName) => {
      edges.push({
        id: `${sourceId}->table:${qName}`,
        source: sourceId,
        target: `table:${qName}`,
        type: 'animated',
        data: { type: 'data_flow' },
      });
    });

    readQualified.forEach((qName) => {
      edges.push({
        id: `table:${qName}->${sourceId}`,
        source: `table:${qName}`,
        target: sourceId,
        type: 'animated',
        data: { type: 'data_flow' },
      });
    });
  });

  uniqueTables.forEach((info, qName) => {
    const isHighlighted = !!(
      lowerCaseSearchTerm && info.label.toLowerCase().includes(lowerCaseSearchTerm)
    );
    nodes.push({
      id: `table:${qName}`,
      type: 'simpleTableNode',
      position: { x: 0, y: 0 },
      data: {
        label: info.label,
        nodeType: 'table',
        columns: [],
        isSelected: `table:${qName}` === selectedNodeId,
        isHighlighted,
        isCollapsed: false,
        sourceName: info.sourceName,
      } as SerializedTableNodeData,
    });
  });

  return { nodes, edges };
}

function buildDirectScriptGraph(scriptMap: Map<string, StatementSlice[]>): SerializedFlowEdge[] {
  const edges: SerializedFlowEdge[] = [];
  const edgeSet = new Set<string>();

  scriptMap.forEach((producerSlices, producerScript) => {
    const { writeQualified: producerWrites } = getScriptIO(producerSlices);

    scriptMap.forEach((consumerSlices, consumerScript) => {
      if (producerScript === consumerScript) return;

      const { readQualified: consumerReads } = getScriptIO(consumerSlices);

      const sharedTables: string[] = [];
      producerWrites.forEach((table) => {
        if (consumerReads.has(table)) {
          const simpleName = table.split('.').pop() || table;
          sharedTables.push(simpleName);
        }
      });

      if (sharedTables.length > 0) {
        const edgeId = `${producerScript}->${consumerScript}`;
        if (!edgeSet.has(edgeId)) {
          edgeSet.add(edgeId);
          const maxTables = UI_CONSTANTS.MAX_EDGE_LABEL_TABLES;
          edges.push({
            id: edgeId,
            source: `script:${producerScript}`,
            target: `script:${consumerScript}`,
            type: 'animated',
            label:
              sharedTables.slice(0, maxTables).join(', ') +
              (sharedTables.length > maxTables ? '...' : ''),
          });
        }
      }
    });
  });

  return edges;
}

function buildScriptLevelGraph(
  result: AnalyzeResult,
  selectedNodeId: string | null,
  searchTerm: string,
  showTables: boolean
): { nodes: SerializedFlowNode[]; edges: SerializedFlowEdge[] } {
  const scriptMap = groupStatementsByScript(sliceStatements(result));
  const scriptNodes = createScriptNodes(scriptMap, selectedNodeId, searchTerm);

  if (showTables) {
    const { nodes: tableNodes, edges: tableEdges } = buildHybridGraph(
      scriptMap,
      selectedNodeId,
      searchTerm
    );
    return {
      nodes: [...scriptNodes, ...tableNodes],
      edges: tableEdges,
    };
  } else {
    const edges = buildDirectScriptGraph(scriptMap);
    return {
      nodes: scriptNodes,
      edges,
    };
  }
}

// =============================================================================
// Worker Message Handler
// =============================================================================

console.log('[GraphBuilder Worker] Worker initialized');

self.onmessage = (event: MessageEvent<GraphBuildRequest | ScriptGraphBuildRequest>) => {
  const request = event.data;

  console.log(`[GraphBuilder Worker] Received request ${request.requestId}, type: ${request.type}`);
  const startTime = performance.now();

  try {
    let nodes: SerializedFlowNode[];
    let edges: SerializedFlowEdge[];
    let lineageNodes: Node[] | undefined;

    if (request.type === 'build-table-graph') {
      const merged = mergeAnalyzeResult(request.result);

      // Convert arrays back to Sets for internal use
      const collapsedNodeIds = new Set(request.collapsedNodeIds);
      const expandedTableIds = new Set(request.expandedTableIds);

      nodes = buildFlowNodes(
        merged,
        request.selectedNodeId,
        request.searchTerm,
        collapsedNodeIds,
        expandedTableIds,
        request.resolvedSchema,
        request.defaultCollapsed
      );

      edges = buildFlowEdges(
        merged,
        request.showColumnEdges,
        request.defaultCollapsed,
        collapsedNodeIds
      );

      lineageNodes = merged.nodes;
    } else if (request.type === 'build-script-graph') {
      const built = buildScriptLevelGraph(
        request.result,
        request.selectedNodeId,
        request.searchTerm,
        request.showTables
      );
      nodes = built.nodes;
      edges = built.edges;
    } else {
      throw new Error(`Unknown request type: ${(request as { type: string }).type}`);
    }

    const duration = performance.now() - startTime;
    console.log(
      `[GraphBuilder Worker] Build completed in ${duration.toFixed(2)}ms: ${nodes.length} nodes, ${edges.length} edges`
    );

    const response: GraphBuildResponse = {
      type: 'build-result',
      requestId: request.requestId,
      nodes,
      edges,
      lineageNodes,
    };

    self.postMessage(response);
  } catch (error) {
    console.error('[GraphBuilder Worker] Error:', error);
    const response: GraphBuildResponse = {
      type: 'build-result',
      requestId: request.requestId,
      nodes: [],
      edges: [],
      error: error instanceof Error ? error.message : 'Unknown error',
    };

    self.postMessage(response);
  }
};
