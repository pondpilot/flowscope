import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Table2 } from 'lucide-react';
import type { TableNodeData } from '../types';
import { sanitizeIdentifier } from '../utils/sanitize';
import { COLORS } from '../constants';

/**
 * A simplified Table Node for the Script/Hybrid view.
 * Displays icon and name only, with fixed handles.
 */
function SimpleTableNodeComponent({ data, selected }: NodeProps): JSX.Element {
  const nodeData = data as TableNodeData;
  const { label, nodeType, isSelected, isHighlighted } = nodeData;
  
  const active = selected || isSelected;
  
  // Determine colors based on node type
  type NodePalette = {
    bg: string;
    headerBg: string;
    border: string;
    text: string;
    textSecondary: string;
    accent: string;
  };
  let palette: NodePalette = COLORS.nodes.table;
  if (nodeType === 'cte') {
    palette = COLORS.nodes.cte;
  } else if (nodeType === 'view') {
    palette = COLORS.nodes.view;
  } else if (nodeType === 'virtualOutput') {
    palette = COLORS.nodes.virtualOutput;
  }

  return (
    <div
      className={`
        flex items-center gap-2 px-3 py-2 rounded-lg border bg-white shadow-sm min-w-[140px] max-w-[200px]
        transition-all duration-200
        ${active ? 'ring-2' : ''}
      `}
      style={{
        borderColor: active ? COLORS.accent : palette.border,
        backgroundColor: isHighlighted ? 'hsl(var(--highlight))' : palette.bg,
        boxShadow: active ? `0 0 0 2px ${COLORS.accent}40` : '0 1px 2px rgba(0,0,0,0.05)',
      }}
    >
      {/* Left Handle (Target) */}
      <Handle 
        type="target" 
        position={Position.Left} 
        className="!w-2 !h-2 !bg-slate-300 !border-none hover:!bg-slate-400"
      />

      <div 
        className="flex h-6 w-6 shrink-0 items-center justify-center rounded"
        style={{ backgroundColor: palette.headerBg, color: palette.text }}
      >
        <Table2 className="h-3.5 w-3.5" />
      </div>

      <div className="flex-1 min-w-0">
        <div className="truncate text-xs font-medium" style={{ color: palette.text }}>
          {sanitizeIdentifier(label)}
        </div>
      </div>

      {/* Right Handle (Source) */}
      <Handle 
        type="source" 
        position={Position.Right} 
        className="!w-2 !h-2 !bg-slate-300 !border-none hover:!bg-slate-400"
      />
    </div>
  );
}

export const SimpleTableNode = memo(SimpleTableNodeComponent);
