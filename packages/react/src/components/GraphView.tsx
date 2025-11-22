import { useMemo, useCallback, useEffect } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  Handle,
  Position,
  BaseEdge,
  getBezierPath,
} from '@xyflow/react';
import type { Node as FlowNode, Edge as FlowEdge, NodeProps, EdgeProps } from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { useLineage } from '../context';
import type {
  GraphViewProps,
  TableNodeData,
  ColumnNodeInfo,
  ScriptNodeData,
} from '../types';
import type { Node, Edge, StatementLineage } from '@pondpilot/flowscope-core';
import { getLayoutedElements } from '../utils/layout';
import { sanitizeIdentifier } from '../utils/sanitize';
import { ScriptNode } from './ScriptNode';
import { ColumnNode } from './ColumnNode';

const colors = {
  table: {
    bg: '#FFFFFF',
    headerBg: '#F2F4F8',
    border: '#DBDDE1',
    text: '#212328',
    textSecondary: '#6F7785',
  },
  cte: {
    bg: '#F5F3FF',
    headerBg: '#EDE9FE',
    border: '#C4B5FD',
    text: '#5B21B6',
    textSecondary: '#7C3AED',
  },
  accent: '#4C61FF',
};

function TableNode({ data, selected }: NodeProps): JSX.Element {
  const nodeData = data as TableNodeData;
  const isCte = nodeData.nodeType === 'cte';
  const isSelected = selected || nodeData.isSelected;
  const isHighlighted = nodeData.isHighlighted;
  const palette = isCte ? colors.cte : colors.table;

  return (
    <div
      style={{
        minWidth: 180,
        borderRadius: 8,
        border: `1px solid ${isSelected ? colors.accent : palette.border}`,
        boxShadow: isSelected
          ? `0 0 0 2px ${colors.accent}40`
          : '0 1px 3px rgba(0,0,0,0.1)',
        overflow: 'hidden',
        backgroundColor: isHighlighted ? 'hsl(var(--highlight))' : palette.bg,
        fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
      }}
    >
      <div
        style={{
          padding: '8px 12px',
          fontSize: 12,
          fontWeight: 500,
          borderBottom: `1px solid ${palette.border}`,
          backgroundColor: palette.headerBg,
          color: palette.text,
          display: 'flex',
          alignItems: 'center',
          gap: 8,
        }}
      >
        <span
          style={{
            textTransform: 'uppercase',
            fontSize: 10,
            opacity: 0.6,
            fontWeight: 600,
          }}
        >
          {nodeData.nodeType}
        </span>
        <span style={{ fontWeight: 600 }}>{sanitizeIdentifier(nodeData.label)}</span>
      </div>
      {nodeData.columns.length > 0 && (
        <div style={{ padding: '6px 12px', maxHeight: 150, overflowY: 'auto', position: 'relative' }}>
          {nodeData.columns.map((col: ColumnNodeInfo) => {
            return (
              <div
                key={col.id}
                style={{
                  fontSize: 12,
                  color: palette.textSecondary,
                  padding: '3px 4px',
                  borderRadius: 4,
                  position: 'relative',
                }}
              >
                {/* Target handle (left side) for this column */}
                <Handle
                  type="target"
                  position={Position.Left}
                  id={col.id}
                  style={{
                    width: 8,
                    height: 8,
                    backgroundColor: colors.accent,
                    border: '2px solid white',
                    left: -4,
                    top: '50%',
                    transform: 'translateY(-50%)',
                  }}
                />
                {sanitizeIdentifier(col.name)}
                {/* Source handle (right side) for this column */}
                <Handle
                  type="source"
                  position={Position.Right}
                  id={col.id}
                  style={{
                    width: 8,
                    height: 8,
                    backgroundColor: colors.accent,
                    border: '2px solid white',
                    right: -4,
                    top: '50%',
                    transform: 'translateY(-50%)',
                  }}
                />
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function AnimatedEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  markerEnd,
  data,
  style,
}: EdgeProps): JSX.Element {
  const [edgePath, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  });

  const expression = data?.expression as string | undefined;
  const sourceColumn = data?.sourceColumn as string | undefined;
  const targetColumn = data?.targetColumn as string | undefined;

  // Build tooltip content
  let tooltipContent = '';
  if (sourceColumn && targetColumn) {
    tooltipContent += `${sourceColumn} → ${targetColumn}`;
  }
  if (expression) {
    tooltipContent += tooltipContent ? '\n\n' : '';
    tooltipContent += `Expression:\n${expression}`;
  }

  return (
    <>
      <BaseEdge
        id={id}
        path={edgePath}
        markerEnd={markerEnd}
        style={{
          stroke: colors.accent,
          strokeWidth: 2,
          ...style,
        }}
      />
      {/* Show marker badge on edges with expressions */}
      {expression && (
        <g transform={`translate(${labelX}, ${labelY})`} style={{ cursor: 'help' }}>
          <title>{tooltipContent}</title>
          {/* Background circle */}
          <circle
            r={10}
            fill="#FFF"
            stroke={colors.accent}
            strokeWidth={2}
          />
          {/* Expression icon - bolt/lightning SVG */}
          <g transform="translate(-6, -6)" style={{ pointerEvents: 'none' }}>
            <path
              d="M7 1L1 7h4l-0.5 4 5-6H5.5L7 1z"
              fill={colors.accent}
            />
          </g>
        </g>
      )}
      {/* Tooltip for all edges */}
      {!expression && tooltipContent && (
        <g transform={`translate(${labelX}, ${labelY})`}>
          <title>{tooltipContent}</title>
        </g>
      )}
    </>
  );
}

const nodeTypes = {
  tableNode: TableNode,
  scriptNode: ScriptNode,
  columnNode: ColumnNode,
};

const edgeTypes = {
  animated: AnimatedEdge,
};

/**
 * Merges multiple statement lineages into a single unified view.
 * Optimizes for the common single-statement case with early return.
 * @param statements Array of statement lineages to merge
 * @returns A single merged statement lineage
 */
function mergeStatements(statements: StatementLineage[]): StatementLineage {
  // Early return optimization for single statement
  if (statements.length === 1) {
    return statements[0];
  }

  const mergedNodes = new Map<string, Node>();
  const mergedEdges = new Map<string, Edge>();

  statements.forEach((stmt) => {
    stmt.nodes.forEach((node) => {
       // For tables, we might want to deduplicate based on label if ID varies
       // But assuming unique IDs for now or consistent naming
       if (!mergedNodes.has(node.id)) {
         mergedNodes.set(node.id, node);
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
    statementType: 'SELECT', // Placeholder for merged view
    nodes: Array.from(mergedNodes.values()),
    edges: Array.from(mergedEdges.values()),
  };
}

function buildFlowNodes(
  statement: StatementLineage,
  selectedNodeId: string | null,
  searchTerm: string
): FlowNode[] {
  const lowerCaseSearchTerm = searchTerm.toLowerCase();
  const tableNodes = statement.nodes.filter((n) => n.type === 'table' || n.type === 'cte');
  const columnNodes = statement.nodes.filter((n) => n.type === 'column');

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
    const columns = tableColumnMap.get(node.id) || [];
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
      } satisfies TableNodeData,
    });
  }

  return flowNodes;
}

function buildFlowEdges(statement: StatementLineage): FlowEdge[] {
  return statement.edges
    .filter((e) => e.type === 'data_flow' || e.type === 'derivation')
    .map((edge) => ({
      id: edge.id,
      source: edge.from,
      target: edge.to,
      type: 'animated',
      data: { type: edge.type },
      label: edge.operation || undefined,
    }));
}

/**
 * Build script-level nodes and edges from multiple statements.
 * Groups statements by source_name and shows script-to-script relationships.
 */
function buildScriptLevelGraph(
  statements: StatementLineage[],
  selectedNodeId: string | null,
  searchTerm: string
): { nodes: FlowNode[]; edges: FlowEdge[] } {
  const lowerCaseSearchTerm = searchTerm.toLowerCase();

  // Group statements by source_name
  const scriptMap = new Map<string, StatementLineage[]>();
  statements.forEach((stmt) => {
    const sourceName = (stmt as any).source_name || 'unknown';
    const existing = scriptMap.get(sourceName) || [];
    existing.push(stmt);
    scriptMap.set(sourceName, existing);
  });

  const flowNodes: FlowNode[] = [];

  // Create script nodes
  scriptMap.forEach((stmts, sourceName) => {
    const tablesRead = new Set<string>();
    const tablesWritten = new Set<string>();

    stmts.forEach((stmt) => {
      stmt.nodes.forEach((node) => {
        if (node.type === 'table') {
          // Determine if table is read or written based on edges
          const isWritten = stmt.edges.some((e) => e.to === node.id && e.type === 'data_flow');
          const isRead = stmt.edges.some((e) => e.from === node.id && e.type === 'data_flow');

          if (isWritten) {
            tablesWritten.add(node.label);
          }
          if (isRead || (!isWritten && !isRead)) {
            tablesRead.add(node.label);
          }
        }
      });
    });

    const isHighlighted = !!(
      lowerCaseSearchTerm && sourceName.toLowerCase().includes(lowerCaseSearchTerm)
    );

    flowNodes.push({
      id: `script:${sourceName}`,
      type: 'scriptNode',
      position: { x: 0, y: 0 },
      data: {
        label: sourceName,
        sourceName,
        tablesRead: Array.from(tablesRead),
        tablesWritten: Array.from(tablesWritten),
        statementCount: stmts.length,
        isSelected: `script:${sourceName}` === selectedNodeId,
        isHighlighted,
      } satisfies ScriptNodeData,
    });
  });

  // Create edges between scripts that share tables
  const flowEdges: FlowEdge[] = [];
  const edgeSet = new Set<string>();

  scriptMap.forEach((producerStmts, producerScript) => {
    const producerTables = new Set<string>();
    producerStmts.forEach((stmt) => {
      stmt.nodes.forEach((node) => {
        if (node.type === 'table') {
          const isWritten = stmt.edges.some((e) => e.to === node.id);
          if (isWritten) {
            producerTables.add(node.label);
          }
        }
      });
    });

    scriptMap.forEach((consumerStmts, consumerScript) => {
      if (producerScript === consumerScript) return;

      const consumerTables = new Set<string>();
      consumerStmts.forEach((stmt) => {
        stmt.nodes.forEach((node) => {
          if (node.type === 'table') {
            const isRead = stmt.edges.some((e) => e.from === node.id);
            if (isRead) {
              consumerTables.add(node.label);
            }
          }
        });
      });

      // Find shared tables
      const sharedTables: string[] = [];
      producerTables.forEach((table) => {
        if (consumerTables.has(table)) {
          sharedTables.push(table);
        }
      });

      if (sharedTables.length > 0) {
        const edgeId = `${producerScript}->${consumerScript}`;
        if (!edgeSet.has(edgeId)) {
          edgeSet.add(edgeId);
          flowEdges.push({
            id: edgeId,
            source: `script:${producerScript}`,
            target: `script:${consumerScript}`,
            type: 'animated',
            label: sharedTables.slice(0, 2).join(', ') + (sharedTables.length > 2 ? '...' : ''),
          });
        }
      }
    });
  });

  return { nodes: flowNodes, edges: flowEdges };
}

/**
 * Build column-level nodes and edges showing column-to-column lineage.
 * Tables are shown with embedded columns (like table view), but edges represent
 * column-to-column relationships instead of table-to-table relationships.
 */
function buildColumnLevelGraph(
  statement: StatementLineage,
  selectedNodeId: string | null,
  searchTerm: string
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

  // Build table nodes with embedded columns (same as table view)
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
    const columns = tableColumnMap.get(node.id) || [];
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
      } satisfies TableNodeData,
    });
  }

  // Add virtual "Output" table node if there are output columns
  if (outputColumns.length > 0) {
    const outputNodeId = 'virtual:output';
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
        nodeType: 'table',
        columns: outputColumns,
        isSelected: outputNodeId === selectedNodeId,
        isHighlighted,
      } satisfies TableNodeData,
    });

    // Update columnToTableMap for output columns
    outputColumns.forEach((col) => {
      columnToTableMap.set(col.id, outputNodeId);
    });
  }

  // Build one edge per column lineage connection
  const flowEdges: FlowEdge[] = [];

  console.log('[Column View] All column lineage edges:',
    statement.edges
      .filter((e) => e.type === 'derivation' || e.type === 'data_flow')
      .map(e => {
        const from = columnNodes.find(c => c.id === e.from);
        const to = columnNodes.find(c => c.id === e.to);
        const fromTable = from ? columnToTableMap.get(from.id) : null;
        const toTable = to ? columnToTableMap.get(to.id) : null;
        return {
          from: from?.label,
          to: to?.label,
          fromTable: tableNodes.find(t => t.id === fromTable)?.label,
          toTable: tableNodes.find(t => t.id === toTable)?.label || (toTable === 'virtual:output' ? 'Output' : toTable),
          type: e.type,
          expression: e.expression
        };
      })
  );

  // Build a map of column name -> tables that have this column
  // This helps us find the CORRECT source table when columns appear in multiple tables
  const columnNameToTables = new Map<string, string[]>();
  for (const [columnId, tableId] of columnToTableMap.entries()) {
    const col = columnNodes.find(c => c.id === columnId);
    if (col) {
      const tables = columnNameToTables.get(col.label) || [];
      if (!tables.includes(tableId)) {
        tables.push(tableId);
      }
      columnNameToTables.set(col.label, tables);
    }
  }

  // Create one edge per column-to-column connection
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
      }
    });

  return { nodes: flowNodes, edges: flowEdges };
}

export function GraphView({ className, onNodeClick, graphContainerRef }: GraphViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, selectedNodeId, searchTerm, viewMode } = state;

  // Merge all statements into one big graph (for table and column modes)
  const statement = useMemo(() => {
    if (!result || !result.statements) return null;
    return mergeStatements(result.statements);
  }, [result]);

  const { layoutedNodes, layoutedEdges } = useMemo(() => {
    if (!result || !result.statements) return { layoutedNodes: [], layoutedEdges: [] };

    let rawNodes: FlowNode[];
    let rawEdges: FlowEdge[];
    let direction: 'LR' | 'TB' = 'LR';

    if (viewMode === 'script') {
      const graph = buildScriptLevelGraph(result.statements, selectedNodeId, searchTerm);
      rawNodes = graph.nodes;
      rawEdges = graph.edges;
      direction = 'LR';
    } else if (viewMode === 'column') {
      if (!statement) return { layoutedNodes: [], layoutedEdges: [] };
      const graph = buildColumnLevelGraph(statement, selectedNodeId, searchTerm);
      rawNodes = graph.nodes;
      rawEdges = graph.edges;
      direction = 'TB';
    } else {
      if (!statement) return { layoutedNodes: [], layoutedEdges: [] };
      rawNodes = buildFlowNodes(statement, selectedNodeId, searchTerm);
      rawEdges = buildFlowEdges(statement);
      direction = 'LR';
    }

    const { nodes: ln, edges: le } = getLayoutedElements(rawNodes, rawEdges, direction);
    return { layoutedNodes: ln, layoutedEdges: le };
  }, [result, statement, selectedNodeId, searchTerm, viewMode]);

  const [nodes, setNodes, onNodesChange] = useNodesState(layoutedNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(layoutedEdges);

  useEffect(() => {
    setNodes(layoutedNodes);
    setEdges(layoutedEdges);
  }, [layoutedNodes, layoutedEdges, setNodes, setEdges]);

  const handleNodeClick = useCallback(
    (_event: React.MouseEvent, node: FlowNode) => {
      actions.selectNode(node.id);
      if (statement) {
        const lineageNode = statement.nodes.find((n) => n.id === node.id);
        if (lineageNode) {
          if (lineageNode.span) {
            actions.highlightSpan(lineageNode.span);
          }
          onNodeClick?.(lineageNode);
        }
      }
    },
    [actions, statement, onNodeClick]
  );

  const handlePaneClick = useCallback(() => {
    actions.selectNode(null);
  }, [actions]);

  if (!result || !statement) {
    return (
      <div
        className={className}
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          height: '100%',
          color: '#9ca3af',
        }}
      >
        <p>No lineage data to display</p>
      </div>
    );
  }

  return (
    <div className={className} style={{ height: '100%' }} ref={graphContainerRef}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={handleNodeClick}
        onPaneClick={handlePaneClick}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        fitView
        minZoom={0.1}
        maxZoom={2}
      >
        <Background />
        <Controls />
        <MiniMap
          nodeColor={(node) => {
            const data = node.data as TableNodeData;
            return data?.nodeType === 'cte' ? '#a855f7' : '#3b82f6';
          }}
        />
      </ReactFlow>
    </div>
  );
}