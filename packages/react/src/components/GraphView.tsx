import { useMemo, useCallback, useEffect, useRef } from 'react';
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
  EdgeLabelRenderer,
  getBezierPath,
  Panel,
} from '@xyflow/react';
import type { Node as FlowNode, Edge as FlowEdge, NodeProps, EdgeProps } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import * as Tooltip from '@radix-ui/react-tooltip';

import { useLineage, useLineageActions } from '../context';
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
import { Button } from './ui/button';
import { Input } from './ui/input';
import { ExportMenu } from './ExportMenu';

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
  virtualOutput: {
    bg: '#F0FDF4',
    headerBg: '#DCFCE7',
    border: '#6EE7B7',
    text: '#047857',
    textSecondary: '#065F46',
  },
  accent: '#4C61FF',
};

function TableNode({ id, data, selected }: NodeProps): JSX.Element {
  const { toggleNodeCollapse } = useLineageActions();
  const nodeData = data as TableNodeData;
  const isCte = nodeData.nodeType === 'cte';
  const isVirtualOutput = nodeData.nodeType === 'virtualOutput';
  const isSelected = selected || nodeData.isSelected;
  const isHighlighted = nodeData.isHighlighted;
  const isCollapsed = nodeData.isCollapsed;

  let palette = colors.table;
  if (isCte) {
    palette = colors.cte;
  } else if (isVirtualOutput) {
    palette = colors.virtualOutput;
  }

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
          borderBottom: isCollapsed ? 'none' : `1px solid ${palette.border}`,
          backgroundColor: palette.headerBg,
          color: palette.text,
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          position: 'relative',
        }}
      >
        {/* Handles for collapsed state */}
        {isCollapsed && (
          <>
            <Handle
              type="target"
              position={Position.Left}
              style={{ opacity: 0, border: 'none', background: 'transparent' }}
            />
            <Handle
              type="source"
              position={Position.Right}
              style={{ opacity: 0, border: 'none', background: 'transparent' }}
            />
          </>
        )}
        
        <button
          onClick={(e) => {
            e.stopPropagation(); // Prevent node selection when toggling
            toggleNodeCollapse(id);
          }}
          style={{
            background: 'none',
            border: 'none',
            cursor: 'pointer',
            padding: 0,
            display: 'flex',
            alignItems: 'center',
            color: palette.textSecondary,
          }}
        >
          {isCollapsed ? (
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M9 18l6-6-6-6" />
            </svg>
          ) : (
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M6 9l6 6 6-6" />
            </svg>
          )}
        </button>

        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{
              textTransform: 'uppercase',
              fontSize: 10,
              opacity: 0.6,
              fontWeight: 600,
              lineHeight: 1,
              marginBottom: 2,
            }}
          >
            {isVirtualOutput ? 'OUTPUT' : nodeData.nodeType}
          </div>
          <div style={{ fontWeight: 600, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
            {sanitizeIdentifier(nodeData.label)}
          </div>
        </div>
      </div>
      
      {!isCollapsed && nodeData.columns.length > 0 && (
        <div style={{ padding: '6px 12px', maxHeight: 1000, overflowY: 'auto', position: 'relative' }}>
          {nodeData.columns.map((col: ColumnNodeInfo) => {
            return (
              <div
                key={col.id}
                style={{
                  fontSize: 12,
                  color: col.isHighlighted ? colors.accent : palette.textSecondary,
                  fontWeight: col.isHighlighted ? 600 : 400,
                  backgroundColor: col.isHighlighted ? `${colors.accent}10` : 'transparent',
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
                    left: -4,
                    top: '50%',
                    transform: 'translateY(-50%)',
                    opacity: 0, // Hide handle visually but keep functional
                    border: 'none',
                    background: 'transparent',
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
                    right: -4,
                    top: '50%',
                    transform: 'translateY(-50%)',
                    opacity: 0, // Hide handle visually but keep functional
                    border: 'none',
                    background: 'transparent',
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
  const isHighlighted = data?.isHighlighted as boolean | undefined;

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
          stroke: isHighlighted ? colors.accent : (style?.stroke || '#b1b1b7'),
          strokeWidth: isHighlighted ? 3 : 2,
          opacity: isHighlighted ? 1 : 0.5,
          ...style,
        }}
      />
      {/* Use EdgeLabelRenderer to render HTML content on top of SVG */}
      {expression && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: 'absolute',
              transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)`,
              pointerEvents: 'all', // Re-enable pointer events for the tooltip trigger
              zIndex: 1000, // Ensure it's above edges
            }}
          >
            <Tooltip.Provider>
              <Tooltip.Root delayDuration={0}>
                <Tooltip.Trigger asChild>
                  <div
                    style={{
                      cursor: 'help',
                      backgroundColor: 'white',
                      border: `2px solid ${colors.accent}`,
                      borderRadius: '50%',
                      width: 20,
                      height: 20,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                    }}
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="none">
                      <path
                        d="M13 2L3 14H12L11 22L21 10H12L13 2Z"
                        fill={colors.accent}
                        stroke={colors.accent}
                        strokeWidth="2"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                    </svg>
                  </div>
                </Tooltip.Trigger>
                <Tooltip.Portal>
                  <Tooltip.Content
                    side="top"
                    sideOffset={5}
                    style={{
                      backgroundColor: '#333',
                      color: 'white',
                      padding: '8px 12px',
                      borderRadius: 4,
                      fontSize: 12,
                      whiteSpace: 'pre-wrap',
                      maxWidth: 300,
                      zIndex: 9999,
                      boxShadow: '0 2px 10px rgba(0,0,0,0.2)',
                    }}
                  >
                    {tooltipContent}
                    <Tooltip.Arrow style={{ fill: '#333' }} />
                  </Tooltip.Content>
                </Tooltip.Portal>
              </Tooltip.Root>
            </Tooltip.Provider>
          </div>
        </EdgeLabelRenderer>
      )}
      {/* Simple title for non-expression edges, if needed, though typically less critical */}
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
    statementType: 'SELECT',
    nodes: Array.from(mergedNodes.values()),
    edges: Array.from(mergedEdges.values()),
  };
}

function buildFlowNodes(
  statement: StatementLineage,
  selectedNodeId: string | null,
  searchTerm: string,
  collapsedNodeIds: Set<string>
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
        isCollapsed: collapsedNodeIds.has(node.id),
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

function buildScriptLevelGraph(
  statements: StatementLineage[],
  selectedNodeId: string | null,
  searchTerm: string
): { nodes: FlowNode[]; edges: FlowEdge[] } {
  const lowerCaseSearchTerm = searchTerm.toLowerCase();

  const scriptMap = new Map<string, StatementLineage[]>();
  statements.forEach((stmt) => {
    const sourceName = (stmt as any).source_name || 'unknown';
    const existing = scriptMap.get(sourceName) || [];
    existing.push(stmt);
    scriptMap.set(sourceName, existing);
  });

  const flowNodes: FlowNode[] = [];

  scriptMap.forEach((stmts, sourceName) => {
    const tablesRead = new Set<string>();
    const tablesWritten = new Set<string>();

    stmts.forEach((stmt) => {
      stmt.nodes.forEach((node) => {
        if (node.type === 'table') {
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

function buildColumnLevelGraph(
  statement: StatementLineage,
  selectedNodeId: string | null,
  searchTerm: string,
  pathHighlightIds: Set<string>,
  collapsedNodeIds: Set<string>
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
          isHighlighted: pathHighlightIds.has(childNode.id),
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
        isHighlighted: pathHighlightIds.has(node.id),
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
        isSelected: node.id === selectedNodeId || pathHighlightIds.has(node.id),
        isHighlighted: isHighlighted,
        isCollapsed: collapsedNodeIds.has(node.id),
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
          const isEdgeHighlighted = pathHighlightIds.has(edge.id);

          const isSourceCollapsed = collapsedNodeIds.has(sourceTableId);
          const isTargetCollapsed = collapsedNodeIds.has(targetTableId);

          flowEdges.push({
            id: edge.id,
            source: sourceTableId,
            target: targetTableId,
            sourceHandle: isSourceCollapsed ? null : edge.from,
            targetHandle: isTargetCollapsed ? null : edge.to,
            type: 'animated',
            animated: isEdgeHighlighted,
            zIndex: isEdgeHighlighted ? 1000 : 0,
            data: {
              type: edge.type,
              expression: edge.expression || targetCol.expression,
              sourceColumn: sourceCol.label,
              targetColumn: targetCol.label,
              isDerived: isDerivedColumn,
              isHighlighted: isEdgeHighlighted,
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

// --- Path Highlighting Logic ---

/**
 * Traverse the graph to find all connected elements (nodes/edges) upstream and downstream.
 */
function findConnectedElements(
  startId: string,
  edges: FlowEdge[]
): Set<string> {
  const visited = new Set<string>();
  const queue: string[] = [startId];
  visited.add(startId);

  // Build adjacency list for simpler traversal
  const downstreamMap = new Map<string, string[]>(); // source -> targets
  const upstreamMap = new Map<string, string[]>();   // target -> sources
  const edgeMap = new Map<string, FlowEdge>();

  edges.forEach(edge => {
    edgeMap.set(edge.id, edge);
    
    // Map using handles (column IDs) as these are the true nodes in column view
    const source = edge.sourceHandle || edge.source;
    const target = edge.targetHandle || edge.target;

    if (!downstreamMap.has(source)) downstreamMap.set(source, []);
    downstreamMap.get(source)?.push(edge.id);

    if (!upstreamMap.has(target)) upstreamMap.set(target, []);
    upstreamMap.get(target)?.push(edge.id);
  });

  // Forward traversal (Downstream)
  const forwardQueue = [startId];
  const forwardVisited = new Set<string>([startId]);
  while (forwardQueue.length > 0) {
    const currentId = forwardQueue.shift()!;
    // If current is an edge ID, move to its target
    if (edgeMap.has(currentId)) {
      const edge = edgeMap.get(currentId)!;
      const target = edge.targetHandle || edge.target;
      if (!forwardVisited.has(target)) {
        forwardVisited.add(target);
        visited.add(target);
        forwardQueue.push(target);
      }
    } else {
      // Current is a node (column) ID, find outgoing edges
      const outgoingEdges = downstreamMap.get(currentId) || [];
      outgoingEdges.forEach(edgeId => {
        if (!forwardVisited.has(edgeId)) {
          forwardVisited.add(edgeId);
          visited.add(edgeId);
          forwardQueue.push(edgeId);
        }
      });
    }
  }

  // Backward traversal (Upstream)
  const backwardQueue = [startId];
  const backwardVisited = new Set<string>([startId]);
  while (backwardQueue.length > 0) {
    const currentId = backwardQueue.shift()!;
    // If current is an edge ID, move to its source
    if (edgeMap.has(currentId)) {
      const edge = edgeMap.get(currentId)!;
      const source = edge.sourceHandle || edge.source;
      if (!backwardVisited.has(source)) {
        backwardVisited.add(source);
        visited.add(source);
        backwardQueue.push(source);
      }
    } else {
      // Current is a node (column) ID, find incoming edges
      const incomingEdges = upstreamMap.get(currentId) || [];
      incomingEdges.forEach(edgeId => {
        if (!backwardVisited.has(edgeId)) {
          backwardVisited.add(edgeId);
          visited.add(edgeId);
          backwardQueue.push(edgeId);
        }
      });
    }
  }

  return visited;
}

export function GraphView({ className, onNodeClick, graphContainerRef }: GraphViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, selectedNodeId, searchTerm, viewMode, collapsedNodeIds } = state;

  // Merge all statements into one big graph (for table and column modes)
  const statement = useMemo(() => {
    if (!result || !result.statements) return null;
    return mergeStatements(result.statements);
  }, [result]);

  // Memoize layout calculation separately from state updates to prevent "jumping"
  // We only want to calculate layout when the underlying data structure changes,
  // NOT when the user drags a node (which updates local node state).
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
      
      // Pass 1: Build basic graph to get edges for highlight calculation
      const tempGraph = buildColumnLevelGraph(statement, selectedNodeId, searchTerm, new Set(), collapsedNodeIds);
      
      let highlightIds = new Set<string>();
      if (selectedNodeId) {
        highlightIds = findConnectedElements(selectedNodeId, tempGraph.edges);
      }

      // Pass 2: Build graph with highlight info
      const graph = buildColumnLevelGraph(statement, selectedNodeId, searchTerm, highlightIds, collapsedNodeIds);
      rawNodes = graph.nodes;
      rawEdges = graph.edges;
      direction = 'LR';
    } else {
      if (!statement) return { layoutedNodes: [], layoutedEdges: [] };
      rawNodes = buildFlowNodes(statement, selectedNodeId, searchTerm, collapsedNodeIds);
      rawEdges = buildFlowEdges(statement);
      direction = 'LR';
    }

    const { nodes: ln, edges: le } = getLayoutedElements(rawNodes, rawEdges, direction);
    return { layoutedNodes: ln, layoutedEdges: le };
  }, [result, statement, selectedNodeId, searchTerm, viewMode, collapsedNodeIds]);

  // Initialize local state with layouted nodes
  const [nodes, setNodes, onNodesChange] = useNodesState([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState([]);

  // Ref to track if we have loaded initial layout
  const isInitialized = useRef(false);
  const lastResultId = useRef<string | null>(null);
  const lastViewMode = useRef<string | null>(null);

  // Update nodes/edges when layout changes, but ONLY if the underlying data structure
  // or view mode has changed. This prevents resetting positions on simple re-renders.
  useEffect(() => {
    const currentResultId = result ? JSON.stringify(result.summary) : null; // Simple content check
    
    const needsUpdate = 
      !isInitialized.current || 
      currentResultId !== lastResultId.current ||
      viewMode !== lastViewMode.current;

    if (needsUpdate && layoutedNodes.length > 0) {
      setNodes(layoutedNodes);
      setEdges(layoutedEdges);
      isInitialized.current = true;
      lastResultId.current = currentResultId;
      lastViewMode.current = viewMode;
    } else if (layoutedNodes.length > 0) {
      // Even if structure didn't change, we might need to update styling (highlighting)
      // We map over existing nodes to preserve positions but update data
      setNodes((currentNodes) => {
        return layoutedNodes.map((layoutNode) => {
          const currentNode = currentNodes.find((n) => n.id === layoutNode.id);
          if (currentNode) {
            // Preserve position, update data/style
            return {
              ...layoutNode,
              position: currentNode.position,
            };
          }
          return layoutNode;
        });
      });
      // Edges can always be replaced as they don't carry position state
      setEdges(layoutedEdges);
    }
  }, [layoutedNodes, layoutedEdges, setNodes, setEdges, result, viewMode]);

  // Force layout recalculation handler
  const handleRearrange = useCallback(() => {
    setNodes(layoutedNodes);
    setEdges(layoutedEdges);
  }, [layoutedNodes, layoutedEdges, setNodes, setEdges]);

  const internalGraphRef = useRef<HTMLDivElement>(null);
  const finalRef = graphContainerRef || internalGraphRef;

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

  const handleEdgeClick = useCallback(
    (_event: React.MouseEvent, edge: FlowEdge) => {
      actions.selectNode(edge.id);
    },
    [actions]
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
    <div className={className} style={{ height: '100%' }} ref={finalRef}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={handleNodeClick}
        onEdgeClick={handleEdgeClick}
        onPaneClick={handlePaneClick}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        fitView
        minZoom={0.1}
        maxZoom={2}
      >
        <Background />
        <Controls />
        <Panel position="top-left">
          <div style={{ width: 250 }}>
            <Input
              placeholder="Search nodes..."
              value={searchTerm}
              onChange={(e) => actions.setSearchTerm(e.target.value)}
              className="bg-white shadow-sm border-gray-200"
            />
          </div>
        </Panel>
        <Panel position="top-right" style={{ display: 'flex', gap: '8px' }}>
          <ExportMenu graphRef={finalRef} />
          <Button 
            onClick={handleRearrange} 
            variant="secondary" 
            size="sm"
            className="shadow-sm"
          >
            Rearrange
          </Button>
        </Panel>
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
