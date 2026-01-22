import { useEffect } from 'react';
import { LineageProvider } from '@pondpilot/flowscope-react';
import '@pondpilot/flowscope-react/styles.css';

import { ProjectProvider } from './lib/project-store';
import { BackendProvider, useBackendReady } from './lib/backend-context';
import { ErrorBoundary } from './components/ErrorBoundary';
import { Workspace } from './components/Workspace';
import { WelcomeModal } from './components/WelcomeModal';
import { GlobalDropZone } from './components/GlobalDropZone';
import { Toaster } from './components/ui/sonner';
import { useShareImport } from './hooks';
import { DebugPanel } from './components/debug/DebugPanel';
import { initializeTheme } from './lib/theme-store';

function ShareImportHandler() {
  useShareImport();
  return null;
}

function AppContent() {
  const { ready, error, isRetrying, retry, backendType } = useBackendReady();

  return (
    <>
      <ShareImportHandler />
      <LineageProvider defaultLayoutAlgorithm="dagre">
        <div className="flex flex-col h-screen bg-background text-foreground overflow-hidden">
          <Workspace
            wasmReady={ready}
            error={error}
            onRetry={retry}
            isRetrying={isRetrying}
            backendType={backendType}
          />
        </div>
        <Toaster position="bottom-right" />
        <WelcomeModal />
        <GlobalDropZone />
        {import.meta.env.DEV && <DebugPanel />}
      </LineageProvider>
    </>
  );
}

function App() {
  useEffect(() => {
    try {
      return initializeTheme();
    } catch (error) {
      console.error('Theme initialization failed:', error);
      document.documentElement.classList.toggle(
        'dark',
        window.matchMedia('(prefers-color-scheme: dark)').matches
      );
    }
  }, []);

  return (
    <ErrorBoundary>
      <BackendProvider>
        <ProjectProvider>
          <AppContent />
        </ProjectProvider>
      </BackendProvider>
    </ErrorBoundary>
  );
}

export default App;
