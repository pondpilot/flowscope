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
      className="fixed bg-white border-2 border-gray-300 rounded-lg shadow-2xl z-[9999] flex flex-col"
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
        className="flex items-center justify-between px-4 py-2 bg-gray-800 text-white rounded-t-md cursor-grab active:cursor-grabbing select-none"
        onMouseDown={handleMouseDown}
      >
        <div className="flex items-center gap-2">
          <span className="font-semibold text-sm">🐛 Debug Panel</span>
          <span className="text-xs text-gray-400">Ctrl+Shift+D</span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setIsMinimized(!isMinimized)}
            className="p-1 hover:bg-gray-700 rounded"
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
            className="p-1 hover:bg-gray-700 rounded"
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
          <div className="flex items-center gap-1 px-2 py-2 bg-gray-100 border-b border-gray-300">
            <button
              onClick={() => setActiveTab('analysis')}
              className={`px-3 py-1 text-sm rounded ${
                activeTab === 'analysis'
                  ? 'bg-white border border-gray-300 font-semibold'
                  : 'text-gray-600 hover:bg-gray-200'
              }`}
            >
              Analysis Result
            </button>
            <button
              onClick={() => setActiveTab('schema')}
              className={`px-3 py-1 text-sm rounded ${
                activeTab === 'schema'
                  ? 'bg-white border border-gray-300 font-semibold'
                  : 'text-gray-600 hover:bg-gray-200'
              }`}
            >
              Schema
            </button>
            <button
              onClick={() => setActiveTab('uiState')}
              className={`px-3 py-1 text-sm rounded ${
                activeTab === 'uiState'
                  ? 'bg-white border border-gray-300 font-semibold'
                  : 'text-gray-600 hover:bg-gray-200'
              }`}
            >
              UI State
            </button>
            <button
              onClick={() => setActiveTab('raw')}
              className={`px-3 py-1 text-sm rounded ${
                activeTab === 'raw'
                  ? 'bg-white border border-gray-300 font-semibold'
                  : 'text-gray-600 hover:bg-gray-200'
              }`}
            >
              Raw JSON
            </button>
            <div className="flex-1" />
            <button
              onClick={handleCopyTab}
              className="flex items-center gap-1 px-2 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-700"
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
                className="flex items-center gap-1 px-2 py-1 text-xs bg-green-600 text-white rounded hover:bg-green-700 ml-1"
                title="Copy all debug data"
              >
                Copy All
              </button>
            )}
          </div>

          {/* Tab Content */}
          <div className="flex-1 overflow-auto p-4 bg-white text-sm font-mono">
            {activeTab === 'analysis' && (
              <div>
                <div className="mb-4 p-3 bg-blue-50 border border-blue-200 rounded">
                  <div className="text-xs font-sans text-gray-700 mb-2">
                    <strong>Summary:</strong>
                  </div>
                  <div className="grid grid-cols-2 gap-2 text-xs font-sans">
                    <div>
                      <span className="text-gray-600">Engine Version:</span>{' '}
                      <span className="font-mono text-gray-800">{engineVersion}</span>
                    </div>
                    <div>
                      <span className="text-gray-600">Has Result:</span>{' '}
                      <span className="font-semibold">
                        {debugData.analysisResult.summary.hasResult ? '✓' : '✗'}
                      </span>
                    </div>
                    <div>
                      <span className="text-gray-600">Statements:</span>{' '}
                      <span className="font-semibold">
                        {debugData.analysisResult.summary.statementCount}
                      </span>
                    </div>
                    <div>
                      <span className="text-gray-600">Issues:</span>{' '}
                      <span className="font-semibold">
                        {debugData.analysisResult.summary.issueCount}
                      </span>
                    </div>
                    <div>
                      <span className="text-gray-600">Has Errors:</span>{' '}
                      <span
                        className={`font-semibold ${
                          debugData.analysisResult.summary.hasErrors
                            ? 'text-red-600'
                            : 'text-green-600'
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
                <div className="mb-4 p-3 bg-green-50 border border-green-200 rounded">
                  <div className="text-xs font-sans text-gray-700 mb-2">
                    <strong>Schema Status:</strong>
                  </div>
                  <div className="grid grid-cols-2 gap-2 text-xs font-sans">
                    <div>
                      <span className="text-gray-600">Has Schema SQL:</span>{' '}
                      <span className="font-semibold">
                        {debugData.schema.hasSchemaSQL ? '✓' : '✗'}
                      </span>
                    </div>
                    <div>
                      <span className="text-gray-600">Resolved Tables:</span>{' '}
                      <span className="font-semibold">
                        {debugData.schema.resolvedTableCount}
                      </span>
                    </div>
                    <div>
                      <span className="text-gray-600">Imported:</span>{' '}
                      <span className="font-semibold text-green-600">
                        {debugData.schema.importedTableCount}
                      </span>
                    </div>
                    <div>
                      <span className="text-gray-600">Implied:</span>{' '}
                      <span className="font-semibold text-blue-600">
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
