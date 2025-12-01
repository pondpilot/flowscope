// Components
export { GraphView } from './components/GraphView';
export { SqlView } from './components/SqlView';
export { ColumnPanel } from './components/ColumnPanel';
export { IssuesPanel } from './components/IssuesPanel';
export { LineageExplorer } from './components/LineageExplorer';
export { SchemaView } from './components/SchemaView';
export { ViewModeSelector } from './components/ViewModeSelector';
export { LayoutSelector } from './components/LayoutSelector';
export { Legend } from './components/Legend';
export { MatrixView } from './components/MatrixView';
export type { MatrixViewControlledState } from './components/MatrixView';
export { type EdgeType } from './components/AnimatedEdge';
export { ErrorBoundary, GraphErrorBoundary } from './components/ErrorBoundary';

// Store and hooks (new Zustand-based)
export {
  useLineageStore,
  useLineage,
  useLineageState,
  useLineageActions,
} from './store';

export { useGraphSearch } from './hooks/useGraphSearch';

// Context (legacy, for backward compatibility - wraps Zustand store)
export { LineageProvider } from './context';
export type { LineageProviderProps } from './context';

// Types
export type {
  LineageState,
  LineageActions,
  LineageContextValue,
  LineageViewMode,
  MatrixSubMode,
  LayoutAlgorithm,
  GraphViewProps,
  ViewportState,
  SqlViewProps,
  ColumnPanelProps,
  IssuesPanelProps,
  LineageExplorerProps,
  TableNodeData,
  ScriptNodeData,
  ColumnNodeData,
  ColumnNodeInfo,
} from './types';

// Re-export core types for convenience
export type {
  AnalyzeResult,
  Node,
  Edge,
  Issue,
  Span,
  StatementLineage,
} from './types';

// Utilities
export {
  escapeHtml,
  sanitizeSqlContent,
  sanitizeErrorMessage,
  sanitizeIdentifier,
} from './utils/sanitize';

// Export utilities
export {
  downloadXlsx,
  downloadJson,
  downloadMermaid,
  downloadHtml,
  generateXlsxWorkbook,
  generateStructuredJson,
  generateMermaid,
  generateAllMermaidDiagrams,
  generateHtmlExport,
  extractScriptInfo,
  extractTableInfo,
  extractColumnMappings,
} from './utils/exportUtils';

export type {
  ScriptInfo,
  TableInfo,
  ColumnMapping,
  MermaidGraphType,
  StructuredLineageJson,
} from './utils/exportUtils';
