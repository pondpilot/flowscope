/**
 * Hook for managing persisted SchemaView state.
 *
 * Provides selected table name state that syncs with the view state store,
 * automatically persisting changes with debouncing.
 */

import { useEffect, useMemo, useCallback, useRef } from 'react';
import {
  useViewStateStore,
  getSchemaStateWithDefaults,
  type SchemaViewState,
} from '@/lib/view-state-store';

const PERSIST_DEBOUNCE_MS = 300;

interface UsePersistedSchemaStateResult {
  /** Currently selected table name */
  selectedTableName: string | undefined;
  /** Set the selected table name */
  setSelectedTableName: (tableName: string | undefined) => void;
  /** Clear the selection */
  clearSelection: () => void;
}

export function usePersistedSchemaState(
  projectId: string | null
): UsePersistedSchemaStateResult {
  const storedState = useViewStateStore((s) =>
    projectId ? s.getViewState(projectId, 'schema') : undefined
  );
  const updateViewState = useViewStateStore((s) => s.updateViewState);

  // Get initial state with defaults (only on mount, not on every storedState change)
  const initialState = useMemo(() => getSchemaStateWithDefaults(storedState), []);

  // Track current state for persistence
  const currentStateRef = useRef<SchemaViewState>(initialState);

  // Get the effective selected table name from stored state
  const selectedTableName = useMemo(() => {
    const stored = storedState ?? {};
    const fullState = getSchemaStateWithDefaults(stored);
    return fullState.selectedTableName ?? undefined;
  }, [storedState]);

  // Debounced persist timer
  const persistTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Handle selection changes
  const setSelectedTableName = useCallback(
    (tableName: string | undefined) => {
      if (!projectId) return;

      // Update current state ref
      currentStateRef.current = {
        ...currentStateRef.current,
        selectedTableName: tableName ?? null,
      };

      // Debounce persistence
      if (persistTimerRef.current) {
        clearTimeout(persistTimerRef.current);
      }

      persistTimerRef.current = setTimeout(() => {
        updateViewState(projectId, 'schema', currentStateRef.current);
      }, PERSIST_DEBOUNCE_MS);
    },
    [projectId, updateViewState]
  );

  // Clear selection
  const clearSelection = useCallback(() => {
    setSelectedTableName(undefined);
  }, [setSelectedTableName]);

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

    const newState = getSchemaStateWithDefaults(storedState);
    currentStateRef.current = newState;
  }, [projectId, storedState]);

  return {
    selectedTableName,
    setSelectedTableName,
    clearSelection,
  };
}
