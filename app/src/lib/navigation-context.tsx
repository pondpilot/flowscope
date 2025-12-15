import { createContext, useContext, useState, useCallback, useMemo, useRef, useEffect, type ReactNode } from 'react';
import { useViewStateStore } from './view-state-store';

export type AnalysisTab = 'lineage' | 'hierarchy' | 'matrix' | 'schema' | 'tags' | 'issues';

const VALID_TABS: readonly AnalysisTab[] = ['lineage', 'hierarchy', 'matrix', 'schema', 'tags', 'issues'];

export function isValidTab(tab: string): tab is AnalysisTab {
  return VALID_TABS.includes(tab as AnalysisTab);
}

export interface NavigationTarget {
  /** Table ID to focus/highlight in the target view */
  tableId?: string;
  /** Table name (for views that use names instead of IDs) */
  tableName?: string;
  /** Statement index to navigate to */
  statementIndex?: number;
  /** Source file name to open in editor */
  sourceName?: string;
  /** Span to highlight in editor */
  span?: { start: number; end: number };
  /** Whether to fit the view to show all nodes */
  fitView?: boolean;
}

interface NavigationContextValue {
  /** Currently active tab */
  activeTab: AnalysisTab;
  /** Set the active tab */
  setActiveTab: (tab: AnalysisTab) => void;
  /** Navigation target for the current tab */
  navigationTarget: NavigationTarget | null;
  /** Navigate to a specific tab with optional target */
  navigateTo: (tab: AnalysisTab, target?: NavigationTarget) => void;
  /** Navigate to editor and highlight a span */
  navigateToEditor: (sourceName: string, span?: { start: number; end: number }) => void;
  /** Clear the navigation target (after consuming it) */
  clearNavigationTarget: () => void;
}

const NavigationContext = createContext<NavigationContextValue | null>(null);

interface NavigationProviderProps {
  children: ReactNode;
  /** Current project ID for persisting active tab */
  projectId: string | null;
  /** Callback to navigate to a file in the editor */
  onNavigateToEditor?: (sourceName: string, span?: { start: number; end: number }) => void;
}

export function NavigationProvider({ children, projectId, onNavigateToEditor }: NavigationProviderProps) {
  // Get persisted active tab from view state store
  const persistedActiveTab = useViewStateStore((s) =>
    projectId ? s.getActiveTab(projectId) : 'lineage'
  );
  const setPersistedActiveTab = useViewStateStore((s) => s.setActiveTab);

  const [activeTab, setActiveTabLocal] = useState<AnalysisTab>(persistedActiveTab);
  const [navigationTarget, setNavigationTarget] = useState<NavigationTarget | null>(null);

  // Sync active tab with persisted state when project changes
  useEffect(() => {
    if (projectId) {
      const stored = useViewStateStore.getState().getActiveTab(projectId);
      setActiveTabLocal(stored);
    }
  }, [projectId]);

  // Wrapper to persist tab changes
  const setActiveTab = useCallback((tab: AnalysisTab) => {
    setActiveTabLocal(tab);
    if (projectId) {
      setPersistedActiveTab(projectId, tab);
    }
  }, [projectId, setPersistedActiveTab]);

  // Use ref for the callback to avoid recreating navigateToEditor on prop changes
  const onNavigateToEditorRef = useRef(onNavigateToEditor);
  useEffect(() => {
    onNavigateToEditorRef.current = onNavigateToEditor;
  }, [onNavigateToEditor]);

  const navigateTo = useCallback((tab: AnalysisTab, target?: NavigationTarget) => {
    if (!isValidTab(tab)) {
      console.warn(`Invalid navigation tab: "${tab}". Valid tabs are: ${VALID_TABS.join(', ')}`);
      return;
    }

    if (target?.tableId && typeof target.tableId !== 'string') {
      console.warn('Invalid navigation target: tableId must be a string');
      return;
    }

    setActiveTab(tab);
    setNavigationTarget(target || null);
  }, [setActiveTab]);

  const navigateToEditor = useCallback((sourceName: string, span?: { start: number; end: number }) => {
    onNavigateToEditorRef.current?.(sourceName, span);
  }, []);

  const clearNavigationTarget = useCallback(() => {
    setNavigationTarget(null);
  }, []);

  const value = useMemo(() => ({
    activeTab,
    setActiveTab,
    navigationTarget,
    navigateTo,
    navigateToEditor,
    clearNavigationTarget,
  }), [activeTab, navigationTarget, navigateTo, navigateToEditor, clearNavigationTarget]);

  return (
    <NavigationContext.Provider value={value}>
      {children}
    </NavigationContext.Provider>
  );
}

export function useNavigation() {
  const context = useContext(NavigationContext);
  if (!context) {
    throw new Error('useNavigation must be used within a NavigationProvider');
  }
  return context;
}
