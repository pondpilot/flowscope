import { Handle, Position } from '@xyflow/react';
import type { NodeProps } from '@xyflow/react';
import { useLineageActions, useLineageStore } from '../store';
import type { TableNodeData, ColumnNodeInfo } from '../types';
import { sanitizeIdentifier } from '../utils/sanitize';
import { GRAPH_CONFIG, COLORS } from '../constants';

const colors = COLORS;

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
  const { toggleNodeCollapse, toggleTableExpansion } = useLineageActions();
  const expandedTableIds = useLineageStore((state) => state.expandedTableIds);

  if (!isTableNodeData(data)) {
    console.error('Invalid node data type for TableNode', data);
    return <div>Invalid node data</div>;
  }

  const nodeData = data;
  const isCte = nodeData.nodeType === 'cte';
  const isVirtualOutput = nodeData.nodeType === 'virtualOutput';
  const isRecursive = !!nodeData.isRecursive;
  const isSelected = selected || nodeData.isSelected;
  const isHighlighted = nodeData.isHighlighted;
  const isCollapsed = nodeData.isCollapsed;
  const isExpanded = expandedTableIds.has(id);
  const hiddenColumnCount = nodeData.hiddenColumnCount || 0;

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
          : isRecursive
            ? `0 0 0 2px ${colors.recursive}20`
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
