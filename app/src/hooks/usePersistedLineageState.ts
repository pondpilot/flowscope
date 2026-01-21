/**
 * Hook for managing persisted LineageView (GraphView) state.
 *
 * Provides controlled state for GraphView that syncs with the view state store,
 * automatically persisting changes with debouncing.
 */

import { useEffect, useMemo, useCallback, useRef } from 'react';
import {
  useViewStateStore,
  getLineageStateWithDefaults,
  type LineageViewState,
  type ViewportState,
} from '@/lib/view-state-store';

const PERSIST_DEBOUNCE_MS = 300;

interface UsePersistedLineageStateResult {
  /** Controlled search term to pass to GraphView */
  searchTerm: string;
  /** Callback for GraphView search term changes */
  onSearchTermChange: (searchTerm: string) => void;
  /** Initial viewport to restore on mount */
  initialViewport: ViewportState | undefined;
  /** Callback for GraphView viewport changes */
  onViewportChange: (viewport: ViewportState) => void;
}

export function usePersistedLineageState(projectId: string | null): UsePersistedLineageStateResult {
  const storedState = useViewStateStore((s) =>
    projectId ? s.getViewState(projectId, 'lineage') : undefined
  );
  const updateViewState = useViewStateStore((s) => s.updateViewState);

  // Get initial state with defaults (only on mount, not on every storedState change)
  const initialState = useMemo(() => getLineageStateWithDefaults(storedState), []);

  // Track current state for persistence
  const currentStateRef = useRef<LineageViewState>(initialState);

  // Get the effective search term from stored state
  const searchTerm = useMemo(() => {
    const stored = storedState ?? {};
    return getLineageStateWithDefaults(stored).searchTerm;
  }, [storedState]);

  // Get the initial viewport from stored state (computed once on mount or project change)
  const initialViewport = useMemo(() => {
    if (!projectId) return undefined;
    const stored = useViewStateStore.getState().getViewState(projectId, 'lineage');
    return getLineageStateWithDefaults(stored).viewport ?? undefined;
    // Re-compute when project changes
  }, [projectId]);

  // Debounced persist timer
  const persistTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Persist state helper
  const persistState = useCallback(() => {
    if (!projectId) return;

    if (persistTimerRef.current) {
      clearTimeout(persistTimerRef.current);
    }

    persistTimerRef.current = setTimeout(() => {
      updateViewState(projectId, 'lineage', currentStateRef.current);
    }, PERSIST_DEBOUNCE_MS);
  }, [projectId, updateViewState]);

  // Handle search term changes from GraphView
  const onSearchTermChange = useCallback(
    (newSearchTerm: string) => {
      if (!projectId) return;

      currentStateRef.current = {
        ...currentStateRef.current,
        searchTerm: newSearchTerm,
      };

      persistState();
    },
    [projectId, persistState]
  );

  // Handle viewport changes from GraphView
  const onViewportChange = useCallback(
    (viewport: ViewportState) => {
      if (!projectId) return;

      currentStateRef.current = {
        ...currentStateRef.current,
        viewport,
      };

      persistState();
    },
    [projectId, persistState]
  );

  // Cleanup timer on unmount
  useEffect(() => {
    return () => {
      if (persistTimerRef.current) {
        clearTimeout(persistTimerRef.current);
      }
    };
  }, []);

  // Reset/sync state when project or stored state changes
  useEffect(() => {
    if (!projectId) return;

    const newState = getLineageStateWithDefaults(storedState);
    currentStateRef.current = newState;
  }, [projectId, storedState]);

  return {
    searchTerm,
    onSearchTermChange,
    initialViewport,
    onViewportChange,
  };
}
