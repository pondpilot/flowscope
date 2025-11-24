import type { Node as FlowNode, Edge as FlowEdge } from '@xyflow/react';
import type { Node, Edge, StatementLineage, ResolvedSchemaMetadata } from '@pondpilot/flowscope-core';
import type {
  TableNodeData,
  ColumnNodeInfo,
  ScriptNodeData,
  StatementLineageWithSource,
} from '../types';
import { GRAPH_CONFIG, UI_CONSTANTS } from '../constants';

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
    stmt.nodes.forEach((node) => {
      if (!mergedNodes.has(node.id)) {
        const nodeWithSource = { ...node };
        if (sourceName) {
          nodeWithSource.metadata = {
            ...node.metadata,
            sourceName,
          };
        }
        mergedNodes.set(node.id, nodeWithSource);
      }
    });

    stmt.edges.forEach((edge) => {
      if (!mergedEdges.has(edge.id)) {
        mergedEdges.set(edge.id, edge);
      }
    });
  });

  return {
    statementIndex: 0,
    statementType: 'SELECT',
    nodes: Array.from(mergedNodes.values()),
    edges: Array.from(mergedEdges.values()),
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
      hiddenColumnCount
    };
  }

  return { columns: existingColumns, hiddenColumnCount };
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
  resolvedSchema: ResolvedSchemaMetadata | null | undefined = null
): FlowNode[] {
  const lowerCaseSearchTerm = searchTerm.toLowerCase();
  const tableNodes = statement.nodes.filter((n) => n.type === 'table' || n.type === 'cte');
  const columnNodes = statement.nodes.filter((n) => n.type === 'column');
  const recursiveNodeIds = new Set(
    statement.edges
      .filter((e) => e.type === 'data_flow' && e.from === e.to)
      .map((e) => e.from)
  );

  const tableColumnMap = new Map<string, ColumnNodeInfo[]>();

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
        });
        tableColumnMap.set(parentNode.id, cols);
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

    const isHighlighted = !!(
      lowerCaseSearchTerm &&
      (node.label.toLowerCase().includes(lowerCaseSearchTerm) ||
        columns.some((col) => col.name.toLowerCase().includes(lowerCaseSearchTerm)))
    );

    const isCollapsed = collapsedNodeIds.has(node.id);

    flowNodes.push({
      id: node.id,
      type: 'tableNode',
      position: { x: 0, y: 0 },
      data: {
        label: node.label,
        nodeType: node.type === 'cte' ? 'cte' : 'table',
        isRecursive: recursiveNodeIds.has(node.id),
        columns: columns,
        isSelected: node.id === selectedNodeId,
        isHighlighted: isHighlighted,
        isCollapsed: isCollapsed,
        hiddenColumnCount,
      } satisfies TableNodeData,
    });
  }

  return flowNodes;
}

/**
 * Build flow edges from statement edges
 */
export function buildFlowEdges(statement: StatementLineage): FlowEdge[] {
  return statement.edges
    .filter((e) => e.type === 'data_flow' || e.type === 'derivation')
    .map((edge) => {
      const isRecursive = edge.type === 'data_flow' && edge.from === edge.to;
      const tooltip = isRecursive ? 'Recursive member' : undefined;
      const label = isRecursive ? undefined : edge.operation || undefined;

      return {
        id: edge.id,
        source: edge.from,
        target: edge.to,
        type: 'animated',
        sourceHandle: isRecursive ? 'rec-top' : undefined,
        targetHandle: isRecursive ? 'rec-top' : undefined,
        data: { type: edge.type, isRecursive, tooltip },
        style: isRecursive
          ? {
              strokeDasharray: '6 4',
            }
          : undefined,
        label,
      };
    });
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
    stmt.nodes.forEach((node) => {
      if (node.type === 'table') {
        const isWritten =
          stmt.edges.some((e) => e.to === node.id && e.type === 'data_flow') ||
          stmt.statementType === 'CREATE_TABLE';
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
      stmt.nodes.forEach((node) => {
        if (node.type === 'table') {
          const qName = node.qualifiedName || node.label;
          const isWritten =
            stmt.edges.some((e) => e.to === node.id && e.type === 'data_flow') ||
            stmt.statementType === 'CREATE_TABLE';

          // If this script writes the table, use its sourceName as the table's source
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
function buildDirectScriptGraph(
  scriptMap: Map<string, StatementLineageWithSource[]>
): FlowEdge[] {
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

/**
 * Build column-level graph with column-to-column lineage
 */
export function buildColumnLevelGraph(
  statement: StatementLineage,
  selectedNodeId: string | null,
  searchTerm: string,
  collapsedNodeIds: Set<string>,
  expandedTableIds: Set<string> = new Set(),
  resolvedSchema: ResolvedSchemaMetadata | null | undefined = null
): { nodes: FlowNode[]; edges: FlowEdge[] } {
  const lowerCaseSearchTerm = searchTerm.toLowerCase();
  const tableNodes = statement.nodes.filter((n) => n.type === 'table' || n.type === 'cte');
  const columnNodes = statement.nodes.filter((n) => n.type === 'column');

  // Build table-to-columns map
  const tableColumnMap = new Map<string, ColumnNodeInfo[]>();
  const columnToTableMap = new Map<string, string>();

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
        });
        tableColumnMap.set(parentNode.id, cols);
        columnToTableMap.set(childNode.id, parentNode.id);
      }
    }
  }

  const flowNodes: FlowNode[] = [];

  // Collect output columns (columns not owned by any table)
  const outputColumns: ColumnNodeInfo[] = [];
  for (const node of columnNodes) {
    if (!columnToTableMap.has(node.id)) {
      outputColumns.push({
        id: node.id,
        name: node.label,
        expression: node.expression,
      });
    }
  }

  for (const node of tableNodes) {
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

    const isHighlighted = !!(
      lowerCaseSearchTerm &&
      (node.label.toLowerCase().includes(lowerCaseSearchTerm) ||
        columns.some((col) => col.name.toLowerCase().includes(lowerCaseSearchTerm)))
    );

    flowNodes.push({
      id: node.id,
      type: 'tableNode',
      position: { x: 0, y: 0 },
      data: {
        label: node.label,
        nodeType: node.type === 'cte' ? 'cte' : 'table',
        columns: columns,
        isSelected: node.id === selectedNodeId,
        isHighlighted: isHighlighted,
        isCollapsed: collapsedNodeIds.has(node.id),
        hiddenColumnCount,
      } satisfies TableNodeData,
    });
  }

  // Add virtual "Output" table node if there are output columns
  if (outputColumns.length > 0) {
    const outputNodeId = GRAPH_CONFIG.VIRTUAL_OUTPUT_NODE_ID;
    const isHighlighted = !!(
      lowerCaseSearchTerm &&
      outputColumns.some((col) => col.name.toLowerCase().includes(lowerCaseSearchTerm))
    );

    flowNodes.push({
      id: outputNodeId,
      type: 'tableNode',
      position: { x: 0, y: 0 },
      data: {
        label: 'Output',
        nodeType: 'virtualOutput',
        columns: outputColumns,
        isSelected: outputNodeId === selectedNodeId,
        isHighlighted,
        isCollapsed: collapsedNodeIds.has(outputNodeId),
      } satisfies TableNodeData,
    });

    // Update columnToTableMap for output columns
    outputColumns.forEach((col) => {
      columnToTableMap.set(col.id, outputNodeId);
    });
  }

  // Build one edge per column lineage connection
  const flowEdges: FlowEdge[] = [];

  statement.edges
    .filter((e) => e.type === 'derivation' || e.type === 'data_flow')
    .forEach((edge) => {
      const sourceCol = columnNodes.find((c) => c.id === edge.from);
      const targetCol = columnNodes.find((c) => c.id === edge.to);

      if (sourceCol && targetCol) {
        const sourceTableId = columnToTableMap.get(edge.from);
        const targetTableId = columnToTableMap.get(edge.to);

        // Only create edges between different tables (skip self-loops)
        if (sourceTableId && targetTableId && sourceTableId !== targetTableId) {
          const hasExpression = edge.expression || targetCol.expression;
          const isDerivedColumn = edge.type === 'derivation' || hasExpression;

          const isSourceCollapsed = collapsedNodeIds.has(sourceTableId);
          const isTargetCollapsed = collapsedNodeIds.has(targetTableId);

          flowEdges.push({
            id: edge.id,
            source: sourceTableId,
            target: targetTableId,
            sourceHandle: isSourceCollapsed ? null : edge.from,
            targetHandle: isTargetCollapsed ? null : edge.to,
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
      } else {
        // Fallback: Table-to-Table edge (e.g. UPDATE/DELETE/MERGE targets)
        // Check if these are table nodes
        const sourceTable = tableNodes.find(n => n.id === edge.from);
        const targetTable = tableNodes.find(n => n.id === edge.to);

        if (sourceTable && targetTable && sourceTable.id !== targetTable.id) {
           flowEdges.push({
            id: edge.id,
            source: sourceTable.id,
            target: targetTable.id,
            // No handles needed for table-to-table (uses default handles)
            sourceHandle: null,
            targetHandle: null,
            type: 'animated',
            data: {
              type: edge.type,
              isDerived: false,
            },
          });
        }
      }
    });

  return { nodes: flowNodes, edges: flowEdges };
}
