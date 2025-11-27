import React, { useMemo, useState, useCallback, useRef, useEffect, memo } from 'react';
import {
  Table2,
  FileCode,
  Search,
  ArrowRight,
  ArrowLeft,
  Minus,
  Database,
  Filter,
  Zap,
  Activity,
  Shuffle,
  Rows3,
  Columns3,
  Info,
  Maximize2,
  Minimize2
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

function getShortName(name: string): string {
  if (name.endsWith('.sql')) {
    return name.slice(0, -4);
  }
  const lastDot = name.lastIndexOf('.');
  if (lastDot !== -1) {
    return name.slice(lastDot + 1);
  }
  return name;
}

// ============================================================================
// Algorithms (Clustering & Transitive)
// ============================================================================

// Barycenter Heuristic for Clustering
function clusterItems(
  items: string[],
  cells: Map<string, Map<string, MatrixCellData>>
): string[] {
  let currentOrder = [...items];

  for (let iter = 0; iter < CLUSTERING_ITERATIONS; iter++) {
    const positions = new Map(currentOrder.map((id, i) => [id, i]));
    const newOrder = [...currentOrder].sort((a, b) => {
      const getBarycenter = (node: string) => {
        let sum = 0;
        let count = 0;
        const row = cells.get(node);
        if (row) {
          for (const [target, cell] of row.entries()) {
            if (cell.type !== 'none' && cell.type !== 'self') {
               sum += positions.get(target) || 0;
               count++;
            }
          }
        }
        return count === 0 ? positions.get(node)! : sum / count;
      };
      return getBarycenter(a) - getBarycenter(b);
    });
    currentOrder = newOrder;
  }
  
  return currentOrder;
}

// Split Transitive Dependency Traversal (Ancestors vs Descendants)
interface TransitiveSet {
  ancestors: Set<string>;
  descendants: Set<string>;
}

function getTransitiveFlow(
  startNode: string,
  cells: Map<string, Map<string, MatrixCellData>>,
  items: string[]
): TransitiveSet {
  const ancestors = new Set<string>();
  const descendants = new Set<string>();
  
  // Find Descendants (Downstream): BFS forward writes
  // Matrix logic: row writes to col
  const dQueue = [startNode];
  while (dQueue.length > 0) {
    const current = dQueue.shift()!;
    const row = cells.get(current);
    if (row) {
      for (const [target, cell] of row.entries()) {
        if (cell.type === 'write' && !descendants.has(target) && target !== startNode) {
          descendants.add(target);
          dQueue.push(target);
        }
      }
    }
  }

  // Find Ancestors (Upstream): BFS backward writes (who writes to current?)
  const aQueue = [startNode];
  // Pre-calculate reverse graph could optimize this, but N is small
  while (aQueue.length > 0) {
    const current = aQueue.shift()!;
    for (const item of items) {
      const cell = cells.get(item)?.get(current);
      if (cell && cell.type === 'write' && !ancestors.has(item) && item !== startNode) {
        ancestors.add(item);
        aQueue.push(item);
      }
    }
  }

  return { ancestors, descendants };
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
  isDimmed: boolean;
  intensity: number; // 0 to 1
  heatmapMode: boolean;
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
  isDimmed,
  intensity,
  heatmapMode,
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
    
    // Heatmap Logic
    if (heatmapMode && hasDependency) {
      return "bg-white dark:bg-slate-950"; // Base fallback
    }

    return "bg-white dark:bg-slate-950";
  }, [isRowHovered, isColHovered, heatmapMode, hasDependency]);

  // Dynamic style for heatmap
  const heatmapStyle = useMemo(() => {
    if (!heatmapMode || !hasDependency) return {};
    const alpha = HEATMAP_MIN_ALPHA + (intensity * HEATMAP_ALPHA_RANGE);
    const color = cellData.type === 'write' ? `rgba(16, 185, 129, ${alpha})` : `rgba(37, 99, 235, ${alpha})`;
    return { backgroundColor: color };
  }, [heatmapMode, hasDependency, intensity, cellData.type]);

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
          className={cn(
            baseClass, 
            bgClass, 
            hasDependency && "cursor-pointer hover:bg-slate-100 dark:hover:bg-slate-800",
            isDimmed && "opacity-20 grayscale"
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
    prev.colName === next.colName &&
    prev.isDimmed === next.isDimmed &&
    prev.intensity === next.intensity &&
    prev.heatmapMode === next.heatmapMode
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

const HEATMAP_MIN_ALPHA = 0.15;
const HEATMAP_ALPHA_RANGE = 0.6;
const CLUSTERING_ITERATIONS = 2;

export function MatrixView({ className = '' }: MatrixViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, matrixSubMode } = state;
  const { setMatrixSubMode, highlightSpan, requestNavigation } = actions;

  const [filterText, setFilterText] = useState('');
  const [filterMode, setFilterMode] = useState<'rows' | 'columns'>('rows');
  const [hoveredCell, setHoveredCell] = useState<{ row: string; col: string } | null>(null);

  // Modes
  const [heatmapMode, setHeatmapMode] = useState(false);
  const [xRayMode, setXRayMode] = useState(false);
  const [xRayFilterMode, setXRayFilterMode] = useState<'dim' | 'hide'>('dim');
  const [clusterMode, setClusterMode] = useState(false);
  const [showLegend, setShowLegend] = useState(true);

  // X-Ray Focus
  const [focusedNode, setFocusedNode] = useState<string | null>(null);
  
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

  // Clustering Logic
  const sortedItems = useMemo(() => {
    if (!clusterMode) return fullMatrixData.items;
    return clusterItems(fullMatrixData.items, fullMatrixData.cells);
  }, [fullMatrixData, clusterMode]);

  // Transitive Flow for X-Ray
  const transitiveFlow = useMemo(() => {
    if (!xRayMode || !focusedNode) return null;
    return getTransitiveFlow(focusedNode, fullMatrixData.cells, sortedItems);
  }, [xRayMode, focusedNode, fullMatrixData, sortedItems]);

  const activeXRaySet = useMemo(() => {
    if (!xRayMode || !focusedNode || !transitiveFlow) return null;
    return new Set([focusedNode, ...transitiveFlow.ancestors, ...transitiveFlow.descendants]);
  }, [xRayMode, focusedNode, transitiveFlow]);

  // Max Dependency Strength for Heatmap
  const maxIntensity = useMemo(() => {
    let max = 0;
    for (const row of fullMatrixData.cells.values()) {
      for (const cell of row.values()) {
        if (cell.type !== 'none' && cell.type !== 'self') {
          let count = 0;
          if (matrixSubMode === 'tables') {
             count = (cell.details as TableDependencyWithDetails)?.columnCount || 0;
          } else {
             count = (cell.details as ScriptDependency)?.sharedTables.length || 0;
          }
          if (count > max) max = count;
        }
      }
    }
    return max || 1;
  }, [fullMatrixData, matrixSubMode]);


  // Filtering
  const filteredRowItems = useMemo(() => {
    let items = sortedItems;
    // 1. X-Ray Filtering
    if (xRayMode && xRayFilterMode === 'hide' && activeXRaySet) {
       items = items.filter(item => activeXRaySet.has(item));
    }
    // 2. Search
    if (!filterText || filterMode !== 'rows') return items;
    const lower = filterText.toLowerCase();
    return items.filter(item => item.toLowerCase().includes(lower));
  }, [sortedItems, filterText, filterMode, xRayMode, xRayFilterMode, activeXRaySet]);

  const filteredColumnItems = useMemo(() => {
    let items = sortedItems;
    // 1. X-Ray Filtering
    if (xRayMode && xRayFilterMode === 'hide' && activeXRaySet) {
       items = items.filter(item => activeXRaySet.has(item));
    }
    // 2. Search
    if (!filterText || filterMode !== 'columns') return items;
    const lower = filterText.toLowerCase();
    return items.filter(item => item.toLowerCase().includes(lower));
  }, [sortedItems, filterText, filterMode, xRayMode, xRayFilterMode, activeXRaySet]);

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
  
  // Toggle Focus
  const toggleFocus = (node: string) => {
    if (!xRayMode) return;
    setFocusedNode(prev => prev === node ? null : node);
  };

  if (!result) {
    return (
      <div className={cn("flex flex-col items-center justify-center h-full text-slate-400 gap-4", className)}>
        <Database className="h-12 w-12 opacity-20" />
        <p>No analysis result available</p>
      </div>
    );
  }

  const isEmpty = filteredRowItems.length === 0 || filteredColumnItems.length === 0;

  return (
    <GraphTooltipProvider>
      <div className={cn("flex flex-col h-full bg-white dark:bg-slate-950", className)}>
        {/* Toolbar */}
        <div className="flex items-center justify-between p-4 border-b border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-950 z-20 gap-4">
          <div className="flex items-center gap-2 bg-slate-100 dark:bg-slate-900 p-1 rounded-lg shrink-0">
            <button
              onClick={() => { setMatrixSubMode('scripts'); setFocusedNode(null); }}
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
              onClick={() => { setMatrixSubMode('tables'); setFocusedNode(null); }}
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
          
          <div className="flex items-center gap-2 border-l border-slate-200 dark:border-slate-800 pl-4">
             {/* X-Ray Toggle */}
             <GraphTooltip delayDuration={300}>
              <GraphTooltipTrigger asChild>
                <button
                  onClick={() => {
                     setXRayMode(!xRayMode);
                     setFocusedNode(null);
                  }}
                  aria-label="Toggle X-Ray Mode"
                  aria-pressed={xRayMode}
                  className={cn(
                    "p-1.5 rounded-md transition-all flex items-center gap-1",
                    xRayMode
                      ? "bg-purple-100 text-purple-600 dark:bg-purple-900/30 dark:text-purple-400 ring-1 ring-purple-500"
                      : "text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800"
                  )}
                >
                  <Zap className="h-4 w-4" />
                  {xRayMode && (
                    <span className="text-[10px] font-semibold">X-RAY</span>
                  )}
                </button>
              </GraphTooltipTrigger>
              <GraphTooltipPortal>
                <GraphTooltipContent side="bottom" className="text-xs">
                  <div className="font-semibold text-slate-100">Impact X-Ray Mode</div>
                  <div className="text-slate-400">Click a row/col header to highlight lineage flow.</div>
                  <div className="mt-1 flex flex-col gap-1 text-[10px]">
                    <div className="flex items-center gap-1.5"><div className="w-2 h-2 bg-blue-500 rounded-full"/>Ancestors (Upstream)</div>
                    <div className="flex items-center gap-1.5"><div className="w-2 h-2 bg-emerald-500 rounded-full"/>Descendants (Downstream)</div>
                  </div>
                </GraphTooltipContent>
              </GraphTooltipPortal>
            </GraphTooltip>

            {/* X-Ray View Mode Toggle (Dim/Hide) */}
            {xRayMode && (
              <GraphTooltip delayDuration={300}>
                <GraphTooltipTrigger asChild>
                  <button
                    onClick={() => setXRayFilterMode(prev => prev === 'dim' ? 'hide' : 'dim')}
                    aria-label={xRayFilterMode === 'hide' ? 'Switch to dim mode' : 'Switch to hide mode'}
                    className={cn(
                      "p-1.5 rounded-md transition-all",
                      xRayFilterMode === 'hide'
                        ? "bg-indigo-100 text-indigo-600 dark:bg-indigo-900/30 dark:text-indigo-400"
                        : "text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800"
                    )}
                  >
                    {xRayFilterMode === 'hide' ? <Minimize2 className="h-4 w-4" /> : <Maximize2 className="h-4 w-4" />}
                  </button>
                </GraphTooltipTrigger>
                <GraphTooltipPortal>
                   <GraphTooltipContent side="bottom" className="text-xs">
                    <div className="font-semibold text-slate-100">X-Ray Visibility</div>
                    <div className="text-slate-400">
                      {xRayFilterMode === 'hide' 
                        ? 'Focus View: Hiding unrelated rows/cols' 
                        : 'Context View: Dimming unrelated rows/cols'}
                    </div>
                  </GraphTooltipContent>
                </GraphTooltipPortal>
              </GraphTooltip>
            )}

            {/* Heatmap Toggle */}
             <GraphTooltip delayDuration={300}>
              <GraphTooltipTrigger asChild>
                <button
                  onClick={() => setHeatmapMode(!heatmapMode)}
                  aria-label="Toggle Heatmap Mode"
                  aria-pressed={heatmapMode}
                  className={cn(
                    "p-1.5 rounded-md transition-all",
                    heatmapMode
                      ? "bg-orange-100 text-orange-600 dark:bg-orange-900/30 dark:text-orange-400 ring-1 ring-orange-500"
                      : "text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800"
                  )}
                >
                  <Activity className="h-4 w-4" />
                </button>
              </GraphTooltipTrigger>
              <GraphTooltipPortal>
                <GraphTooltipContent side="bottom" className="text-xs">
                  <div className="font-semibold text-slate-100">Dependency Heatmap</div>
                  <div className="text-slate-400">Color intensity shows connection strength.</div>
                  <div className="text-slate-500 text-[10px] mt-1">Based on column mapping count.</div>
                </GraphTooltipContent>
              </GraphTooltipPortal>
            </GraphTooltip>

             {/* Clustering Toggle */}
             <GraphTooltip delayDuration={300}>
              <GraphTooltipTrigger asChild>
                <button
                  onClick={() => setClusterMode(!clusterMode)}
                  aria-label="Toggle Clustering Mode"
                  aria-pressed={clusterMode}
                  className={cn(
                    "p-1.5 rounded-md transition-all",
                    clusterMode
                      ? "bg-blue-100 text-blue-600 dark:bg-blue-900/30 dark:text-blue-400 ring-1 ring-blue-500"
                      : "text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800"
                  )}
                >
                  <Shuffle className="h-4 w-4" />
                </button>
              </GraphTooltipTrigger>
              <GraphTooltipPortal>
                <GraphTooltipContent side="bottom" className="text-xs">
                  <div className="font-semibold text-slate-100">Smart Clustering</div>
                  <div className="text-slate-400">Reorders matrix to group related items.</div>
                </GraphTooltipContent>
              </GraphTooltipPortal>
            </GraphTooltip>

            {/* Legend Toggle */}
            {!showLegend && (
              <GraphTooltip delayDuration={300}>
                <GraphTooltipTrigger asChild>
                  <button
                    onClick={() => setShowLegend(true)}
                    aria-label="Show Legend"
                    className="p-1.5 rounded-md transition-all text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800"
                  >
                    <Info className="h-4 w-4" />
                  </button>
                </GraphTooltipTrigger>
                <GraphTooltipPortal>
                  <GraphTooltipContent side="bottom" className="text-xs">
                    <div className="font-semibold text-slate-100">Show Legend</div>
                    <div className="text-slate-400">Display the dependency legend.</div>
                  </GraphTooltipContent>
                </GraphTooltipPortal>
              </GraphTooltip>
            )}
          </div>

          <div className="relative group ml-auto flex items-center">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-slate-400 group-focus-within:text-indigo-500 transition-colors z-10" />
            <input
              type="text"
              placeholder={`Filter ${filterMode}...`}
              value={filterText}
              onChange={(e) => setFilterText(e.target.value)}
              className="pl-9 pr-20 py-1.5 text-sm bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-md focus:outline-none focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 transition-all w-64"
            />
            <div className="absolute right-1 top-1/2 -translate-y-1/2 flex items-center bg-slate-100 dark:bg-slate-800 rounded p-0.5 gap-0.5">
              <button
                onClick={() => setFilterMode('rows')}
                title="Filter rows"
                className={cn(
                  "p-1 rounded transition-all",
                  filterMode === 'rows'
                    ? "bg-white dark:bg-slate-700 text-indigo-600 dark:text-indigo-400 shadow-sm"
                    : "text-slate-400 hover:text-slate-600 dark:hover:text-slate-300"
                )}
              >
                <Rows3 className="h-3.5 w-3.5" />
              </button>
              <button
                onClick={() => setFilterMode('columns')}
                title="Filter columns"
                className={cn(
                  "p-1 rounded transition-all",
                  filterMode === 'columns'
                    ? "bg-white dark:bg-slate-700 text-indigo-600 dark:text-indigo-400 shadow-sm"
                    : "text-slate-400 hover:text-slate-600 dark:hover:text-slate-300"
                )}
              >
                <Columns3 className="h-3.5 w-3.5" />
              </button>
            </div>
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
                gridTemplateColumns: `${firstColumnWidth}px repeat(${filteredColumnItems.length}, ${CELL_WIDTH}px)`,
                minWidth: 'min-content'
              }}
              role="grid"
            >
              {/* Top Left Corner */}
              <div 
                className="sticky top-0 left-0 z-30 bg-white dark:bg-slate-950 border-b border-r border-slate-200 dark:border-slate-800 shadow-[2px_2px_10px_rgba(0,0,0,0.05)]"
                style={{ height: headerHeight }}
              >
                 <div
                  onMouseDown={handleColumnResizeStart}
                  className={cn(
                    "absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-indigo-400 transition-colors z-40",
                    resizingMode === 'column' ? "bg-indigo-500" : "bg-transparent"
                  )}
                />
                 <div
                  onMouseDown={handleHeaderResizeStart}
                  className={cn(
                    "absolute bottom-0 left-0 right-0 h-1 cursor-row-resize hover:bg-indigo-400 transition-colors z-40",
                    resizingMode === 'header' ? "bg-indigo-500" : "bg-transparent"
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

                return (
                  <div
                    key={`col-${item}`}
                    className={cn(
                      "sticky top-0 z-20 bg-white dark:bg-slate-950 border-b border-r border-slate-200 dark:border-slate-800 shadow-sm group cursor-pointer transition-colors duration-200",
                      hoveredCell?.col === item && "bg-slate-50 dark:bg-slate-900",
                      
                      // Highlight logic
                      isFocused && "bg-purple-100 dark:bg-purple-900/40",
                      isAncestor && "bg-blue-50 dark:bg-blue-900/20",
                      isDescendant && "bg-emerald-50 dark:bg-emerald-900/20",
                      
                      isDimmed && "opacity-20 grayscale"
                    )}
                    style={{ height: headerHeight }}
                    onClick={() => toggleFocus(item)}
                    title={xRayMode ? "Click to focus X-Ray" : item}
                  >
                    <div className="w-full h-full flex items-end justify-center pb-2">
                      <span 
                        className={cn(
                          "block text-xs font-medium text-slate-600 dark:text-slate-400 hover:text-indigo-600 dark:hover:text-indigo-400 transition-colors whitespace-nowrap overflow-hidden text-ellipsis",
                          isFocused && "text-purple-700 dark:text-purple-300 font-bold",
                          isAncestor && "text-blue-600 dark:text-blue-400 font-semibold",
                          isDescendant && "text-emerald-600 dark:text-emerald-400 font-semibold"
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

                return (
                  <React.Fragment key={`row-${rowItem}`}>
                    {/* Row Header */}
                    <div
                      className={cn(
                        "sticky left-0 z-20 bg-white dark:bg-slate-950 border-b border-r border-slate-200 dark:border-slate-800 px-3 flex items-center shadow-[2px_0_5px_rgba(0,0,0,0.02)] cursor-pointer transition-colors duration-200",
                        hoveredCell?.row === rowItem && "bg-slate-50 dark:bg-slate-900",
                        
                        isFocused && "bg-purple-100 dark:bg-purple-900/40",
                        isAncestor && "bg-blue-50 dark:bg-blue-900/20",
                        isDescendant && "bg-emerald-50 dark:bg-emerald-900/20",
                        
                        isDimmed && "opacity-20 grayscale"
                      )}
                      style={{ height: CELL_HEIGHT }}
                      onClick={() => toggleFocus(rowItem)}
                      title={xRayMode ? "Click to focus X-Ray" : rowItem}
                    >
                      <span 
                        className={cn(
                          "text-xs font-medium text-slate-700 dark:text-slate-300 truncate w-full",
                          isFocused && "text-purple-700 dark:text-purple-300 font-bold",
                          isAncestor && "text-blue-600 dark:text-blue-400 font-semibold",
                          isDescendant && "text-emerald-600 dark:text-emerald-400 font-semibold"
                        )}
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
                        const isRowActive = focusedNode === rowItem || transitiveFlow?.ancestors.has(rowItem) || transitiveFlow?.descendants.has(rowItem);
                        const isColActive = focusedNode === colItem || transitiveFlow?.ancestors.has(colItem) || transitiveFlow?.descendants.has(colItem);
                        
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

        {/* Legend */}
        {showLegend && (
            <div className="p-3 border-t border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-900/50 flex items-center justify-between gap-6 text-xs text-slate-500">
            <div className="flex items-center gap-6">
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
            
            <button
                onClick={() => setShowLegend(false)}
                aria-label="Hide Legend"
                className="text-slate-400 hover:text-slate-600"
            >
                <Info className="h-4 w-4" />
            </button>
            </div>
        )}
      </div>
    </GraphTooltipProvider>
  );
}