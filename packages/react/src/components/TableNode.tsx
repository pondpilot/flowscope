import { Handle, Position } from '@xyflow/react';
import type { NodeProps } from '@xyflow/react';
import { useLineageActions } from '../store';
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
