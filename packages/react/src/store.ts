import { createContext, createElement, useContext, type ReactNode } from 'react';
import { useStore } from 'zustand';
import { createStore, type StoreApi } from 'zustand/vanilla';
import type { AnalyzeResult, Span } from '@pondpilot/flowscope-core';
import type { LineageViewMode, NavigationRequest } from './types';

const VIEW_MODE_STORAGE_KEY = 'flowscope-view-mode';

/**
 * Load the view mode from localStorage, defaulting to 'table' if not found or invalid.
 */
function loadViewMode(): LineageViewMode {
  try {
    const stored = localStorage.getItem(VIEW_MODE_STORAGE_KEY);
    if (stored === 'script' || stored === 'table' || stored === 'column') {
      return stored;
    }
  } catch {
    // localStorage might not be available (SSR, etc.)
  }
  return 'table';
}

/**
 * Save the view mode to localStorage.
 */
function saveViewMode(mode: LineageViewMode): void {
  try {
    localStorage.setItem(VIEW_MODE_STORAGE_KEY, mode);
  } catch {
    // localStorage might not be available
  }
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
  collapsedNodeIds: Set<string>;
  expandedTableIds: Set<string>; // Tables with all columns shown
  showScriptTables: boolean;
  navigationRequest: NavigationRequest | null;

  // Actions
  setResult: (result: AnalyzeResult | null) => void;
  setSql: (sql: string) => void;
  selectNode: (nodeId: string | null) => void;
  toggleNodeCollapse: (nodeId: string) => void;
  toggleTableExpansion: (tableId: string) => void;
  selectStatement: (index: number) => void;
  highlightSpan: (span: Span | null) => void;
  setSearchTerm: (term: string) => void;
  setViewMode: (mode: LineageViewMode) => void;
  toggleShowScriptTables: () => void;
  requestNavigation: (request: NavigationRequest | null) => void;
}

/**
 * Build a new, isolated Zustand store instance.
 */
export function createLineageStore(initialState?: Partial<LineageState>): StoreApi<LineageState> {
  const initialViewMode = initialState?.viewMode ?? loadViewMode();

  return createStore<LineageState>((set) => ({
    // Initial state
    result: null,
    sql: '',
    selectedNodeId: null,
    selectedStatementIndex: 0,
    highlightedSpan: null,
    searchTerm: '',
    viewMode: initialViewMode,
    collapsedNodeIds: new Set(),
    expandedTableIds: new Set(),
    showScriptTables: false,
    navigationRequest: null,
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

    toggleShowScriptTables: () => set((state) => ({ showScriptTables: !state.showScriptTables })),

    requestNavigation: (request) => set({ navigationRequest: request }),
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
      collapsedNodeIds: store.collapsedNodeIds,
      expandedTableIds: store.expandedTableIds,
      showScriptTables: store.showScriptTables,
      navigationRequest: store.navigationRequest,
    },
    actions: {
      setResult: store.setResult,
      setSql: store.setSql,
      selectNode: store.selectNode,
      toggleNodeCollapse: store.toggleNodeCollapse,
      toggleTableExpansion: store.toggleTableExpansion,
      selectStatement: store.selectStatement,
      highlightSpan: store.highlightSpan,
      setSearchTerm: store.setSearchTerm,
      setViewMode: store.setViewMode,
      toggleShowScriptTables: store.toggleShowScriptTables,
      requestNavigation: store.requestNavigation,
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
  const collapsedNodeIds = useLineageStore((state) => state.collapsedNodeIds);
  const expandedTableIds = useLineageStore((state) => state.expandedTableIds);
  const showScriptTables = useLineageStore((state) => state.showScriptTables);
  const navigationRequest = useLineageStore((state) => state.navigationRequest);

  return {
    result,
    sql,
    selectedNodeId,
    selectedStatementIndex,
    highlightedSpan,
    searchTerm,
    viewMode,
    collapsedNodeIds,
    expandedTableIds,
    showScriptTables,
    navigationRequest,
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
  const selectStatement = useLineageStore((state) => state.selectStatement);
  const highlightSpan = useLineageStore((state) => state.highlightSpan);
  const setSearchTerm = useLineageStore((state) => state.setSearchTerm);
  const setViewMode = useLineageStore((state) => state.setViewMode);
  const toggleShowScriptTables = useLineageStore((state) => state.toggleShowScriptTables);
  const requestNavigation = useLineageStore((state) => state.requestNavigation);

  return {
    setResult,
    setSql,
    selectNode,
    toggleNodeCollapse,
    toggleTableExpansion,
    selectStatement,
    highlightSpan,
    setSearchTerm,
    setViewMode,
    toggleShowScriptTables,
    requestNavigation,
  };
}
