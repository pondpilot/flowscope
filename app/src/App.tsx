import { useEffect, useState, useCallback } from 'react';
import { initWasm } from '@pondpilot/flowscope-core';
import { LineageProvider } from '@pondpilot/flowscope-react';
import '@pondpilot/flowscope-react/styles.css';

import { ProjectProvider } from './lib/project-store';
import { ErrorBoundary } from './components/ErrorBoundary';
import { Workspace } from './components/Workspace';

function App() {
  const [wasmReady, setWasmReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isRetrying, setIsRetrying] = useState(false);

  const initializeWasm = useCallback(async () => {
    setError(null);
    setIsRetrying(true);
    try {
      await initWasm();
      setWasmReady(true);
    } catch (err) {
      setError(`Failed to load WASM: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setIsRetrying(false);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;

    initWasm()
      .then(() => {
        if (!cancelled) {
          setWasmReady(true);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setError(`Failed to load WASM: ${err instanceof Error ? err.message : String(err)}`);
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    document.documentElement.classList.remove('dark');
  }, []);

  return (
    <ErrorBoundary>
      <ProjectProvider>
        <LineageProvider>
          <div className="flex flex-col h-screen bg-background text-foreground overflow-hidden">
            <Workspace
              wasmReady={wasmReady}
              error={error}
              onRetry={initializeWasm}
              isRetrying={isRetrying}
            />
          </div>
        </LineageProvider>
      </ProjectProvider>
    </ErrorBoundary>
  );
}

export default App;
