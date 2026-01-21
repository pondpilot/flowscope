/**
 * Hook for managing persisted MatrixView state.
 *
 * Provides controlled state for MatrixView that syncs with the view state store,
 * automatically persisting changes with debouncing.
 */

import { useEffect, useMemo, useCallback, useRef } from 'react';
import {
  useViewStateStore,
  getMatrixStateWithDefaults,
  type MatrixViewState,
} from '@/lib/view-state-store';
import type { MatrixViewControlledState } from '@pondpilot/flowscope-react';

const PERSIST_DEBOUNCE_MS = 300;

interface UsePersistedMatrixStateResult {
  /** Controlled state to pass to MatrixView */
  controlledState: Partial<MatrixViewControlledState>;
  /** Callback for MatrixView state changes */
  onStateChange: (state: Partial<MatrixViewControlledState>) => void;
}

export function usePersistedMatrixState(projectId: string | null): UsePersistedMatrixStateResult {
  const storedState = useViewStateStore((s) =>
    projectId ? s.getViewState(projectId, 'matrix') : undefined
  );
  const updateViewState = useViewStateStore((s) => s.updateViewState);

  // Get initial state with defaults (only on mount, not on every storedState change)
  const initialState = useMemo(() => getMatrixStateWithDefaults(storedState), []);

  // Track current state for persistence
  const currentStateRef = useRef<MatrixViewState>(initialState);

  // Track the actual controlled state - starts from stored/default
  const controlledState = useMemo((): Partial<MatrixViewControlledState> => {
    const stored = storedState ?? {};
    const defaults = getMatrixStateWithDefaults(stored);
    return {
      filterText: defaults.filterText,
      filterMode: defaults.filterMode,
      heatmapMode: defaults.heatmapMode,
      xRayMode: defaults.xRayMode,
      xRayFilterMode: defaults.xRayFilterMode,
      clusterMode: defaults.clusterMode,
      complexityMode: defaults.complexityMode,
      showLegend: defaults.showLegend,
      focusedNode: defaults.focusedNode,
      firstColumnWidth: defaults.firstColumnWidth,
      headerHeight: defaults.headerHeight,
    };
  }, [storedState]);

  // Debounced persist timer
  const persistTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Handle state changes from MatrixView
  const onStateChange = useCallback(
    (newState: Partial<MatrixViewControlledState>) => {
      if (!projectId) return;

      // Update current state ref
      currentStateRef.current = {
        ...currentStateRef.current,
        ...newState,
      };

      // Debounce persistence
      if (persistTimerRef.current) {
        clearTimeout(persistTimerRef.current);
      }

      persistTimerRef.current = setTimeout(() => {
        updateViewState(projectId, 'matrix', currentStateRef.current);
      }, PERSIST_DEBOUNCE_MS);
    },
    [projectId, updateViewState]
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

    const newState = getMatrixStateWithDefaults(storedState);
    currentStateRef.current = newState;
  }, [projectId, storedState]);

  return {
    controlledState,
    onStateChange,
  };
}
