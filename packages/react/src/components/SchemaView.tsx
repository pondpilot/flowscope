import { useMemo, useEffect } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  Handle,
  Position,
} from '@xyflow/react';
import type { Node as FlowNode, NodeProps } from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { getLayoutedElements } from '../utils/layout';
import { COLORS, GRAPH_CONFIG } from '../constants';
import type { SchemaTable, ResolvedSchemaTable, ColumnSchema, SchemaOrigin } from '@pondpilot/flowscope-core';

interface SchemaViewProps {
  schema: (SchemaTable | ResolvedSchemaTable)[];
}

interface SchemaTableNodeData extends Record<string, unknown> {
  label: string;
  columns: ColumnSchema[];
  origin?: SchemaOrigin;
}

function SchemaTableNode({ data }: NodeProps<FlowNode<SchemaTableNodeData>>): JSX.Element {
  // Color coding based on origin: imported (table palette) vs implied (cte palette)
  const palette = data.origin === 'imported' ? COLORS.nodes.table : COLORS.nodes.cte;

  return (
    <div
      style={{
        minWidth: 180,
        borderRadius: 8,
        border: `1px solid ${palette.border}`,
        boxShadow: '0 1px 3px rgba(0,0,0,0.1)',
        overflow: 'hidden',
        backgroundColor: palette.bg,
        fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
      }}
    >
      <Handle type="target" position={Position.Left} style={{ opacity: 0 }} />
      <div
        style={{
          padding: '8px 12px',
          fontSize: 12,
          fontWeight: 500,
          borderBottom: `1px solid ${palette.border}`,
          backgroundColor: palette.headerBg,
          color: palette.text,
        }}
      >
        <span style={{ fontWeight: 600 }}>{data.label}</span>
      </div>
      {(data.columns || []).length > 0 && (
        <div style={{ padding: '6px 12px', maxHeight: GRAPH_CONFIG.MAX_COLUMN_HEIGHT, overflowY: 'auto' }}>
          {(data.columns || []).map((col: ColumnSchema) => (
            <div
              key={col.name}
              style={{
                fontSize: 12,
                color: palette.textSecondary,
                padding: '3px 4px',
                borderRadius: 4,
              }}
            >
              {col.name} <span style={{ opacity: 0.6 }}>({col.dataType})</span>
            </div>
          ))}
        </div>
      )}
      <Handle type="source" position={Position.Right} style={{ opacity: 0 }} />
    </div>
  );
}

const nodeTypes = {
  schemaTableNode: SchemaTableNode,
};

function buildSchemaFlowNodes(schema: (SchemaTable | ResolvedSchemaTable)[]): FlowNode[] {
  return schema.map((table) => {
    // Type guard to check if table is ResolvedSchemaTable
    const isResolvedTable = (t: SchemaTable | ResolvedSchemaTable): t is ResolvedSchemaTable => {
      return 'origin' in t;
    };

    return {
      id: table.name,
      type: 'schemaTableNode',
      position: { x: 0, y: 0 },
      data: {
        label: table.name,
        columns: table.columns || [],
        origin: isResolvedTable(table) ? table.origin : undefined,
      } satisfies SchemaTableNodeData,
    };
  });
}

export function SchemaView({ schema }: SchemaViewProps): JSX.Element {
  const { layoutedNodes, layoutedEdges } = useMemo(() => {
    if (schema.length === 0) return { layoutedNodes: [], layoutedEdges: [] };
    const rawNodes = buildSchemaFlowNodes(schema);
    const { nodes: ln, edges: le } = getLayoutedElements(rawNodes, [], 'TB'); // 'TB' for top-to-bottom layout
    return { layoutedNodes: ln, layoutedEdges: le };
  }, [schema]);

  const [nodes, setNodes, onNodesChange] = useNodesState(layoutedNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(layoutedEdges);

  useEffect(() => {
    setNodes(layoutedNodes);
    setEdges(layoutedEdges);
  }, [layoutedNodes, layoutedEdges, setNodes, setEdges]);

  if (schema.length === 0) {
    return (
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          height: '100%',
          color: '#9ca3af',
        }}
      >
        <p>No schema data to display</p>
      </div>
    );
  }

  return (
    <div style={{ height: '100%' }}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        nodeTypes={nodeTypes}
        fitView
        minZoom={0.1}
        maxZoom={2}
      >
        <Background />
        <Controls />
        <MiniMap
          nodeColor={(node) => {
            const origin = (node.data as SchemaTableNodeData)?.origin;
            return origin === 'imported' ? COLORS.nodes.table.accent : COLORS.nodes.cte.accent;
          }}
        />
      </ReactFlow>
    </div>
  );
}
