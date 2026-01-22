/**
 * Backend context for analysis operations.
 *
 * Provides access to the backend adapter throughout the component tree.
 * The adapter is initialized once and shared across all components.
 */

import React, { createContext, useContext, useState, useEffect, useCallback, useMemo } from 'react';
import type { BackendAdapter, BackendDetectionResult } from './backend-adapter';
import { createBackendAdapter } from './backend-adapter';

export interface BackendState {
  ready: boolean;
  error: string | null;
  isRetrying: boolean;
  backendType: 'rest' | 'wasm' | null;
}

interface BackendContextValue extends BackendState {
  adapter: BackendAdapter | null;
  retry: () => void;
}

const BackendContext = createContext<BackendContextValue | null>(null);

interface BackendProviderProps {
  children: React.ReactNode;
  preferWasm?: boolean;
}

export function BackendProvider({ children, preferWasm = false }: BackendProviderProps) {
  const [state, setState] = useState<BackendState>({
    ready: false,
    error: null,
    isRetrying: false,
    backendType: null,
  });
  const [adapter, setAdapter] = useState<BackendAdapter | null>(null);

  const initializeBackend = useCallback(
    async (isRetry = false) => {
      if (isRetry) {
        setState((prev) => ({ ...prev, error: null, isRetrying: true }));
      }

      try {
        const result: BackendDetectionResult = await createBackendAdapter(preferWasm);
        setAdapter(result.adapter);
        setState({
          ready: true,
          error: null,
          isRetrying: false,
          backendType: result.detectedType,
        });
      } catch (error: unknown) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        setState({
          ready: false,
          error: `Failed to initialize backend: ${errorMessage}`,
          isRetrying: false,
          backendType: null,
        });
        setAdapter(null);
      }
    },
    [preferWasm]
  );

  useEffect(() => {
    let cancelled = false;

    initializeBackend().then(() => {
      if (cancelled) {
        // Component unmounted during init
      }
    });

    return () => {
      cancelled = true;
    };
  }, [initializeBackend]);

  const value = useMemo(
    () => ({
      ...state,
      adapter,
      retry: () => initializeBackend(true),
    }),
    [state, adapter, initializeBackend]
  );

  return <BackendContext.Provider value={value}>{children}</BackendContext.Provider>;
}

/**
 * Hook to access the backend adapter and state.
 */
export function useBackend(): BackendContextValue {
  const context = useContext(BackendContext);
  if (!context) {
    throw new Error('useBackend must be used within a BackendProvider');
  }
  return context;
}

/**
 * Hook to check if backend is ready (for backwards compatibility).
 * Returns the same interface as the old useAnalysisWorkerInit hook.
 */
export function useBackendReady() {
  const { ready, error, isRetrying, retry, backendType } = useBackend();
  return {
    ready,
    error,
    isRetrying,
    retry,
    backendType,
  };
}
