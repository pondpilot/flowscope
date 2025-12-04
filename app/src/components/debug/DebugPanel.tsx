import { useState, useEffect, useRef } from 'react';
import { X, Minimize2, Maximize2, Copy, Check } from 'lucide-react';
import { useDebugData } from '../../hooks/useDebugData';
import { JsonTreeView } from './JsonTreeView';
import { getEngineVersion, isWasmInitialized } from '@pondpilot/flowscope-core';

type TabId = 'analysis' | 'schema' | 'uiState' | 'raw';

interface Position {
  x: number;
  y: number;
}

export function DebugPanel() {
  const [isVisible, setIsVisible] = useState(false);
  const [isMinimized, setIsMinimized] = useState(false);
  const [activeTab, setActiveTab] = useState<TabId>('analysis');
  const [position, setPosition] = useState<Position>({ x: 20, y: 20 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragOffset, setDragOffset] = useState<Position>({ x: 0, y: 0 });
  const [copied, setCopied] = useState(false);
  const [engineVersion, setEngineVersion] = useState<string>('unknown');

  const panelRef = useRef<HTMLDivElement>(null);
  const debugData = useDebugData();

  useEffect(() => {
    if (isVisible && isWasmInitialized()) {
      try {
        setEngineVersion(getEngineVersion());
      } catch (e) {
        console.warn('Could not fetch engine version', e);
      }
    }
  }, [isVisible]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && e.key === 'D') {
        e.preventDefault();
        setIsVisible((prev) => !prev);
        if (isMinimized) {
          setIsMinimized(false);
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isMinimized]);

  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      setPosition({
        x: e.clientX - dragOffset.x,
        y: e.clientY - dragOffset.y,
      });
    };

    const handleMouseUp = () => {
      setIsDragging(false);
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isDragging, dragOffset]);

  const handleMouseDown = (e: React.MouseEvent) => {
    if (panelRef.current && e.target === e.currentTarget) {
      const rect = panelRef.current.getBoundingClientRect();
      setDragOffset({
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      });
      setIsDragging(true);
    }
  };

  const handleCopy = async (data: unknown) => {
    try {
      await navigator.clipboard.writeText(JSON.stringify(data, null, 2));
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };

  const handleCopyAll = () => {
    handleCopy(debugData);
  };

  const getCurrentTabData = () => {
    switch (activeTab) {
      case 'analysis':
        return debugData.analysisResult;
      case 'schema':
        return debugData.schema;
      case 'uiState':
        return debugData.uiState;
      case 'raw':
        return debugData;
      default:
        return null;
    }
  };

  const handleCopyTab = () => {
    handleCopy(getCurrentTabData());
  };

  if (!isVisible) {
    return null;
  }

  return (
    <div
      ref={panelRef}
      className="fixed bg-background border-2 border-border rounded-lg shadow-2xl z-[9999] flex flex-col"
      style={{
        left: `${position.x}px`,
        top: `${position.y}px`,
        width: isMinimized ? '300px' : '600px',
        height: isMinimized ? 'auto' : '500px',
        cursor: isDragging ? 'grabbing' : 'default',
      }}
    >
      {/* Header - Draggable */}
      <div
        className="flex items-center justify-between px-4 py-2 bg-blue-grey-800 text-white rounded-t-md cursor-grab active:cursor-grabbing select-none"
        onMouseDown={handleMouseDown}
      >
        <div className="flex items-center gap-2">
          <span className="font-semibold text-sm">🐛 Debug Panel</span>
          <span className="text-xs text-blue-grey-400">Ctrl+Shift+D</span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setIsMinimized(!isMinimized)}
            className="p-1 hover:bg-blue-grey-700 rounded"
            title={isMinimized ? 'Maximize' : 'Minimize'}
          >
            {isMinimized ? (
              <Maximize2 className="w-4 h-4" />
            ) : (
              <Minimize2 className="w-4 h-4" />
            )}
          </button>
          <button
            onClick={() => setIsVisible(false)}
            className="p-1 hover:bg-blue-grey-700 rounded"
            title="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Content */}
      {!isMinimized && (
        <>
          {/* Tabs */}
          <div className="flex items-center gap-1 px-2 py-2 bg-muted border-b border-border">
            <button
              onClick={() => setActiveTab('analysis')}
              className={`px-3 py-1 text-sm rounded ${
                activeTab === 'analysis'
                  ? 'bg-background border border-border font-semibold'
                  : 'text-muted-foreground hover:bg-secondary'
              }`}
            >
              Analysis Result
            </button>
            <button
              onClick={() => setActiveTab('schema')}
              className={`px-3 py-1 text-sm rounded ${
                activeTab === 'schema'
                  ? 'bg-background border border-border font-semibold'
                  : 'text-muted-foreground hover:bg-secondary'
              }`}
            >
              Schema
            </button>
            <button
              onClick={() => setActiveTab('uiState')}
              className={`px-3 py-1 text-sm rounded ${
                activeTab === 'uiState'
                  ? 'bg-background border border-border font-semibold'
                  : 'text-muted-foreground hover:bg-secondary'
              }`}
            >
              UI State
            </button>
            <button
              onClick={() => setActiveTab('raw')}
              className={`px-3 py-1 text-sm rounded ${
                activeTab === 'raw'
                  ? 'bg-background border border-border font-semibold'
                  : 'text-muted-foreground hover:bg-secondary'
              }`}
            >
              Raw JSON
            </button>
            <div className="flex-1" />
            <button
              onClick={handleCopyTab}
              className="flex items-center gap-1 px-2 py-1 text-xs bg-primary text-primary-foreground rounded hover:bg-primary/90"
              title="Copy current tab data"
            >
              {copied ? (
                <>
                  <Check className="w-3 h-3" />
                  Copied!
                </>
              ) : (
                <>
                  <Copy className="w-3 h-3" />
                  Copy Tab
                </>
              )}
            </button>
            {activeTab === 'raw' && (
              <button
                onClick={handleCopyAll}
                className="flex items-center gap-1 px-2 py-1 text-xs bg-success-light dark:bg-success-dark text-white rounded hover:opacity-90 ml-1"
                title="Copy all debug data"
              >
                Copy All
              </button>
            )}
          </div>

          {/* Tab Content */}
          <div className="flex-1 overflow-auto p-4 bg-background text-sm font-mono">
            {activeTab === 'analysis' && (
              <div>
                <div className="mb-4 p-3 bg-highlight border border-primary/20 rounded">
                  <div className="text-xs font-sans text-foreground mb-2">
                    <strong>Summary:</strong>
                  </div>
                  <div className="grid grid-cols-2 gap-2 text-xs font-sans">
                    <div>
                      <span className="text-muted-foreground">Engine Version:</span>{' '}
                      <span className="font-mono text-foreground">{engineVersion}</span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">Has Result:</span>{' '}
                      <span className="font-semibold">
                        {debugData.analysisResult.summary.hasResult ? '✓' : '✗'}
                      </span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">Statements:</span>{' '}
                      <span className="font-semibold">
                        {debugData.analysisResult.summary.statementCount}
                      </span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">Issues:</span>{' '}
                      <span className="font-semibold">
                        {debugData.analysisResult.summary.issueCount}
                      </span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">Has Errors:</span>{' '}
                      <span
                        className={`font-semibold ${
                          debugData.analysisResult.summary.hasErrors
                            ? 'text-error-light dark:text-error-dark'
                            : 'text-success-light dark:text-success-dark'
                        }`}
                      >
                        {debugData.analysisResult.summary.hasErrors ? 'Yes' : 'No'}
                      </span>
                    </div>
                  </div>
                </div>
                <JsonTreeView data={debugData.analysisResult} />
              </div>
            )}
            {activeTab === 'schema' && (
              <div>
                <div className="mb-4 p-3 bg-success-light/10 dark:bg-success-dark/10 border border-success-light/20 dark:border-success-dark/20 rounded">
                  <div className="text-xs font-sans text-foreground mb-2">
                    <strong>Schema Status:</strong>
                  </div>
                  <div className="grid grid-cols-2 gap-2 text-xs font-sans">
                    <div>
                      <span className="text-muted-foreground">Has Schema SQL:</span>{' '}
                      <span className="font-semibold">
                        {debugData.schema.hasSchemaSQL ? '✓' : '✗'}
                      </span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">Resolved Tables:</span>{' '}
                      <span className="font-semibold">
                        {debugData.schema.resolvedTableCount}
                      </span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">Imported:</span>{' '}
                      <span className="font-semibold text-success-light dark:text-success-dark">
                        {debugData.schema.importedTableCount}
                      </span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">Implied:</span>{' '}
                      <span className="font-semibold text-primary">
                        {debugData.schema.impliedTableCount}
                      </span>
                    </div>
                  </div>
                </div>
                <JsonTreeView data={debugData.schema} />
              </div>
            )}
            {activeTab === 'uiState' && (
              <JsonTreeView data={debugData.uiState} />
            )}
            {activeTab === 'raw' && (
              <JsonTreeView data={debugData} />
            )}
          </div>
        </>
      )}
    </div>
  );
}
