import { useEffect } from 'react';
import { LineageProvider } from '@pondpilot/flowscope-react';
import '@pondpilot/flowscope-react/styles.css';

import { ProjectProvider } from './lib/project-store';
import { ErrorBoundary } from './components/ErrorBoundary';
import { Workspace } from './components/Workspace';
import { WelcomeModal } from './components/WelcomeModal';
import { Toaster } from './components/ui/sonner';
import { useWasmInit, useShareImport } from './hooks';
import { DebugPanel } from './components/debug/DebugPanel';

function ShareImportHandler() {
  useShareImport();
  return null;
}

function App() {
  const { ready: wasmReady, error, isRetrying, retry } = useWasmInit();

  useEffect(() => {
    document.documentElement.classList.remove('dark');
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
          {import.meta.env.DEV && <DebugPanel />}
        </LineageProvider>
      </ProjectProvider>
    </ErrorBoundary>
  );
}

export default App;
