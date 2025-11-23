// Components
export { GraphView } from './components/GraphView';
export { SqlView } from './components/SqlView';
export { ColumnPanel } from './components/ColumnPanel';
export { IssuesPanel } from './components/IssuesPanel';
export { LineageExplorer } from './components/LineageExplorer';
export { SchemaView } from './components/SchemaView';
export { ViewModeSelector } from './components/ViewModeSelector';
export { ErrorBoundary, GraphErrorBoundary } from './components/ErrorBoundary';

// Context and hooks
export {
  LineageProvider,
  useLineage,
  useLineageState,
  useLineageActions,
} from './context';
export type { LineageProviderProps } from './context';

// Types
export type {
  LineageState,
  LineageActions,
  LineageContextValue,
  LineageViewMode,
  GraphViewProps,
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
