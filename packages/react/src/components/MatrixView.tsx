import React, {
  useMemo,
  useState,
  useCallback,
  useRef,
  useEffect,
  useId,
  startTransition,
  type JSX,
} from 'react';
import { useDebounce } from '../hooks/useDebounce';
import { Database, Loader2 } from 'lucide-react';
import { useLineage } from '../store';
import { GraphTooltipProvider } from './ui/graph-tooltip';
import {
  type TableDependencyWithDetails,
  type ScriptDependency,
  type MatrixData,
} from '../utils/matrixUtils';
import { buildMatrixInWorker, cancelPendingMatrixBuilds } from '../utils/matrixWorkerService';
import { clusterItems, filterItems, getTransitiveFlow } from './matrix/algorithms';
import { MatrixGrid } from './matrix/MatrixGrid';
import { MatrixLegend } from './matrix/MatrixLegend';
import { MatrixToolbar } from './matrix/MatrixToolbar';
import type { MatrixViewControlledState, MatrixWorkerPayload } from './matrix/types';
import { useImmediateControlledMatrixState } from './matrix/useImmediateControlledMatrixState';
import { cn, getShortName } from './matrix/utils';

export type { MatrixViewControlledState } from './matrix/types';

const EMPTY_MATRIX: MatrixData = { items: [], cells: new Map() };

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

const MAX_AUTOCOMPLETE_SUGGESTIONS = 8;
const SEARCH_DEBOUNCE_DELAY = 200;
const MATRIX_DEBUG = !!(import.meta as { env?: { DEV?: boolean } }).env?.DEV;
const MAX_MATRIX_ITEMS = 50;

export function MatrixView({
  className = '',
  controlledState,
  onStateChange,
}: MatrixViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, matrixSubMode } = state;
  const { setMatrixSubMode, highlightSpan, requestNavigation } = actions;

  // Immediate local state (mirrors controlled values when provided)
  const [filterText, setFilterText] = useImmediateControlledMatrixState(
    'filterText',
    controlledState,
    onStateChange,
    ''
  );
  const [filterMode, setFilterMode] = useImmediateControlledMatrixState(
    'filterMode',
    controlledState,
    onStateChange,
    'rows'
  );
  const [heatmapMode, setHeatmapMode] = useImmediateControlledMatrixState(
    'heatmapMode',
    controlledState,
    onStateChange,
    false
  );
  const [xRayMode, setXRayMode] = useImmediateControlledMatrixState(
    'xRayMode',
    controlledState,
    onStateChange,
    false
  );
  const [xRayFilterMode, setXRayFilterMode] = useImmediateControlledMatrixState(
    'xRayFilterMode',
    controlledState,
    onStateChange,
    'dim'
  );
  const [clusterMode, setClusterMode] = useImmediateControlledMatrixState(
    'clusterMode',
    controlledState,
    onStateChange,
    false
  );
  const [complexityMode, setComplexityMode] = useImmediateControlledMatrixState(
    'complexityMode',
    controlledState,
    onStateChange,
    false
  );
  const [showLegend, setShowLegend] = useImmediateControlledMatrixState(
    'showLegend',
    controlledState,
    onStateChange,
    true
  );
  const [focusedNode, setFocusedNode] = useImmediateControlledMatrixState(
    'focusedNode',
    controlledState,
    onStateChange,
    null
  );
  const [firstColumnWidth, setFirstColumnWidth] = useImmediateControlledMatrixState(
    'firstColumnWidth',
    controlledState,
    onStateChange,
    DEFAULT_FIRST_COLUMN_WIDTH
  );
  const [headerHeight, setHeaderHeight] = useImmediateControlledMatrixState(
    'headerHeight',
    controlledState,
    onStateChange,
    DEFAULT_HEADER_HEIGHT
  );

  const debouncedFilterText = useDebounce(filterText, SEARCH_DEBOUNCE_DELAY);
  const [hoveredCell, setHoveredCell] = useState<{ row: string; col: string } | null>(null);
  const [resizingMode, setResizingMode] = useState<'none' | 'column' | 'header'>('none');

  // Autocomplete
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [activeSuggestionIndex, setActiveSuggestionIndex] = useState(0);
  const searchContainerRef = useRef<HTMLDivElement>(null);
  const suggestionsListId = useId();
  const activeOptionId = useId();
  const matrixBuildStartRef = useRef<number | null>(null);
  const matrixPayloadSetAtRef = useRef<number | null>(null);

  const resizeStartPos = useRef(0);
  const resizeStartSize = useRef(0);
  const scrollContainerRef = useRef<HTMLDivElement>(null);

  // Resize logic
  const handleColumnResizeStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setResizingMode('column');
      resizeStartPos.current = e.clientX;
      resizeStartSize.current = firstColumnWidth;
    },
    [firstColumnWidth]
  );

  const handleHeaderResizeStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setResizingMode('header');
      resizeStartPos.current = e.clientY;
      resizeStartSize.current = headerHeight;
    },
    [headerHeight]
  );

  useEffect(() => {
    if (resizingMode === 'none') return;
    const handleMouseMove = (e: MouseEvent) => {
      if (resizingMode === 'column') {
        const delta = e.clientX - resizeStartPos.current;
        setFirstColumnWidth(
          Math.min(
            MAX_FIRST_COLUMN_WIDTH,
            Math.max(MIN_FIRST_COLUMN_WIDTH, resizeStartSize.current + delta)
          )
        );
      } else if (resizingMode === 'header') {
        const delta = e.clientY - resizeStartPos.current;
        setHeaderHeight(
          Math.min(MAX_HEADER_HEIGHT, Math.max(MIN_HEADER_HEIGHT, resizeStartSize.current + delta))
        );
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

  const [matrixPayload, setMatrixPayload] = useState<MatrixWorkerPayload | null>(null);
  const [isMatrixLoading, setIsMatrixLoading] = useState(false);
  const [matrixError, setMatrixError] = useState<string | null>(null);

  useEffect(() => {
    if (!result?.statements) {
      setMatrixPayload(null);
      setIsMatrixLoading(false);
      setMatrixError(null);
      return;
    }

    let cancelled = false;
    setIsMatrixLoading(true);
    setMatrixError(null);
    setMatrixPayload(null);
    matrixBuildStartRef.current = performance.now();

    buildMatrixInWorker(result, { maxItems: MAX_MATRIX_ITEMS })
      .then((payload) => {
        if (cancelled) return;
        const receivedAt = performance.now();
        if (MATRIX_DEBUG && matrixBuildStartRef.current !== null) {
          console.log(
            `[MatrixView] Worker payload received in ${(receivedAt - matrixBuildStartRef.current).toFixed(1)}ms`
          );
        }
        matrixPayloadSetAtRef.current = receivedAt;
        // Update loading state immediately (urgent) so spinner disappears
        setIsMatrixLoading(false);
        // Defer payload update which may trigger expensive re-renders
        startTransition(() => {
          setMatrixPayload(payload);
        });
      })
      .catch((error) => {
        if (cancelled) return;
        if (error instanceof Error && error.message === 'Build cancelled') {
          return;
        }
        console.error('[MatrixView] Matrix build failed:', error);
        setMatrixError('Failed to build matrix data.');
        setIsMatrixLoading(false);
      });

    return () => {
      cancelled = true;
      cancelPendingMatrixBuilds();
    };
  }, [result]);

  const fullMatrixData = useMemo(() => {
    if (!matrixPayload) return EMPTY_MATRIX;
    return matrixSubMode === 'tables' ? matrixPayload.tableMatrix : matrixPayload.scriptMatrix;
  }, [matrixSubMode, matrixPayload]);

  const allColumnNames = matrixPayload?.allColumnNames ?? [];

  const limitInfo = useMemo(() => {
    if (!matrixPayload) return null;
    if (matrixSubMode === 'tables') {
      return {
        label: 'tables',
        total: matrixPayload.tableItemCount,
        shown: matrixPayload.tableItemsRendered,
      };
    }
    return {
      label: 'scripts',
      total: matrixPayload.scriptItemCount,
      shown: matrixPayload.scriptItemsRendered,
    };
  }, [matrixPayload, matrixSubMode]);

  useEffect(() => {
    if (!MATRIX_DEBUG || !matrixPayload) return;
    const commitAt = performance.now();
    const setAt = matrixPayloadSetAtRef.current;
    if (setAt) {
      console.log(`[MatrixView] Render commit after payload: ${(commitAt - setAt).toFixed(1)}ms`);
    }
    const tableItems = matrixPayload.tableMatrix.items.length;
    const scriptItems = matrixPayload.scriptMatrix.items.length;
    console.log(
      `[MatrixView] Matrix sizes: tables=${tableItems} (${tableItems * tableItems} cells), scripts=${scriptItems} (${scriptItems * scriptItems} cells)`
    );
  }, [matrixPayload]);

  const matrixMetrics = useMemo(() => {
    if (!matrixPayload) {
      return {
        rowCounts: new Map<string, number>(),
        colCounts: new Map<string, number>(),
        maxRow: 0,
        maxCol: 0,
        maxIntensity: 1,
      };
    }
    return matrixSubMode === 'tables' ? matrixPayload.tableMetrics : matrixPayload.scriptMetrics;
  }, [matrixPayload, matrixSubMode]);

  // Clustering Logic
  const sortedItems = useMemo(() => {
    if (!clusterMode) return fullMatrixData.items;
    const start = MATRIX_DEBUG ? performance.now() : 0;
    const result = clusterItems(fullMatrixData.items, fullMatrixData.cells);
    if (MATRIX_DEBUG) {
      const duration = performance.now() - start;
      if (duration > 8) {
        console.log(`[MatrixView] clusterItems: ${duration.toFixed(1)}ms`);
      }
    }
    return result;
  }, [fullMatrixData, clusterMode]);

  // Transitive Flow for X-Ray
  const transitiveFlow = useMemo(() => {
    if (!xRayMode || !focusedNode) return null;
    const start = MATRIX_DEBUG ? performance.now() : 0;
    const result = getTransitiveFlow(focusedNode, fullMatrixData.cells, sortedItems);
    if (MATRIX_DEBUG) {
      const duration = performance.now() - start;
      if (duration > 8) {
        console.log(`[MatrixView] getTransitiveFlow: ${duration.toFixed(1)}ms`);
      }
    }
    return result;
  }, [xRayMode, focusedNode, fullMatrixData, sortedItems]);

  const activeXRaySet = useMemo(() => {
    if (!xRayMode || !focusedNode || !transitiveFlow) return null;
    return new Set([focusedNode, ...transitiveFlow.ancestors, ...transitiveFlow.descendants]);
  }, [xRayMode, focusedNode, transitiveFlow]);

  const maxIntensity = matrixMetrics.maxIntensity || 1;

  // Field Tracing Logic (uses debounced text for expensive computation)
  const matchingFieldNodes = useMemo(() => {
    if (!debouncedFilterText || filterMode !== 'fields') return null;
    const start = MATRIX_DEBUG ? performance.now() : 0;
    const lower = debouncedFilterText.toLowerCase();
    const matchedNodes = new Set<string>();

    for (const [rowId, rowCells] of fullMatrixData.cells) {
      for (const [colId, cell] of rowCells) {
        if (cell.type === 'write' || cell.type === 'read') {
          // Check Table Dependencies
          if (matrixSubMode === 'tables') {
            const details = cell.details as TableDependencyWithDetails;
            if (details && details.columns) {
              const hasMatch = details.columns.some(
                (c) =>
                  c.source.toLowerCase().includes(lower) || c.target.toLowerCase().includes(lower)
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
    if (MATRIX_DEBUG) {
      const duration = performance.now() - start;
      if (duration > 8) {
        console.log(`[MatrixView] matchingFieldNodes: ${duration.toFixed(1)}ms`);
      }
    }
    return matchedNodes;
  }, [fullMatrixData, debouncedFilterText, filterMode, matrixSubMode]);

  // Filtering (uses debounced text)
  const filteredRowItems = useMemo(() => {
    const start = MATRIX_DEBUG ? performance.now() : 0;
    const result = filterItems({
      items: sortedItems,
      filterMode,
      filterText: debouncedFilterText,
      matchingFieldNodes,
      xRayMode,
      xRayFilterMode,
      activeXRaySet,
      targetMode: 'rows',
    });
    if (MATRIX_DEBUG) {
      const duration = performance.now() - start;
      if (duration > 8) {
        console.log(`[MatrixView] filteredRowItems: ${duration.toFixed(1)}ms`);
      }
    }
    return result;
  }, [
    sortedItems,
    debouncedFilterText,
    filterMode,
    matchingFieldNodes,
    xRayMode,
    xRayFilterMode,
    activeXRaySet,
  ]);

  const filteredColumnItems = useMemo(() => {
    const start = MATRIX_DEBUG ? performance.now() : 0;
    const result = filterItems({
      items: sortedItems,
      filterMode,
      filterText: debouncedFilterText,
      matchingFieldNodes,
      xRayMode,
      xRayFilterMode,
      activeXRaySet,
      targetMode: 'columns',
    });
    if (MATRIX_DEBUG) {
      const duration = performance.now() - start;
      if (duration > 8) {
        console.log(`[MatrixView] filteredColumnItems: ${duration.toFixed(1)}ms`);
      }
    }
    return result;
  }, [
    sortedItems,
    debouncedFilterText,
    filterMode,
    matchingFieldNodes,
    xRayMode,
    xRayFilterMode,
    activeXRaySet,
  ]);

  useEffect(() => {
    if (!MATRIX_DEBUG) return;
    const rows = filteredRowItems.length;
    const cols = filteredColumnItems.length;
    const cells = rows * cols;
    if (cells > 5000) {
      console.log(`[MatrixView] render grid: rows=${rows}, cols=${cols}, cells=${cells}`);
    }
  }, [filteredRowItems.length, filteredColumnItems.length]);

  const handleCellHover = useCallback((row: string, col: string) => {
    setHoveredCell({ row, col });
  }, []);

  // Autocomplete Logic
  const suggestions = useMemo(() => {
    if (!filterText) return [];
    const start = MATRIX_DEBUG ? performance.now() : 0;
    const lower = filterText.toLowerCase();
    let source: string[] = [];

    if (filterMode === 'fields') {
      source = allColumnNames;
    } else {
      source = sortedItems.map(getShortName);
    }

    const matches = Array.from(
      new Set(source.filter((s) => s.toLowerCase().includes(lower)))
    ).slice(0, MAX_AUTOCOMPLETE_SUGGESTIONS);
    if (MATRIX_DEBUG) {
      const duration = performance.now() - start;
      if (duration > 8) {
        console.log(`[MatrixView] suggestions: ${duration.toFixed(1)}ms`);
      }
    }
    return matches;
  }, [filterText, filterMode, allColumnNames, sortedItems]);
  const suggestionCount = suggestions.length;

  const handleSearchKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActiveSuggestionIndex((prev) => Math.min(prev + 1, suggestions.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActiveSuggestionIndex((prev) => Math.max(prev - 1, 0));
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
    setActiveSuggestionIndex((prev) => {
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
      if (
        searchContainerRef.current &&
        !searchContainerRef.current.contains(event.target as Node)
      ) {
        setShowSuggestions(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handleCellClick = useCallback(
    (rowName: string, colName: string) => {
      const cellData = fullMatrixData.cells.get(rowName)?.get(colName);
      if (!cellData || cellData.type === 'self' || cellData.type === 'none') return;

      if (matrixSubMode === 'tables') {
        const details = cellData.details as TableDependencyWithDetails | undefined;
        const location = details?.locations[0];
        if (location?.sourceName) {
          requestNavigation({
            sourceName: location.sourceName,
            span: location.span,
            targetName: details?.sourceTable,
            targetType: 'table',
          });
        } else if (location?.span) {
          highlightSpan(location.span);
        } else if (details?.spans.length) {
          highlightSpan(details.spans[0]);
        }
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
    },
    [fullMatrixData, matrixSubMode, highlightSpan, requestNavigation]
  );

  // Toggle Focus
  const toggleFocus = (node: string) => {
    if (!xRayMode) return;
    setFocusedNode((prev) => (prev === node ? null : node));
  };

  if (!result) {
    return (
      <div
        className={cn(
          'flex flex-col items-center justify-center h-full text-slate-400 gap-4',
          className
        )}
      >
        <Database className="h-12 w-12 opacity-20" />
        <p>No analysis result available</p>
      </div>
    );
  }

  if (matrixError) {
    return (
      <div
        className={cn(
          'flex flex-col items-center justify-center h-full text-slate-400 gap-4',
          className
        )}
      >
        <Database className="h-12 w-12 opacity-20" />
        <p>{matrixError}</p>
      </div>
    );
  }

  if (isMatrixLoading || !matrixPayload) {
    return (
      <div
        className={cn(
          'flex flex-col items-center justify-center h-full text-slate-400 gap-3',
          className
        )}
      >
        <Loader2 className="h-6 w-6 animate-spin text-slate-400" />
        <p>Building matrix...</p>
      </div>
    );
  }

  const isEmpty = filteredRowItems.length === 0 || filteredColumnItems.length === 0;

  return (
    <GraphTooltipProvider>
      <div className={cn('flex flex-col h-full bg-background', className)}>
        {/* Toolbar */}
        <MatrixToolbar
          matrixSubMode={matrixSubMode}
          setMatrixSubMode={setMatrixSubMode}
          setFocusedNode={setFocusedNode}
          xRayMode={xRayMode}
          setXRayMode={setXRayMode}
          xRayFilterMode={xRayFilterMode}
          setXRayFilterMode={setXRayFilterMode}
          heatmapMode={heatmapMode}
          setHeatmapMode={setHeatmapMode}
          clusterMode={clusterMode}
          setClusterMode={setClusterMode}
          complexityMode={complexityMode}
          setComplexityMode={setComplexityMode}
          showLegend={showLegend}
          setShowLegend={setShowLegend}
          searchContainerRef={searchContainerRef}
          filterMode={filterMode}
          setFilterMode={setFilterMode}
          filterText={filterText}
          setFilterText={setFilterText}
          setShowSuggestions={setShowSuggestions}
          handleSearchKeyDown={handleSearchKeyDown}
          showSuggestions={showSuggestions}
          suggestions={suggestions}
          suggestionsListId={suggestionsListId}
          activeOptionId={activeOptionId}
          activeSuggestionIndex={activeSuggestionIndex}
          setActiveSuggestionIndex={setActiveSuggestionIndex}
        />

        {limitInfo && limitInfo.total > limitInfo.shown && (
          <div className="px-4 py-2 text-xs text-slate-500 border-b border-slate-200 dark:border-slate-800 bg-slate-50/60 dark:bg-slate-900/30">
            Showing top {limitInfo.shown} of {limitInfo.total} {limitInfo.label}. Refine filters to
            narrow results.
          </div>
        )}

        {/* Content */}
        <MatrixGrid
          isEmpty={isEmpty}
          setFilterText={setFilterText}
          resizingMode={resizingMode}
          scrollContainerRef={scrollContainerRef}
          firstColumnWidth={firstColumnWidth}
          filteredColumnItems={filteredColumnItems}
          filteredRowItems={filteredRowItems}
          headerHeight={headerHeight}
          handleColumnResizeStart={handleColumnResizeStart}
          handleHeaderResizeStart={handleHeaderResizeStart}
          focusedNode={focusedNode}
          transitiveFlow={transitiveFlow}
          xRayMode={xRayMode}
          matrixMetrics={matrixMetrics}
          complexityMode={complexityMode}
          hoveredCell={hoveredCell}
          toggleFocus={toggleFocus}
          fullMatrixData={fullMatrixData}
          heatmapMode={heatmapMode}
          matrixSubMode={matrixSubMode}
          maxIntensity={maxIntensity}
          filterMode={filterMode}
          filterText={filterText}
          handleCellHover={handleCellHover}
          setHoveredCell={setHoveredCell}
          handleCellClick={handleCellClick}
        />

        {showLegend && (
          <MatrixLegend
            xRayMode={xRayMode}
            heatmapMode={heatmapMode}
            clusterMode={clusterMode}
            complexityMode={complexityMode}
            setShowLegend={setShowLegend}
          />
        )}
      </div>
    </GraphTooltipProvider>
  );
}
