import { createContext, createElement, useContext, type ReactNode } from 'react';
import { useStore } from 'zustand';
import { createStore, type StoreApi } from 'zustand/vanilla';
import type { AnalyzeResult, Span } from '@pondpilot/flowscope-core';
import type { LineageViewMode, LayoutAlgorithm, NavigationRequest, MatrixSubMode, TableFilterDirection, TableFilter } from './types';

const DEFAULT_LAYOUT_ALGORITHM: LayoutAlgorithm = 'dagre';

// Storage keys
const STORAGE_KEYS = {
  viewMode: 'flowscope-view-mode',
  layoutAlgorithm: 'flowscope-layout-algorithm',
  defaultCollapsed: 'flowscope-default-collapsed',
  columnEdges: 'flowscope-column-edges',
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

function loadLayoutAlgorithm(defaultAlgorithm: LayoutAlgorithm = DEFAULT_LAYOUT_ALGORITHM): LayoutAlgorithm {
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

export interface LineageState {
  // Data
  result: AnalyzeResult | null;
  sql: string;

  // Selection & UI state
  selectedNodeId: string | null;
  selectedStatementIndex: number;
  highlightedSpan: Span | null;
  searchTerm: string;
  viewMode: LineageViewMode;
  matrixSubMode: MatrixSubMode;
  layoutAlgorithm: LayoutAlgorithm;
  // Node IDs whose collapsed state differs from defaultCollapsed.
  // When defaultCollapsed is true, these are expanded nodes (overrides).
  // When defaultCollapsed is false, these are collapsed nodes (overrides).
  collapsedNodeIds: Set<string>;
  expandedTableIds: Set<string>; // Tables with all columns shown
  defaultCollapsed: boolean; // Whether tables are collapsed by default
  showColumnEdges: boolean; // Whether to show column-level edges instead of table-level
  showScriptTables: boolean;
  navigationRequest: NavigationRequest | null;
  tableFilter: TableFilter;

  // Actions
  setResult: (result: AnalyzeResult | null) => void;
  setSql: (sql: string) => void;
  selectNode: (nodeId: string | null) => void;
  toggleNodeCollapse: (nodeId: string) => void;
  toggleTableExpansion: (tableId: string) => void;
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
  toggleColumnEdges: () => void;
  toggleShowScriptTables: () => void;
  requestNavigation: (request: NavigationRequest | null) => void;
  setTableFilter: (filter: TableFilter) => void;
  toggleTableFilterSelection: (tableLabel: string) => void;
  setTableFilterDirection: (direction: TableFilterDirection) => void;
  clearTableFilter: () => void;
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

  return createStore<LineageState>((set) => ({
    // Initial state
    result: null,
    sql: '',
    selectedNodeId: null,
    selectedStatementIndex: 0,
    highlightedSpan: null,
    searchTerm: '',
    viewMode: initialViewMode,
    matrixSubMode: 'tables',
    layoutAlgorithm: initialLayoutAlgorithm,
    collapsedNodeIds: new Set(),
    expandedTableIds: new Set(),
    defaultCollapsed: initialDefaultCollapsed,
    showColumnEdges: initialColumnEdges,
    showScriptTables: false,
    navigationRequest: null,
    tableFilter: { selectedTableLabels: new Set(), direction: 'both' },
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
          collapsedNodeIds: new Set(),
          expandedTableIds: new Set(),
          selectedStatementIndex: statementCount === 0 ? 0 : newSelectedStatementIndex,
        };
      }),

    setSql: (sql) => set({ sql }),

    selectNode: (nodeId) =>
      set({
        selectedNodeId: nodeId,
        highlightedSpan: nodeId === null ? null : undefined,
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

    toggleColumnEdges: () =>
      set((state) => {
        const newValue = !state.showColumnEdges;
        saveColumnEdges(newValue);
        return { showColumnEdges: newValue };
      }),

    toggleShowScriptTables: () => set((state) => ({ showScriptTables: !state.showScriptTables })),

    requestNavigation: (request) => set({ navigationRequest: request }),

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

/**
 * Hook to access the full lineage store.
 * Compatible with the previous useLineage API for easier migration.
 */
export function useLineage() {
  const store = useLineageStore();

  return {
    state: {
      result: store.result,
      sql: store.sql,
      selectedNodeId: store.selectedNodeId,
      selectedStatementIndex: store.selectedStatementIndex,
      highlightedSpan: store.highlightedSpan,
      searchTerm: store.searchTerm,
      viewMode: store.viewMode,
      matrixSubMode: store.matrixSubMode,
      layoutAlgorithm: store.layoutAlgorithm,
      collapsedNodeIds: store.collapsedNodeIds,
      expandedTableIds: store.expandedTableIds,
      defaultCollapsed: store.defaultCollapsed,
      showColumnEdges: store.showColumnEdges,
      showScriptTables: store.showScriptTables,
      navigationRequest: store.navigationRequest,
      tableFilter: store.tableFilter,
    },
    actions: {
      setResult: store.setResult,
      setSql: store.setSql,
      selectNode: store.selectNode,
      toggleNodeCollapse: store.toggleNodeCollapse,
      toggleTableExpansion: store.toggleTableExpansion,
      setAllNodesCollapsed: store.setAllNodesCollapsed,
      selectStatement: store.selectStatement,
      highlightSpan: store.highlightSpan,
      setSearchTerm: store.setSearchTerm,
      setViewMode: store.setViewMode,
      setMatrixSubMode: store.setMatrixSubMode,
      setLayoutAlgorithm: store.setLayoutAlgorithm,
      toggleColumnEdges: store.toggleColumnEdges,
      toggleShowScriptTables: store.toggleShowScriptTables,
      requestNavigation: store.requestNavigation,
      setTableFilter: store.setTableFilter,
      toggleTableFilterSelection: store.toggleTableFilterSelection,
      setTableFilterDirection: store.setTableFilterDirection,
      clearTableFilter: store.clearTableFilter,
    },
  };
}

/**
 * Hook to access only the lineage state.
 * Note: This returns the store directly to avoid re-render issues.
 * Access individual properties as needed.
 */
export function useLineageState() {
  const result = useLineageStore((state) => state.result);
  const sql = useLineageStore((state) => state.sql);
  const selectedNodeId = useLineageStore((state) => state.selectedNodeId);
  const selectedStatementIndex = useLineageStore((state) => state.selectedStatementIndex);
  const highlightedSpan = useLineageStore((state) => state.highlightedSpan);
  const searchTerm = useLineageStore((state) => state.searchTerm);
  const viewMode = useLineageStore((state) => state.viewMode);
  const matrixSubMode = useLineageStore((state) => state.matrixSubMode);
  const layoutAlgorithm = useLineageStore((state) => state.layoutAlgorithm);
  const collapsedNodeIds = useLineageStore((state) => state.collapsedNodeIds);
  const expandedTableIds = useLineageStore((state) => state.expandedTableIds);
  const defaultCollapsed = useLineageStore((state) => state.defaultCollapsed);
  const showColumnEdges = useLineageStore((state) => state.showColumnEdges);
  const showScriptTables = useLineageStore((state) => state.showScriptTables);
  const navigationRequest = useLineageStore((state) => state.navigationRequest);
  const tableFilter = useLineageStore((state) => state.tableFilter);

  return {
    result,
    sql,
    selectedNodeId,
    selectedStatementIndex,
    highlightedSpan,
    searchTerm,
    viewMode,
    matrixSubMode,
    layoutAlgorithm,
    collapsedNodeIds,
    expandedTableIds,
    defaultCollapsed,
    showColumnEdges,
    showScriptTables,
    navigationRequest,
    tableFilter,
  };
}

/**
 * Hook to access only the lineage actions.
 */
export function useLineageActions() {
  const setResult = useLineageStore((state) => state.setResult);
  const setSql = useLineageStore((state) => state.setSql);
  const selectNode = useLineageStore((state) => state.selectNode);
  const toggleNodeCollapse = useLineageStore((state) => state.toggleNodeCollapse);
  const toggleTableExpansion = useLineageStore((state) => state.toggleTableExpansion);
  const setAllNodesCollapsed = useLineageStore((state) => state.setAllNodesCollapsed);
  const selectStatement = useLineageStore((state) => state.selectStatement);
  const highlightSpan = useLineageStore((state) => state.highlightSpan);
  const setSearchTerm = useLineageStore((state) => state.setSearchTerm);
  const setViewMode = useLineageStore((state) => state.setViewMode);
  const setMatrixSubMode = useLineageStore((state) => state.setMatrixSubMode);
  const setLayoutAlgorithm = useLineageStore((state) => state.setLayoutAlgorithm);
  const toggleColumnEdges = useLineageStore((state) => state.toggleColumnEdges);
  const toggleShowScriptTables = useLineageStore((state) => state.toggleShowScriptTables);
  const requestNavigation = useLineageStore((state) => state.requestNavigation);
  const setTableFilter = useLineageStore((state) => state.setTableFilter);
  const toggleTableFilterSelection = useLineageStore((state) => state.toggleTableFilterSelection);
  const setTableFilterDirection = useLineageStore((state) => state.setTableFilterDirection);
  const clearTableFilter = useLineageStore((state) => state.clearTableFilter);

  return {
    setResult,
    setSql,
    selectNode,
    toggleNodeCollapse,
    toggleTableExpansion,
    setAllNodesCollapsed,
    selectStatement,
    highlightSpan,
    setSearchTerm,
    setViewMode,
    setMatrixSubMode,
    setLayoutAlgorithm,
    toggleColumnEdges,
    toggleShowScriptTables,
    requestNavigation,
    setTableFilter,
    toggleTableFilterSelection,
    setTableFilterDirection,
    clearTableFilter,
  };
}
