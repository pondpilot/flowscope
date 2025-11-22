import { memo } from 'react';
import type { NodeProps } from '@xyflow/react';
import type { ScriptNodeData } from '../types';

/**
 * React Flow node component for rendering script/file nodes in script-level view.
 * Displays script name, I/O summary, and statement count.
 */
function ScriptNodeComponent({ data }: NodeProps): JSX.Element {
  const nodeData = data as ScriptNodeData;
  const { label, tablesRead, tablesWritten, statementCount, isSelected, isHighlighted } = nodeData;

  return (
    <div
      className={`
        px-4 py-3 rounded-lg border-2 bg-white shadow-md min-w-[200px]
        ${isSelected ? 'border-blue-500 ring-2 ring-blue-200' : 'border-purple-400'}
        ${isHighlighted ? 'ring-2 ring-yellow-300' : ''}
        transition-all duration-200
      `}
    >
      <div className="flex items-center gap-2 mb-2">
        <div className="w-2 h-2 rounded-full bg-purple-500" />
        <div className="font-semibold text-gray-900 text-sm">{label}</div>
      </div>

      <div className="space-y-1 text-xs text-gray-600">
        <div className="flex items-center gap-1">
          <span className="font-medium">Statements:</span>
          <span className="text-gray-800">{statementCount}</span>
        </div>

        {tablesRead.length > 0 && (
          <div>
            <span className="font-medium text-green-700">Reads:</span>
            <div className="ml-2 mt-0.5">
              {tablesRead.slice(0, 3).map((table: string, i: number) => (
                <div key={i} className="text-green-600 truncate">
                  {table}
                </div>
              ))}
              {tablesRead.length > 3 && (
                <div className="text-green-500 italic">+{tablesRead.length - 3} more</div>
              )}
            </div>
          </div>
        )}

        {tablesWritten.length > 0 && (
          <div>
            <span className="font-medium text-blue-700">Writes:</span>
            <div className="ml-2 mt-0.5">
              {tablesWritten.slice(0, 3).map((table: string, i: number) => (
                <div key={i} className="text-blue-600 truncate">
                  {table}
                </div>
              ))}
              {tablesWritten.length > 3 && (
                <div className="text-blue-500 italic">+{tablesWritten.length - 3} more</div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export const ScriptNode = memo(ScriptNodeComponent);
