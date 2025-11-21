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
import type { SchemaTable, ColumnSchema } from '@pondpilot/flowscope-core';

interface SchemaViewProps {
  schema: SchemaTable[];
}

interface SchemaTableNodeData extends Record<string, unknown> {
  label: string;
  columns: ColumnSchema[];
}

function SchemaTableNode({ data }: NodeProps<FlowNode<SchemaTableNodeData>>): JSX.Element {
  return (
    <div
      style={{
        minWidth: 180,
        borderRadius: 8,
        border: '1px solid #DBDDE1',
        boxShadow: '0 1px 3px rgba(0,0,0,0.1)',
        overflow: 'hidden',
        backgroundColor: '#FFFFFF',
        fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
      }}
    >
      <Handle type="target" position={Position.Left} style={{ opacity: 0 }} />
      <div
        style={{
          padding: '8px 12px',
          fontSize: 12,
          fontWeight: 500,
          borderBottom: '1px solid #DBDDE1',
          backgroundColor: '#F2F4F8',
          color: '#212328',
        }}
      >
        <span style={{ fontWeight: 600 }}>{data.label}</span>
      </div>
      {(data.columns || []).length > 0 && (
        <div style={{ padding: '6px 12px', maxHeight: 150, overflowY: 'auto' }}>
          {(data.columns || []).map((col: ColumnSchema) => (
            <div
              key={col.name}
              style={{
                fontSize: 12,
                color: '#6F7785',
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

function buildSchemaFlowNodes(schema: SchemaTable[]): FlowNode[] {
  return schema.map((table) => ({
    id: table.name,
    type: 'schemaTableNode',
    position: { x: 0, y: 0 },
    data: {
      label: table.name,
      columns: table.columns || [],
    } satisfies SchemaTableNodeData,
  }));
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
          nodeColor={() => '#3b82f6'}
        />
      </ReactFlow>
    </div>
  );
}
