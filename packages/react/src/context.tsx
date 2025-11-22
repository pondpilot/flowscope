import {
  createContext,
  useContext,
  useState,
  useCallback,
  useMemo,
  type ReactNode,
} from 'react';
import type { AnalyzeResult, Span } from '@pondpilot/flowscope-core';
import type {
  LineageContextValue,
  LineageState,
  LineageActions,
  LineageViewMode,
} from './types';

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

const LineageContext = createContext<LineageContextValue | null>(null);

/**
 * Props for the LineageProvider component.
 */
export interface LineageProviderProps {
  /** Child components to render within the provider */
  children: ReactNode;
  /** Initial analysis result to populate the context with */
  initialResult?: AnalyzeResult | null;
  /** Initial SQL text to populate the context with */
  initialSql?: string;
}

/**
 * Context provider for SQL lineage analysis state and actions.
 * Manages global state for lineage visualization including result data,
 * node selection, highlighting, and search functionality.
 *
 * @example
 * ```tsx
 * <LineageProvider initialResult={result} initialSql={sqlText}>
 *   <YourComponents />
 * </LineageProvider>
 * ```
 */
export function LineageProvider({
  children,
  initialResult = null,
  initialSql = '',
}: LineageProviderProps): JSX.Element {
  const [result, setResult] = useState<AnalyzeResult | null>(initialResult);
  const [sql, setSql] = useState(initialSql);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedStatementIndex, setSelectedStatementIndex] = useState(0);
  const [highlightedSpan, setHighlightedSpan] = useState<Span | null>(null);
  const [searchTerm, setSearchTerm] = useState('');
  const [viewMode, setViewModeState] = useState<LineageViewMode>(() => loadViewMode());
  const [collapsedNodeIds, setCollapsedNodeIds] = useState<Set<string>>(new Set());

  const updateResult = useCallback(
    (nextResult: AnalyzeResult | null) => {
      setResult(nextResult);
      setSelectedNodeId(null);
      setHighlightedSpan(null);
      setCollapsedNodeIds(new Set());
      setSelectedStatementIndex((previousIndex) => {
        const statementCount = nextResult?.statements.length ?? 0;
        if (statementCount === 0) {
          return 0;
        }
        const maxIndex = statementCount - 1;
        return Math.max(0, Math.min(previousIndex, maxIndex));
      });
    },
    []
  );

  const selectNode = useCallback((nodeId: string | null) => {
    setSelectedNodeId(nodeId);
    if (nodeId === null) {
      setHighlightedSpan(null);
    }
  }, []);

  const toggleNodeCollapse = useCallback((nodeId: string) => {
    setCollapsedNodeIds((prev) => {
      const next = new Set(prev);
      if (next.has(nodeId)) {
        next.delete(nodeId);
      } else {
        next.add(nodeId);
      }
      return next;
    });
  }, []);

  const selectStatement = useCallback((index: number) => {
    setSelectedStatementIndex(index);
    setSelectedNodeId(null);
    setHighlightedSpan(null);
    setCollapsedNodeIds(new Set());
  }, []);

  const highlightSpan = useCallback((span: Span | null) => {
    setHighlightedSpan(span);
  }, []);

  const setViewMode = useCallback((mode: LineageViewMode) => {
    setViewModeState(mode);
    saveViewMode(mode);
  }, []);

  const state: LineageState = useMemo(
    () => ({
      result,
      sql,
      selectedNodeId,
      selectedStatementIndex,
      highlightedSpan,
      searchTerm,
      viewMode,
      collapsedNodeIds,
    }),
    [result, sql, selectedNodeId, selectedStatementIndex, highlightedSpan, searchTerm, viewMode, collapsedNodeIds]
  );

  const actions: LineageActions = useMemo(
    () => ({
      setResult: updateResult,
      setSql,
      selectNode,
      toggleNodeCollapse,
      selectStatement,
      highlightSpan,
      setSearchTerm,
      setViewMode,
    }),
    [updateResult, setSql, selectNode, toggleNodeCollapse, selectStatement, highlightSpan, setSearchTerm, setViewMode]
  );

  const value = useMemo(() => ({ state, actions }), [state, actions]);

  return <LineageContext.Provider value={value}>{children}</LineageContext.Provider>;
}

/**
 * Hook to access the full lineage context including state and actions.
 * Must be used within a LineageProvider.
 *
 * @returns The lineage context value containing state and actions
 * @throws Error if used outside of a LineageProvider
 *
 * @example
 * ```tsx
 * const { state, actions } = useLineage();
 * actions.setSearchTerm('users');
 * console.log(state.searchTerm);
 * ```
 */
export function useLineage(): LineageContextValue {
  const context = useContext(LineageContext);
  if (!context) {
    throw new Error('useLineage must be used within a LineageProvider');
  }
  return context;
}

/**
 * Hook to access only the lineage state.
 * Convenience hook for components that only need to read state.
 *
 * @returns The current lineage state
 * @throws Error if used outside of a LineageProvider
 */
export function useLineageState(): LineageState {
  return useLineage().state;
}

/**
 * Hook to access only the lineage actions.
 * Convenience hook for components that only need to dispatch actions.
 *
 * @returns The lineage actions object
 * @throws Error if used outside of a LineageProvider
 */
export function useLineageActions(): LineageActions {
  return useLineage().actions;
}
