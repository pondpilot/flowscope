import { useEffect } from 'react';
import { LineageProvider } from '@pondpilot/flowscope-react';
import '@pondpilot/flowscope-react/styles.css';

import { ProjectProvider } from './lib/project-store';
import { ErrorBoundary } from './components/ErrorBoundary';
import { Workspace } from './components/Workspace';
import { WelcomeModal } from './components/WelcomeModal';
import { GlobalDropZone } from './components/GlobalDropZone';
import { Toaster } from './components/ui/sonner';
import { useWasmInit, useShareImport } from './hooks';
import { DebugPanel } from './components/debug/DebugPanel';
import { initializeTheme } from './lib/theme-store';

function ShareImportHandler() {
  useShareImport();
  return null;
}

function App() {
  const { ready: wasmReady, error, isRetrying, retry } = useWasmInit();

  useEffect(() => {
    try {
      const cleanup = initializeTheme();
      return cleanup;
    } catch (error) {
      console.error('Theme initialization failed:', error);
      // Fallback to system preference on initialization failure
      if (window.matchMedia('(prefers-color-scheme: dark)').matches) {
        document.documentElement.classList.add('dark');
      } else {
        document.documentElement.classList.remove('dark');
      }
      return () => {};
    }
  }, []);

  return (
    <ErrorBoundary>
      <ProjectProvider>
        <ShareImportHandler />
        <LineageProvider defaultLayoutAlgorithm="dagre">
          <div className="flex flex-col h-screen bg-background text-foreground overflow-hidden">
            <Workspace wasmReady={wasmReady} error={error} onRetry={retry} isRetrying={isRetrying} />
          </div>
          <Toaster position="bottom-right" />
          <WelcomeModal />
          <GlobalDropZone />
          {import.meta.env.DEV && <DebugPanel />}
        </LineageProvider>
      </ProjectProvider>
    </ErrorBoundary>
  );
}

export default App;
