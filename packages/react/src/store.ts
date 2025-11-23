import { create } from 'zustand';
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
  showScriptTables: boolean;
  navigationRequest: NavigationRequest | null;

  // Actions
  setResult: (result: AnalyzeResult | null) => void;
  setSql: (sql: string) => void;
  selectNode: (nodeId: string | null) => void;
  toggleNodeCollapse: (nodeId: string) => void;
  selectStatement: (index: number) => void;
  highlightSpan: (span: Span | null) => void;
  setSearchTerm: (term: string) => void;
  setViewMode: (mode: LineageViewMode) => void;
  toggleShowScriptTables: () => void;
  requestNavigation: (request: NavigationRequest | null) => void;
}

/**
 * Zustand store for SQL lineage analysis state.
 * Replaces the previous React Context implementation with better performance
 * and simpler API.
 */
export const useLineageStore = create<LineageState>((set) => ({
  // Initial state
  result: null,
  sql: '',
  selectedNodeId: null,
  selectedStatementIndex: 0,
  highlightedSpan: null,
  searchTerm: '',
  viewMode: loadViewMode(),
  collapsedNodeIds: new Set(),
  showScriptTables: false,
  navigationRequest: null,

  // Actions
  setResult: (result) =>
    set((state) => {
      const statementCount = result?.statements.length ?? 0;
      const maxIndex = Math.max(0, statementCount - 1);
      const newSelectedStatementIndex = Math.max(0, Math.min(state.selectedStatementIndex, maxIndex));

      return {
        result,
        selectedNodeId: null,
        highlightedSpan: null,
        collapsedNodeIds: new Set(),
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
      showScriptTables: store.showScriptTables,
      navigationRequest: store.navigationRequest,
    },
    actions: {
      setResult: store.setResult,
      setSql: store.setSql,
      selectNode: store.selectNode,
      toggleNodeCollapse: store.toggleNodeCollapse,
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
    selectStatement,
    highlightSpan,
    setSearchTerm,
    setViewMode,
    toggleShowScriptTables,
    requestNavigation,
  };
}
