import React, { useMemo, useState, useCallback, useRef, useEffect, memo } from 'react';
import {
  Table2,
  FileCode,
  Search,
  ArrowRight,
  ArrowLeft,
  Minus,
  Database,
  Filter
} from 'lucide-react';
import { useLineage } from '../store';
import type { MatrixSubMode } from '../types';
import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipProvider,
  GraphTooltipTrigger,
  GraphTooltipArrow,
  GraphTooltipPortal,
} from './ui/graph-tooltip';
import {
  extractTableDependenciesWithDetails,
  extractScriptDependencies,
  buildTableMatrix,
  buildScriptMatrix,
  type TableDependencyWithDetails,
  type ScriptDependency,
  type MatrixCellData,
} from '../utils/matrixUtils';

// ============================================================================
// Utilities
// ============================================================================

function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Get a display-friendly short name from a full identifier.
 * For tables with schema (e.g., "schema.table"), returns just the table name.
 * For scripts (e.g., "01_schema.sql"), removes the .sql extension.
 */
function getShortName(name: string): string {
  // If it looks like a file path (contains .sql), remove the extension
  if (name.endsWith('.sql')) {
    return name.slice(0, -4);
  }
  // For qualified names like schema.table, return just the last part
  const lastDot = name.lastIndexOf('.');
  if (lastDot !== -1) {
    return name.slice(lastDot + 1);
  }
  return name;
}


// ============================================================================
// Sub-components
// ============================================================================

interface MatrixCellProps {
  cellData: MatrixCellData;
  rowName: string;
  colName: string;
  isRowHovered: boolean;
  isColHovered: boolean;
  onHover: (row: string, col: string) => void;
  onLeave: () => void;
  onClick: (row: string, col: string) => void;
  subMode: MatrixSubMode;
  style: React.CSSProperties;
}

const MatrixCell = memo(function MatrixCell({
  cellData,
  rowName,
  colName,
  isRowHovered,
  isColHovered,
  onHover,
  onLeave,
  onClick,
  subMode,
  style,
}: MatrixCellProps) {
  const isSelf = cellData.type === 'self';
  const isNone = cellData.type === 'none';
  const hasDependency = !isSelf && !isNone;

  const baseClass = "flex items-center justify-center transition-all duration-200 border-r border-b border-slate-100 dark:border-slate-800";
  
  const bgClass = useMemo(() => {
    if (isRowHovered && isColHovered) return "bg-indigo-100 dark:bg-indigo-900/30 ring-1 ring-inset ring-indigo-500 z-10";
    if (isRowHovered) return "bg-slate-50 dark:bg-slate-800/50";
    if (isColHovered) return "bg-slate-50 dark:bg-slate-800/50";
    return "bg-white dark:bg-slate-950";
  }, [isRowHovered, isColHovered]);

  const content = useMemo(() => {
    switch (cellData.type) {
      case 'self':
        return <Minus className="h-3 w-3 text-slate-300 dark:text-slate-700" />;
      case 'write':
        return <ArrowRight className="h-4 w-4 text-emerald-600 dark:text-emerald-400" strokeWidth={2.5} />;
      case 'read':
        return <ArrowLeft className="h-4 w-4 text-blue-600 dark:text-blue-400" strokeWidth={2.5} />;
      default:
        return null;
    }
  }, [cellData.type]);

  const tooltipContent = useMemo(() => {
    const displayRowName = getShortName(rowName);
    const displayColName = getShortName(colName);

    if (isSelf) return <div className="text-slate-300 whitespace-nowrap">{displayRowName} (self)</div>;
    if (isNone) return <div className="text-slate-400 whitespace-nowrap">No dependency</div>;

    const isWrite = cellData.type === 'write';

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
              <span className="font-semibold text-white">{details.columnCount}</span> column{details.columnCount > 1 ? 's' : ''} mapped
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
  }, [cellData, rowName, colName, subMode, isSelf, isNone]);

  return (
    <GraphTooltip delayDuration={300}>
      <GraphTooltipTrigger asChild>
        <div
          className={cn(baseClass, bgClass, hasDependency && "cursor-pointer hover:bg-slate-100 dark:hover:bg-slate-800")}
          style={style}
          onMouseEnter={() => onHover(rowName, colName)}
          onMouseLeave={onLeave}
          onClick={() => onClick(rowName, colName)}
          role="gridcell"
        >
          {content}
        </div>
      </GraphTooltipTrigger>
      <GraphTooltipPortal>
        <GraphTooltipContent side="top" className="!max-w-none">
          {tooltipContent}
          <GraphTooltipArrow />
        </GraphTooltipContent>
      </GraphTooltipPortal>
    </GraphTooltip>
  );
}, (prev, next) => {
  return (
    prev.cellData === next.cellData &&
    prev.isRowHovered === next.isRowHovered &&
    prev.isColHovered === next.isColHovered &&
    prev.subMode === next.subMode &&
    prev.rowName === next.rowName &&
    prev.colName === next.colName
  );
});

// ============================================================================
// Main Component
// ============================================================================

interface MatrixViewProps {
  className?: string;
}

const DEFAULT_FIRST_COLUMN_WIDTH = 200;
const MIN_FIRST_COLUMN_WIDTH = 100;
const MAX_FIRST_COLUMN_WIDTH = 400;

const DEFAULT_HEADER_HEIGHT = 160;
const MIN_HEADER_HEIGHT = 80;
const MAX_HEADER_HEIGHT = 400;

const CELL_WIDTH = 48;
const CELL_HEIGHT = 36;

export function MatrixView({ className = '' }: MatrixViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, matrixSubMode } = state;
  const { setMatrixSubMode, highlightSpan, requestNavigation } = actions;

  const [filterText, setFilterText] = useState('');
  const [hoveredCell, setHoveredCell] = useState<{ row: string; col: string } | null>(null);
  
  const [firstColumnWidth, setFirstColumnWidth] = useState(DEFAULT_FIRST_COLUMN_WIDTH);
  const [headerHeight, setHeaderHeight] = useState(DEFAULT_HEADER_HEIGHT);

  const [resizingMode, setResizingMode] = useState<'none' | 'column' | 'header'>('none');
  
  const resizeStartPos = useRef(0);
  const resizeStartSize = useRef(0);
  const scrollContainerRef = useRef<HTMLDivElement>(null);

  // Resize logic
  const handleColumnResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setResizingMode('column');
    resizeStartPos.current = e.clientX;
    resizeStartSize.current = firstColumnWidth;
  }, [firstColumnWidth]);

  const handleHeaderResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setResizingMode('header');
    resizeStartPos.current = e.clientY;
    resizeStartSize.current = headerHeight;
  }, [headerHeight]);

  useEffect(() => {
    if (resizingMode === 'none') return;

    const handleMouseMove = (e: MouseEvent) => {
      if (resizingMode === 'column') {
        const delta = e.clientX - resizeStartPos.current;
        setFirstColumnWidth(Math.min(MAX_FIRST_COLUMN_WIDTH, Math.max(MIN_FIRST_COLUMN_WIDTH, resizeStartSize.current + delta)));
      } else if (resizingMode === 'header') {
        const delta = e.clientY - resizeStartPos.current;
        setHeaderHeight(Math.min(MAX_HEADER_HEIGHT, Math.max(MIN_HEADER_HEIGHT, resizeStartSize.current + delta)));
      }
    };

    const handleMouseUp = () => setResizingMode('none');
    
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [resizingMode]);

  // Data processing
  const { tableDeps, scriptDepsData } = useMemo(() => ({
    tableDeps: result ? extractTableDependenciesWithDetails(result.statements) : [],
    scriptDepsData: result ? extractScriptDependencies(result.statements) : { dependencies: [], allScripts: [] },
  }), [result]);

  const fullMatrixData = useMemo(() =>
    matrixSubMode === 'tables'
      ? buildTableMatrix(tableDeps)
      : buildScriptMatrix(scriptDepsData.dependencies, scriptDepsData.allScripts),
  [matrixSubMode, tableDeps, scriptDepsData]);

  // Filtering
  const filteredRowItems = useMemo(() => {
    if (!filterText) return fullMatrixData.items;
    const lower = filterText.toLowerCase();
    return fullMatrixData.items.filter(item => item.toLowerCase().includes(lower));
  }, [fullMatrixData.items, filterText]);

  const handleCellHover = useCallback((row: string, col: string) => {
    setHoveredCell({ row, col });
  }, []);

  const handleCellClick = useCallback((rowName: string, colName: string) => {
    const cellData = fullMatrixData.cells.get(rowName)?.get(colName);
    if (!cellData || cellData.type === 'self' || cellData.type === 'none') return;

    if (matrixSubMode === 'tables') {
      const details = cellData.details as TableDependencyWithDetails | undefined;
      if (details?.spans.length) highlightSpan(details.spans[0]);
    } else {
      const details = cellData.details as ScriptDependency | undefined;
      if (details) {
        requestNavigation({
          sourceName: details.sourceScript,
          targetName: details.sharedTables[0],
          targetType: 'table',
        });
      }
    }
  }, [fullMatrixData, matrixSubMode, highlightSpan, requestNavigation]);

  if (!result) {
    return (
      <div className={cn("flex flex-col items-center justify-center h-full text-slate-400 gap-4", className)}>
        <Database className="h-12 w-12 opacity-20" />
        <p>No analysis result available</p>
      </div>
    );
  }

  const isEmpty = filteredRowItems.length === 0;

  return (
    <GraphTooltipProvider>
      <div className={cn("flex flex-col h-full bg-white dark:bg-slate-950", className)}>
        {/* Toolbar */}
        <div className="flex items-center justify-between p-4 border-b border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-950 z-20">
          <div className="flex items-center gap-2 bg-slate-100 dark:bg-slate-900 p-1 rounded-lg">
            <button
              onClick={() => setMatrixSubMode('scripts')}
              className={cn(
                "flex items-center gap-2 px-3 py-1.5 text-sm font-medium rounded-md transition-all",
                matrixSubMode === 'scripts'
                  ? "bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100 shadow-sm"
                  : "text-slate-500 hover:text-slate-700 dark:hover:text-slate-300"
              )}
            >
              <FileCode className="h-4 w-4" />
              <span>Scripts</span>
            </button>
            <button
              onClick={() => setMatrixSubMode('tables')}
              className={cn(
                "flex items-center gap-2 px-3 py-1.5 text-sm font-medium rounded-md transition-all",
                matrixSubMode === 'tables'
                  ? "bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100 shadow-sm"
                  : "text-slate-500 hover:text-slate-700 dark:hover:text-slate-300"
              )}
            >
              <Table2 className="h-4 w-4" />
              <span>Tables</span>
            </button>
          </div>

          <div className="relative group">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-slate-400 group-focus-within:text-indigo-500 transition-colors" />
            <input
              type="text"
              placeholder="Filter rows..."
              value={filterText}
              onChange={(e) => setFilterText(e.target.value)}
              className="pl-9 pr-4 py-1.5 text-sm bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-md focus:outline-none focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 transition-all w-64"
            />
          </div>
        </div>

        {/* Content */}
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
            className={cn("flex-1 overflow-auto relative custom-scrollbar", resizingMode !== 'none' && (resizingMode === 'column' ? "select-none cursor-col-resize" : "select-none cursor-row-resize"))}
            ref={scrollContainerRef}
          >
            <div 
              className="grid"
              style={{ 
                gridTemplateColumns: `${firstColumnWidth}px repeat(${fullMatrixData.items.length}, ${CELL_WIDTH}px)`,
                minWidth: 'min-content'
              }}
              role="grid"
            >
              {/* Top Left Corner */}
              <div 
                className="sticky top-0 left-0 z-30 bg-white dark:bg-slate-950 border-b border-r border-slate-200 dark:border-slate-800 shadow-[2px_2px_10px_rgba(0,0,0,0.05)]"
                style={{ height: headerHeight }}
              >
                 {/* Width resizer */}
                 <div
                  onMouseDown={handleColumnResizeStart}
                  className={cn(
                    "absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-indigo-400 transition-colors z-40",
                    resizingMode === 'column' ? "bg-indigo-500" : "bg-transparent"
                  )}
                />
                 {/* Height resizer */}
                 <div
                  onMouseDown={handleHeaderResizeStart}
                  className={cn(
                    "absolute bottom-0 left-0 right-0 h-1 cursor-row-resize hover:bg-indigo-400 transition-colors z-40",
                    resizingMode === 'header' ? "bg-indigo-500" : "bg-transparent"
                  )}
                />
              </div>

              {/* Column Headers */}
              {fullMatrixData.items.map((item) => (
                <div
                  key={`col-${item}`}
                  className={cn(
                    "sticky top-0 z-20 bg-white dark:bg-slate-950 border-b border-r border-slate-200 dark:border-slate-800 shadow-sm",
                    hoveredCell?.col === item && "bg-slate-50 dark:bg-slate-900"
                  )}
                  style={{ height: headerHeight }}
                >
                  <div className="w-full h-full flex items-end justify-center pb-2">
                    <span 
                      className="block text-xs font-medium text-slate-600 dark:text-slate-400 hover:text-indigo-600 dark:hover:text-indigo-400 transition-colors whitespace-nowrap overflow-hidden text-ellipsis" 
                      title={item}
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
              ))}

              {/* Rows */}
              {filteredRowItems.map((rowItem) => (
                <React.Fragment key={`row-${rowItem}`}>
                  {/* Row Header */}
                  <div
                    className={cn(
                      "sticky left-0 z-20 bg-white dark:bg-slate-950 border-b border-r border-slate-200 dark:border-slate-800 px-3 flex items-center shadow-[2px_0_5px_rgba(0,0,0,0.02)]",
                      hoveredCell?.row === rowItem && "bg-slate-50 dark:bg-slate-900"
                    )}
                    style={{ height: CELL_HEIGHT }}
                  >
                    <span 
                      className="text-xs font-medium text-slate-700 dark:text-slate-300 truncate w-full"
                      title={rowItem}
                    >
                      {getShortName(rowItem)}
                    </span>
                    <div
                      onMouseDown={handleColumnResizeStart}
                      className={cn(
                        "absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-indigo-400 transition-colors z-40",
                        resizingMode === 'column' ? "bg-indigo-500" : "bg-transparent"
                      )}
                    />
                  </div>

                  {/* Cells */}
                  {fullMatrixData.items.map((colItem) => {
                    const cellData = fullMatrixData.cells.get(rowItem)?.get(colItem);
                    if (!cellData) return <div key={`${rowItem}-${colItem}`} />;
                    
                    return (
                      <MatrixCell
                        key={`${rowItem}-${colItem}`}
                        cellData={cellData}
                        rowName={rowItem}
                        colName={colItem}
                        isRowHovered={hoveredCell?.row === rowItem}
                        isColHovered={hoveredCell?.col === colItem}
                        onHover={handleCellHover}
                        onLeave={() => setHoveredCell(null)}
                        onClick={handleCellClick}
                        subMode={matrixSubMode}
                        style={{ height: CELL_HEIGHT, width: CELL_WIDTH }}
                      />
                    );
                  })}
                </React.Fragment>
              ))}
            </div>
          </div>
        )}

        {/* Legend */}
        <div className="p-3 border-t border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-900/50 flex items-center gap-6 text-xs text-slate-500">
          <div className="flex items-center gap-2">
            <div className="p-1 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded shadow-sm">
              <ArrowRight className="h-3 w-3 text-emerald-600 dark:text-emerald-400" strokeWidth={3} />
            </div>
            <span>Writes to</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="p-1 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded shadow-sm">
              <ArrowLeft className="h-3 w-3 text-blue-600 dark:text-blue-400" strokeWidth={3} />
            </div>
            <span>Reads from</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="p-1 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded shadow-sm">
              <Minus className="h-3 w-3 text-slate-300" />
            </div>
            <span>Self</span>
          </div>
        </div>
      </div>
    </GraphTooltipProvider>
  );
}
