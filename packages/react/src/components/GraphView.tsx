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
import type { Node, StatementLineage } from '@pondpilot/flowscope-core';
import { getLayoutedElements } from '../utils/layout';

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
        backgroundColor: palette.bg,
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
        <span style={{ fontWeight: 600 }}>{nodeData.label}</span>
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
              {col.name}
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

function buildFlowNodes(
  statement: StatementLineage,
  selectedNodeId: string | null
): FlowNode[] {
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
    flowNodes.push({
      id: node.id,
      type: 'tableNode',
      position: { x: 0, y: 0 },
      data: {
        label: node.label,
        nodeType: node.type === 'cte' ? 'cte' : 'table',
        columns: tableColumnMap.get(node.id) || [],
        isSelected: node.id === selectedNodeId,
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

export function GraphView({ className, onNodeClick }: GraphViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, selectedStatementIndex, selectedNodeId } = state;

  const statement = result?.statements[selectedStatementIndex];

  const { layoutedNodes, layoutedEdges } = useMemo(() => {
    if (!statement) return { layoutedNodes: [], layoutedEdges: [] };
    const rawNodes = buildFlowNodes(statement, selectedNodeId);
    const rawEdges = buildFlowEdges(statement);
    const { nodes: ln, edges: le } = getLayoutedElements(rawNodes, rawEdges, 'LR');
    return { layoutedNodes: ln, layoutedEdges: le };
  }, [statement, selectedNodeId]);

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
    <div className={className} style={{ height: '100%' }}>
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
