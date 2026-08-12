import React, {
  type Dispatch,
  type JSX,
  type MouseEventHandler,
  type RefObject,
  type SetStateAction,
} from 'react';
import { Filter } from 'lucide-react';
import type { MatrixSubMode } from '../../types';
import type {
  MatrixData,
  ScriptDependency,
  TableDependencyWithDetails,
} from '../../utils/matrixUtils';
import { MatrixCell } from './MatrixCell';
import { CELL_HEIGHT, CELL_WIDTH } from './constants';
import type { FilterMode, MatrixMetrics, TransitiveSet } from './types';
import { cn, getShortName } from './utils';

type HoveredCell = { row: string; col: string } | null;
type ResizingMode = 'none' | 'column' | 'header';

interface MatrixGridProps {
  isEmpty: boolean;
  setFilterText: Dispatch<SetStateAction<string>>;
  resizingMode: ResizingMode;
  scrollContainerRef: RefObject<HTMLDivElement | null>;
  firstColumnWidth: number;
  filteredColumnItems: string[];
  filteredRowItems: string[];
  headerHeight: number;
  handleColumnResizeStart: MouseEventHandler<HTMLDivElement>;
  handleHeaderResizeStart: MouseEventHandler<HTMLDivElement>;
  focusedNode: string | null;
  transitiveFlow: TransitiveSet | null;
  xRayMode: boolean;
  matrixMetrics: MatrixMetrics;
  complexityMode: boolean;
  hoveredCell: HoveredCell;
  toggleFocus: (node: string) => void;
  fullMatrixData: MatrixData;
  heatmapMode: boolean;
  matrixSubMode: MatrixSubMode;
  maxIntensity: number;
  filterMode: FilterMode;
  filterText: string;
  handleCellHover: (row: string, col: string) => void;
  setHoveredCell: Dispatch<SetStateAction<HoveredCell>>;
  handleCellClick: (row: string, col: string) => void;
}

export function MatrixGrid({
  isEmpty,
  setFilterText,
  resizingMode,
  scrollContainerRef,
  firstColumnWidth,
  filteredColumnItems,
  filteredRowItems,
  headerHeight,
  handleColumnResizeStart,
  handleHeaderResizeStart,
  focusedNode,
  transitiveFlow,
  xRayMode,
  matrixMetrics,
  complexityMode,
  hoveredCell,
  toggleFocus,
  fullMatrixData,
  heatmapMode,
  matrixSubMode,
  maxIntensity,
  filterMode,
  filterText,
  handleCellHover,
  setHoveredCell,
  handleCellClick,
}: MatrixGridProps): JSX.Element {
  return (
    <>
      {isEmpty ? (
        <div className="flex-1 flex flex-col items-center justify-center text-slate-400 gap-3">
          <Filter className="h-10 w-10 opacity-20" />
          <p>No items match your filter</p>
          <button
            onClick={() => setFilterText('')}
            className="text-indigo-600 hover:text-indigo-700 text-sm font-medium"
          >
            Clear filter
          </button>
        </div>
      ) : (
        <div
          className={cn(
            'flex-1 overflow-auto relative custom-scrollbar',
            resizingMode !== 'none' &&
              (resizingMode === 'column'
                ? 'select-none cursor-col-resize'
                : 'select-none cursor-row-resize')
          )}
          ref={scrollContainerRef}
        >
          <div
            className="grid"
            style={{
              gridTemplateColumns: `${firstColumnWidth}px repeat(${filteredColumnItems.length}, ${CELL_WIDTH}px)`,
              minWidth: 'min-content',
            }}
            role="grid"
          >
            {/* Top Left Corner */}
            <div
              className="sticky top-0 left-0 z-30 bg-background border-b border-r border-slate-200 dark:border-slate-600 shadow-[2px_2px_10px_rgba(0,0,0,0.05)]"
              style={{ height: headerHeight }}
            >
              <div
                onMouseDown={handleColumnResizeStart}
                className={cn(
                  'absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-indigo-400 transition-colors z-40',
                  resizingMode === 'column' ? 'bg-indigo-500' : 'bg-transparent'
                )}
              />
              <div
                onMouseDown={handleHeaderResizeStart}
                className={cn(
                  'absolute bottom-0 left-0 right-0 h-1 cursor-row-resize hover:bg-indigo-400 transition-colors z-40',
                  resizingMode === 'header' ? 'bg-indigo-500' : 'bg-transparent'
                )}
              />
            </div>

            {/* Column Headers */}
            {filteredColumnItems.map((item) => {
              const isFocused = focusedNode === item;

              const isAncestor = transitiveFlow?.ancestors.has(item);
              const isDescendant = transitiveFlow?.descendants.has(item);
              const isRelated = isAncestor || isDescendant;

              // Dim if X-Ray on, node selected, and this is NOT involved
              const isDimmed = xRayMode && focusedNode && !isRelated && !isFocused;

              // Complexity
              const fanIn = matrixMetrics.colCounts.get(item) || 0;
              const complexityPct = matrixMetrics.maxCol ? (fanIn / matrixMetrics.maxCol) * 100 : 0;

              return (
                <div
                  key={`col-${item}`}
                  className={cn(
                    'sticky top-0 z-20 bg-background border-b border-r border-slate-200 dark:border-slate-600 shadow-xs group cursor-pointer transition-colors duration-200 relative',
                    hoveredCell?.col === item && 'bg-slate-50 dark:bg-slate-900',

                    // Highlight logic
                    isFocused && 'bg-purple-100 dark:bg-purple-900/40',
                    isAncestor && 'bg-blue-50 dark:bg-blue-900/20',
                    isDescendant && 'bg-emerald-50 dark:bg-emerald-900/20',

                    isDimmed && 'opacity-20 grayscale'
                  )}
                  style={{ height: headerHeight }}
                  onClick={() => toggleFocus(item)}
                  title={xRayMode ? 'Click to focus X-Ray' : item}
                >
                  {/* Complexity Bar (Vertical, growing from bottom) */}
                  {complexityMode && complexityPct > 0 && (
                    <div
                      className="absolute bottom-0 left-0 right-0 bg-emerald-500/10 dark:bg-emerald-400/10 transition-all z-0"
                      style={{ height: `${complexityPct}%` }}
                    />
                  )}

                  <div className="w-full h-full flex items-end justify-center pb-2 relative z-10">
                    <span
                      className={cn(
                        'block text-xs font-medium text-slate-600 dark:text-slate-400 hover:text-indigo-600 dark:hover:text-indigo-400 transition-colors whitespace-nowrap overflow-hidden text-ellipsis',
                        isFocused && 'text-purple-700 dark:text-purple-300 font-bold',
                        isAncestor && 'text-blue-600 dark:text-blue-400 font-semibold',
                        isDescendant && 'text-emerald-600 dark:text-emerald-400 font-semibold'
                      )}
                      style={{
                        writingMode: 'vertical-rl',
                        transform: 'rotate(180deg)',
                        maxHeight: '100%',
                      }}
                    >
                      {getShortName(item)}
                    </span>
                  </div>
                </div>
              );
            })}

            {/* Rows */}
            {filteredRowItems.map((rowItem) => {
              const isFocused = focusedNode === rowItem;
              const isAncestor = transitiveFlow?.ancestors.has(rowItem);
              const isDescendant = transitiveFlow?.descendants.has(rowItem);
              const isRelated = isAncestor || isDescendant;
              const isDimmed = xRayMode && focusedNode && !isRelated && !isFocused;

              // Complexity
              const fanOut = matrixMetrics.rowCounts.get(rowItem) || 0;
              const complexityPct = matrixMetrics.maxRow
                ? (fanOut / matrixMetrics.maxRow) * 100
                : 0;

              return (
                <React.Fragment key={`row-${rowItem}`}>
                  {/* Row Header */}
                  <div
                    className={cn(
                      'sticky left-0 z-20 bg-background border-b border-r border-slate-200 dark:border-slate-600 px-3 flex items-center shadow-[2px_0_5px_rgba(0,0,0,0.02)] cursor-pointer transition-colors duration-200 relative',
                      hoveredCell?.row === rowItem && 'bg-slate-50 dark:bg-slate-900',

                      isFocused && 'bg-purple-100 dark:bg-purple-900/40',
                      isAncestor && 'bg-blue-50 dark:bg-blue-900/20',
                      isDescendant && 'bg-emerald-50 dark:bg-emerald-900/20',

                      isDimmed && 'opacity-20 grayscale'
                    )}
                    style={{ height: CELL_HEIGHT }}
                    onClick={() => toggleFocus(rowItem)}
                    title={xRayMode ? 'Click to focus X-Ray' : rowItem}
                  >
                    {/* Complexity Bar (Horizontal, growing from left) */}
                    {complexityMode && complexityPct > 0 && (
                      <div
                        className="absolute top-0 bottom-0 left-0 bg-blue-500/10 dark:bg-blue-400/10 transition-all z-0"
                        style={{ width: `${complexityPct}%` }}
                      />
                    )}

                    <span
                      className={cn(
                        'text-xs font-medium text-slate-700 dark:text-slate-300 truncate w-full relative z-10',
                        isFocused && 'text-purple-700 dark:text-purple-300 font-bold',
                        isAncestor && 'text-blue-600 dark:text-blue-400 font-semibold',
                        isDescendant && 'text-emerald-600 dark:text-emerald-400 font-semibold'
                      )}
                    >
                      {getShortName(rowItem)}
                    </span>
                    <div
                      onMouseDown={handleColumnResizeStart}
                      className={cn(
                        'absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-indigo-400 transition-colors z-40',
                        resizingMode === 'column' ? 'bg-indigo-500' : 'bg-transparent'
                      )}
                    />
                  </div>

                  {/* Cells */}
                  {filteredColumnItems.map((colItem) => {
                    const cellData = fullMatrixData.cells.get(rowItem)?.get(colItem);
                    if (!cellData) return <div key={`${rowItem}-${colItem}`} />;

                    let cellIntensity = 0;
                    if (heatmapMode) {
                      let count = 0;
                      if (matrixSubMode === 'tables') {
                        count = (cellData.details as TableDependencyWithDetails)?.columnCount || 0;
                      } else {
                        count = (cellData.details as ScriptDependency)?.sharedTables.length || 0;
                      }
                      cellIntensity = count / maxIntensity;
                    }

                    // Dimming logic
                    let isCellDimmed = false;
                    if (xRayMode && focusedNode) {
                      const isRowActive =
                        focusedNode === rowItem ||
                        transitiveFlow?.ancestors.has(rowItem) ||
                        transitiveFlow?.descendants.has(rowItem);
                      const isColActive =
                        focusedNode === colItem ||
                        transitiveFlow?.ancestors.has(colItem) ||
                        transitiveFlow?.descendants.has(colItem);

                      if (!isRowActive || !isColActive) {
                        isCellDimmed = true;
                      }
                    }

                    return (
                      <MatrixCell
                        key={`${rowItem}-${colItem}`}
                        cellData={cellData}
                        rowName={rowItem}
                        colName={colItem}
                        isRowHovered={hoveredCell?.row === rowItem}
                        isColHovered={hoveredCell?.col === colItem}
                        isDimmed={isCellDimmed}
                        intensity={cellIntensity}
                        heatmapMode={heatmapMode}
                        filterMode={filterMode}
                        filterText={filterText}
                        onHover={handleCellHover}
                        onLeave={() => setHoveredCell(null)}
                        onClick={handleCellClick}
                        subMode={matrixSubMode}
                        style={{ height: CELL_HEIGHT, width: CELL_WIDTH }}
                      />
                    );
                  })}
                </React.Fragment>
              );
            })}
          </div>
        </div>
      )}
    </>
  );
}
