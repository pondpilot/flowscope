import { memo, type JSX, type CSSProperties, useCallback, type ReactElement } from 'react';
import { Handle, Position } from '@xyflow/react';
import type { NodeProps } from '@xyflow/react';
import { List } from 'react-window';
import { useLineageActions, useLineageStore } from '../store';
import type { TableNodeData, ColumnNodeInfo } from '../types';
import { sanitizeIdentifier } from '../utils/sanitize';
import { GRAPH_CONFIG, MAX_FILTER_DISPLAY_LENGTH, getNamespaceColor } from '../constants';
import { useColors, useIsDarkMode } from '../hooks/useColors';
import type { AggregationInfo } from '@pondpilot/flowscope-core';

// Virtualization thresholds
const COLUMN_VIRTUALIZATION_THRESHOLD = 20;
const COLUMN_ROW_HEIGHT = 24;

interface AggregationIndicatorProps {
  aggregation?: AggregationInfo;
  colors: {
    groupingKey: string;
    aggregation: string;
  };
}

/**
 * Render aggregation indicator for a column.
 * Shows a badge for GROUP BY keys or aggregate functions.
 */
function AggregationIndicator({
  aggregation,
  colors,
}: AggregationIndicatorProps): JSX.Element | null {
  if (!aggregation) return null;

  if (aggregation.isGroupingKey) {
    return (
      <span
        role="img"
        aria-label="GROUP BY key column"
        style={{
          backgroundColor: `${colors.groupingKey}15`,
          color: colors.groupingKey,
          borderRadius: 4,
          padding: '1px 4px',
          fontSize: 9,
          fontWeight: 600,
          marginLeft: 4,
        }}
        title="GROUP BY key"
      >
        KEY
      </span>
    );
  }

  // Aggregated column
  const funcName = aggregation.function || 'AGG';
  const tooltipText = aggregation.distinct ? `${funcName} DISTINCT` : funcName;

  return (
    <span
      role="img"
      aria-label={`Aggregated with ${tooltipText}`}
      style={{
        backgroundColor: `${colors.aggregation}15`,
        color: colors.aggregation,
        borderRadius: 4,
        padding: '1px 4px',
        fontSize: 9,
        fontWeight: 600,
        marginLeft: 4,
      }}
      title={tooltipText}
    >
      {aggregation.distinct ? `${funcName}(D)` : funcName}
    </span>
  );
}

interface AriaAttributes {
  'aria-posinset': number;
  'aria-setsize': number;
  role: 'listitem';
}

interface ColumnRowProps {
  col: ColumnNodeInfo;
  style?: CSSProperties;
  ariaAttributes?: AriaAttributes;
  showColumnEdges: boolean;
  onSelectColumn: (id: string) => void;
  colors: ReturnType<typeof useColors>;
  textSecondary: string;
}

/**
 * Single column row component, extracted for virtualization support.
 */
function ColumnRow({
  col,
  style,
  ariaAttributes,
  showColumnEdges,
  onSelectColumn,
  colors,
  textSecondary,
}: ColumnRowProps): JSX.Element {
  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      if (showColumnEdges) {
        e.stopPropagation();
        onSelectColumn(col.id);
      }
    },
    [showColumnEdges, onSelectColumn, col.id]
  );

  return (
    <div
      style={{
        ...style,
        fontSize: 12,
        color: col.isHighlighted ? colors.interactive.selection : textSecondary,
        fontWeight: col.isHighlighted ? 600 : 400,
        backgroundColor: col.isHighlighted ? colors.interactive.hover : 'transparent',
        padding: '3px 4px',
        borderRadius: 4,
        position: 'relative',
        cursor: showColumnEdges ? 'pointer' : 'inherit',
        boxSizing: 'border-box',
      }}
      onClick={handleClick}
      {...ariaAttributes}
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
      <span style={{ display: 'flex', alignItems: 'center', minWidth: 0 }}>
        <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {sanitizeIdentifier(col.name)}
        </span>
        <AggregationIndicator aggregation={col.aggregation} colors={colors} />
      </span>
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
}

/**
 * Props passed to the virtualized row component via rowProps.
 */
interface VirtualizedColumnRowProps {
  columns: ColumnNodeInfo[];
  showColumnEdges: boolean;
  onSelectColumn: (id: string) => void;
  colors: ReturnType<typeof useColors>;
  textSecondary: string;
}

/**
 * Row component for react-window v2 virtualized list.
 * Receives index and style from List, plus custom props via rowProps.
 */
function VirtualizedColumnRow({
  index,
  style,
  ariaAttributes,
  columns,
  showColumnEdges,
  onSelectColumn,
  colors,
  textSecondary,
}: {
  index: number;
  style: CSSProperties;
  ariaAttributes: AriaAttributes;
} & VirtualizedColumnRowProps): ReactElement {
  const col = columns[index];
  return (
    <ColumnRow
      col={col}
      style={style}
      ariaAttributes={ariaAttributes}
      showColumnEdges={showColumnEdges}
      onSelectColumn={onSelectColumn}
      colors={colors}
      textSecondary={textSecondary}
    />
  );
}

// Type guard for safer type checking
function isTableNodeData(data: unknown): data is TableNodeData {
  return (
    typeof data === 'object' &&
    data !== null &&
    'label' in data &&
    'nodeType' in data &&
    'columns' in data
  );
}

/**
 * Get the header label text for a table node.
 * Shows namespace (database.schema) when available, falls back to node type.
 */
function getNodeHeaderLabel(nodeData: TableNodeData, isVirtualOutput: boolean): string {
  if (isVirtualOutput) {
    return 'OUTPUT';
  }
  if (nodeData.database && nodeData.schema) {
    return `${nodeData.database}.${nodeData.schema}`;
  }
  if (nodeData.schema) {
    return nodeData.schema;
  }
  return nodeData.nodeType;
}

function TableNodeComponent({ id, data, selected }: NodeProps): JSX.Element {
  const { toggleNodeCollapse, toggleTableExpansion, selectNode } = useLineageActions();
  // Use derived selector to avoid new Set reference on each render
  const isExpanded = useLineageStore((state) => state.expandedTableIds.has(id));
  const showColumnEdges = useLineageStore((state) => state.showColumnEdges);
  const colors = useColors();
  const isDark = useIsDarkMode();

  if (!isTableNodeData(data)) {
    console.error('Invalid node data type for TableNode', data);
    return <div>Invalid node data</div>;
  }

  const nodeData = data;
  const isCte = nodeData.nodeType === 'cte';
  const isView = nodeData.nodeType === 'view';
  const isVirtualOutput = nodeData.nodeType === 'virtualOutput';
  const isRecursive = !!nodeData.isRecursive;
  const isBaseTable = !!nodeData.isBaseTable;
  const isSelected = selected || nodeData.isSelected;
  const isHighlighted = nodeData.isHighlighted;
  const isCollapsed = nodeData.isCollapsed;
  // isExpanded is now derived directly from the store selector above
  const hiddenColumnCount = nodeData.hiddenColumnCount || 0;

  type NodePalette = {
    bg: string;
    headerBg: string;
    border: string;
    text: string;
    textSecondary: string;
    accent: string;
  };
  let palette: NodePalette = colors.nodes.table;
  if (isCte) {
    palette = colors.nodes.cte;
  } else if (isView) {
    palette = colors.nodes.view;
  } else if (isVirtualOutput) {
    palette = colors.nodes.virtualOutput;
  }

  // Get schema color for left border band
  const schemaColor = getNamespaceColor(nodeData.schema, isDark);

  return (
    <div
      onClick={() => {
        // Allow clicking anywhere in the table to select it
        // Columns handle their own selection and stop propagation
        selectNode(id);
      }}
      style={{
        minWidth: 180,
        borderRadius: 8,
        borderTop: `1px solid ${isSelected ? colors.interactive.selection : palette.border}`,
        borderRight: `1px solid ${isSelected ? colors.interactive.selection : palette.border}`,
        borderBottom: `1px solid ${isSelected ? colors.interactive.selection : palette.border}`,
        borderLeft: schemaColor
          ? `3px solid ${schemaColor}`
          : `1px solid ${isSelected ? colors.interactive.selection : palette.border}`,
        boxShadow: isSelected
          ? `0 0 0 2px ${colors.interactive.selectionRing}`
          : isRecursive
            ? `0 0 0 2px ${colors.recursive}20`
            : '0 1px 3px rgba(0,0,0,0.1)',
        overflow: 'hidden',
        backgroundColor: isHighlighted ? colors.interactive.related : palette.bg,
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
        {/* Always render default handles for table-level connections */}
        <Handle
          type="target"
          position={Position.Left}
          style={{
            opacity: 0,
            border: 'none',
            background: 'transparent',
            top: '50%',
            left: 0,
            transform: 'translate(-50%, -50%)',
            zIndex: 10,
          }}
        />
        {isRecursive && (
          <Handle
            type="target"
            position={Position.Top}
            id="rec-top"
            style={{
              opacity: 0,
              border: 'none',
              background: 'transparent',
              top: -4,
              left: '20%',
              transform: 'translate(-50%, -50%)',
              zIndex: 12,
            }}
          />
        )}
        <Handle
          type="source"
          position={Position.Right}
          style={{
            opacity: 0,
            border: 'none',
            background: 'transparent',
            top: '50%',
            right: 0,
            transform: 'translate(50%, -50%)',
            zIndex: 10,
          }}
        />

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
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
            >
              <path d="M9 18l6-6-6-6" />
            </svg>
          ) : (
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
            >
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
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
            }}
            title={nodeData.qualifiedName || undefined}
          >
            {getNodeHeaderLabel(nodeData, isVirtualOutput)}
          </div>
          <div
            style={{
              fontWeight: 600,
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
            }}
            title={nodeData.qualifiedName || nodeData.label}
          >
            {sanitizeIdentifier(nodeData.label)}
          </div>
        </div>

        {isBaseTable && !isVirtualOutput && (
          <span
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              backgroundColor: `${colors.accent}18`,
              color: colors.accent,
              borderRadius: 999,
              padding: '3px 8px',
              fontSize: 10,
              fontWeight: 700,
              letterSpacing: 0.3,
              textTransform: 'uppercase',
            }}
            title="Primary base table for joins"
          >
            BASE
          </span>
        )}

        {hiddenColumnCount > 0 && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              toggleTableExpansion(id);
            }}
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              backgroundColor: isExpanded ? `${colors.accent}20` : `${colors.accent}15`,
              color: colors.accent,
              borderRadius: 999,
              padding: '4px 8px',
              fontSize: 10,
              fontWeight: 600,
              border: 'none',
              cursor: 'pointer',
              transition: 'background-color 0.15s',
            }}
            title={
              isExpanded
                ? `Hide ${hiddenColumnCount} column${hiddenColumnCount !== 1 ? 's' : ''}`
                : `Show ${hiddenColumnCount} more column${hiddenColumnCount !== 1 ? 's' : ''}`
            }
            onMouseEnter={(e) => {
              e.currentTarget.style.backgroundColor = `${colors.accent}30`;
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.backgroundColor = isExpanded
                ? `${colors.accent}20`
                : `${colors.accent}15`;
            }}
          >
            {isExpanded ? `−${hiddenColumnCount}` : `+${hiddenColumnCount}`}
          </button>
        )}

        {isRecursive && (
          <span
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 4,
              backgroundColor: `${colors.recursive}15`,
              color: colors.recursive,
              borderRadius: 999,
              padding: '4px 8px',
              fontSize: 10,
              fontWeight: 700,
              letterSpacing: 0.25,
            }}
            title="Recursive CTE"
          >
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M3 12a6 6 0 0 1 9-5l2 1" />
              <path d="M21 12a6 6 0 0 1-9 5l-2-1" />
              <path d="M7 10h4v4" />
              <path d="M17 14h-4v-4" />
            </svg>
            Recursive
          </span>
        )}
      </div>

      {!isCollapsed && nodeData.columns.length > 0 && (
        <div style={{ padding: '6px 12px', position: 'relative' }}>
          {nodeData.columns.length >= COLUMN_VIRTUALIZATION_THRESHOLD ? (
            // Virtualized list for large column counts
            <List
              style={{
                height: Math.min(
                  nodeData.columns.length * COLUMN_ROW_HEIGHT,
                  GRAPH_CONFIG.MAX_COLUMN_HEIGHT
                ),
                overflowX: 'hidden',
              }}
              rowCount={nodeData.columns.length}
              rowHeight={COLUMN_ROW_HEIGHT}
              rowComponent={VirtualizedColumnRow}
              rowProps={{
                columns: nodeData.columns,
                showColumnEdges,
                onSelectColumn: selectNode,
                colors,
                textSecondary: palette.textSecondary,
              }}
              overscanCount={5}
            />
          ) : (
            // Regular rendering for small column counts (avoid virtualization overhead)
            // maxHeight ensures consistent behavior with virtualized list
            <div
              style={{
                maxHeight: GRAPH_CONFIG.MAX_COLUMN_HEIGHT,
                overflowY: 'auto',
                overflowX: 'hidden',
              }}
            >
              {nodeData.columns.map((col: ColumnNodeInfo) => (
                <ColumnRow
                  key={col.id}
                  col={col}
                  showColumnEdges={showColumnEdges}
                  onSelectColumn={selectNode}
                  colors={colors}
                  textSecondary={palette.textSecondary}
                />
              ))}
            </div>
          )}
        </div>
      )}
      {!isCollapsed && nodeData.filters && nodeData.filters.length > 0 && (
        <div
          style={{
            padding: '6px 12px',
            borderTop: `1px solid ${palette.border}`,
            backgroundColor: `${colors.filter}08`,
          }}
        >
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 4,
              marginBottom: 4,
            }}
          >
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke={colors.filter}
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3" />
            </svg>
            <span
              style={{
                fontSize: 10,
                fontWeight: 600,
                color: colors.filter,
                textTransform: 'uppercase',
                letterSpacing: 0.5,
              }}
            >
              Filters
            </span>
          </div>
          {nodeData.filters.map((filter, index) => (
            <div
              key={index}
              style={{
                fontSize: 11,
                color: palette.textSecondary,
                padding: '2px 0',
                fontFamily: 'ui-monospace, SFMono-Regular, Consolas, monospace',
                wordBreak: 'break-word',
              }}
              title={filter.expression}
            >
              {filter.expression.length > MAX_FILTER_DISPLAY_LENGTH
                ? `${filter.expression.substring(0, MAX_FILTER_DISPLAY_LENGTH)}...`
                : filter.expression}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Memoized TableNode component to prevent unnecessary re-renders.
 *
 * Custom comparison checks specific props that affect visual output.
 * This is more efficient than deep equality checks but requires maintenance
 * when new visual properties are added to TableNodeData.
 *
 * IMPORTANT: When adding new properties to TableNodeData that affect rendering,
 * update this comparator to include the new property. Otherwise, the component
 * may not re-render when the new property changes.
 *
 * Properties currently checked:
 * - Node identity: id, selected
 * - Visual state: isCollapsed, isSelected, isHighlighted, isRecursive, isBaseTable
 * - Display data: label, nodeType, schema, database, hiddenColumnCount
 * - Columns: id, name, isHighlighted (for each column)
 * - Filters: expression (for each filter)
 */
export const TableNode = memo(TableNodeComponent, (prev, next) => {
  // Fast path: check primitive props first
  if (prev.id !== next.id) return false;
  if (prev.selected !== next.selected) return false;

  // Check data object properties that affect rendering
  const prevData = prev.data as TableNodeData | undefined;
  const nextData = next.data as TableNodeData | undefined;

  if (!prevData || !nextData) return prevData === nextData;

  // Check all properties that affect visual output
  if (prevData.isCollapsed !== nextData.isCollapsed) return false;
  if (prevData.isSelected !== nextData.isSelected) return false;
  if (prevData.isHighlighted !== nextData.isHighlighted) return false;
  if (prevData.label !== nextData.label) return false;
  if (prevData.nodeType !== nextData.nodeType) return false;
  if (prevData.schema !== nextData.schema) return false;
  if (prevData.database !== nextData.database) return false;
  if (prevData.isRecursive !== nextData.isRecursive) return false;
  if (prevData.isBaseTable !== nextData.isBaseTable) return false;
  if (prevData.hiddenColumnCount !== nextData.hiddenColumnCount) return false;

  // Check columns array
  if (prevData.columns.length !== nextData.columns.length) return false;

  // For large column arrays, use sampling to avoid O(n) comparison on every render.
  // This checks first, middle, last + a few samples for a balance of accuracy and performance.
  const columnCount = prevData.columns.length;
  const SAMPLING_THRESHOLD = 50;

  const compareColumn = (index: number): boolean => {
    const prevCol = prevData.columns[index];
    const nextCol = nextData.columns[index];
    return (
      prevCol.id === nextCol.id &&
      prevCol.name === nextCol.name &&
      prevCol.isHighlighted === nextCol.isHighlighted
    );
  };

  if (columnCount > SAMPLING_THRESHOLD) {
    // Sample-based comparison for large arrays: first, last, middle, quartiles
    const sampleIndices = [
      0,
      Math.floor(columnCount / 4),
      Math.floor(columnCount / 2),
      Math.floor((columnCount * 3) / 4),
      columnCount - 1,
    ];
    for (const idx of sampleIndices) {
      if (!compareColumn(idx)) return false;
    }
  } else {
    // Full comparison for small arrays
    for (let i = 0; i < columnCount; i++) {
      if (!compareColumn(i)) return false;
    }
  }

  // Check filters array
  const prevFilters = prevData.filters || [];
  const nextFilters = nextData.filters || [];
  if (prevFilters.length !== nextFilters.length) return false;
  for (let i = 0; i < prevFilters.length; i++) {
    if (prevFilters[i].expression !== nextFilters[i].expression) return false;
  }

  return true;
});
