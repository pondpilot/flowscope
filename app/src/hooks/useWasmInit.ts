import { useState, useEffect, useCallback } from 'react';
import { initWasm } from '@pondpilot/flowscope-core';
import type { WasmState } from '@/types';

export function useWasmInit() {
  const [state, setState] = useState<WasmState>({
    ready: false,
    error: null,
    isRetrying: false,
  });

  const initializeWasm = useCallback(async () => {
    setState(prev => ({ ...prev, error: null, isRetrying: true }));

    try {
      await initWasm();
      setState({ ready: true, error: null, isRetrying: false });
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setState({
        ready: false,
        error: `Failed to load WASM: ${errorMessage}`,
        isRetrying: false,
      });
    }
  }, []);

  useEffect(() => {
    let cancelled = false;

    initWasm()
      .then(() => {
        if (!cancelled) {
          setState({ ready: true, error: null, isRetrying: false });
        }
      })
      .catch(err => {
        if (!cancelled) {
          const errorMessage = err instanceof Error ? err.message : String(err);
          setState({
            ready: false,
            error: `Failed to load WASM: ${errorMessage}`,
            isRetrying: false,
          });
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return {
    ...state,
    retry: initializeWasm,
  };
}
