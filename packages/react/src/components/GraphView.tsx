import { useMemo, useCallback, useEffect, useRef, useState } from 'react';
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
import { Search, Network } from 'lucide-react';

import { useLineage, useLineageActions } from '../context';
import type {
  GraphViewProps,
  TableNodeData,
  ColumnNodeInfo,
  ScriptNodeData,
  StatementLineageWithSource,
} from '../types';
import type { Node, Edge, StatementLineage } from '@pondpilot/flowscope-core';
import { getLayoutedElements } from '../utils/layout';
import { sanitizeIdentifier } from '../utils/sanitize';
import { findConnectedElements } from '../utils/graphTraversal';
import { ScriptNode } from './ScriptNode';
import { ColumnNode } from './ColumnNode';
import { Input } from './ui/input';
import { ExportMenu } from './ExportMenu';
import { ViewModeSelector } from './ViewModeSelector';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipProvider,
  GraphTooltipTrigger,
  GraphTooltipArrow,
  GraphTooltipPortal,
} from './ui/graph-tooltip';
import { UI_CONSTANTS, GRAPH_CONFIG, COLORS } from '../constants';

// Type guards for safer type checking
function isTableNodeData(data: unknown): data is TableNodeData {
  return (
    typeof data === 'object' &&
    data !== null &&
    'label' in data &&
    'nodeType' in data &&
    'columns' in data
  );
}

// Constants are now imported from '../constants'
const colors = COLORS;

function TableNode({ id, data, selected }: NodeProps): JSX.Element {
  const { toggleNodeCollapse } = useLineageActions();

  if (!isTableNodeData(data)) {
    console.error('Invalid node data type for TableNode', data);
    return <div>Invalid node data</div>;
  }

  const nodeData = data;
  const isCte = nodeData.nodeType === 'cte';
  const isVirtualOutput = nodeData.nodeType === 'virtualOutput';
  const isSelected = selected || nodeData.isSelected;
  const isHighlighted = nodeData.isHighlighted;
  const isCollapsed = nodeData.isCollapsed;

  let palette: typeof colors.table | typeof colors.cte | typeof colors.virtualOutput = colors.table;
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
            e.stopPropagation();
            toggleNodeCollapse(id);
          }}
          style={{
            background: 'none',
            border: 'none',
            cursor: 'pointer',
            padding: 8,
            margin: -8,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            color: palette.textSecondary,
            borderRadius: 4,
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
        <div style={{ padding: '6px 12px', maxHeight: GRAPH_CONFIG.MAX_COLUMN_HEIGHT, overflowY: 'auto', position: 'relative' }}>
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
                    opacity: 0,
                    border: 'none',
                    background: 'transparent',
                  }}
                />
                {sanitizeIdentifier(col.name)}
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
                    opacity: 0,
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
      {expression && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: 'absolute',
              transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)`,
              pointerEvents: 'all',
              zIndex: 1000,
            }}
          >
            <GraphTooltipProvider>
              <GraphTooltip delayDuration={GRAPH_CONFIG.TOOLTIP_DELAY}>
                <GraphTooltipTrigger asChild>
                  <button
                    type="button"
                    aria-label="View expression details"
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
                      padding: 0,
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
                  </button>
                </GraphTooltipTrigger>
                <GraphTooltipContent side="top">
                  {tooltipContent}
                  <GraphTooltipArrow />
                </GraphTooltipContent>
              </GraphTooltip>
            </GraphTooltipProvider>
          </div>
        </EdgeLabelRenderer>
      )}
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

function mergeStatements(statements: StatementLineage[]): StatementLineage {
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

    const isCollapsed = collapsedNodeIds.has(node.id);

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
        isCollapsed: isCollapsed,
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
  statements: StatementLineageWithSource[],
  selectedNodeId: string | null,
  searchTerm: string
): { nodes: FlowNode[]; edges: FlowEdge[] } {
  const lowerCaseSearchTerm = searchTerm.toLowerCase();

  const scriptMap = new Map<string, StatementLineageWithSource[]>();
  statements.forEach((stmt) => {
    const sourceName = stmt.sourceName || 'unknown';
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
          // A table is written if it has incoming data flow edges
          // OR if it's a CREATE_TABLE statement (which defines the table without data flow)
          const isWritten = 
            stmt.edges.some((e) => e.to === node.id && e.type === 'data_flow') ||
            (stmt.statementType === 'CREATE_TABLE');

          const isRead = stmt.edges.some((e) => e.from === node.id && e.type === 'data_flow');

          // For display in tooltip, use human readable label
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
          const isWritten = 
            stmt.edges.some((e) => e.to === node.id) ||
            (stmt.statementType === 'CREATE_TABLE');
            
          if (isWritten) {
            // Use qualifiedName for matching if available, else label
            producerTables.add(node.qualifiedName || node.label);
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
              consumerTables.add(node.qualifiedName || node.label);
            }
          }
        });
      });

      const sharedTables: string[] = [];
      producerTables.forEach((table) => {
        if (consumerTables.has(table)) {
          // Convert back to simple name for label if it looks like a qualified name
          const simpleName = table.split('.').pop() || table;
          sharedTables.push(simpleName);
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
        isCollapsed: collapsedNodeIds.has(node.id),
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
      }
    });

  return { nodes: flowNodes, edges: flowEdges };
}

function enhanceGraphWithHighlights(
  graph: { nodes: FlowNode[]; edges: FlowEdge[] },
  highlightIds: Set<string>
): { nodes: FlowNode[]; edges: FlowEdge[] } {
  const enhancedNodes = graph.nodes.map(node => {
    if (!isTableNodeData(node.data)) return node;

    const nodeData = node.data;
    if (!nodeData.columns) return node;

    const enhancedColumns = nodeData.columns.map(col => ({
      ...col,
      isHighlighted: highlightIds.has(col.id),
    }));

    return {
      ...node,
      data: {
        ...nodeData,
        columns: enhancedColumns,
        isSelected: nodeData.isSelected || highlightIds.has(node.id),
      },
    };
  });

  const enhancedEdges = graph.edges.map(edge => ({
    ...edge,
    animated: highlightIds.has(edge.id),
    zIndex: highlightIds.has(edge.id) ? GRAPH_CONFIG.HIGHLIGHTED_EDGE_Z_INDEX : 0,
    data: {
      ...edge.data,
      isHighlighted: highlightIds.has(edge.id),
    },
  }));

  return { nodes: enhancedNodes, edges: enhancedEdges };
}

export function GraphView({ className, onNodeClick, graphContainerRef }: GraphViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, selectedNodeId, searchTerm, viewMode, collapsedNodeIds } = state;

  // Local state for debounced search input
  const [localSearchTerm, setLocalSearchTerm] = useState(searchTerm);

  // Debounce search updates to global state
  useEffect(() => {
    const handler = setTimeout(() => {
      actions.setSearchTerm(localSearchTerm);
    }, UI_CONSTANTS.SEARCH_DEBOUNCE_DELAY);

    return () => {
      clearTimeout(handler);
    };
  }, [localSearchTerm, actions]);

  // Sync local state if global state changes externally
  useEffect(() => {
    setLocalSearchTerm(searchTerm);
  }, [searchTerm]);

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

      const graph = buildColumnLevelGraph(statement, selectedNodeId, searchTerm, collapsedNodeIds);

      let highlightIds = new Set<string>();
      if (selectedNodeId) {
        highlightIds = findConnectedElements(selectedNodeId, graph.edges);
      }

      const enhancedGraph = enhanceGraphWithHighlights(graph, highlightIds);
      rawNodes = enhancedGraph.nodes;
      rawEdges = enhancedGraph.edges;
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

  const [nodes, setNodes, onNodesChange] = useNodesState<FlowNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<FlowEdge>([]);

  const isInitialized = useRef(false);
  const lastResultId = useRef<string | null>(null);
  const lastViewMode = useRef<string | null>(null);

  useEffect(() => {
    const currentResultId = result ? JSON.stringify(result.summary) : null;
    
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
      setNodes((currentNodes) => {
        return layoutedNodes.map((layoutNode) => {
          const currentNode = currentNodes.find((n) => n.id === layoutNode.id);
          if (currentNode) {
            return {
              ...layoutNode,
              position: currentNode.position,
            };
          }
          return layoutNode;
        });
      });
      setEdges(layoutedEdges);
    }
  }, [layoutedNodes, layoutedEdges, setNodes, setEdges, result, viewMode, collapsedNodeIds]);

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
        <Panel position="top-left" className="flex gap-3">
          <div className="flex items-center rounded-lg border border-slate-200/60 bg-white px-1 py-1 shadow-sm backdrop-blur-sm">
            <ViewModeSelector />
          </div>
          <div className="relative flex items-center rounded-lg border border-slate-200/60 bg-white px-2 py-1 shadow-sm backdrop-blur-sm" style={{ minWidth: UI_CONSTANTS.SEARCH_MIN_WIDTH }}>
            <Search className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-slate-400" strokeWidth={1.5} />
            <Input
              placeholder="Search..."
              value={localSearchTerm}
              onChange={(e) => setLocalSearchTerm(e.target.value)}
              className="h-8 border-0 bg-transparent pl-8 pr-2 text-sm shadow-none placeholder:text-slate-400 focus-visible:ring-0"
            />
          </div>
        </Panel>
        <Panel position="top-right" className="flex gap-3">
          <div className="flex items-center rounded-lg border border-slate-200/60 bg-white px-1 py-1 shadow-sm backdrop-blur-sm">
            <GraphTooltipProvider>
              <GraphTooltip delayDuration={UI_CONSTANTS.TOOLTIP_DELAY}>
                <GraphTooltipTrigger asChild>
                  <button
                    onClick={handleRearrange}
                    className="flex h-8 w-8 shrink-0 items-center justify-center rounded text-slate-500 transition-colors hover:bg-slate-50 hover:text-slate-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
                    aria-label="Rearrange graph layout"
                  >
                    <Network className="h-4 w-4" strokeWidth={1.5} />
                  </button>
                </GraphTooltipTrigger>
                <GraphTooltipPortal>
                  <GraphTooltipContent side="bottom">
                    <p>Rearrange layout</p>
                    <GraphTooltipArrow />
                  </GraphTooltipContent>
                </GraphTooltipPortal>
              </GraphTooltip>
            </GraphTooltipProvider>
          </div>
          <div className="flex items-center rounded-lg border border-slate-200/60 bg-white px-1 py-1 shadow-sm backdrop-blur-sm">
            <ExportMenu graphRef={finalRef} />
          </div>
        </Panel>
        <MiniMap
          nodeColor={(node) => {
            if (isTableNodeData(node.data)) {
              return node.data.nodeType === 'cte' ? '#a855f7' : '#3b82f6';
            }
            return '#3b82f6';
          }}
        />
      </ReactFlow>
    </div>
  );
}