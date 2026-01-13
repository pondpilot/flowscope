import React, { useMemo, useState, useCallback, useRef, useEffect, memo, useId, type JSX } from 'react';
import { useDebounce } from '../hooks/useDebounce';
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
  Minimize2,
  BarChart2,
  ScanLine
} from 'lucide-react';
import { useLineage } from '../store';
import type { MatrixSubMode } from '../types';
import { PANEL_STYLES } from '../constants';
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
  extractAllColumnNames,
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
    // Normalize separators to forward slash and take the last segment
    const normalized = name.replace(/\\/g, '/');
    const lastSlash = normalized.lastIndexOf('/');
    const fileName = lastSlash !== -1 ? normalized.slice(lastSlash + 1) : normalized;
    return fileName.slice(0, -4);
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
  
  // Simple heuristic: few iterations of barycenter is sufficient for good clustering
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
  filterMode: 'rows' | 'columns' | 'fields'; // Added
  filterText: string; // Added
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

  const baseClass = "flex items-center justify-center transition-all duration-200 border-r border-b border-slate-100 dark:border-slate-700";
  
  const bgClass = useMemo(() => {
    if (isRowHovered && isColHovered) return "bg-indigo-100 dark:bg-indigo-900/30 ring-1 ring-inset ring-indigo-500 z-10";
    if (isRowHovered) return "bg-slate-50 dark:bg-slate-800/50";
    if (isColHovered) return "bg-slate-50 dark:bg-slate-800/50";
    
    // Heatmap Logic
    if (heatmapMode && hasDependency) {
      return "bg-background"; // Base fallback
    }

    return "bg-background";
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

    // Field Tracing Specific Tooltip
    if (filterMode === 'fields' && filterText && subMode === 'tables') {
      const details = cellData.details as TableDependencyWithDetails | undefined;
      const lowerSearch = filterText.toLowerCase();
      
      if (details && details.columns) {
        // Find matching columns
        const matchedCols = details.columns.filter(c => 
          c.source.toLowerCase().includes(lowerSearch) || 
          c.target.toLowerCase().includes(lowerSearch)
        );

        if (matchedCols.length > 0) {
          return (
            <div className="space-y-3 min-w-[200px]">
              <div className="flex items-center justify-between border-b border-white/10 pb-2">
                <div className="flex items-center gap-2 font-medium text-sm">
                  <span className="text-slate-400">{isWrite ? displayRowName : displayColName}</span>
                  <ArrowRight className="h-3.5 w-3.5 text-slate-500" />
                  <span className="text-white">{isWrite ? displayColName : displayRowName}</span>
                </div>
              </div>
              
              <div className="space-y-2">
                {matchedCols.slice(0, MAX_TOOLTIP_COLUMN_MATCHES).map((col, i) => (
                  <div key={i} className="text-xs bg-white/5 p-2 rounded border border-white/5">
                    <div className="flex items-center gap-2 mb-1">
                      <span className={cn("font-mono text-slate-300", col.source.toLowerCase().includes(lowerSearch) && "text-amber-400 font-bold")}>
                        {col.source}
                      </span>
                      <ArrowRight className="h-3 w-3 text-slate-600" />
                      <span className={cn("font-mono text-slate-300", col.target.toLowerCase().includes(lowerSearch) && "text-amber-400 font-bold")}>
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
  }, [cellData, rowName, colName, subMode, isSelf, isNone, filterMode, filterText]);

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
    prev.heatmapMode === next.heatmapMode &&
    prev.filterMode === next.filterMode && // Added
    prev.filterText === next.filterText // Added
  );
});

// ============================================================================
// Main Component
// ============================================================================

/** Exported state interface for controlled mode */
export interface MatrixViewControlledState {
  filterText: string;
  filterMode: 'rows' | 'columns' | 'fields';
  heatmapMode: boolean;
  xRayMode: boolean;
  xRayFilterMode: 'dim' | 'hide';
  clusterMode: boolean;
  complexityMode: boolean;
  showLegend: boolean;
  focusedNode: string | null;
  firstColumnWidth: number;
  headerHeight: number;
}

interface MatrixViewProps {
  className?: string;
  /** Controlled state - when provided, component uses this state instead of internal state */
  controlledState?: Partial<MatrixViewControlledState>;
  /** Callback when state changes - called with the updated state slice */
  onStateChange?: (state: Partial<MatrixViewControlledState>) => void;
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

const MAX_AUTOCOMPLETE_SUGGESTIONS = 8;
const MAX_TOOLTIP_COLUMN_MATCHES = 5;
const SEARCH_DEBOUNCE_DELAY = 200;

// ============================================================================
// Helper Functions
// ============================================================================

type FilterMode = 'rows' | 'columns' | 'fields';
type XRayFilterMode = 'dim' | 'hide';

interface FilterItemsParams {
  items: string[];
  filterMode: FilterMode;
  filterText: string;
  matchingFieldNodes: Set<string> | null;
  xRayMode: boolean;
  xRayFilterMode: XRayFilterMode;
  activeXRaySet: Set<string> | null;
  targetMode: 'rows' | 'columns';
}

function filterItems({
  items,
  filterMode,
  filterText,
  matchingFieldNodes,
  xRayMode,
  xRayFilterMode,
  activeXRaySet,
  targetMode,
}: FilterItemsParams): string[] {
  // 1. Field Trace Filtering
  if (filterMode === 'fields' && matchingFieldNodes) {
    return items.filter(item => matchingFieldNodes.has(item));
  }
  // 2. X-Ray Filtering
  if (xRayMode && xRayFilterMode === 'hide' && activeXRaySet) {
    return items.filter(item => activeXRaySet.has(item));
  }
  // 3. Text Search (only for matching target mode)
  if (filterMode === targetMode && filterText) {
    const lower = filterText.toLowerCase();
    return items.filter(item => item.toLowerCase().includes(lower));
  }

  return items;
}

function useImmediateControlledMatrixState<K extends keyof MatrixViewControlledState>(
  key: K,
  controlledState: Partial<MatrixViewControlledState> | undefined,
  onStateChange: ((state: Partial<MatrixViewControlledState>) => void) | undefined,
  defaultValue: MatrixViewControlledState[K]
): [MatrixViewControlledState[K], (value: React.SetStateAction<MatrixViewControlledState[K]>) => void] {
  const controlledValue = controlledState?.[key] as MatrixViewControlledState[K] | undefined;
  const [value, setValue] = useState<MatrixViewControlledState[K]>(() =>
    controlledValue !== undefined ? controlledValue : defaultValue
  );

  // Track the previous controlled value to detect external changes
  const prevControlledValueRef = useRef(controlledValue);

  useEffect(() => {
    // Only sync if the controlled value actually changed externally,
    // not when local state changes. This prevents the input from being
    // reset to stale controlled values during typing.
    if (controlledValue !== prevControlledValueRef.current) {
      prevControlledValueRef.current = controlledValue;
      if (controlledValue !== undefined) {
        setValue(controlledValue);
      }
    }
  }, [controlledValue]);

  const setValueImmediate = useCallback((
    valueOrUpdater: React.SetStateAction<MatrixViewControlledState[K]>
  ) => {
    setValue((prev) => {
      const nextValue = typeof valueOrUpdater === 'function'
        ? (valueOrUpdater as (prevState: MatrixViewControlledState[K]) => MatrixViewControlledState[K])(prev)
        : valueOrUpdater;

      onStateChange?.({ [key]: nextValue });
      return nextValue;
    });
  }, [key, onStateChange]);

  return [value, setValueImmediate];
}

export function MatrixView({ className = '', controlledState, onStateChange }: MatrixViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, matrixSubMode } = state;
  const { setMatrixSubMode, highlightSpan, requestNavigation } = actions;

  // Immediate local state (mirrors controlled values when provided)
  const [filterText, setFilterText] = useImmediateControlledMatrixState('filterText', controlledState, onStateChange, '');
  const [filterMode, setFilterMode] = useImmediateControlledMatrixState('filterMode', controlledState, onStateChange, 'rows');
  const [heatmapMode, setHeatmapMode] = useImmediateControlledMatrixState('heatmapMode', controlledState, onStateChange, false);
  const [xRayMode, setXRayMode] = useImmediateControlledMatrixState('xRayMode', controlledState, onStateChange, false);
  const [xRayFilterMode, setXRayFilterMode] = useImmediateControlledMatrixState('xRayFilterMode', controlledState, onStateChange, 'dim');
  const [clusterMode, setClusterMode] = useImmediateControlledMatrixState('clusterMode', controlledState, onStateChange, false);
  const [complexityMode, setComplexityMode] = useImmediateControlledMatrixState('complexityMode', controlledState, onStateChange, false);
  const [showLegend, setShowLegend] = useImmediateControlledMatrixState('showLegend', controlledState, onStateChange, true);
  const [focusedNode, setFocusedNode] = useImmediateControlledMatrixState('focusedNode', controlledState, onStateChange, null);
  const [firstColumnWidth, setFirstColumnWidth] = useImmediateControlledMatrixState('firstColumnWidth', controlledState, onStateChange, DEFAULT_FIRST_COLUMN_WIDTH);
  const [headerHeight, setHeaderHeight] = useImmediateControlledMatrixState('headerHeight', controlledState, onStateChange, DEFAULT_HEADER_HEIGHT);

  const debouncedFilterText = useDebounce(filterText, SEARCH_DEBOUNCE_DELAY);
  const [hoveredCell, setHoveredCell] = useState<{ row: string; col: string } | null>(null);
  const [resizingMode, setResizingMode] = useState<'none' | 'column' | 'header'>('none');
  
  // Autocomplete
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [activeSuggestionIndex, setActiveSuggestionIndex] = useState(0);
  const searchContainerRef = useRef<HTMLDivElement>(null);
  const suggestionsListId = useId();
  const activeOptionId = useId();
  
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
  const { tableDeps, scriptDepsData, allColumnNames } = useMemo(() => ({
    tableDeps: result ? extractTableDependenciesWithDetails(result.statements) : [],
    scriptDepsData: result ? extractScriptDependencies(result.statements) : { dependencies: [], allScripts: [] },
    allColumnNames: result ? extractAllColumnNames(result.statements) : [],
  }), [result]);

  const fullMatrixData = useMemo(() =>
    matrixSubMode === 'tables'
      ? buildTableMatrix(tableDeps)
      : buildScriptMatrix(scriptDepsData.dependencies, scriptDepsData.allScripts),
  [matrixSubMode, tableDeps, scriptDepsData]);

  // Complexity Metrics
  const complexityStats = useMemo(() => {
    const rowCounts = new Map<string, number>();
    const colCounts = new Map<string, number>();
    let maxRow = 0;
    let maxCol = 0;

    // Initialize
    for (const item of fullMatrixData.items) {
      rowCounts.set(item, 0);
      colCounts.set(item, 0);
    }

    // Iterate 'write' relationships
    for (const [rowId, rowCells] of fullMatrixData.cells) {
      for (const [colId, cell] of rowCells) {
        if (cell.type === 'write') {
           // Row is writing (Fan-Out)
           const r = (rowCounts.get(rowId) || 0) + 1;
           rowCounts.set(rowId, r);
           maxRow = Math.max(maxRow, r);

           // Col is being written to (Fan-In)
           const c = (colCounts.get(colId) || 0) + 1;
           colCounts.set(colId, c);
           maxCol = Math.max(maxCol, c);
        }
      }
    }

    return { rowCounts, colCounts, maxRow, maxCol };
  }, [fullMatrixData]);

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


  // Field Tracing Logic (uses debounced text for expensive computation)
  const matchingFieldNodes = useMemo(() => {
    if (!debouncedFilterText || filterMode !== 'fields') return null;
    const lower = debouncedFilterText.toLowerCase();
    const matchedNodes = new Set<string>();

    for (const [rowId, rowCells] of fullMatrixData.cells) {
      for (const [colId, cell] of rowCells) {
        if (cell.type === 'write' || cell.type === 'read') {
           // Check Table Dependencies
           if (matrixSubMode === 'tables') {
             const details = cell.details as TableDependencyWithDetails;
             if (details && details.columns) {
               const hasMatch = details.columns.some(c =>
                 c.source.toLowerCase().includes(lower) ||
                 c.target.toLowerCase().includes(lower)
               );
               if (hasMatch) {
                 matchedNodes.add(rowId);
                 matchedNodes.add(colId);
               }
             }
           }
           // Check Script Dependencies (if we had column data, which we don't usually, but scripts touch tables)
           // For now, field search is primary for Table mode.
        }
      }
    }
    return matchedNodes;
  }, [fullMatrixData, debouncedFilterText, filterMode, matrixSubMode]);

  // Filtering (uses debounced text)
  const filteredRowItems = useMemo(() => {
    return filterItems({
      items: sortedItems,
      filterMode,
      filterText: debouncedFilterText,
      matchingFieldNodes,
      xRayMode,
      xRayFilterMode,
      activeXRaySet,
      targetMode: 'rows',
    });
  }, [sortedItems, debouncedFilterText, filterMode, matchingFieldNodes, xRayMode, xRayFilterMode, activeXRaySet]);

  const filteredColumnItems = useMemo(() => {
    return filterItems({
      items: sortedItems,
      filterMode,
      filterText: debouncedFilterText,
      matchingFieldNodes,
      xRayMode,
      xRayFilterMode,
      activeXRaySet,
      targetMode: 'columns',
    });
  }, [sortedItems, debouncedFilterText, filterMode, matchingFieldNodes, xRayMode, xRayFilterMode, activeXRaySet]);

  const handleCellHover = useCallback((row: string, col: string) => {
    setHoveredCell({ row, col });
  }, []);

  // Autocomplete Logic
  const suggestions = useMemo(() => {
    if (!filterText) return [];
    const lower = filterText.toLowerCase();
    let source: string[] = [];
    
    if (filterMode === 'fields') {
       source = allColumnNames;
    } else {
       source = sortedItems.map(getShortName);
    }
    
    const matches = Array.from(new Set(source.filter(s => s.toLowerCase().includes(lower)))).slice(0, MAX_AUTOCOMPLETE_SUGGESTIONS);
    return matches;
  }, [filterText, filterMode, allColumnNames, sortedItems]);
  const suggestionCount = suggestions.length;

  const handleSearchKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActiveSuggestionIndex(prev => Math.min(prev + 1, suggestions.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActiveSuggestionIndex(prev => Math.max(prev - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (suggestions.length > 0) {
        const clampedIndex = Math.max(0, Math.min(activeSuggestionIndex, suggestions.length - 1));
        const selectedSuggestion = suggestions[clampedIndex];
        if (selectedSuggestion) {
          if (clampedIndex !== activeSuggestionIndex) {
            setActiveSuggestionIndex(clampedIndex);
          }
          setFilterText(selectedSuggestion);
          setShowSuggestions(false);
        }
      }
    } else if (e.key === 'Escape') {
      setShowSuggestions(false);
    }
  };

  useEffect(() => {
    setActiveSuggestionIndex(prev => {
      if (suggestionCount === 0) {
        return 0;
      }
      const clampedIndex = Math.min(prev, suggestionCount - 1);
      return clampedIndex < 0 ? 0 : clampedIndex;
    });
  }, [suggestionCount]);

  useEffect(() => {
    setActiveSuggestionIndex(0);
  }, [filterText]);

  // Close suggestions when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (searchContainerRef.current && !searchContainerRef.current.contains(event.target as Node)) {
        setShowSuggestions(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
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
      <div className={cn("flex flex-col h-full bg-background", className)}>
        {/* Toolbar */}
        <div className="flex items-center justify-between p-4 border-b border-slate-200 dark:border-slate-800 bg-background z-40 gap-4">
          <div className={`${PANEL_STYLES.selector} shrink-0`}>
            <button
              onClick={() => { setMatrixSubMode('scripts'); setFocusedNode(null); }}
              className={cn(
                "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-full px-3 h-7 text-sm font-medium transition-all duration-200",
                matrixSubMode === 'scripts'
                  ? "bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100"
                  : "text-slate-500 hover:text-slate-700 dark:hover:text-slate-300"
              )}
            >
              <FileCode className="size-4" />
              <span>Scripts</span>
            </button>
            <button
              onClick={() => { setMatrixSubMode('tables'); setFocusedNode(null); }}
              className={cn(
                "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-full px-3 h-7 text-sm font-medium transition-all duration-200",
                matrixSubMode === 'tables'
                  ? "bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100"
                  : "text-slate-500 hover:text-slate-700 dark:hover:text-slate-300"
              )}
            >
              <Table2 className="size-4" />
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

            {/* Complexity Toggle */}
            <GraphTooltip delayDuration={300}>
              <GraphTooltipTrigger asChild>
                <button
                  onClick={() => setComplexityMode(!complexityMode)}
                  aria-label="Toggle Complexity Margins"
                  aria-pressed={complexityMode}
                  className={cn(
                    "p-1.5 rounded-md transition-all",
                    complexityMode
                      ? "bg-teal-100 text-teal-600 dark:bg-teal-900/30 dark:text-teal-400 ring-1 ring-teal-500"
                      : "text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800"
                  )}
                >
                  <BarChart2 className="h-4 w-4" />
                </button>
              </GraphTooltipTrigger>
              <GraphTooltipPortal>
                <GraphTooltipContent side="bottom" className="text-xs">
                  <div className="font-semibold text-slate-100">Complexity Margins</div>
                  <div className="text-slate-400">Visual bars for Fan-In/Fan-Out density.</div>
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

          <div
            className="relative group ml-auto flex items-center rounded-full border border-slate-200/60 dark:border-slate-700/60 bg-white/95 dark:bg-slate-900/95 h-9 shadow-sm backdrop-blur-sm"
            ref={searchContainerRef}
          >
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-slate-400 pointer-events-none z-10" strokeWidth={1.5} />
            <input
              type="text"
              placeholder={`Filter ${filterMode}...`}
              value={filterText}
              onChange={(e) => {
                setFilterText(e.target.value);
                setShowSuggestions(true);
              }}
              onFocus={() => setShowSuggestions(true)}
              onKeyDown={handleSearchKeyDown}
              role="combobox"
              aria-expanded={showSuggestions && (suggestions.length > 0 || filterText.length > 0)}
              aria-haspopup="listbox"
              aria-controls={suggestionsListId}
              aria-activedescendant={showSuggestions && suggestions.length > 0 ? `${activeOptionId}-${activeSuggestionIndex}` : undefined}
              aria-autocomplete="list"
              className="h-7 pl-8 pr-24 text-sm bg-transparent border-0 rounded-full focus:outline-none focus:ring-0 w-64 placeholder:text-slate-400"
            />

            {/* Autocomplete Dropdown */}
            {showSuggestions && (suggestions.length > 0 || filterText.length > 0) && (
              <div
                id={suggestionsListId}
                role="listbox"
                aria-label="Search suggestions"
                className="absolute top-full left-0 right-0 mt-1 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-lg shadow-lg z-[100] max-h-60 overflow-auto py-1"
              >
                {suggestions.length > 0 ? (
                  suggestions.map((suggestion, index) => (
                    <button
                      key={suggestion}
                      id={`${activeOptionId}-${index}`}
                      role="option"
                      aria-selected={index === activeSuggestionIndex}
                      className={cn(
                        "w-full text-left px-3 py-2 text-sm transition-colors flex items-center gap-2",
                        index === activeSuggestionIndex
                          ? "bg-indigo-50 dark:bg-indigo-900/20 text-indigo-700 dark:text-indigo-300"
                          : "text-slate-700 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-800"
                      )}
                      onClick={() => {
                        setFilterText(suggestion);
                        setShowSuggestions(false);
                      }}
                      onMouseEnter={() => setActiveSuggestionIndex(index)}
                    >
                      {filterMode === 'fields' ? (
                        <ScanLine className="h-3 w-3 opacity-50" />
                      ) : (
                        matrixSubMode === 'scripts' ? <FileCode className="h-3 w-3 opacity-50" /> : <Table2 className="h-3 w-3 opacity-50" />
                      )}
                      <span className="truncate">{suggestion}</span>
                    </button>
                  ))
                ) : filterText.length > 0 ? (
                  <div className="px-3 py-2 text-sm text-slate-500 dark:text-slate-400 italic">
                    No matches found
                  </div>
                ) : null}
              </div>
            )}

            <div className="absolute right-1 top-1/2 -translate-y-1/2 flex items-center bg-slate-100 dark:bg-slate-800 rounded-full p-0.5 gap-0.5">
              <button
                onClick={() => {
                  if (filterMode !== 'rows') {
                    setFilterMode('rows');
                    setFilterText('');
                  }
                }}
                title="Filter rows"
                aria-label="Filter rows"
                aria-pressed={filterMode === 'rows'}
                className={cn(
                  "p-1 rounded-full transition-all",
                  filterMode === 'rows'
                    ? "bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-sm"
                    : "text-slate-400 hover:text-slate-600 dark:hover:text-slate-300"
                )}
              >
                <Rows3 className="h-3.5 w-3.5" />
              </button>
              <button
                onClick={() => {
                  if (filterMode !== 'columns') {
                    setFilterMode('columns');
                    setFilterText('');
                  }
                }}
                title="Filter columns"
                aria-label="Filter columns"
                aria-pressed={filterMode === 'columns'}
                className={cn(
                  "p-1 rounded-full transition-all",
                  filterMode === 'columns'
                    ? "bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-sm"
                    : "text-slate-400 hover:text-slate-600 dark:hover:text-slate-300"
                )}
              >
                <Columns3 className="h-3.5 w-3.5" />
              </button>
              <button
                onClick={() => {
                  if (filterMode !== 'fields') {
                    setFilterMode('fields');
                    setFilterText('');
                  }
                }}
                title="Trace Column (Fields)"
                aria-label="Trace column fields"
                aria-pressed={filterMode === 'fields'}
                className={cn(
                  "p-1 rounded-full transition-all",
                  filterMode === 'fields'
                    ? "bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-sm"
                    : "text-slate-400 hover:text-slate-600 dark:hover:text-slate-300"
                )}
              >
                <ScanLine className="h-3.5 w-3.5" />
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
                className="sticky top-0 left-0 z-30 bg-background border-b border-r border-slate-200 dark:border-slate-600 shadow-[2px_2px_10px_rgba(0,0,0,0.05)]"
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

                // Complexity
                const fanIn = complexityStats.colCounts.get(item) || 0;
                const complexityPct = complexityStats.maxCol ? (fanIn / complexityStats.maxCol) * 100 : 0;

                return (
                  <div
                    key={`col-${item}`}
                    className={cn(
                      "sticky top-0 z-20 bg-background border-b border-r border-slate-200 dark:border-slate-600 shadow-sm group cursor-pointer transition-colors duration-200 relative",
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

                // Complexity
                const fanOut = complexityStats.rowCounts.get(rowItem) || 0;
                const complexityPct = complexityStats.maxRow ? (fanOut / complexityStats.maxRow) * 100 : 0;

                return (
                  <React.Fragment key={`row-${rowItem}`}>
                    {/* Row Header */}
                    <div
                      className={cn(
                        "sticky left-0 z-20 bg-background border-b border-r border-slate-200 dark:border-slate-600 px-3 flex items-center shadow-[2px_0_5px_rgba(0,0,0,0.02)] cursor-pointer transition-colors duration-200 relative",
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
                      {/* Complexity Bar (Horizontal, growing from left) */}
                      {complexityMode && complexityPct > 0 && (
                        <div 
                          className="absolute top-0 bottom-0 left-0 bg-blue-500/10 dark:bg-blue-400/10 transition-all z-0"
                          style={{ width: `${complexityPct}%` }}
                        />
                      )}

                      <span 
                        className={cn(
                          "text-xs font-medium text-slate-700 dark:text-slate-300 truncate w-full relative z-10",
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
            
            <div className="flex items-center gap-4 text-[10px] text-slate-400 uppercase tracking-wider font-semibold">
             {xRayMode && <span className="text-purple-500 animate-pulse">X-Ray Active</span>}
             {heatmapMode && <span className="text-orange-500">Heatmap Active</span>}
             {clusterMode && <span className="text-blue-500">Sorted by Clusters</span>}
             {complexityMode && <span className="text-teal-500">Complexity Margins</span>}
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
