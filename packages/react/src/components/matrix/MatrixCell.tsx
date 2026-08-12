import { memo, useMemo, type CSSProperties } from 'react';
import { ArrowLeft, ArrowRight, Minus } from 'lucide-react';
import type { MatrixSubMode } from '../../types';
import type {
  MatrixCellData,
  ScriptDependency,
  TableDependencyWithDetails,
} from '../../utils/matrixUtils';
import {
  GraphTooltip,
  GraphTooltipArrow,
  GraphTooltipContent,
  GraphTooltipPortal,
  GraphTooltipTrigger,
} from '../ui/graph-tooltip';
import { HEATMAP_ALPHA_RANGE, HEATMAP_MIN_ALPHA, MAX_TOOLTIP_COLUMN_MATCHES } from './constants';
import { cn, getShortName } from './utils';

interface MatrixCellProps {
  cellData: MatrixCellData;
  rowName: string;
  colName: string;
  isRowHovered: boolean;
  isColHovered: boolean;
  isDimmed: boolean;
  intensity: number; // 0 to 1
  heatmapMode: boolean;
  filterMode: 'rows' | 'columns' | 'fields'; // Added
  filterText: string; // Added
  onHover: (row: string, col: string) => void;
  onLeave: () => void;
  onClick: (row: string, col: string) => void;
  subMode: MatrixSubMode;
  style: CSSProperties;
}

export const MatrixCell = memo(
  function MatrixCell({
    cellData,
    rowName,
    colName,
    isRowHovered,
    isColHovered,
    isDimmed,
    intensity,
    heatmapMode,
    filterMode,
    filterText,
    onHover,
    onLeave,
    onClick,
    subMode,
    style,
  }: MatrixCellProps) {
    const isSelf = cellData.type === 'self';
    const isNone = cellData.type === 'none';
    const hasDependency = !isSelf && !isNone;

    const baseClass =
      'flex items-center justify-center transition-all duration-200 border-r border-b border-slate-100 dark:border-slate-700';

    const bgClass = useMemo(() => {
      if (isRowHovered && isColHovered)
        return 'bg-indigo-100 dark:bg-indigo-900/30 ring-1 ring-inset ring-indigo-500 z-10';
      if (isRowHovered) return 'bg-slate-50 dark:bg-slate-800/50';
      if (isColHovered) return 'bg-slate-50 dark:bg-slate-800/50';

      // Heatmap Logic
      if (heatmapMode && hasDependency) {
        return 'bg-background'; // Base fallback
      }

      return 'bg-background';
    }, [isRowHovered, isColHovered, heatmapMode, hasDependency]);

    // Dynamic style for heatmap
    const heatmapStyle = useMemo(() => {
      if (!heatmapMode || !hasDependency) return {};
      const alpha = HEATMAP_MIN_ALPHA + intensity * HEATMAP_ALPHA_RANGE;
      const color =
        cellData.type === 'write' ? `rgba(16, 185, 129, ${alpha})` : `rgba(37, 99, 235, ${alpha})`;
      return { backgroundColor: color };
    }, [heatmapMode, hasDependency, intensity, cellData.type]);

    const content = useMemo(() => {
      switch (cellData.type) {
        case 'self':
          return <Minus className="h-3 w-3 text-slate-300 dark:text-slate-700" />;
        case 'write':
          return (
            <ArrowRight
              className="h-4 w-4 text-emerald-600 dark:text-emerald-400"
              strokeWidth={2.5}
            />
          );
        case 'read':
          return (
            <ArrowLeft className="h-4 w-4 text-blue-600 dark:text-blue-400" strokeWidth={2.5} />
          );
        case 'none':
        default:
          return null;
      }
    }, [cellData.type]);

    const tooltipContent = useMemo(() => {
      const displayRowName = getShortName(rowName);
      const displayColName = getShortName(colName);

      if (isSelf)
        return <div className="text-slate-300 whitespace-nowrap">{displayRowName} (self)</div>;
      if (isNone) return <div className="text-slate-400 whitespace-nowrap">No dependency</div>;

      const isWrite = cellData.type === 'write';

      // Field Tracing Specific Tooltip
      if (filterMode === 'fields' && filterText && subMode === 'tables') {
        const details = cellData.details as TableDependencyWithDetails | undefined;
        const lowerSearch = filterText.toLowerCase();

        if (details && details.columns) {
          // Find matching columns
          const matchedCols = details.columns.filter(
            (c) =>
              c.source.toLowerCase().includes(lowerSearch) ||
              c.target.toLowerCase().includes(lowerSearch)
          );

          if (matchedCols.length > 0) {
            return (
              <div className="space-y-3 min-w-[200px]">
                <div className="flex items-center justify-between border-b border-white/10 pb-2">
                  <div className="flex items-center gap-2 font-medium text-sm">
                    <span className="text-slate-400">
                      {isWrite ? displayRowName : displayColName}
                    </span>
                    <ArrowRight className="h-3.5 w-3.5 text-slate-500" />
                    <span className="text-white">{isWrite ? displayColName : displayRowName}</span>
                  </div>
                </div>

                <div className="space-y-2">
                  {matchedCols.slice(0, MAX_TOOLTIP_COLUMN_MATCHES).map((col, i) => (
                    <div key={i} className="text-xs bg-white/5 p-2 rounded border border-white/5">
                      <div className="flex items-center gap-2 mb-1">
                        <span
                          className={cn(
                            'font-mono text-slate-300',
                            col.source.toLowerCase().includes(lowerSearch) &&
                              'text-amber-400 font-bold'
                          )}
                        >
                          {col.source}
                        </span>
                        <ArrowRight className="h-3 w-3 text-slate-600" />
                        <span
                          className={cn(
                            'font-mono text-slate-300',
                            col.target.toLowerCase().includes(lowerSearch) &&
                              'text-amber-400 font-bold'
                          )}
                        >
                          {col.target}
                        </span>
                      </div>
                      {col.expression && col.expression !== col.target && (
                        <div className="text-[10px] text-slate-500 font-mono border-t border-white/5 pt-1 mt-1 truncate max-w-[250px]">
                          = {col.expression}
                        </div>
                      )}
                    </div>
                  ))}
                  {matchedCols.length > MAX_TOOLTIP_COLUMN_MATCHES && (
                    <div className="text-[10px] text-slate-500 italic pl-1">
                      + {matchedCols.length - MAX_TOOLTIP_COLUMN_MATCHES} more matched columns...
                    </div>
                  )}
                </div>
              </div>
            );
          }
        }
      }

      // Standard Tooltip
      if (subMode === 'tables') {
        const details = cellData.details as TableDependencyWithDetails | undefined;
        return (
          <div className="space-y-2">
            <div className="flex items-center gap-2 font-medium text-sm whitespace-nowrap">
              <span className="text-slate-400">{isWrite ? displayRowName : displayColName}</span>
              <ArrowRight className="h-3.5 w-3.5 text-slate-500 shrink-0" />
              <span className="text-white">{isWrite ? displayColName : displayRowName}</span>
            </div>
            {details && details.columnCount > 0 && (
              <div className="text-xs text-slate-300 bg-white/10 px-2 py-1.5 rounded border border-white/10 whitespace-nowrap">
                <span className="font-semibold text-white">{details.columnCount}</span> column
                {details.columnCount > 1 ? 's' : ''} mapped
              </div>
            )}
          </div>
        );
      } else {
        const details = cellData.details as ScriptDependency | undefined;
        return (
          <div className="space-y-2">
            <div className="flex items-center gap-2 font-medium text-sm whitespace-nowrap">
              <span className="text-slate-400">{isWrite ? displayRowName : displayColName}</span>
              <ArrowRight className="h-3.5 w-3.5 text-slate-500 shrink-0" />
              <span className="text-white">{isWrite ? displayColName : displayRowName}</span>
            </div>
            {details && (
              <div className="text-xs text-slate-300 bg-white/10 px-2 py-1.5 rounded border border-white/10 whitespace-nowrap">
                <span className="font-semibold text-white">Via:</span>{' '}
                {details.sharedTables.slice(0, 3).join(', ')}
                {details.sharedTables.length > 3 && '...'}
              </div>
            )}
          </div>
        );
      }
    }, [cellData, rowName, colName, subMode, isSelf, isNone, filterMode, filterText]);

    return (
      <GraphTooltip delayDuration={300}>
        <GraphTooltipTrigger asChild>
          <div
            className={cn(
              baseClass,
              bgClass,
              hasDependency && 'cursor-pointer hover:bg-slate-100 dark:hover:bg-slate-800',
              isDimmed && 'opacity-20 grayscale'
            )}
            style={{ ...style, ...heatmapStyle }}
            onMouseEnter={() => onHover(rowName, colName)}
            onMouseLeave={onLeave}
            onClick={() => onClick(rowName, colName)}
            role="gridcell"
          >
            {content}
          </div>
        </GraphTooltipTrigger>
        <GraphTooltipPortal>
          <GraphTooltipContent side="top" className="max-w-none!">
            {tooltipContent}
            <GraphTooltipArrow />
          </GraphTooltipContent>
        </GraphTooltipPortal>
      </GraphTooltip>
    );
  },
  (prev, next) => {
    return (
      prev.cellData === next.cellData &&
      prev.isRowHovered === next.isRowHovered &&
      prev.isColHovered === next.isColHovered &&
      prev.subMode === next.subMode &&
      prev.rowName === next.rowName &&
      prev.colName === next.colName &&
      prev.isDimmed === next.isDimmed &&
      prev.intensity === next.intensity &&
      prev.heatmapMode === next.heatmapMode &&
      prev.filterMode === next.filterMode && // Added
      prev.filterText === next.filterText // Added
    );
  }
);

// ============================================================================
// Main Component
// ============================================================================
