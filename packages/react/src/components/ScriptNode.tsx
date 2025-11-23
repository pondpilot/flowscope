import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { FileCode } from 'lucide-react';
import type { ScriptNodeData } from '../types';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipProvider,
  GraphTooltipTrigger,
  GraphTooltipArrow,
  GraphTooltipPortal,
} from './ui/graph-tooltip';
import { UI_CONSTANTS } from '../constants';

/**
 * React Flow node component for rendering script/file nodes in script-level view.
 * Displays script name with details in a tooltip.
 */
function ScriptNodeComponent({ data, selected }: NodeProps): JSX.Element {
  const nodeData = data as ScriptNodeData;
  const { label, tablesRead, tablesWritten, statementCount, isSelected, isHighlighted } = nodeData;

  // Determine selection state from either prop or data
  const active = selected || isSelected;

  return (
    <GraphTooltipProvider>
      <GraphTooltip delayDuration={UI_CONSTANTS.TOOLTIP_DELAY_NODE}>
        <GraphTooltipTrigger asChild>
          <div
            className={`
              min-w-[180px] rounded-lg border bg-white shadow-sm transition-all duration-200
              ${active ? 'border-[#4C61FF] ring-2 ring-[#4C61FF]/20' : 'border-[#DBDDE1]'}
              ${isHighlighted ? 'bg-[hsl(var(--highlight))]' : 'bg-white'}
            `}
          >
            <Handle type="target" position={Position.Left} className="!bg-transparent !border-none" />

            <div className="flex items-center gap-2 px-3 py-2.5">
              <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded bg-purple-50 text-purple-600">
                <FileCode className="h-4 w-4" strokeWidth={1.5} />
              </div>
              <div className="flex-1 overflow-hidden">
                <div className="truncate text-xs font-medium text-gray-500 uppercase tracking-wider mb-0.5">
                  Script
                </div>
                <div className="truncate text-sm font-semibold text-gray-900" title={label}>
                  {label}
                </div>
              </div>
            </div>

            <Handle type="source" position={Position.Right} className="!bg-transparent !border-none" />
          </div>
        </GraphTooltipTrigger>
        
        <GraphTooltipPortal>
          <GraphTooltipContent side="right" sideOffset={10} className="max-w-[300px] p-0 overflow-hidden bg-white text-gray-900 border border-gray-200 shadow-xl">
            <div className="bg-gray-50 px-3 py-2 border-b border-gray-100">
              <h4 className="font-semibold text-sm">{label}</h4>
              <p className="text-xs text-gray-500">{statementCount} statement{statementCount !== 1 ? 's' : ''}</p>
            </div>
            
            <div className="p-3 space-y-3 text-xs">
              {tablesRead.length > 0 && (
                <div>
                  <div className="font-semibold text-green-700 mb-1 flex items-center gap-1">
                    <span className="w-1.5 h-1.5 rounded-full bg-green-500"></span>
                    Reads ({tablesRead.length})
                  </div>
                  <div className="text-gray-600 pl-2.5 leading-relaxed">
                    {tablesRead.slice(0, UI_CONSTANTS.MAX_TOOLTIP_TABLES).join(', ')}
                    {tablesRead.length > UI_CONSTANTS.MAX_TOOLTIP_TABLES && (
                      <span className="text-gray-400"> +{tablesRead.length - UI_CONSTANTS.MAX_TOOLTIP_TABLES} more</span>
                    )}
                  </div>
                </div>
              )}

              {tablesWritten.length > 0 && (
                <div>
                  <div className="font-semibold text-blue-700 mb-1 flex items-center gap-1">
                    <span className="w-1.5 h-1.5 rounded-full bg-blue-500"></span>
                    Writes ({tablesWritten.length})
                  </div>
                  <div className="text-gray-600 pl-2.5 leading-relaxed">
                    {tablesWritten.slice(0, UI_CONSTANTS.MAX_TOOLTIP_TABLES).join(', ')}
                    {tablesWritten.length > UI_CONSTANTS.MAX_TOOLTIP_TABLES && (
                      <span className="text-gray-400"> +{tablesWritten.length - UI_CONSTANTS.MAX_TOOLTIP_TABLES} more</span>
                    )}
                  </div>
                </div>
              )}
              
              {tablesRead.length === 0 && tablesWritten.length === 0 && (
                <div className="text-gray-400 italic">No table dependencies detected</div>
              )}
            </div>
            <GraphTooltipArrow className="fill-white" />
          </GraphTooltipContent>
        </GraphTooltipPortal>
      </GraphTooltip>
    </GraphTooltipProvider>
  );
}

export const ScriptNode = memo(ScriptNodeComponent);
