import { memo, type JSX } from 'react';
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
import { useColors } from '../hooks/useColors';

/**
 * React Flow node component for rendering script/file nodes in script-level view.
 * Displays script name with details in a tooltip.
 */
function ScriptNodeComponent({ data, selected }: NodeProps): JSX.Element {
  const colors = useColors();
  const scriptColors = colors.nodes.script;
  const nodeData = data as ScriptNodeData;
  const { label, tablesRead, tablesWritten, statementCount, isSelected, isHighlighted } = nodeData;

  // Determine selection state from either prop or data
  const active = selected || isSelected;

  return (
    <GraphTooltipProvider>
      <GraphTooltip delayDuration={UI_CONSTANTS.TOOLTIP_DELAY_NODE}>
        <GraphTooltipTrigger asChild>
          <div
            style={{
              backgroundColor: isHighlighted ? colors.interactive.related : scriptColors.bg,
              borderColor: active ? colors.interactive.selection : scriptColors.border,
              boxShadow: active ? `0 0 0 2px ${colors.interactive.selectionRing}` : undefined,
            }}
            className="min-w-[180px] rounded-lg border shadow-xs transition-all duration-200"
          >
            <Handle
              type="target"
              position={Position.Left}
              className="bg-transparent! border-none!"
            />

            <div className="flex items-center gap-2 px-3 py-2.5">
              <div
                className="flex h-8 w-8 shrink-0 items-center justify-center rounded"
                style={{ backgroundColor: scriptColors.headerBg, color: scriptColors.accent }}
              >
                <FileCode className="h-4 w-4" strokeWidth={1.5} />
              </div>
              <div className="flex-1 overflow-hidden">
                <div
                  className="truncate text-xs font-medium uppercase tracking-wider mb-0.5"
                  style={{ color: scriptColors.textSecondary }}
                >
                  Script
                </div>
                <div
                  className="truncate text-sm font-semibold"
                  style={{ color: scriptColors.text }}
                  title={label}
                >
                  {label}
                </div>
              </div>
            </div>

            <Handle
              type="source"
              position={Position.Right}
              className="bg-transparent! border-none!"
            />
          </div>
        </GraphTooltipTrigger>

        <GraphTooltipPortal>
          <GraphTooltipContent
            side="right"
            sideOffset={10}
            className="max-w-[300px] p-0 overflow-hidden bg-white dark:bg-slate-800 text-gray-900 dark:text-slate-100 border border-gray-200 dark:border-slate-700 shadow-xl"
          >
            <div
              className="px-3 py-2 border-b"
              style={{ backgroundColor: scriptColors.headerBg, borderColor: scriptColors.border }}
            >
              <h4 className="font-semibold text-sm" style={{ color: scriptColors.text }}>
                {label}
              </h4>
              <p className="text-xs" style={{ color: scriptColors.textSecondary }}>
                {statementCount} statement{statementCount !== 1 ? 's' : ''}
              </p>
            </div>

            <div className="p-3 space-y-3 text-xs">
              {tablesRead.length > 0 && (
                <div>
                  <div
                    className="font-semibold mb-1 flex items-center gap-1"
                    style={{ color: colors.status.success }}
                  >
                    <span
                      className="w-1.5 h-1.5 rounded-full"
                      style={{ backgroundColor: colors.status.success }}
                    />
                    Reads ({tablesRead.length})
                  </div>
                  <div className="text-gray-600 dark:text-slate-300 pl-2.5 leading-relaxed">
                    {tablesRead.slice(0, UI_CONSTANTS.MAX_TOOLTIP_TABLES).join(', ')}
                    {tablesRead.length > UI_CONSTANTS.MAX_TOOLTIP_TABLES && (
                      <span className="text-gray-400 dark:text-slate-500">
                        {' '}
                        +{tablesRead.length - UI_CONSTANTS.MAX_TOOLTIP_TABLES} more
                      </span>
                    )}
                  </div>
                </div>
              )}

              {tablesWritten.length > 0 && (
                <div>
                  <div
                    className="font-semibold mb-1 flex items-center gap-1"
                    style={{ color: colors.status.info }}
                  >
                    <span
                      className="w-1.5 h-1.5 rounded-full"
                      style={{ backgroundColor: colors.status.info }}
                    />
                    Writes ({tablesWritten.length})
                  </div>
                  <div className="text-gray-600 dark:text-slate-300 pl-2.5 leading-relaxed">
                    {tablesWritten.slice(0, UI_CONSTANTS.MAX_TOOLTIP_TABLES).join(', ')}
                    {tablesWritten.length > UI_CONSTANTS.MAX_TOOLTIP_TABLES && (
                      <span className="text-gray-400 dark:text-slate-500">
                        {' '}
                        +{tablesWritten.length - UI_CONSTANTS.MAX_TOOLTIP_TABLES} more
                      </span>
                    )}
                  </div>
                </div>
              )}

              {tablesRead.length === 0 && tablesWritten.length === 0 && (
                <div className="text-gray-400 dark:text-slate-500 italic">
                  No table dependencies detected
                </div>
              )}
            </div>
            <GraphTooltipArrow className="fill-white dark:fill-slate-800" />
          </GraphTooltipContent>
        </GraphTooltipPortal>
      </GraphTooltip>
    </GraphTooltipProvider>
  );
}

export const ScriptNode = memo(ScriptNodeComponent);
