import type { Node as FlowNode, Edge as FlowEdge } from '@xyflow/react';
import type {
  Node,
  Edge,
  StatementLineage,
  ResolvedSchemaMetadata,
  GlobalLineage,
  GlobalNode,
} from '@pondpilot/flowscope-core';
import { isTableLikeType } from '@pondpilot/flowscope-core';
import type {
  TableNodeData,
  ColumnNodeInfo,
  ScriptNodeData,
  StatementLineageWithSource,
} from '../types';
import { GRAPH_CONFIG, UI_CONSTANTS } from '../constants';
import {
  getCreatedRelationNodeIds,
  OUTPUT_NODE_TYPE,
  JOIN_DEPENDENCY_EDGE_TYPE,
  buildColumnOwnershipMap,
  buildJoinedTableIds,
  formatJoinType,
  groupOutputColumns,
  resolveOutputMapping,
  edgePairKey,
  syntheticEdgeId,
  isNodeHighlighted,
  createStatementScope,
  withStatementScope,
} from './lineageHelpers';
import { mergeNodesForNavigation } from './nodeOccurrences';

const SELECT_STATEMENT_TYPES = new Set([
  'SELECT',
  'WITH',
  'UNION',
  'INTERSECT',
  'EXCEPT',
  'VALUES',
]);

/**
 * Determine if a node should be collapsed based on the default state and overrides.
 *
 * When defaultCollapsed is true, nodes are collapsed by default and the overrideIds
 * contains nodes that should be expanded (exceptions to the default).
 * When defaultCollapsed is false, nodes are expanded by default and the overrideIds
 * contains nodes that should be collapsed.
 */
export function computeIsCollapsed(
  nodeId: string,
  defaultCollapsed: boolean,
  overrideIds: Set<string>
): boolean {
  return defaultCollapsed ? !overrideIds.has(nodeId) : overrideIds.has(nodeId);
}

/**
 * Merge multiple statements into a single statement for visualization
 */
export function mergeStatements(statements: StatementLineage[]): StatementLineage {
  if (statements.length === 1) {
    return statements[0];
  }

  const mergedNodes = new Map<string, Node>();
  const mergedEdges = new Map<string, Edge>();

  statements.forEach((stmt) => {
    const sourceName = stmt.sourceName;
    const statementScope = createStatementScope(stmt.statementIndex, sourceName);
    stmt.nodes.forEach((node) => {
      const nodeWithScope = withStatementScope(
        sourceName
          ? {
              ...node,
              metadata: {
                ...(node.metadata || {}),
                sourceName,
              },
            }
          : { ...node },
        statementScope
      );
      const mergedNode = mergeNodesForNavigation(
        mergedNodes.get(node.id) ?? null,
        nodeWithScope,
        sourceName
      );
      mergedNodes.set(node.id, mergedNode);
    });

    stmt.edges.forEach((edge) => {
      if (!mergedEdges.has(edge.id)) {
        mergedEdges.set(edge.id, withStatementScope(edge, statementScope));
      }
    });
  });

  // Aggregate stats from all statements
  const totalJoinCount = statements.reduce((sum, stmt) => sum + stmt.joinCount, 0);
  const maxComplexity =
    statements.length > 0 ? Math.max(...statements.map((stmt) => stmt.complexityScore)) : 1;

  return {
    statementIndex: 0,
    statementType: 'SELECT',
    nodes: Array.from(mergedNodes.values()),
    edges: Array.from(mergedEdges.values()),
    joinCount: totalJoinCount,
    complexityScore: maxComplexity,
  };
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

  // Try exact match first (qualified name)
  if (qualifiedName) {
    const table = resolvedSchema.tables.find((t) => {
      const schemaQualified = [t.catalog, t.schema, t.name].filter(Boolean).join('.');
      return schemaQualified === qualifiedName;
    });
    if (table) return table;
  }

  // Try matching by table name only
  const table = resolvedSchema.tables.find((t) => t.name === tableLabel);
  return table || null;
}

/**
 * Process table columns by injecting missing schema columns when expanded.
 * Returns the final columns list and count of hidden columns.
 */
function processTableColumns(
  tableLabel: string,
  qualifiedName: string | undefined,
  nodeId: string,
  existingColumns: ColumnNodeInfo[],
  isExpanded: boolean,
  resolvedSchema: ResolvedSchemaMetadata | null | undefined
): { columns: ColumnNodeInfo[]; hiddenColumnCount: number } {
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

  // If expanded, add missing columns to the list
  if (isExpanded && missingColumns.length > 0) {
    const injectedColumns: ColumnNodeInfo[] = missingColumns.map((col) => ({
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
 * Base options shared by all node data builder functions.
 */
interface NodeBuilderOptions {
  selectedNodeId: string | null;
  searchTerm: string;
  isCollapsed: boolean;
}

/**
 * Options for building table/CTE node data.
 */
interface TableNodeBuilderOptions extends NodeBuilderOptions {
  hiddenColumnCount?: number;
  isRecursive?: boolean;
  isBaseTable?: boolean;
}

/**
 * Build TableNodeData for a table/CTE node.
 * Shared between table-level and column-level graph builders to ensure feature parity.
 */
function buildTableNodeData(
  node: Node,
  columns: ColumnNodeInfo[],
  options: TableNodeBuilderOptions,
  globalNodeMap?: Map<string, GlobalNode>
): TableNodeData {
  let nodeType: 'table' | 'view' | 'cte' | 'virtualOutput' = 'table';
  if (node.type === 'cte') {
    nodeType = 'cte';
  } else if (node.type === 'view') {
    nodeType = 'view';
  }

  // Look up canonical info from GlobalNode (more reliable than parsing qualifiedName)
  const globalNode = globalNodeMap?.get(node.id);
  const canonical = globalNode?.canonicalName;

  // Construct qualified name from canonical components
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
 * Shared between table-level and column-level graph builders.
 */
function buildOutputNodeData(
  nodeId: string,
  label: string,
  outputColumns: ColumnNodeInfo[],
  options: NodeBuilderOptions
): TableNodeData {
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
 * Build table-level flow nodes with columns
 */
export function buildFlowNodes(
  statement: StatementLineage,
  selectedNodeId: string | null,
  searchTerm: string,
  collapsedNodeIds: Set<string>,
  expandedTableIds: Set<string> = new Set(),
  resolvedSchema: ResolvedSchemaMetadata | null | undefined = null,
  defaultCollapsed: boolean = false,
  globalLineage?: GlobalLineage
): FlowNode[] {
  // Create lookup map for GlobalNode canonical info
  const globalNodeMap = new Map<string, GlobalNode>();
  if (globalLineage?.nodes) {
    for (const gn of globalLineage.nodes) {
      globalNodeMap.set(gn.id, gn);
    }
  }

  const tableNodes = statement.nodes.filter((n) => isTableLikeType(n.type));
  const columnNodes = statement.nodes.filter((n) => n.type === 'column');
  const outputNodes = statement.nodes.filter((n) => n.type === OUTPUT_NODE_TYPE);
  const isSelect = isSelectStatement(statement);
  // Identify tables introduced via JOIN (base tables are those NOT in this set)
  const joinedTableIds = buildJoinedTableIds(statement.edges, statement.nodes);
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
    statement.edges.filter((e) => e.type === 'data_flow' && e.from === e.to).map((e) => e.from)
  );

  const tableColumnMap = new Map<string, ColumnNodeInfo[]>();
  const ownedColumnIds = new Set<string>();

  for (const edge of statement.edges) {
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

  const nodesByType = { table: [] as Node[], cte: [] as Node[] };
  for (const node of tableNodes) {
    if (node.type === 'cte') {
      nodesByType.cte.push(node);
    } else {
      nodesByType.table.push(node);
    }
  }

  const flowNodes: FlowNode[] = [];

  for (const node of [...nodesByType.table, ...nodesByType.cte]) {
    const existingColumns = tableColumnMap.get(node.id) || [];
    const isExpanded = expandedTableIds.has(node.id);

    // Process columns with schema injection
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
      data: buildTableNodeData(
        node,
        columns,
        {
          selectedNodeId,
          searchTerm,
          isCollapsed: computeIsCollapsed(node.id, defaultCollapsed, collapsedNodeIds),
          hiddenColumnCount,
          isRecursive: recursiveNodeIds.has(node.id),
          isBaseTable: baseTableIds.has(node.id),
        },
        globalNodeMap
      ),
    });
  }

  const outputColumnsByNodeId = groupOutputColumns(
    outputNodes,
    statement.edges,
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
 * Check if a statement is a SELECT-like read query based on analyzer metadata.
 */
function isSelectStatement(statement: StatementLineage): boolean {
  const normalizedType = (statement.statementType || '').toUpperCase();
  return SELECT_STATEMENT_TYPES.has(normalizedType);
}

/**
 * Build React Flow edges from statement lineage data.
 *
 * This function handles multiple scenarios to correctly visualize data flow:
 *
 * 1. **DML/DDL statements** (INSERT, UPDATE, CREATE TABLE AS, MERGE, CREATE VIEW):
 *    Creates direct edges between table nodes based on column ownership. The backend
 *    provides data_flow/derivation edges between columns; this function resolves
 *    those to their owning tables and deduplicates to create table-to-table edges.
 *
 * 2. **SELECT statements** (SELECT, WITH, UNION, INTERSECT, EXCEPT, VALUES):
 *    Identifies output columns (those not owned by any table) and creates edges
 *    from their source tables to a virtual "Output" node. This correctly visualizes
 *    complex statements that may contain both intermediate table-to-table flows
 *    (e.g., within CTEs) and a final projection to the output.
 *
 * The function also:
 * - Attaches join type labels to edges based on the source node's join metadata
 * - Handles fallback cases where edges connect columns to tables directly (e.g., CREATE VIEW)
 * - Deduplicates edges to avoid rendering multiple edges between the same table pair
 */
export function buildFlowEdges(
  statement: StatementLineage,
  showColumnEdges: boolean = false,
  defaultCollapsed: boolean = false,
  collapsedNodeIds: Set<string> = new Set()
): FlowEdge[] {
  const tableNodes = statement.nodes.filter((n) => isTableLikeType(n.type));
  const columnNodes = statement.nodes.filter((n) => n.type === 'column');
  const outputNodes = statement.nodes.filter((n) => n.type === OUTPUT_NODE_TYPE);
  const isSelect = isSelectStatement(statement);

  // Build table ID -> Node map for join type lookup
  const tableNodeMap = new Map<string, Node>();
  for (const node of tableNodes) {
    tableNodeMap.set(node.id, node);
  }

  // Build ownership map: column ID -> table ID
  const columnToTableMap = buildColumnOwnershipMap(statement.edges, tableNodes, (n) => n.id);

  const { outputNodeIds, outputColumnIds } = resolveOutputMapping(
    statement.edges,
    outputNodes,
    columnNodes,
    columnToTableMap,
    isSelect,
    GRAPH_CONFIG.VIRTUAL_OUTPUT_NODE_ID
  );

  // Column-level edges: one edge per column lineage connection
  if (showColumnEdges) {
    const flowEdges: FlowEdge[] = [];
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

      // Map core edge type to UI edge type (snake_case -> camelCase)
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

    // Build column ID -> Node map for O(1) lookups (avoids O(E × C) find operations)
    const columnNodeMap = new Map<string, Node>();
    for (const col of columnNodes) {
      columnNodeMap.set(col.id, col);
    }

    statement.edges
      .filter((e) => e.type === 'derivation' || e.type === 'data_flow')
      .forEach((edge) => {
        const sourceCol = columnNodeMap.get(edge.from);
        const targetCol = columnNodeMap.get(edge.to);
        const sourceRelationId = tableNodeMap.has(edge.from) ? edge.from : undefined;
        const targetRelationId = columnToTableMap.get(edge.to);

        if (sourceCol && targetCol) {
          const sourceTableId = columnToTableMap.get(edge.from);
          const targetTableId = columnToTableMap.get(edge.to);

          // Only create edges between different tables (skip self-loops)
          if (sourceTableId && targetTableId && sourceTableId !== targetTableId) {
            tablePairsFromColumns.add(edgePairKey(sourceTableId, targetTableId));
            const hasExpression = edge.expression || targetCol.expression;
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

    statement.edges
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

  // Table-level edges: one edge per table pair (deduplicated)
  const flowEdges: FlowEdge[] = [];
  const seenEdges = new Set<string>();
  const selectOutputPairs = new Map<
    string,
    { sourceId: string; targetId: string; joinType?: string; joinCondition?: string }
  >();

  for (const edge of statement.edges) {
    if (edge.type === 'data_flow' || edge.type === 'derivation') {
      if (isSelect && outputColumnIds.has(edge.to)) {
        // Resolve source table: either the column's owning table, or the relation
        // itself when a table node directly feeds an output column (e.g., COUNT(*)).
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

      // Find source and target tables via column ownership
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
        // Fallback: Handle edges where one endpoint is a column and the other is a table/view
        // This handles CREATE VIEW (column -> view) and other DDL patterns
        const sourceFromColumn = columnToTableMap.get(edge.from);
        const targetFromColumn = columnToTableMap.get(edge.to);
        const sourceTable = tableNodeMap.get(edge.from);
        const targetTable = tableNodeMap.get(edge.to);

        // Resolve actual source and target table IDs
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

  statement.edges
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
          // Map core edge type to UI edge type (snake_case -> camelCase)
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

/**
 * Extract input/output tables for a set of statements from a script
 */
function getScriptIO(stmts: StatementLineageWithSource[]) {
  const reads = new Set<string>();
  const writes = new Set<string>();
  const readQualified = new Set<string>();
  const writeQualified = new Set<string>();

  stmts.forEach((stmt) => {
    const createdRelationIds = getCreatedRelationNodeIds(stmt);
    stmt.nodes.forEach((node) => {
      if (node.type === OUTPUT_NODE_TYPE) {
        writes.add(node.label);
        writeQualified.add(node.qualifiedName || node.label);
        return;
      }

      if (node.type === 'table' || node.type === 'view') {
        const isWritten =
          stmt.edges.some((e) => e.to === node.id && e.type === 'data_flow') ||
          createdRelationIds.has(node.id);
        const isRead = stmt.edges.some((e) => e.from === node.id && e.type === 'data_flow');

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

/**
 * Group statements by their source script name
 */
function groupStatementsByScript(
  statements: StatementLineageWithSource[]
): Map<string, StatementLineageWithSource[]> {
  const scriptMap = new Map<string, StatementLineageWithSource[]>();
  statements.forEach((stmt) => {
    const sourceName = stmt.sourceName || 'unknown';
    const existing = scriptMap.get(sourceName) || [];
    existing.push(stmt);
    scriptMap.set(sourceName, existing);
  });
  return scriptMap;
}

/**
 * Create script node elements from script map
 */
function createScriptNodes(
  scriptMap: Map<string, StatementLineageWithSource[]>,
  selectedNodeId: string | null,
  searchTerm: string
): FlowNode[] {
  const lowerCaseSearchTerm = searchTerm.toLowerCase();
  const nodes: FlowNode[] = [];

  scriptMap.forEach((stmts, sourceName) => {
    const { reads, writes } = getScriptIO(stmts);
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
        statementCount: stmts.length,
        isSelected: `script:${sourceName}` === selectedNodeId,
        isHighlighted,
      } satisfies ScriptNodeData,
    });
  });

  return nodes;
}

/**
 * Build hybrid graph with script and table nodes
 */
function buildHybridGraph(
  scriptMap: Map<string, StatementLineageWithSource[]>,
  selectedNodeId: string | null,
  searchTerm: string
): { nodes: FlowNode[]; edges: FlowEdge[] } {
  const lowerCaseSearchTerm = searchTerm.toLowerCase();
  const nodes: FlowNode[] = [];
  const edges: FlowEdge[] = [];
  const uniqueTables = new Map<string, { label: string; sourceName?: string }>();

  scriptMap.forEach((stmts) => {
    const { readQualified, writeQualified } = getScriptIO(stmts);

    // Collect unique table info, prioritizing the writer for sourceName
    stmts.forEach((stmt) => {
      const createdRelationIds = getCreatedRelationNodeIds(stmt);
      stmt.nodes.forEach((node) => {
        if (node.type === OUTPUT_NODE_TYPE) {
          const qName = node.qualifiedName || node.label;
          uniqueTables.set(qName, { label: node.label, sourceName: stmt.sourceName });
          return;
        }

        if (node.type === 'table' || node.type === 'view') {
          const qName = node.qualifiedName || node.label;
          const isWritten =
            stmt.edges.some((e) => e.to === node.id && e.type === 'data_flow') ||
            createdRelationIds.has(node.id);

          // If this script writes the table/view, use its sourceName as the source
          if (isWritten) {
            uniqueTables.set(qName, { label: node.label, sourceName: stmt.sourceName });
          } else if (!uniqueTables.has(qName)) {
            uniqueTables.set(qName, { label: node.label });
          }
        }
      });
    });

    const sourceId = `script:${stmts[0].sourceName || 'unknown'}`;

    // Edges: Script -> Table (Writes)
    writeQualified.forEach((qName) => {
      edges.push({
        id: `${sourceId}->table:${qName}`,
        source: sourceId,
        target: `table:${qName}`,
        type: 'animated',
        data: { type: 'data_flow' },
      });
    });

    // Edges: Table -> Script (Reads)
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

  // Create Table Nodes
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
      } satisfies TableNodeData,
    });
  });

  return { nodes, edges };
}

/**
 * Build direct script-to-script graph
 */
function buildDirectScriptGraph(scriptMap: Map<string, StatementLineageWithSource[]>): FlowEdge[] {
  const edges: FlowEdge[] = [];
  const edgeSet = new Set<string>();

  scriptMap.forEach((producerStmts, producerScript) => {
    const { writeQualified: producerWrites } = getScriptIO(producerStmts);

    scriptMap.forEach((consumerStmts, consumerScript) => {
      if (producerScript === consumerScript) return;

      const { readQualified: consumerReads } = getScriptIO(consumerStmts);

      // Find intersection
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

/**
 * Build script-level graph (with or without table nodes)
 */
export function buildScriptLevelGraph(
  statements: StatementLineageWithSource[],
  selectedNodeId: string | null,
  searchTerm: string,
  showTables: boolean
): { nodes: FlowNode[]; edges: FlowEdge[] } {
  const scriptMap = groupStatementsByScript(statements);
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
