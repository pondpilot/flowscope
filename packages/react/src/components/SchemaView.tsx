import { useMemo, useEffect, useCallback, useState, memo, type JSX } from 'react';
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
  getStraightPath,
} from '@xyflow/react';
import type { Node as FlowNode, Edge as FlowEdge, NodeProps, EdgeProps } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import dagre from 'dagre';
import { KeyRound, Link2, Table2, Eye, ArrowDownUp, ArrowLeftRight } from 'lucide-react';

import { useNodeFocus } from '../hooks/useNodeFocus';
import { useIsDarkMode } from '../hooks/useColors';
import { collectTableLookupKeys, resolveForeignKeyTarget } from '../utils/schemaUtils';
import { COLORS, SCHEMA_CONFIG, SCHEMA_COLORS, SCHEMA_NODE_PALETTES } from '../constants';
import type {
  SchemaTable,
  ResolvedSchemaTable,
  ColumnSchema,
  ResolvedColumnSchema,
  SchemaOrigin,
  ForeignKeyRef,
} from '@pondpilot/flowscope-core';

// ============================================================================
// Types
// ============================================================================

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
  nodeType?: 'table' | 'view';
  isSelected?: boolean;
  isHighlighted?: boolean;
  highlightedColumns?: string[];
}

type LayoutDirection = 'TB' | 'LR';

// ============================================================================
// Utility Functions
// ============================================================================

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

/**
 * Get data type icon based on SQL type
 */
function getDataTypeIcon(dataType?: string): string {
  if (!dataType) return '';
  const type = dataType.toUpperCase();

  if (
    type.includes('INT') ||
    type.includes('NUMERIC') ||
    type.includes('DECIMAL') ||
    type.includes('FLOAT') ||
    type.includes('DOUBLE') ||
    type.includes('NUMBER')
  ) {
    return '#'; // Number
  }
  if (type.includes('CHAR') || type.includes('TEXT') || type.includes('STRING')) {
    return 'Aa'; // Text
  }
  if (type.includes('DATE') || type.includes('TIME') || type.includes('TIMESTAMP')) {
    return '📅'; // Date/time
  }
  if (type.includes('BOOL')) {
    return '◉'; // Boolean
  }
  if (
    type.includes('JSON') ||
    type.includes('ARRAY') ||
    type.includes('STRUCT') ||
    type.includes('MAP')
  ) {
    return '{}'; // Complex type
  }
  if (type.includes('BLOB') || type.includes('BINARY') || type.includes('BYTES')) {
    return '⬡'; // Binary
  }
  return '';
}

// ============================================================================
// Schema Layout
// ============================================================================

function getSchemaLayoutedElements(
  nodes: FlowNode<SchemaTableNodeData>[],
  edges: FlowEdge[],
  direction: LayoutDirection
): { nodes: FlowNode<SchemaTableNodeData>[]; edges: FlowEdge[] } {
  if (nodes.length === 0) return { nodes, edges };

  const dagreGraph = new dagre.graphlib.Graph();
  dagreGraph.setDefaultEdgeLabel(() => ({}));

  const isHorizontal = direction === 'LR';
  dagreGraph.setGraph({
    rankdir: direction,
    nodesep: isHorizontal ? SCHEMA_CONFIG.DAGRE_NODESEP_LR : SCHEMA_CONFIG.DAGRE_NODESEP_TB,
    ranksep: isHorizontal ? SCHEMA_CONFIG.DAGRE_RANKSEP_LR : SCHEMA_CONFIG.DAGRE_RANKSEP_TB,
    edgesep: 50,
    marginx: 40,
    marginy: 40,
  });

  nodes.forEach((node) => {
    const columnCount = node.data.columns?.length || 0;
    const height =
      SCHEMA_CONFIG.NODE_HEADER_HEIGHT + columnCount * SCHEMA_CONFIG.NODE_HEIGHT_PER_COLUMN;
    dagreGraph.setNode(node.id, { width: SCHEMA_CONFIG.NODE_MIN_WIDTH, height });
  });

  edges.forEach((edge) => {
    dagreGraph.setEdge(edge.source, edge.target);
  });

  dagre.layout(dagreGraph);

  const layoutedNodes = nodes.map((node) => {
    const nodeWithPosition = dagreGraph.node(node.id);
    if (!nodeWithPosition) return node;

    const columnCount = node.data.columns?.length || 0;
    const height =
      SCHEMA_CONFIG.NODE_HEADER_HEIGHT + columnCount * SCHEMA_CONFIG.NODE_HEIGHT_PER_COLUMN;

    return {
      ...node,
      position: {
        x: nodeWithPosition.x - SCHEMA_CONFIG.NODE_MIN_WIDTH / 2,
        y: nodeWithPosition.y - height / 2,
      },
    };
  });

  // Update edge source/target positions based on direction
  const layoutedEdges = edges.map((edge) => ({
    ...edge,
    sourcePosition: isHorizontal ? Position.Right : Position.Bottom,
    targetPosition: isHorizontal ? Position.Left : Position.Top,
  }));

  return { nodes: layoutedNodes, edges: layoutedEdges };
}

// ============================================================================
// Schema Table Node Component
// ============================================================================

const SchemaTableNodeComponent = ({
  data,
}: NodeProps<FlowNode<SchemaTableNodeData>>): JSX.Element => {
  const isDark = useIsDarkMode();
  const paletteKey = data.origin === 'imported' ? 'imported' : 'cte';
  const colors = isDark
    ? SCHEMA_NODE_PALETTES.dark[paletteKey]
    : SCHEMA_NODE_PALETTES.light[paletteKey];
  const isSelected = data.isSelected === true;
  const isHighlighted = data.isHighlighted === true;
  const highlightedColumns = data.highlightedColumns || [];
  const isView = data.nodeType === 'view';

  const NodeIcon = isView ? Eye : Table2;

  return (
    <div
      className="schema-table-node"
      style={{
        minWidth: SCHEMA_CONFIG.NODE_MIN_WIDTH,
        borderRadius: 8,
        border:
          isSelected || isHighlighted
            ? `2px solid ${SCHEMA_COLORS.selection.border}`
            : `1px solid ${colors.border}`,
        boxShadow:
          isSelected || isHighlighted
            ? `0 0 0 3px ${SCHEMA_COLORS.selection.ring}`
            : '0 1px 3px rgba(0,0,0,0.1)',
        overflow: 'hidden',
        backgroundColor: colors.bg,
        fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
        transition: `border-color ${SCHEMA_CONFIG.TRANSITION_DURATION}, box-shadow ${SCHEMA_CONFIG.TRANSITION_DURATION}`,
      }}
    >
      <Handle type="target" position={Position.Left} style={{ opacity: 0 }} />

      {/* Header */}
      <div
        style={{
          padding: '8px 12px',
          fontSize: 12,
          fontWeight: 500,
          borderBottom: `1px solid ${colors.border}`,
          backgroundColor: colors.headerBg,
          color: colors.text,
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          cursor: 'grab',
        }}
        className="schema-node-header"
      >
        <NodeIcon size={14} style={{ opacity: 0.7, flexShrink: 0 }} />
        <span
          style={{
            fontWeight: 600,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          {data.label}
        </span>
      </div>

      {/* Columns */}
      {(data.columns || []).length > 0 && (
        <div style={{ maxHeight: 400, overflowY: 'auto', overflowX: 'hidden' }} className="nodrag">
          {(data.columns || []).map((col: ColumnWithConstraints) => {
            const isPK = col.isPrimaryKey === true;
            const hasFK = col.foreignKey != null;
            const isColumnHighlighted = highlightedColumns.includes(col.name);
            const typeIcon = getDataTypeIcon(col.dataType);

            return (
              <div
                key={col.name}
                style={{
                  fontSize: 12,
                  color: colors.textSecondary,
                  padding: '6px 12px',
                  display: 'flex',
                  alignItems: 'center',
                  gap: 6,
                  borderBottom: `1px solid ${isDark ? 'rgba(255,255,255,0.05)' : 'rgba(0,0,0,0.05)'}`,
                  backgroundColor: isColumnHighlighted
                    ? SCHEMA_COLORS.highlight.background
                    : 'transparent',
                  transition: `background-color ${SCHEMA_CONFIG.TRANSITION_DURATION}`,
                  position: 'relative',
                }}
              >
                {/* Constraint indicators */}
                <div style={{ display: 'flex', alignItems: 'center', minWidth: 32, gap: 2 }}>
                  {isPK && (
                    <span title="Primary Key">
                      <KeyRound size={14} style={{ color: SCHEMA_COLORS.primaryKey }} />
                    </span>
                  )}
                  {hasFK && (
                    <span
                      title={`Foreign Key → ${col.foreignKey?.table}.${col.foreignKey?.column}`}
                    >
                      <Link2 size={14} style={{ color: SCHEMA_COLORS.foreignKey }} />
                    </span>
                  )}
                </div>

                {/* Column name */}
                <span
                  style={{
                    flex: 1,
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {col.name}
                </span>

                {/* Data type with icon */}
                {col.dataType && (
                  <span
                    style={{
                      opacity: 0.6,
                      display: 'flex',
                      alignItems: 'center',
                      gap: 4,
                      flexShrink: 0,
                    }}
                  >
                    {typeIcon && <span style={{ fontSize: 10, opacity: 0.7 }}>{typeIcon}</span>}
                    <span style={{ fontSize: 11 }}>({col.dataType})</span>
                  </span>
                )}

                {/* Handles for FK/PK connections */}
                {hasFK && (
                  <Handle
                    id={`${data.label}-${col.name}-source`}
                    type="source"
                    position={Position.Right}
                    style={{
                      position: 'absolute',
                      top: '50%',
                      right: -4,
                      transform: 'translateY(-50%)',
                      width: 8,
                      height: 8,
                      backgroundColor: SCHEMA_COLORS.foreignKey,
                      border: 'none',
                    }}
                  />
                )}
                {isPK && (
                  <Handle
                    id={`${data.label}-${col.name}-target`}
                    type="target"
                    position={Position.Left}
                    style={{
                      position: 'absolute',
                      top: '50%',
                      left: -4,
                      transform: 'translateY(-50%)',
                      width: 8,
                      height: 8,
                      backgroundColor: SCHEMA_COLORS.primaryKey,
                      border: 'none',
                    }}
                  />
                )}
              </div>
            );
          })}
        </div>
      )}

      <Handle type="source" position={Position.Right} style={{ opacity: 0 }} />
    </div>
  );
};

// Custom comparison for memo
const areNodePropsEqual = (
  prevProps: NodeProps<FlowNode<SchemaTableNodeData>>,
  nextProps: NodeProps<FlowNode<SchemaTableNodeData>>
): boolean => {
  const prevData = prevProps.data;
  const nextData = nextProps.data;

  // Check scalar properties
  if (
    prevData.isHighlighted !== nextData.isHighlighted ||
    prevData.isSelected !== nextData.isSelected ||
    prevData.label !== nextData.label ||
    prevData.origin !== nextData.origin ||
    prevData.nodeType !== nextData.nodeType
  ) {
    return false;
  }

  // Check highlighted columns array
  const prevHighlighted = prevData.highlightedColumns || [];
  const nextHighlighted = nextData.highlightedColumns || [];
  if (prevHighlighted.length !== nextHighlighted.length) {
    return false;
  }
  for (let i = 0; i < prevHighlighted.length; i += 1) {
    if (prevHighlighted[i] !== nextHighlighted[i]) {
      return false;
    }
  }

  // Check columns array (compare by reference since columns are built once per schema)
  const prevCols = prevData.columns || [];
  const nextCols = nextData.columns || [];
  if (prevCols.length !== nextCols.length) {
    return false;
  }
  // Compare column identity and key properties that affect rendering
  for (let i = 0; i < prevCols.length; i += 1) {
    const prevCol = prevCols[i];
    const nextCol = nextCols[i];
    if (
      prevCol.name !== nextCol.name ||
      prevCol.dataType !== nextCol.dataType ||
      prevCol.isPrimaryKey !== nextCol.isPrimaryKey ||
      prevCol.foreignKey?.table !== nextCol.foreignKey?.table ||
      prevCol.foreignKey?.column !== nextCol.foreignKey?.column
    ) {
      return false;
    }
  }

  return true;
};

const SchemaTableNode = memo(SchemaTableNodeComponent, areNodePropsEqual);

// ============================================================================
// Shared SVG Definitions (rendered once at ReactFlow level)
// ============================================================================

function SchemaSvgDefs(): JSX.Element {
  return (
    <svg style={{ position: 'absolute', width: 0, height: 0 }}>
      <defs>
        <marker
          id="schema-arrowhead"
          markerWidth={7}
          markerHeight={7}
          refX={6.5}
          refY={3.5}
          orient="auto"
        >
          <polygon points="0 0, 7 3.5, 0 7" fill={SCHEMA_COLORS.edge.default} />
        </marker>
        <marker
          id="schema-arrowhead-selected"
          markerWidth={7}
          markerHeight={7}
          refX={6.5}
          refY={3.5}
          orient="auto"
        >
          <polygon points="0 0, 7 3.5, 0 7" fill={SCHEMA_COLORS.edge.selected} />
        </marker>
      </defs>
      <style>
        {`
          @keyframes schemaDashAnimation {
            to {
              stroke-dashoffset: ${SCHEMA_CONFIG.EDGE_DASH_OFFSET};
            }
          }
        `}
      </style>
    </svg>
  );
}

// ============================================================================
// Angled Edge Component with Animation
// ============================================================================

const AngledEdgeComponent = ({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  label,
  selected,
  style = {},
}: EdgeProps): JSX.Element => {
  const isDark = useIsDarkMode();
  const gap = SCHEMA_CONFIG.EDGE_GAP;

  // Create angled path
  let path = '';
  const srcPos = sourcePosition || Position.Right;
  const tgtPos = targetPosition || Position.Left;

  if (srcPos === Position.Right && tgtPos === Position.Left) {
    if (Math.abs(sourceY - targetY) < 5) {
      const midX = sourceX + (targetX - sourceX) / 2;
      path = `M ${sourceX},${sourceY} L ${midX},${sourceY} L ${midX},${targetY} L ${targetX},${targetY}`;
    } else {
      const midX = Math.max(
        sourceX + gap,
        Math.min(targetX - gap, sourceX + (targetX - sourceX) / 2)
      );
      path = `M ${sourceX},${sourceY} L ${midX},${sourceY} L ${midX},${targetY} L ${targetX},${targetY}`;
    }
  } else if (srcPos === Position.Bottom && tgtPos === Position.Top) {
    const midY = sourceY + (targetY - sourceY) / 2;
    path = `M ${sourceX},${sourceY} L ${sourceX},${midY} L ${targetX},${midY} L ${targetX},${targetY}`;
  } else {
    const [straightPath] = getStraightPath({ sourceX, sourceY, targetX, targetY });
    path = straightPath;
  }

  const edgeColor = selected ? SCHEMA_COLORS.edge.selected : SCHEMA_COLORS.edge.default;
  const edgeWidth = selected ? SCHEMA_CONFIG.EDGE_SELECTED_WIDTH : SCHEMA_CONFIG.EDGE_DEFAULT_WIDTH;

  // Label position
  let labelX = (sourceX + targetX) / 2;
  let labelY = (sourceY + targetY) / 2;

  if (srcPos === Position.Right && tgtPos === Position.Left) {
    const midX = Math.max(
      sourceX + gap,
      Math.min(targetX - gap, sourceX + (targetX - sourceX) / 2)
    );
    labelX = sourceX + (midX - sourceX) / 2 + gap / 2;
    labelY = sourceY;
  } else if (srcPos === Position.Bottom && tgtPos === Position.Top) {
    const midY = sourceY + (targetY - sourceY) / 2;
    labelX = (sourceX + targetX) / 2;
    labelY = midY;
  }

  return (
    <>
      {/* Glow effect for selected edges */}
      {selected && (
        <path
          d={path}
          strokeWidth={edgeWidth + 6}
          stroke={edgeColor}
          strokeOpacity={SCHEMA_CONFIG.EDGE_GLOW_OPACITY}
          fill="none"
          style={{ filter: `blur(${SCHEMA_CONFIG.EDGE_GLOW_BLUR}px)` }}
        />
      )}

      {/* Main edge path */}
      <path
        id={id}
        className="react-flow__edge-path"
        d={path}
        strokeWidth={edgeWidth}
        stroke={edgeColor}
        style={{
          ...style,
          strokeDasharray: selected ? undefined : SCHEMA_CONFIG.EDGE_DASH_PATTERN,
          transition: 'stroke-width 0.2s, stroke 0.2s',
        }}
        markerEnd={selected ? 'url(#schema-arrowhead-selected)' : 'url(#schema-arrowhead)'}
        fill="none"
      />

      {/* Animated dash overlay for non-selected edges */}
      {!selected && (
        <path
          d={path}
          fill="none"
          strokeWidth={edgeWidth}
          stroke={edgeColor}
          strokeDasharray={SCHEMA_CONFIG.EDGE_DASH_PATTERN}
          style={{
            animation: `schemaDashAnimation ${SCHEMA_CONFIG.EDGE_ANIMATION_DURATION} linear infinite`,
            pointerEvents: 'none',
          }}
        />
      )}

      {/* Invisible wider path for easier interaction */}
      <path
        d={path}
        fill="none"
        strokeWidth={20}
        stroke="transparent"
        style={{ cursor: 'pointer' }}
      />

      {/* Label */}
      {label && (
        <g style={{ pointerEvents: 'none' }}>
          {(() => {
            const labelText = String(label);
            const labelWidth = Math.max(60, Math.min(140, labelText.length * 7 + 16));
            const labelHeight = 20;

            return (
              <>
                <rect
                  x={labelX - labelWidth / 2}
                  y={labelY - labelHeight / 2}
                  width={labelWidth}
                  height={labelHeight}
                  rx={10}
                  ry={10}
                  fill={isDark ? '#1e293b' : 'white'}
                  stroke={edgeColor}
                  strokeWidth={1.5}
                  strokeOpacity={0.8}
                />
                <text
                  x={labelX}
                  y={labelY + 1}
                  textAnchor="middle"
                  dominantBaseline="middle"
                  fill={isDark ? '#e2e8f0' : '#475569'}
                  fontSize={11}
                  fontWeight={selected ? 600 : 400}
                >
                  {labelText}
                </text>
              </>
            );
          })()}
        </g>
      )}
    </>
  );
};

const AngledEdge = memo(AngledEdgeComponent);

// ============================================================================
// Layout Controls Component
// ============================================================================

interface SchemaControlsProps {
  direction: LayoutDirection;
  onDirectionChange: (direction: LayoutDirection) => void;
}

function SchemaControls({ direction, onDirectionChange }: SchemaControlsProps): JSX.Element {
  const isDark = useIsDarkMode();

  return (
    <div
      style={{
        position: 'absolute',
        top: 10,
        left: 10,
        zIndex: 5,
        display: 'flex',
        gap: 4,
        padding: 4,
        backgroundColor: isDark ? 'rgba(30, 41, 59, 0.95)' : 'rgba(255, 255, 255, 0.95)',
        borderRadius: 8,
        border: `1px solid ${isDark ? 'rgba(255,255,255,0.1)' : 'rgba(0,0,0,0.1)'}`,
        boxShadow: '0 1px 3px rgba(0,0,0,0.1)',
        backdropFilter: 'blur(8px)',
      }}
    >
      <button
        onClick={() => onDirectionChange('TB')}
        title="Vertical Layout"
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          width: 28,
          height: 28,
          border: 'none',
          borderRadius: 6,
          cursor: 'pointer',
          backgroundColor:
            direction === 'TB'
              ? isDark
                ? 'rgba(59, 130, 246, 0.3)'
                : 'rgba(59, 130, 246, 0.15)'
              : 'transparent',
          color: direction === 'TB' ? '#3b82f6' : isDark ? '#94a3b8' : '#64748b',
          transition: 'all 0.15s',
        }}
      >
        <ArrowDownUp size={16} />
      </button>
      <button
        onClick={() => onDirectionChange('LR')}
        title="Horizontal Layout"
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          width: 28,
          height: 28,
          border: 'none',
          borderRadius: 6,
          cursor: 'pointer',
          backgroundColor:
            direction === 'LR'
              ? isDark
                ? 'rgba(59, 130, 246, 0.3)'
                : 'rgba(59, 130, 246, 0.15)'
              : 'transparent',
          color: direction === 'LR' ? '#3b82f6' : isDark ? '#94a3b8' : '#64748b',
          transition: 'all 0.15s',
        }}
      >
        <ArrowLeftRight size={16} />
      </button>
    </div>
  );
}

// ============================================================================
// Node and Edge Types
// ============================================================================

const nodeTypes = {
  schemaTableNode: SchemaTableNode,
};

const edgeTypes = {
  schemaEdge: AngledEdge,
};

// ============================================================================
// Build Flow Elements
// ============================================================================

export function buildSchemaFlowEdges(schema: (SchemaTable | ResolvedSchemaTable)[]): FlowEdge[] {
  const edges: FlowEdge[] = [];
  const seenEdgeIds = new Set<string>();
  const normalizedEdgeKeys = new Set<string>();
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
      if (seenEdgeIds.has(edgeId)) continue;
      seenEdgeIds.add(edgeId);

      const resolvedTarget = resolveForeignKeyTarget(col.foreignKey.table, tableLookup);
      if (resolvedTarget) {
        // Use sorted array for consistent ordering regardless of case sensitivity
        const [first, second] = [table.name, resolvedTarget].sort();
        const [firstCol, secondCol] =
          first === table.name
            ? [col.name, col.foreignKey.column]
            : [col.foreignKey.column, col.name];
        const normalizedKey = `${first}.${firstCol}->${second}.${secondCol}`;

        if (normalizedEdgeKeys.has(normalizedKey)) {
          continue;
        }

        normalizedEdgeKeys.add(normalizedKey);
        edges.push({
          id: edgeId,
          source: table.name,
          target: resolvedTarget,
          type: 'schemaEdge',
          animated: false,
          label: `${col.name} → ${col.foreignKey.column}`,
          data: {
            sourceColumn: col.name,
            targetColumn: col.foreignKey.column,
          },
        });
      }
    }
  }

  return edges;
}

export function buildSchemaFlowNodes(
  schema: (SchemaTable | ResolvedSchemaTable)[]
): FlowNode<SchemaTableNodeData>[] {
  return schema.map((table) => {
    return {
      id: table.name,
      type: 'schemaTableNode',
      position: { x: 0, y: 0 },
      dragHandle: '.schema-node-header',
      data: {
        label: table.name,
        columns: getColumnsWithConstraintMetadata(table),
        origin: isResolvedSchemaTable(table) ? table.origin : undefined,
        nodeType: 'table',
      } satisfies SchemaTableNodeData,
    };
  });
}

// ============================================================================
// Connected Highlighting Hook
// ============================================================================

interface HighlightState {
  selectedTable: string | null;
  connectedTables: Set<string>;
  highlightedColumnsMap: Map<string, string[]>;
}

function useConnectedHighlighting(edges: FlowEdge[], selectedTableName?: string): HighlightState {
  return useMemo(() => {
    if (!selectedTableName) {
      return {
        selectedTable: null,
        connectedTables: new Set<string>(),
        highlightedColumnsMap: new Map<string, string[]>(),
      };
    }

    const connectedTables = new Set<string>();
    const highlightedColumnsMap = new Map<string, string[]>();

    // Find all edges connected to the selected table
    for (const edge of edges) {
      if (edge.source === selectedTableName) {
        connectedTables.add(edge.target);
        // Highlight the FK column in source and PK column in target
        const sourceCol = (edge.data as { sourceColumn?: string })?.sourceColumn;
        const targetCol = (edge.data as { targetColumn?: string })?.targetColumn;
        if (sourceCol) {
          const existing = highlightedColumnsMap.get(selectedTableName) || [];
          if (!existing.includes(sourceCol)) {
            highlightedColumnsMap.set(selectedTableName, [...existing, sourceCol]);
          }
        }
        if (targetCol) {
          const existing = highlightedColumnsMap.get(edge.target) || [];
          if (!existing.includes(targetCol)) {
            highlightedColumnsMap.set(edge.target, [...existing, targetCol]);
          }
        }
      }
      if (edge.target === selectedTableName) {
        connectedTables.add(edge.source);
        const sourceCol = (edge.data as { sourceColumn?: string })?.sourceColumn;
        const targetCol = (edge.data as { targetColumn?: string })?.targetColumn;
        if (sourceCol) {
          const existing = highlightedColumnsMap.get(edge.source) || [];
          if (!existing.includes(sourceCol)) {
            highlightedColumnsMap.set(edge.source, [...existing, sourceCol]);
          }
        }
        if (targetCol) {
          const existing = highlightedColumnsMap.get(selectedTableName) || [];
          if (!existing.includes(targetCol)) {
            highlightedColumnsMap.set(selectedTableName, [...existing, targetCol]);
          }
        }
      }
    }

    return {
      selectedTable: selectedTableName,
      connectedTables,
      highlightedColumnsMap,
    };
  }, [edges, selectedTableName]);
}

// ============================================================================
// Main SchemaView Component
// ============================================================================

function SchemaViewInner({
  schema,
  selectedTableName,
  onClearSelection,
}: SchemaViewProps): JSX.Element {
  const isDark = useIsDarkMode();
  const [direction, setDirection] = useState<LayoutDirection>('TB');

  // Focus on selected node when selection changes
  useNodeFocus({ focusNodeId: selectedTableName });

  // Build initial nodes and edges
  const { rawNodes, rawEdges } = useMemo(() => {
    if (schema.length === 0) return { rawNodes: [], rawEdges: [] };
    return {
      rawNodes: buildSchemaFlowNodes(schema),
      rawEdges: buildSchemaFlowEdges(schema),
    };
  }, [schema]);

  // Get highlighting state
  const highlighting = useConnectedHighlighting(rawEdges, selectedTableName);

  // Apply layout
  const { nodes: layoutedNodes, edges: layoutedEdges } = useMemo(() => {
    if (rawNodes.length === 0)
      return { nodes: [] as FlowNode<SchemaTableNodeData>[], edges: [] as FlowEdge[] };
    return getSchemaLayoutedElements(rawNodes, rawEdges, direction);
  }, [rawNodes, rawEdges, direction]);

  // Apply selection and highlighting to nodes
  const nodesWithHighlighting = useMemo(() => {
    return layoutedNodes.map((node: FlowNode<SchemaTableNodeData>) => ({
      ...node,
      data: {
        ...node.data,
        isSelected: selectedTableName ? node.id === selectedTableName : false,
        isHighlighted: highlighting.connectedTables.has(node.id),
        highlightedColumns: highlighting.highlightedColumnsMap.get(node.id) || [],
      },
    }));
  }, [layoutedNodes, selectedTableName, highlighting]);

  const [nodes, setNodes, onNodesChange] = useNodesState(nodesWithHighlighting);
  const [edges, setEdges, onEdgesChange] = useEdgesState(layoutedEdges);

  // Update nodes when layout, selection, or highlighting changes
  useEffect(() => {
    setNodes(nodesWithHighlighting);
    setEdges(layoutedEdges);
  }, [nodesWithHighlighting, layoutedEdges, setNodes, setEdges]);

  const handlePaneClick = useCallback(() => {
    onClearSelection?.();
  }, [onClearSelection]);

  const handleDirectionChange = useCallback((newDirection: LayoutDirection) => {
    setDirection(newDirection);
  }, []);

  return (
    <div style={{ height: '100%', position: 'relative' }}>
      <SchemaSvgDefs />
      <SchemaControls direction={direction} onDirectionChange={handleDirectionChange} />
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onPaneClick={handlePaneClick}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        fitView
        minZoom={0.05}
        maxZoom={2}
      >
        <Background color={isDark ? '#334155' : '#e2e8f0'} gap={20} size={1} />
        <Controls />
        <MiniMap
          nodeColor={(node) => {
            const nodeData = node.data as SchemaTableNodeData;
            if (nodeData?.isSelected) return SCHEMA_COLORS.selection.border;
            if (nodeData?.isHighlighted) return SCHEMA_COLORS.edge.selected;
            const origin = nodeData?.origin;
            return origin === 'imported' ? COLORS.nodes.table.accent : COLORS.nodes.cte.accent;
          }}
          maskColor={isDark ? 'rgba(0, 0, 0, 0.7)' : 'rgba(255, 255, 255, 0.7)'}
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
