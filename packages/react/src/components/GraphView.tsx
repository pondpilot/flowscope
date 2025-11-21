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
  getSmoothStepPath,
} from '@xyflow/react';
import type { Node as FlowNode, Edge as FlowEdge, NodeProps, EdgeProps } from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { useLineage } from '../context';
import type { GraphViewProps, TableNodeData, ColumnNodeInfo } from '../types';
import type { Node, Edge, StatementLineage } from '@pondpilot/flowscope-core';
import { getLayoutedElements } from '../utils/layout';
import { sanitizeIdentifier } from '../utils/sanitize';

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
      <Handle
        type="target"
        position={Position.Left}
        style={{
          width: 10,
          height: 10,
          backgroundColor: colors.accent,
          border: '2px solid white',
        }}
      />
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
        <div style={{ padding: '6px 12px', maxHeight: 150, overflowY: 'auto' }}>
          {nodeData.columns.map((col: ColumnNodeInfo) => (
            <div
              key={col.id}
              style={{
                fontSize: 12,
                color: palette.textSecondary,
                padding: '3px 4px',
                borderRadius: 4,
              }}
            >
              {sanitizeIdentifier(col.name)}
            </div>
          ))}
        </div>
      )}
      <Handle
        type="source"
        position={Position.Right}
        style={{
          width: 10,
          height: 10,
          backgroundColor: colors.accent,
          border: '2px solid white',
        }}
      />
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
}: EdgeProps): JSX.Element {
  const [edgePath] = getSmoothStepPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  });

  const isDataFlow = data?.type === 'data_flow';

  return (
    <BaseEdge
      id={id}
      path={edgePath}
      markerEnd={markerEnd}
      style={{
        stroke: isDataFlow ? colors.accent : '#94a3b8',
        strokeWidth: 2,
        strokeDasharray: isDataFlow ? '5,5' : undefined,
      }}
    />
  );
}

const nodeTypes = {
  tableNode: TableNode,
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

export function GraphView({ className, onNodeClick, graphContainerRef }: GraphViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, selectedNodeId, searchTerm } = state;

  // Merge all statements into one big graph
  const statement = useMemo(() => {
    if (!result || !result.statements) return null;
    return mergeStatements(result.statements);
  }, [result]);

  const { layoutedNodes, layoutedEdges } = useMemo(() => {
    if (!statement) return { layoutedNodes: [], layoutedEdges: [] };
    const rawNodes = buildFlowNodes(statement, selectedNodeId, searchTerm);
    const rawEdges = buildFlowEdges(statement);
    const { nodes: ln, edges: le } = getLayoutedElements(rawNodes, rawEdges, 'LR');
    return { layoutedNodes: ln, layoutedEdges: le };
  }, [statement, selectedNodeId, searchTerm]);

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