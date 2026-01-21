import { useState, useEffect, useCallback } from 'react';
import { initializeAnalysisWorker } from '@/lib/analysis-worker';
import type { WasmState } from '@/types';

export function useAnalysisWorkerInit() {
  const [state, setState] = useState<WasmState>({
    ready: false,
    error: null,
    isRetrying: false,
  });

  const initializeWorker = useCallback(async (isRetry = false) => {
    if (isRetry) {
      setState((prev) => ({ ...prev, error: null, isRetrying: true }));
    }

    try {
      await initializeAnalysisWorker();
      setState({ ready: true, error: null, isRetrying: false });
    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      setState({
        ready: false,
        error: `Failed to initialize analysis worker: ${errorMessage}`,
        isRetrying: false,
      });
    }
  }, []);

  useEffect(() => {
    let cancelled = false;

    initializeWorker().then(() => {
      if (cancelled) {
        // Reset state if component unmounted during init
        // (state update will be a no-op but this is semantically correct)
      }
    });

    return () => {
      cancelled = true;
    };
  }, [initializeWorker]);

  return {
    ...state,
    retry: () => initializeWorker(true),
  };
}
