import type { Dispatch, JSX, KeyboardEventHandler, RefObject, SetStateAction } from 'react';
import {
  Activity,
  BarChart2,
  Columns3,
  FileCode,
  Info,
  Maximize2,
  Minimize2,
  Rows3,
  ScanLine,
  Search,
  Shuffle,
  Table2,
  Zap,
} from 'lucide-react';
import { PANEL_STYLES } from '../../constants';
import type { MatrixSubMode } from '../../types';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipPortal,
  GraphTooltipTrigger,
} from '../ui/graph-tooltip';
import type { FilterMode, XRayFilterMode } from './types';
import { cn } from './utils';

interface MatrixToolbarProps {
  matrixSubMode: MatrixSubMode;
  setMatrixSubMode: (mode: MatrixSubMode) => void;
  setFocusedNode: Dispatch<SetStateAction<string | null>>;
  xRayMode: boolean;
  setXRayMode: Dispatch<SetStateAction<boolean>>;
  xRayFilterMode: XRayFilterMode;
  setXRayFilterMode: Dispatch<SetStateAction<XRayFilterMode>>;
  heatmapMode: boolean;
  setHeatmapMode: Dispatch<SetStateAction<boolean>>;
  clusterMode: boolean;
  setClusterMode: Dispatch<SetStateAction<boolean>>;
  complexityMode: boolean;
  setComplexityMode: Dispatch<SetStateAction<boolean>>;
  showLegend: boolean;
  setShowLegend: Dispatch<SetStateAction<boolean>>;
  searchContainerRef: RefObject<HTMLDivElement | null>;
  filterMode: FilterMode;
  setFilterMode: Dispatch<SetStateAction<FilterMode>>;
  filterText: string;
  setFilterText: Dispatch<SetStateAction<string>>;
  setShowSuggestions: Dispatch<SetStateAction<boolean>>;
  handleSearchKeyDown: KeyboardEventHandler<HTMLInputElement>;
  showSuggestions: boolean;
  suggestions: string[];
  suggestionsListId: string;
  activeOptionId: string;
  activeSuggestionIndex: number;
  setActiveSuggestionIndex: Dispatch<SetStateAction<number>>;
}

export function MatrixToolbar({
  matrixSubMode,
  setMatrixSubMode,
  setFocusedNode,
  xRayMode,
  setXRayMode,
  xRayFilterMode,
  setXRayFilterMode,
  heatmapMode,
  setHeatmapMode,
  clusterMode,
  setClusterMode,
  complexityMode,
  setComplexityMode,
  showLegend,
  setShowLegend,
  searchContainerRef,
  filterMode,
  setFilterMode,
  filterText,
  setFilterText,
  setShowSuggestions,
  handleSearchKeyDown,
  showSuggestions,
  suggestions,
  suggestionsListId,
  activeOptionId,
  activeSuggestionIndex,
  setActiveSuggestionIndex,
}: MatrixToolbarProps): JSX.Element {
  return (
    <div className="flex items-center justify-between p-4 border-b border-slate-200 dark:border-slate-800 bg-background z-40 gap-4">
      <div className={`${PANEL_STYLES.selector} shrink-0`}>
        <button
          onClick={() => {
            setMatrixSubMode('scripts');
            setFocusedNode(null);
          }}
          className={cn(
            'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-full px-3 h-7 text-sm font-medium transition-all duration-200',
            matrixSubMode === 'scripts'
              ? 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100'
              : 'text-slate-500 hover:text-slate-700 dark:hover:text-slate-300'
          )}
        >
          <FileCode className="size-4" />
          <span>Scripts</span>
        </button>
        <button
          onClick={() => {
            setMatrixSubMode('tables');
            setFocusedNode(null);
          }}
          className={cn(
            'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-full px-3 h-7 text-sm font-medium transition-all duration-200',
            matrixSubMode === 'tables'
              ? 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100'
              : 'text-slate-500 hover:text-slate-700 dark:hover:text-slate-300'
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
                'p-1.5 rounded-md transition-all flex items-center gap-1',
                xRayMode
                  ? 'bg-purple-100 text-purple-600 dark:bg-purple-900/30 dark:text-purple-400 ring-1 ring-purple-500'
                  : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800'
              )}
            >
              <Zap className="h-4 w-4" />
            </button>
          </GraphTooltipTrigger>
          <GraphTooltipPortal>
            <GraphTooltipContent side="bottom" className="text-xs">
              <div className="font-semibold text-slate-100">Impact X-Ray Mode</div>
              <div className="text-slate-400">
                Click a row/col header to highlight lineage flow.
              </div>
              <div className="mt-1 flex flex-col gap-1 text-[10px]">
                <div className="flex items-center gap-1.5">
                  <div className="w-2 h-2 bg-blue-500 rounded-full" />
                  Ancestors (Upstream)
                </div>
                <div className="flex items-center gap-1.5">
                  <div className="w-2 h-2 bg-emerald-500 rounded-full" />
                  Descendants (Downstream)
                </div>
              </div>
            </GraphTooltipContent>
          </GraphTooltipPortal>
        </GraphTooltip>

        {/* X-Ray View Mode Toggle (Dim/Hide) */}
        {xRayMode && (
          <GraphTooltip delayDuration={300}>
            <GraphTooltipTrigger asChild>
              <button
                onClick={() => setXRayFilterMode((prev) => (prev === 'dim' ? 'hide' : 'dim'))}
                aria-label={
                  xRayFilterMode === 'hide' ? 'Switch to dim mode' : 'Switch to hide mode'
                }
                className={cn(
                  'p-1.5 rounded-md transition-all',
                  xRayFilterMode === 'hide'
                    ? 'bg-indigo-100 text-indigo-600 dark:bg-indigo-900/30 dark:text-indigo-400'
                    : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800'
                )}
              >
                {xRayFilterMode === 'hide' ? (
                  <Minimize2 className="h-4 w-4" />
                ) : (
                  <Maximize2 className="h-4 w-4" />
                )}
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
                'p-1.5 rounded-md transition-all',
                heatmapMode
                  ? 'bg-orange-100 text-orange-600 dark:bg-orange-900/30 dark:text-orange-400 ring-1 ring-orange-500'
                  : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800'
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
                'p-1.5 rounded-md transition-all',
                clusterMode
                  ? 'bg-blue-100 text-blue-600 dark:bg-blue-900/30 dark:text-blue-400 ring-1 ring-blue-500'
                  : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800'
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
                'p-1.5 rounded-md transition-all',
                complexityMode
                  ? 'bg-teal-100 text-teal-600 dark:bg-teal-900/30 dark:text-teal-400 ring-1 ring-teal-500'
                  : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800'
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
        className="relative group ml-auto flex items-center rounded-full border border-slate-200/60 dark:border-slate-700/60 bg-white/95 dark:bg-slate-900/95 h-9 shadow-xs backdrop-blur-xs"
        ref={searchContainerRef}
      >
        <Search
          className="absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-slate-400 pointer-events-none z-10"
          strokeWidth={1.5}
        />
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
          aria-activedescendant={
            showSuggestions && suggestions.length > 0
              ? `${activeOptionId}-${activeSuggestionIndex}`
              : undefined
          }
          aria-autocomplete="list"
          data-matrix-search-input
          className="h-7 pl-8 pr-24 text-sm bg-transparent border-0 rounded-full focus:outline-hidden focus:ring-0 w-64 placeholder:text-slate-400"
        />

        {/* Autocomplete Dropdown */}
        {showSuggestions && (suggestions.length > 0 || filterText.length > 0) && (
          <div
            id={suggestionsListId}
            role="listbox"
            aria-label="Search suggestions"
            className="absolute top-full left-0 right-0 mt-1 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-lg shadow-lg z-100 max-h-60 overflow-auto py-1"
          >
            {suggestions.length > 0 ? (
              suggestions.map((suggestion, index) => (
                <button
                  key={suggestion}
                  id={`${activeOptionId}-${index}`}
                  role="option"
                  aria-selected={index === activeSuggestionIndex}
                  className={cn(
                    'w-full text-left px-3 py-2 text-sm transition-colors flex items-center gap-2',
                    index === activeSuggestionIndex
                      ? 'bg-indigo-50 dark:bg-indigo-900/20 text-indigo-700 dark:text-indigo-300'
                      : 'text-slate-700 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-800'
                  )}
                  onClick={() => {
                    setFilterText(suggestion);
                    setShowSuggestions(false);
                  }}
                  onMouseEnter={() => setActiveSuggestionIndex(index)}
                >
                  {filterMode === 'fields' ? (
                    <ScanLine className="h-3 w-3 opacity-50" />
                  ) : matrixSubMode === 'scripts' ? (
                    <FileCode className="h-3 w-3 opacity-50" />
                  ) : (
                    <Table2 className="h-3 w-3 opacity-50" />
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
              'p-1 rounded-full transition-all',
              filterMode === 'rows'
                ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-xs'
                : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-300'
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
              'p-1 rounded-full transition-all',
              filterMode === 'columns'
                ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-xs'
                : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-300'
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
              'p-1 rounded-full transition-all',
              filterMode === 'fields'
                ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-xs'
                : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-300'
            )}
          >
            <ScanLine className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
}
