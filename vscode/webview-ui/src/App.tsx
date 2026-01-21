import { useState, useEffect, useCallback } from 'react';
import {
  LineageProvider,
  GraphView,
  GraphErrorBoundary,
  type AnalyzeResult,
} from '@pondpilot/flowscope-react';

// VSCode API type
declare function acquireVsCodeApi(): {
  postMessage(message: unknown): void;
  getState(): unknown;
  setState(state: unknown): void;
};

const vscode = acquireVsCodeApi();

interface Message {
  type: 'update' | 'error' | 'empty';
  data?: {
    result: AnalyzeResult;
    sql?: string;
  };
  message?: string;
}

export function App() {
  const [result, setResult] = useState<AnalyzeResult | null>(null);
  const [sql, setSql] = useState<string>('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const handleMessage = (event: MessageEvent<Message>) => {
      const message = event.data;

      switch (message.type) {
        case 'update':
          if (message.data?.result) {
            setResult(message.data.result);
            setSql(message.data.sql ?? '');
            setError(null);
          }
          break;
        case 'error':
          setError(message.message ?? 'Unknown error');
          setResult(null);
          break;
        case 'empty':
          setResult(null);
          setSql('');
          setError(null);
          break;
      }
    };

    window.addEventListener('message', handleMessage);

    // Request initial data from extension
    vscode.postMessage({ type: 'ready' });

    return () => window.removeEventListener('message', handleMessage);
  }, []);

  const handleNodeClick = useCallback((node: unknown) => {
    vscode.postMessage({ type: 'nodeClick', node });
  }, []);

  if (error) {
    return (
      <div className="flex h-full items-center justify-center p-4">
        <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-4 text-red-400">
          <h3 className="font-semibold">Error</h3>
          <p className="mt-2 text-sm">{error}</p>
        </div>
      </div>
    );
  }

  if (!result) {
    return (
      <div className="flex h-full items-center justify-center text-gray-400">
        <div className="text-center">
          <p className="text-lg">No lineage data</p>
          <p className="mt-2 text-sm opacity-70">Open a SQL file to see lineage</p>
        </div>
      </div>
    );
  }

  return (
    <GraphErrorBoundary>
      <LineageProvider initialResult={result} initialSql={sql}>
        <div className="h-full w-full">
          <GraphView onNodeClick={handleNodeClick} />
        </div>
      </LineageProvider>
    </GraphErrorBoundary>
  );
}
