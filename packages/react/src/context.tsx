import { useEffect, useRef, type ReactNode } from 'react';
import type { AnalyzeResult } from '@pondpilot/flowscope-core';
import { createLineageStore, LineageStoreProvider, type LineageState } from './store';
import type { StoreApi } from 'zustand';

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
 * Legacy context provider for SQL lineage analysis state and actions.
 * This component now wraps the Zustand store for backward compatibility.
 *
 * New code should use the Zustand hooks directly:
 * - useLineageStore() for full store access
 * - useLineage() for structured state/actions
 * - useLineageState() for state-only
 * - useLineageActions() for actions-only
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
  const storeRef = useRef<StoreApi<LineageState>>();

  if (!storeRef.current) {
    storeRef.current = createLineageStore();
  }

  const store = storeRef.current;

  // Initialize store with initial values
  useEffect(() => {
    if (initialResult !== null) {
      store.getState().setResult(initialResult);
    }
  }, [initialResult, store]);

  useEffect(() => {
    if (initialSql) {
      store.getState().setSql(initialSql);
    }
  }, [initialSql, store]);

  return <LineageStoreProvider store={store}>{children}</LineageStoreProvider>;
}
