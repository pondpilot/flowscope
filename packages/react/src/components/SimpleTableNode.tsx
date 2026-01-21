import { memo, type JSX } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Table2 } from 'lucide-react';
import type { TableNodeData } from '../types';
import { sanitizeIdentifier } from '../utils/sanitize';
import { useColors, useIsDarkMode } from '../hooks/useColors';
import { getNamespaceColor } from '../constants';

/**
 * A simplified Table Node for the Script/Hybrid view.
 * Displays icon and name only, with fixed handles.
 */
function SimpleTableNodeComponent({ data, selected }: NodeProps): JSX.Element {
  const colors = useColors();
  const isDark = useIsDarkMode();
  const nodeData = data as TableNodeData;
  const { label, nodeType, isSelected, isHighlighted, schema, database, qualifiedName } = nodeData;

  const active = selected || isSelected;

  // Get schema color for left border band
  const schemaColor = getNamespaceColor(schema, isDark);

  // Determine colors based on node type
  type NodePalette = {
    bg: string;
    headerBg: string;
    border: string;
    text: string;
    textSecondary: string;
    accent: string;
  };
  let palette: NodePalette = colors.nodes.table;
  if (nodeType === 'cte') {
    palette = colors.nodes.cte;
  } else if (nodeType === 'view') {
    palette = colors.nodes.view;
  } else if (nodeType === 'virtualOutput') {
    palette = colors.nodes.virtualOutput;
  }

  return (
    <div
      className={`
        flex items-center gap-2 px-3 py-2 rounded-lg border shadow-xs min-w-[140px] max-w-[200px]
        transition-all duration-200
        ${active ? 'ring-2' : ''}
      `}
      style={{
        borderColor: active ? colors.accent : palette.border,
        borderLeftWidth: schemaColor ? 3 : undefined,
        borderLeftColor: schemaColor,
        backgroundColor: isHighlighted ? colors.interactive.related : palette.bg,
        boxShadow: active ? `0 0 0 2px ${colors.interactive.selectionRing}` : undefined,
      }}
    >
      {/* Left Handle (Target) */}
      <Handle
        type="target"
        position={Position.Left}
        className="w-2! h-2! bg-slate-300! border-none! hover:bg-slate-400!"
      />

      <div
        className="flex h-6 w-6 shrink-0 items-center justify-center rounded"
        style={{ backgroundColor: palette.headerBg, color: palette.text }}
      >
        <Table2 className="h-3.5 w-3.5" />
      </div>

      <div className="flex-1 min-w-0">
        <div
          className="truncate text-xs font-medium"
          style={{ color: palette.text }}
          title={qualifiedName || label}
        >
          {sanitizeIdentifier(label)}
        </div>
        {/* Show namespace when available */}
        {(database || schema) && (
          <div
            className="truncate text-[10px] uppercase"
            style={{ color: palette.textSecondary, opacity: 0.7 }}
            title={qualifiedName || undefined}
          >
            {database && schema ? `${database}.${schema}` : schema}
          </div>
        )}
      </div>

      {/* Right Handle (Source) */}
      <Handle
        type="source"
        position={Position.Right}
        className="w-2! h-2! bg-slate-300! border-none! hover:bg-slate-400!"
      />
    </div>
  );
}

export const SimpleTableNode = memo(SimpleTableNodeComponent);
