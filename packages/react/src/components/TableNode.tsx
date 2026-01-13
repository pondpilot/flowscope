import { Handle, Position } from '@xyflow/react';
import type { NodeProps } from '@xyflow/react';
import { useLineageActions, useLineageStore } from '../store';
import type { TableNodeData, ColumnNodeInfo } from '../types';
import { sanitizeIdentifier } from '../utils/sanitize';
import { GRAPH_CONFIG, MAX_FILTER_DISPLAY_LENGTH } from '../constants';
import { useColors } from '../hooks/useColors';
import type { AggregationInfo } from '@pondpilot/flowscope-core';

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
function AggregationIndicator({ aggregation, colors }: AggregationIndicatorProps): JSX.Element | null {
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

export function TableNode({ id, data, selected }: NodeProps): JSX.Element {
  const { toggleNodeCollapse, toggleTableExpansion, selectNode } = useLineageActions();
  const expandedTableIds = useLineageStore((state) => state.expandedTableIds);
  const showColumnEdges = useLineageStore((state) => state.showColumnEdges);
  const colors = useColors();

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
  const isExpanded = expandedTableIds.has(id);
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
        border: `1px solid ${isSelected ? colors.interactive.selection : palette.border}`,
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
            zIndex: 10 
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
            zIndex: 10
          }}
        />

        {isCollapsed && (
          <>
            {/* Collapsed specific handles if needed, but the above covers it */}
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
        <div style={{ padding: '6px 12px', maxHeight: GRAPH_CONFIG.MAX_COLUMN_HEIGHT, overflowY: 'auto', position: 'relative' }}>
          {nodeData.columns.map((col: ColumnNodeInfo) => {
            return (
              <div
                key={col.id}
                onClick={showColumnEdges ? (e) => {
                  e.stopPropagation();
                  selectNode(col.id);
                } : undefined}
                style={{
                  fontSize: 12,
                  color: col.isHighlighted ? colors.interactive.selection : palette.textSecondary,
                  fontWeight: col.isHighlighted ? 600 : 400,
                  backgroundColor: col.isHighlighted ? colors.interactive.hover : 'transparent',
                  padding: '3px 4px',
                  borderRadius: 4,
                  position: 'relative',
                  cursor: showColumnEdges ? 'pointer' : 'inherit',
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
          })}
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
