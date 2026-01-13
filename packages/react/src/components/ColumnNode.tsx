import { memo, type JSX } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import type { ColumnNodeData } from '../types';

/**
 * React Flow node component for rendering standalone column nodes in column-level view.
 * Displays column name, parent table, and optional expression.
 */
function ColumnNodeComponent({ data }: NodeProps): JSX.Element {
  const nodeData = data as ColumnNodeData;
  const { label, tableName, expression, isSelected, isHighlighted } = nodeData;

  return (
    <div
      className={`
        px-3 py-2 rounded border bg-white shadow-sm min-w-[120px] max-w-[200px]
        ${isSelected ? 'border-blue-500 ring-2 ring-blue-200' : 'border-gray-300'}
        ${isHighlighted ? 'ring-2 ring-yellow-300' : ''}
        transition-all duration-200
      `}
    >
      <Handle type="target" position={Position.Top} className="w-2 h-2" />

      <div className="text-xs text-gray-500 mb-0.5 truncate">{tableName}</div>
      <div className="font-medium text-gray-900 text-sm truncate">{label}</div>

      {expression && (
        <div className="text-xs text-gray-600 mt-1 font-mono truncate" title={expression}>
          {expression}
        </div>
      )}

      <Handle type="source" position={Position.Bottom} className="w-2 h-2" />
    </div>
  );
}

export const ColumnNode = memo(ColumnNodeComponent);
