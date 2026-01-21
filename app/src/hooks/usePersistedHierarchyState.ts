/**
 * Hook for managing persisted HierarchyView state.
 *
 * Provides local state that syncs with the view state store,
 * automatically persisting changes with debouncing.
 */

import { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import {
  useViewStateStore,
  getHierarchyStateWithDefaults,
  type HierarchyViewState,
} from '@/lib/view-state-store';

const PERSIST_DEBOUNCE_MS = 300;

interface UsePersistedHierarchyStateResult {
  // State values
  expandedNodes: Set<string>;
  filter: string;
  detailsPanelHeight: number;
  focusedNodeKey: string | null;
  unusedExpanded: boolean;

  // State setters
  setExpandedNodes: React.Dispatch<React.SetStateAction<Set<string>>>;
  setFilter: React.Dispatch<React.SetStateAction<string>>;
  setDetailsPanelHeight: React.Dispatch<React.SetStateAction<number>>;
  setFocusedNodeKey: React.Dispatch<React.SetStateAction<string | null>>;
  setUnusedExpanded: React.Dispatch<React.SetStateAction<boolean>>;

  // Utility functions
  toggleNode: (nodeId: string) => void;
}

export function usePersistedHierarchyState(
  projectId: string | null
): UsePersistedHierarchyStateResult {
  const storedState = useViewStateStore((s) =>
    projectId ? s.getViewState(projectId, 'hierarchy') : undefined
  );
  const updateViewState = useViewStateStore((s) => s.updateViewState);

  // Get initial state with defaults (only on mount, not on every storedState change)
  const initialState = useMemo(() => getHierarchyStateWithDefaults(storedState), []);

  // Local state - initialized from store
  const [expandedNodes, setExpandedNodes] = useState<Set<string>>(
    () => new Set(initialState.expandedNodes)
  );
  const [filter, setFilter] = useState(initialState.filter);
  const [detailsPanelHeight, setDetailsPanelHeight] = useState(initialState.detailsPanelHeight);
  const [focusedNodeKey, setFocusedNodeKey] = useState<string | null>(initialState.focusedNodeKey);
  const [unusedExpanded, setUnusedExpanded] = useState(initialState.unusedExpanded);

  // Track last persisted values to avoid unnecessary writes
  const lastPersistedRef = useRef<HierarchyViewState | null>(cloneHierarchyState(initialState));

  // Debounced persist to store
  useEffect(() => {
    if (!projectId) return;

    const nextSnapshot: HierarchyViewState = {
      expandedNodes: Array.from(expandedNodes),
      filter,
      detailsPanelHeight,
      focusedNodeKey,
      unusedExpanded,
    };

    if (areHierarchyStatesEqual(lastPersistedRef.current, nextSnapshot)) {
      return;
    }

    const handler = setTimeout(() => {
      updateViewState(projectId, 'hierarchy', nextSnapshot);
      lastPersistedRef.current = cloneHierarchyState(nextSnapshot);
    }, PERSIST_DEBOUNCE_MS);

    return () => clearTimeout(handler);
  }, [
    projectId,
    expandedNodes,
    filter,
    detailsPanelHeight,
    focusedNodeKey,
    unusedExpanded,
    updateViewState,
  ]);

  // Rehydrate state when the project or stored snapshot changes
  useEffect(() => {
    if (!projectId) return;

    const hydratedState = getHierarchyStateWithDefaults(storedState);
    if (areHierarchyStatesEqual(lastPersistedRef.current, hydratedState)) {
      return;
    }

    setExpandedNodes(new Set(hydratedState.expandedNodes));
    setFilter(hydratedState.filter);
    setDetailsPanelHeight(hydratedState.detailsPanelHeight);
    setFocusedNodeKey(hydratedState.focusedNodeKey);
    setUnusedExpanded(hydratedState.unusedExpanded);
    lastPersistedRef.current = cloneHierarchyState(hydratedState);
  }, [projectId, storedState]);

  // Utility function for toggling nodes
  const toggleNode = useCallback((nodeId: string) => {
    setExpandedNodes((prev) => {
      const next = new Set(prev);
      if (next.has(nodeId)) {
        next.delete(nodeId);
      } else {
        next.add(nodeId);
      }
      return next;
    });
  }, []);

  return {
    expandedNodes,
    filter,
    detailsPanelHeight,
    focusedNodeKey,
    unusedExpanded,
    setExpandedNodes,
    setFilter,
    setDetailsPanelHeight,
    setFocusedNodeKey,
    setUnusedExpanded,
    toggleNode,
  };
}

function cloneHierarchyState(state: HierarchyViewState): HierarchyViewState {
  return {
    expandedNodes: [...state.expandedNodes],
    filter: state.filter,
    detailsPanelHeight: state.detailsPanelHeight,
    focusedNodeKey: state.focusedNodeKey,
    unusedExpanded: state.unusedExpanded,
  };
}

function areHierarchyStatesEqual(
  previous: HierarchyViewState | null,
  next: HierarchyViewState
): boolean {
  if (!previous) return false;

  if (
    previous.filter !== next.filter ||
    previous.detailsPanelHeight !== next.detailsPanelHeight ||
    previous.focusedNodeKey !== next.focusedNodeKey ||
    previous.unusedExpanded !== next.unusedExpanded
  ) {
    return false;
  }

  if (previous.expandedNodes.length !== next.expandedNodes.length) {
    return false;
  }

  for (let i = 0; i < previous.expandedNodes.length; i += 1) {
    if (previous.expandedNodes[i] !== next.expandedNodes[i]) {
      return false;
    }
  }

  return true;
}
