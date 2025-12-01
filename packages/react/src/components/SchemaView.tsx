import { useMemo, useEffect, useCallback } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  ReactFlowProvider,
  Handle,
  Position,
} from '@xyflow/react';
import type { Node as FlowNode, Edge as FlowEdge, NodeProps } from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { getLayoutedElements } from '../utils/layout';
import { useNodeFocus } from '../hooks/useNodeFocus';
import {
  collectTableLookupKeys,
  resolveForeignKeyTarget,
} from '../utils/schemaUtils';
import { COLORS, CONSTRAINT_STYLES, GRAPH_CONFIG } from '../constants';
import type {
  SchemaTable,
  ResolvedSchemaTable,
  ColumnSchema,
  ResolvedColumnSchema,
  SchemaOrigin,
  ForeignKeyRef,
} from '@pondpilot/flowscope-core';

interface SchemaViewProps {
  schema: (SchemaTable | ResolvedSchemaTable)[];
  /** Table name to focus on (will center and highlight the table) */
  selectedTableName?: string;
  /** Callback when selection should be cleared */
  onClearSelection?: () => void;
}

type ColumnWithConstraints = ColumnSchema | ResolvedColumnSchema;

interface SchemaTableNodeData extends Record<string, unknown> {
  label: string;
  columns: ColumnWithConstraints[];
  origin?: SchemaOrigin;
  isSelected?: boolean;
}

function isResolvedSchemaTable(
  table: SchemaTable | ResolvedSchemaTable
): table is ResolvedSchemaTable {
  return 'origin' in table;
}

function buildConstraintForeignKeyMap(
  table: SchemaTable | ResolvedSchemaTable
): Map<string, ForeignKeyRef> | null {
  if (!isResolvedSchemaTable(table) || !table.constraints || table.constraints.length === 0) {
    return null;
  }

  const constraintForeignKeys = new Map<string, ForeignKeyRef>();
  for (const constraint of table.constraints) {
    if (constraint.constraintType !== 'foreign_key') continue;
    const referencedTable = constraint.referencedTable;
    if (!referencedTable) continue;
    if (!constraint.columns || constraint.columns.length === 0) continue;

    const referencedColumns = constraint.referencedColumns || [];
    constraint.columns.forEach((columnName, idx) => {
      if (constraintForeignKeys.has(columnName)) {
        return;
      }
      const targetColumn = referencedColumns[idx] ?? referencedColumns[0] ?? 'primary_key';
      constraintForeignKeys.set(columnName, {
        table: referencedTable,
        column: targetColumn,
      });
    });
  }

  return constraintForeignKeys.size > 0 ? constraintForeignKeys : null;
}

function getColumnsWithConstraintMetadata(
  table: SchemaTable | ResolvedSchemaTable
): ColumnWithConstraints[] {
  const columns = table.columns || [];
  const constraintForeignKeys = buildConstraintForeignKeyMap(table);
  if (!constraintForeignKeys) {
    return columns;
  }

  return columns.map((column) => {
    if (column.foreignKey || !constraintForeignKeys.has(column.name)) {
      return column;
    }
    const fk = constraintForeignKeys.get(column.name);
    if (!fk) {
      return column;
    }
    return { ...column, foreignKey: fk };
  });
}

function SchemaTableNode({ data }: NodeProps<FlowNode<SchemaTableNodeData>>): JSX.Element {
  // Color coding based on origin: imported (table palette) vs implied (cte palette)
  const palette = data.origin === 'imported' ? COLORS.nodes.table : COLORS.nodes.cte;
  const isSelected = data.isSelected === true;

  return (
    <div
      style={{
        minWidth: 180,
        borderRadius: 8,
        border: isSelected ? '2px solid #3b82f6' : `1px solid ${palette.border}`,
        boxShadow: isSelected ? '0 0 0 3px rgba(59, 130, 246, 0.3)' : '0 1px 3px rgba(0,0,0,0.1)',
        overflow: 'hidden',
        backgroundColor: palette.bg,
        fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
        transition: 'border-color 0.2s, box-shadow 0.2s',
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
          {(data.columns || []).map((col: ColumnWithConstraints) => {
            const isPK = col.isPrimaryKey === true;
            const hasFK = col.foreignKey != null;
            return (
              <div
                key={col.name}
                style={{
                  fontSize: 12,
                  color: palette.textSecondary,
                  padding: '3px 4px',
                  borderRadius: 4,
                  display: 'flex',
                  alignItems: 'center',
                  gap: 4,
                }}
              >
                {isPK && (
                  <span
                    role="img"
                    aria-label="Primary Key"
                    title="Primary Key"
                    style={CONSTRAINT_STYLES.primaryKey}
                  >
                    PK
                  </span>
                )}
                {hasFK && (
                  <span
                    role="img"
                    aria-label={`Foreign Key to ${col.foreignKey?.table}`}
                    title={`FK → ${col.foreignKey?.table}`}
                    style={CONSTRAINT_STYLES.foreignKey}
                  >
                    FK
                  </span>
                )}
                <span>{col.name}</span>
                {col.dataType && <span style={{ opacity: 0.6 }}>({col.dataType})</span>}
              </div>
            );
          })}
        </div>
      )}
      <Handle type="source" position={Position.Right} style={{ opacity: 0 }} />
    </div>
  );
}

const nodeTypes = {
  schemaTableNode: SchemaTableNode,
};

export function buildSchemaFlowEdges(schema: (SchemaTable | ResolvedSchemaTable)[]): FlowEdge[] {
  const edges: FlowEdge[] = [];
  const seenEdgeIds = new Set<string>();
  const tableLookup = new Map<string, string>();

  for (const table of schema) {
    for (const key of collectTableLookupKeys(table)) {
      if (!tableLookup.has(key)) {
        tableLookup.set(key, table.name);
      }
    }
  }

  for (const table of schema) {
    const columns = getColumnsWithConstraintMetadata(table);
    for (const col of columns) {
      if (!col.foreignKey) continue;

      const edgeId = `fk-${table.name}-${col.name}-${col.foreignKey.table}-${col.foreignKey.column}`;
      // Skip duplicate edges (e.g., if same FK is defined in both column and table constraints)
      if (seenEdgeIds.has(edgeId)) continue;
      seenEdgeIds.add(edgeId);

      const resolvedTarget = resolveForeignKeyTarget(col.foreignKey.table, tableLookup);
      if (resolvedTarget) {
        edges.push({
          id: edgeId,
          source: table.name,
          target: resolvedTarget,
          type: 'smoothstep',
          animated: false,
          style: CONSTRAINT_STYLES.edge,
          label: `${col.name} → ${col.foreignKey.column}`,
          labelStyle: CONSTRAINT_STYLES.edgeLabel,
          labelBgStyle: CONSTRAINT_STYLES.edgeLabelBg,
        });
      }
    }
  }

  return edges;
}

export function buildSchemaFlowNodes(schema: (SchemaTable | ResolvedSchemaTable)[]): FlowNode[] {
  return schema.map((table) => {
    return {
      id: table.name,
      type: 'schemaTableNode',
      position: { x: 0, y: 0 },
      data: {
        label: table.name,
        columns: getColumnsWithConstraintMetadata(table),
        origin: isResolvedSchemaTable(table) ? table.origin : undefined,
      } satisfies SchemaTableNodeData,
    };
  });
}

function SchemaViewInner({ schema, selectedTableName, onClearSelection }: SchemaViewProps): JSX.Element {
  // Focus on selected node when selection changes
  useNodeFocus({ focusNodeId: selectedTableName });

  const { layoutedNodes, layoutedEdges } = useMemo(() => {
    if (schema.length === 0) return { layoutedNodes: [], layoutedEdges: [] };
    const rawNodes = buildSchemaFlowNodes(schema);
    const rawEdges = buildSchemaFlowEdges(schema);
    const { nodes: ln, edges: le } = getLayoutedElements(rawNodes, rawEdges, 'TB');
    return { layoutedNodes: ln, layoutedEdges: le };
  }, [schema]);

  // Apply selection to nodes
  const nodesWithSelection = useMemo(() => {
    return layoutedNodes.map((node) => ({
      ...node,
      data: {
        ...node.data,
        isSelected: selectedTableName ? node.id === selectedTableName : false,
      },
    }));
  }, [layoutedNodes, selectedTableName]);

  const [nodes, setNodes, onNodesChange] = useNodesState(nodesWithSelection);
  const [edges, setEdges, onEdgesChange] = useEdgesState(layoutedEdges);

  // Update nodes when layout or selection changes
  // Note: setNodes/setEdges trigger re-renders if included in dependencies,
  // causing infinite loops. They are stable callbacks and safe to omit.
  useEffect(() => {
    setNodes(nodesWithSelection);
    setEdges(layoutedEdges);
  }, [nodesWithSelection, layoutedEdges]); // setNodes/setEdges omitted intentionally

  const handlePaneClick = useCallback(() => {
    onClearSelection?.();
  }, [onClearSelection]);

  return (
    <div style={{ height: '100%' }}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onPaneClick={handlePaneClick}
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

export function SchemaView(props: SchemaViewProps): JSX.Element {
  if (props.schema.length === 0) {
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
    <ReactFlowProvider>
      <SchemaViewInner {...props} />
    </ReactFlowProvider>
  );
}
