import { createContext, createElement, useContext, useMemo, type ReactNode } from 'react';
import { useStore } from 'zustand';
import { useShallow } from 'zustand/react/shallow';
import { createStore, type StoreApi } from 'zustand/vanilla';
import type { AnalyzeResult, Span } from '@pondpilot/flowscope-core';
import type {
  LineageViewMode,
  LayoutAlgorithm,
  LayoutMetrics,
  GraphBuildMetrics,
  NavigationRequest,
  MatrixSubMode,
  TableFilterDirection,
  TableFilter,
  LineageState as LineageContextState,
  LineageActions as LineageContextActions,
  LineageContextValue,
} from './types';
import { findMergedNodeById } from './utils/nodeOccurrences';

const DEFAULT_LAYOUT_ALGORITHM: LayoutAlgorithm = 'dagre';

// Storage keys
const STORAGE_KEYS = {
  viewMode: 'flowscope-view-mode',
  layoutAlgorithm: 'flowscope-layout-algorithm',
  defaultCollapsed: 'flowscope-default-collapsed',
  columnEdges: 'flowscope-column-edges',
  hideCTEs: 'flowscope-hide-ctes',
} as const;

/**
 * Generic localStorage helper for loading values with validation.
 * Returns the default value if localStorage is unavailable or the value is invalid.
 */
function loadFromStorage<T>(
  key: string,
  isValid: (value: string) => boolean,
  parse: (value: string) => T,
  defaultValue: T
): T {
  try {
    const stored = localStorage.getItem(key);
    if (stored !== null && isValid(stored)) {
      return parse(stored);
    }
  } catch {
    // localStorage might not be available (SSR, etc.)
  }
  return defaultValue;
}

/**
 * Generic localStorage helper for saving values.
 * Silently fails if localStorage is unavailable.
 */
function saveToStorage(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // localStorage might not be available
  }
}

/**
 * Migrate legacy 'column' view mode to 'table' with column edges enabled.
 * Returns true if migration was performed.
 */
function migrateLegacyColumnViewMode(): boolean {
  try {
    const stored = localStorage.getItem(STORAGE_KEYS.viewMode);
    if (stored === 'column') {
      localStorage.setItem(STORAGE_KEYS.viewMode, 'table');
      localStorage.setItem(STORAGE_KEYS.columnEdges, 'true');
      return true;
    }
  } catch {
    // localStorage might not be available
  }
  return false;
}

function loadViewMode(): LineageViewMode {
  // Migrate legacy column view mode first
  migrateLegacyColumnViewMode();

  return loadFromStorage(
    STORAGE_KEYS.viewMode,
    (v) => v === 'script' || v === 'table',
    (v) => v as LineageViewMode,
    'table'
  );
}

function saveViewMode(mode: LineageViewMode): void {
  saveToStorage(STORAGE_KEYS.viewMode, mode);
}

function loadLayoutAlgorithm(
  defaultAlgorithm: LayoutAlgorithm = DEFAULT_LAYOUT_ALGORITHM
): LayoutAlgorithm {
  return loadFromStorage(
    STORAGE_KEYS.layoutAlgorithm,
    (v) => v === 'dagre' || v === 'elk',
    (v) => v as LayoutAlgorithm,
    defaultAlgorithm
  );
}

function saveLayoutAlgorithm(algorithm: LayoutAlgorithm): void {
  saveToStorage(STORAGE_KEYS.layoutAlgorithm, algorithm);
}

function loadDefaultCollapsed(): boolean {
  return loadFromStorage(
    STORAGE_KEYS.defaultCollapsed,
    (v) => v === 'true' || v === 'false',
    (v) => v === 'true',
    true
  );
}

function saveDefaultCollapsed(collapsed: boolean): void {
  saveToStorage(STORAGE_KEYS.defaultCollapsed, String(collapsed));
}

function loadColumnEdges(): boolean {
  return loadFromStorage(
    STORAGE_KEYS.columnEdges,
    (v) => v === 'true' || v === 'false',
    (v) => v === 'true',
    false
  );
}

function saveColumnEdges(show: boolean): void {
  saveToStorage(STORAGE_KEYS.columnEdges, String(show));
}

function loadHideCTEs(): boolean {
  return loadFromStorage(
    STORAGE_KEYS.hideCTEs,
    (v) => v === 'true' || v === 'false',
    (v) => v === 'true',
    false
  );
}

function saveHideCTEs(hide: boolean): void {
  saveToStorage(STORAGE_KEYS.hideCTEs, String(hide));
}

export interface LineageState {
  // Data
  result: AnalyzeResult | null;
  sql: string;

  // Selection & UI state
  selectedNodeId: string | null;
  selectedStatementIndex: number;
  highlightedSpan: Span | null;
  /**
   * Index into the selected node's `nameSpans` currently highlighted in the
   * editor. Resets to 0 whenever a new node is selected. Used by the per-node
   * ◀ n/total ▶ controls and the keyboard `n` / `Shift+n` shortcuts. CTE
   * `bodySpan` is handled separately (via double-click) and is not part of
   * this cycle.
   */
  focusedOccurrenceIndex: number;
  searchTerm: string;
  viewMode: LineageViewMode;
  matrixSubMode: MatrixSubMode;
  layoutAlgorithm: LayoutAlgorithm;
  layoutMetrics: LayoutMetrics;
  graphMetrics: GraphBuildMetrics;
  // Node IDs whose collapsed state differs from defaultCollapsed.
  // When defaultCollapsed is true, these are expanded nodes (overrides).
  // When defaultCollapsed is false, these are collapsed nodes (overrides).
  collapsedNodeIds: Set<string>;
  expandedTableIds: Set<string>; // Tables with all columns shown
  defaultCollapsed: boolean; // Whether tables are collapsed by default
  showColumnEdges: boolean; // Whether to show column-level edges instead of table-level
  hideCTEs: boolean; // Whether to hide CTEs and show bypass edges (A→CTE→B becomes A→B)
  showScriptTables: boolean;
  navigationRequest: NavigationRequest | null;
  visibleGraphNodeIds: ReadonlySet<string>;
  /**
   * Active reveal-in-graph request. Set by the SqlView "Reveal in lineage"
   * action (#24). The nonce forces re-triggering when the same node is
   * revealed twice in a row so the pulse animation restarts.
   *
   * `suppressNavigation` is consumed by GraphView's graph→editor bounce effect,
   * keyed on the nonce so each reveal suppresses at most one bounce even when
   * multiple reveals land in quick succession.
   */
  revealRequest: { nodeId: string; nonce: number; suppressNavigation: boolean } | null;
  tableFilter: TableFilter;
  isLayouting: boolean;
  isBuilding: boolean;
  /**
   * Snapshot of the SQL text each path held when the most recent analysis
   * ran, keyed by the same `sourceName` the analyzer uses. Consumers diff
   * this against live file content to decide whether the graph's spans are
   * still aligned with the editor (#22). `null` before any analysis has run.
   */
  analyzedContentByPath: ReadonlyMap<string, string> | null;
  /**
   * Paths whose current content has diverged from `analyzedContentByPath`.
   * Populated by the app layer (which owns the live file content) via
   * `setStalePaths`. Nav affordances gated by membership in this set (#22).
   */
  stalePaths: ReadonlySet<string>;

  // Actions
  setResult: (result: AnalyzeResult | null) => void;
  setSql: (sql: string) => void;
  selectNode: (nodeId: string | null) => void;
  /**
   * Cycle the highlighted occurrence of the currently selected node.
   * Wraps around at the ends. No-op if no node is selected, the node has
   * fewer than 2 occurrences, or the node has no `nameSpans`.
   */
  cycleOccurrence: (direction: 'next' | 'prev') => void;
  /**
   * Focus a specific occurrence of the currently selected node by index.
   * No-op if the index is out of range.
   */
  focusOccurrence: (index: number) => void;
  toggleNodeCollapse: (nodeId: string) => void;
  setNodeCollapsed: (nodeId: string, collapsed: boolean) => void;
  toggleTableExpansion: (tableId: string) => void;
  setTableExpanded: (tableId: string, expanded: boolean) => void;
  /**
   * Set all nodes to collapsed or expanded state.
   * This updates the defaultCollapsed setting and clears all per-node overrides
   * (collapsedNodeIds), resetting all nodes to the new default state.
   * Only affects table/column nodes, not script nodes.
   */
  setAllNodesCollapsed: (collapsed: boolean) => void;
  selectStatement: (index: number) => void;
  highlightSpan: (span: Span | null) => void;
  setSearchTerm: (term: string) => void;
  setViewMode: (mode: LineageViewMode) => void;
  setMatrixSubMode: (mode: MatrixSubMode) => void;
  setLayoutAlgorithm: (algorithm: LayoutAlgorithm) => void;
  setLayoutMetrics: (metrics: LayoutMetrics) => void;
  setGraphMetrics: (metrics: GraphBuildMetrics) => void;
  toggleColumnEdges: () => void;
  toggleHideCTEs: () => void;
  toggleShowScriptTables: () => void;
  requestNavigation: (request: NavigationRequest | null) => void;
  setVisibleGraphNodeIds: (nodeIds: Iterable<string>) => void;
  /**
   * Selects the node, bumps the reveal nonce (so repeat invocations retrigger
   * the graph pulse), and suppresses the immediate graph→editor bounce. The GraphView
   * observes `revealRequest` and drives fitView + the pulse animation.
   */
  revealNodeInGraph: (nodeId: string) => void;
  /** Clear any pending reveal request. */
  clearRevealRequest: () => void;
  setTableFilter: (filter: TableFilter) => void;
  toggleTableFilterSelection: (tableLabel: string) => void;
  setTableFilterDirection: (direction: TableFilterDirection) => void;
  clearTableFilter: () => void;
  setIsLayouting: (isLayouting: boolean) => void;
  setIsBuilding: (isBuilding: boolean) => void;
  /**
   * Replace the analyzed-content snapshot. Called after a successful
   * analysis run with a path→content map covering every file that went
   * into the analyzer. Pass `null` to clear (e.g. when the result itself
   * is cleared).
   */
  setAnalyzedContent: (map: ReadonlyMap<string, string> | null) => void;
  /**
   * Replace the set of paths whose live content diverges from the analyzed
   * snapshot. Computed by the app and pushed here so that components
   * without access to the project store (the graph's OccurrenceCycler,
   * useOccurrenceShortcuts) can gate on staleness uniformly.
   */
  setStalePaths: (paths: Iterable<string>) => void;
}

/**
 * Build a new, isolated Zustand store instance.
 */
export function createLineageStore(
  initialState?: Partial<LineageState>,
  options?: { defaultLayoutAlgorithm?: LayoutAlgorithm }
): StoreApi<LineageState> {
  const initialViewMode = initialState?.viewMode ?? loadViewMode();
  const fallbackAlgorithm = options?.defaultLayoutAlgorithm ?? DEFAULT_LAYOUT_ALGORITHM;
  const initialLayoutAlgorithm =
    initialState?.layoutAlgorithm ?? loadLayoutAlgorithm(fallbackAlgorithm);
  const initialDefaultCollapsed = initialState?.defaultCollapsed ?? loadDefaultCollapsed();
  const initialColumnEdges = initialState?.showColumnEdges ?? loadColumnEdges();
  const initialHideCTEs = initialState?.hideCTEs ?? loadHideCTEs();

  return createStore<LineageState>((set) => ({
    // Initial state
    result: null,
    sql: '',
    selectedNodeId: null,
    selectedStatementIndex: 0,
    highlightedSpan: null,
    focusedOccurrenceIndex: 0,
    searchTerm: '',
    viewMode: initialViewMode,
    matrixSubMode: 'tables',
    layoutAlgorithm: initialLayoutAlgorithm,
    layoutMetrics: {
      lastDurationMs: null,
      nodeCount: 0,
      edgeCount: 0,
      algorithm: null,
      lastUpdatedAt: null,
    },
    graphMetrics: {
      lastDurationMs: null,
      nodeCount: 0,
      edgeCount: 0,
      lastUpdatedAt: null,
    },
    collapsedNodeIds: new Set(),
    expandedTableIds: new Set(),
    defaultCollapsed: initialDefaultCollapsed,
    showColumnEdges: initialColumnEdges,
    hideCTEs: initialHideCTEs,
    showScriptTables: false,
    navigationRequest: null,
    visibleGraphNodeIds: new Set(),
    revealRequest: null,
    tableFilter: { selectedTableLabels: new Set(), direction: 'both' },
    isLayouting: false,
    isBuilding: false,
    analyzedContentByPath: null,
    stalePaths: new Set(),
    ...initialState,

    // Actions
    setResult: (result) =>
      set((state) => {
        const statementCount = result?.statements.length ?? 0;
        const maxIndex = Math.max(0, statementCount - 1);
        const newSelectedStatementIndex = Math.max(
          0,
          Math.min(state.selectedStatementIndex, maxIndex)
        );

        return {
          result,
          selectedNodeId: null,
          highlightedSpan: null,
          focusedOccurrenceIndex: 0,
          revealRequest: null,
          collapsedNodeIds: new Set(),
          expandedTableIds: new Set(),
          selectedStatementIndex: statementCount === 0 ? 0 : newSelectedStatementIndex,
          // Clearing the result invalidates any staleness derived from it.
          // A successful re-analysis will immediately refill these via
          // setAnalyzedContent / setStalePaths in the app layer.
          ...(result === null ? { analyzedContentByPath: null, stalePaths: new Set() } : {}),
        };
      }),

    setSql: (sql) => set({ sql }),

    selectNode: (nodeId) =>
      set((state) => ({
        selectedNodeId: nodeId,
        // Clearing selection also clears the highlight; selecting a node
        // preserves whatever span the caller just highlighted (e.g. the
        // GraphView click handler sets `highlightedSpan` right before
        // calling `selectNode`).
        highlightedSpan: nodeId === null ? null : state.highlightedSpan,
        focusedOccurrenceIndex: 0,
      })),

    cycleOccurrence: (direction) =>
      set((state) => {
        if (state.selectedNodeId === null || state.result === null) {
          return state;
        }
        const node = findMergedNodeById(state.result, state.selectedNodeId);
        const spans = node?.nameSpans ?? [];
        if (spans.length < 2) {
          return state;
        }
        const delta = direction === 'next' ? 1 : -1;
        const nextIndex = (state.focusedOccurrenceIndex + delta + spans.length) % spans.length;
        return {
          focusedOccurrenceIndex: nextIndex,
          highlightedSpan: spans[nextIndex],
        };
      }),

    focusOccurrence: (index) =>
      set((state) => {
        if (state.selectedNodeId === null || state.result === null) {
          return state;
        }
        const node = findMergedNodeById(state.result, state.selectedNodeId);
        const spans = node?.nameSpans ?? [];
        if (index < 0 || index >= spans.length) {
          return state;
        }
        return {
          focusedOccurrenceIndex: index,
          highlightedSpan: spans[index],
        };
      }),

    toggleNodeCollapse: (nodeId) =>
      set((state) => {
        const newCollapsedNodeIds = new Set(state.collapsedNodeIds);
        if (newCollapsedNodeIds.has(nodeId)) {
          newCollapsedNodeIds.delete(nodeId);
        } else {
          newCollapsedNodeIds.add(nodeId);
        }
        return { collapsedNodeIds: newCollapsedNodeIds };
      }),

    setNodeCollapsed: (nodeId, collapsed) =>
      set((state) => {
        const newCollapsedNodeIds = new Set(state.collapsedNodeIds);
        const isCollapsed = state.defaultCollapsed
          ? !newCollapsedNodeIds.has(nodeId)
          : newCollapsedNodeIds.has(nodeId);

        if (isCollapsed === collapsed) {
          return state;
        }

        if (state.defaultCollapsed) {
          if (collapsed) {
            newCollapsedNodeIds.delete(nodeId);
          } else {
            newCollapsedNodeIds.add(nodeId);
          }
        } else if (collapsed) {
          newCollapsedNodeIds.add(nodeId);
        } else {
          newCollapsedNodeIds.delete(nodeId);
        }

        return { collapsedNodeIds: newCollapsedNodeIds };
      }),

    toggleTableExpansion: (tableId) =>
      set((state) => {
        const newExpandedTableIds = new Set(state.expandedTableIds);
        if (newExpandedTableIds.has(tableId)) {
          newExpandedTableIds.delete(tableId);
        } else {
          newExpandedTableIds.add(tableId);
        }
        return { expandedTableIds: newExpandedTableIds };
      }),

    setTableExpanded: (tableId, expanded) =>
      set((state) => {
        const newExpandedTableIds = new Set(state.expandedTableIds);
        const isExpanded = newExpandedTableIds.has(tableId);

        if (isExpanded === expanded) {
          return state;
        }

        if (expanded) {
          newExpandedTableIds.add(tableId);
        } else {
          newExpandedTableIds.delete(tableId);
        }

        return { expandedTableIds: newExpandedTableIds };
      }),

    setAllNodesCollapsed: (collapsed) => {
      saveDefaultCollapsed(collapsed);
      set({
        defaultCollapsed: collapsed,
        collapsedNodeIds: new Set(),
      });
    },

    selectStatement: (index) =>
      set({
        selectedStatementIndex: index,
        selectedNodeId: null,
        highlightedSpan: null,
        focusedOccurrenceIndex: 0,
        collapsedNodeIds: new Set(),
      }),

    highlightSpan: (span) => set({ highlightedSpan: span }),

    setSearchTerm: (term) => set({ searchTerm: term }),

    setViewMode: (mode) => {
      saveViewMode(mode);
      set({ viewMode: mode });
    },

    setMatrixSubMode: (mode) => set({ matrixSubMode: mode }),

    setLayoutAlgorithm: (algorithm) => {
      saveLayoutAlgorithm(algorithm);
      set({ layoutAlgorithm: algorithm });
    },

    setLayoutMetrics: (metrics: LayoutMetrics) => set({ layoutMetrics: metrics }),

    setGraphMetrics: (metrics: GraphBuildMetrics) => set({ graphMetrics: metrics }),

    toggleColumnEdges: () =>
      set((state) => {
        const newValue = !state.showColumnEdges;
        saveColumnEdges(newValue);
        return { showColumnEdges: newValue };
      }),

    toggleHideCTEs: () =>
      set((state) => {
        const newValue = !state.hideCTEs;
        saveHideCTEs(newValue);
        return { hideCTEs: newValue };
      }),

    toggleShowScriptTables: () => set((state) => ({ showScriptTables: !state.showScriptTables })),

    requestNavigation: (request) => set({ navigationRequest: request }),

    setVisibleGraphNodeIds: (nodeIds) =>
      set((state) => {
        const next = new Set(nodeIds);
        if (next.size !== state.visibleGraphNodeIds.size) {
          return { visibleGraphNodeIds: next };
        }
        for (const id of next) {
          if (!state.visibleGraphNodeIds.has(id)) {
            return { visibleGraphNodeIds: next };
          }
        }
        return state;
      }),

    revealNodeInGraph: (nodeId) =>
      set((state) => ({
        selectedNodeId: nodeId,
        highlightedSpan: null,
        focusedOccurrenceIndex: 0,
        revealRequest: {
          nodeId,
          nonce: (state.revealRequest?.nonce ?? 0) + 1,
          suppressNavigation: true,
        },
      })),

    clearRevealRequest: () => set({ revealRequest: null }),

    setTableFilter: (filter) => set({ tableFilter: filter }),

    toggleTableFilterSelection: (tableLabel) =>
      set((state) => {
        const newSelectedTableLabels = new Set(state.tableFilter.selectedTableLabels);
        if (newSelectedTableLabels.has(tableLabel)) {
          newSelectedTableLabels.delete(tableLabel);
        } else {
          newSelectedTableLabels.add(tableLabel);
        }
        return {
          tableFilter: {
            ...state.tableFilter,
            selectedTableLabels: newSelectedTableLabels,
          },
        };
      }),

    setTableFilterDirection: (direction) =>
      set((state) => ({
        tableFilter: {
          ...state.tableFilter,
          direction,
        },
      })),

    clearTableFilter: () =>
      set({
        tableFilter: { selectedTableLabels: new Set(), direction: 'both' },
      }),

    setIsLayouting: (isLayouting) => set({ isLayouting }),

    setIsBuilding: (isBuilding) => set({ isBuilding }),

    setAnalyzedContent: (map) => set({ analyzedContentByPath: map }),

    setStalePaths: (paths) =>
      set((state) => {
        const next = new Set(paths);
        if (next.size !== state.stalePaths.size) {
          return { stalePaths: next };
        }
        for (const path of next) {
          if (!state.stalePaths.has(path)) {
            return { stalePaths: next };
          }
        }
        return state;
      }),
  }));
}

const LineageStoreContext = createContext<StoreApi<LineageState> | null>(null);

interface LineageStoreProviderProps {
  store: StoreApi<LineageState>;
  children: ReactNode;
}

export function LineageStoreProvider({ store, children }: LineageStoreProviderProps) {
  return createElement(LineageStoreContext.Provider, { value: store, children });
}

export function useLineageStore(): LineageState;
export function useLineageStore<T>(selector: (state: LineageState) => T): T;
export function useLineageStore<T>(selector?: (state: LineageState) => T) {
  const store = useContext(LineageStoreContext);
  if (!store) {
    throw new Error('useLineageStore must be used within a LineageProvider');
  }

  if (selector) {
    return useStore(store, selector);
  }
  return useStore(store);
}

function selectLineageState(store: LineageState): LineageContextState {
  return {
    result: store.result,
    sql: store.sql,
    selectedNodeId: store.selectedNodeId,
    selectedStatementIndex: store.selectedStatementIndex,
    highlightedSpan: store.highlightedSpan,
    focusedOccurrenceIndex: store.focusedOccurrenceIndex,
    searchTerm: store.searchTerm,
    viewMode: store.viewMode,
    matrixSubMode: store.matrixSubMode,
    layoutAlgorithm: store.layoutAlgorithm,
    layoutMetrics: store.layoutMetrics,
    graphMetrics: store.graphMetrics,
    collapsedNodeIds: store.collapsedNodeIds,
    expandedTableIds: store.expandedTableIds,
    defaultCollapsed: store.defaultCollapsed,
    showColumnEdges: store.showColumnEdges,
    hideCTEs: store.hideCTEs,
    showScriptTables: store.showScriptTables,
    navigationRequest: store.navigationRequest,
    visibleGraphNodeIds: store.visibleGraphNodeIds,
    revealRequest: store.revealRequest,
    tableFilter: store.tableFilter,
    isLayouting: store.isLayouting,
    isBuilding: store.isBuilding,
    analyzedContentByPath: store.analyzedContentByPath,
    stalePaths: store.stalePaths,
  };
}

function selectLineageActions(store: LineageState): LineageContextActions {
  return {
    setResult: store.setResult,
    setSql: store.setSql,
    selectNode: store.selectNode,
    cycleOccurrence: store.cycleOccurrence,
    focusOccurrence: store.focusOccurrence,
    toggleNodeCollapse: store.toggleNodeCollapse,
    setNodeCollapsed: store.setNodeCollapsed,
    toggleTableExpansion: store.toggleTableExpansion,
    setTableExpanded: store.setTableExpanded,
    setAllNodesCollapsed: store.setAllNodesCollapsed,
    selectStatement: store.selectStatement,
    highlightSpan: store.highlightSpan,
    setSearchTerm: store.setSearchTerm,
    setViewMode: store.setViewMode,
    setMatrixSubMode: store.setMatrixSubMode,
    setLayoutAlgorithm: store.setLayoutAlgorithm,
    setLayoutMetrics: store.setLayoutMetrics,
    setGraphMetrics: store.setGraphMetrics,
    toggleColumnEdges: store.toggleColumnEdges,
    toggleHideCTEs: store.toggleHideCTEs,
    toggleShowScriptTables: store.toggleShowScriptTables,
    requestNavigation: store.requestNavigation,
    setVisibleGraphNodeIds: store.setVisibleGraphNodeIds,
    revealNodeInGraph: store.revealNodeInGraph,
    clearRevealRequest: store.clearRevealRequest,
    setTableFilter: store.setTableFilter,
    toggleTableFilterSelection: store.toggleTableFilterSelection,
    setTableFilterDirection: store.setTableFilterDirection,
    clearTableFilter: store.clearTableFilter,
    setIsLayouting: store.setIsLayouting,
    setIsBuilding: store.setIsBuilding,
    setAnalyzedContent: store.setAnalyzedContent,
    setStalePaths: store.setStalePaths,
  };
}

/**
 * Hook to access lineage state. Pass a selector to subscribe only to the
 * fields a component consumes. The no-argument form preserves the legacy
 * state object API and keeps its reference stable until a state field changes.
 */
export function useLineageState(): LineageContextState;
export function useLineageState<T>(selector: (state: LineageContextState) => T): T;
export function useLineageState<T>(
  selector?: (state: LineageContextState) => T
): LineageContextState | T {
  const activeSelector = useShallow((store: LineageState): LineageContextState | T =>
    selector ? selector(store) : selectLineageState(store)
  );

  return useLineageStore<LineageContextState | T>(activeSelector);
}

/**
 * Hook to access lineage actions. Action references and the no-argument action
 * object remain stable for the lifetime of the store. A selector can narrow
 * the returned API for components that use only a subset of actions.
 */
export function useLineageActions(): LineageContextActions;
export function useLineageActions<T>(selector: (actions: LineageContextActions) => T): T;
export function useLineageActions<T>(
  selector?: (actions: LineageContextActions) => T
): LineageContextActions | T {
  const activeSelector = useShallow((store: LineageState): LineageContextActions | T => {
    const actions = selectLineageActions(store);
    return selector ? selector(actions) : actions;
  });

  return useLineageStore<LineageContextActions | T>(activeSelector);
}

/**
 * Hook to access the legacy structured state/actions value. Both nested
 * objects are referentially stable until their selected store fields change.
 */
export function useLineage(): LineageContextValue {
  const state = useLineageState();
  const actions = useLineageActions();

  return useMemo(() => ({ state, actions }), [state, actions]);
}
