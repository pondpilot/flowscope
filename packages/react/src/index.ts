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
export { SearchAutocomplete } from './components/SearchAutocomplete';
export type {
  SearchAutocompleteProps,
  SearchAutocompleteRef,
} from './components/SearchAutocomplete';
export { TableFilterDropdown } from './components/TableFilterDropdown';

// Store and hooks (new Zustand-based)
export { useLineageStore, useLineage, useLineageState, useLineageActions } from './store';

export { useGraphSearch } from './hooks/useGraphSearch';
export { useSearchSuggestions } from './hooks/useSearchSuggestions';
export { useColors, useIsDarkMode } from './hooks/useColors';
export type {
  SearchSuggestion,
  SearchableType,
  UseSearchSuggestionsOptions,
  UseSearchSuggestionsResult,
} from './hooks/useSearchSuggestions';

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
  LayoutMetrics,
  GraphBuildMetrics,
  TableFilterDirection,
  TableFilter,
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
export type { AnalyzeResult, Node, Edge, Issue, Span, StatementLineage } from './types';

// Utilities
export {
  escapeHtml,
  sanitizeSqlContent,
  sanitizeErrorMessage,
  sanitizeIdentifier,
} from './utils/sanitize';

// Namespace/schema utilities
export { NAMESPACE_COLORS, getNamespaceColor } from './constants';

// Export utilities
export {
  downloadCsvArchive,
  downloadHtml,
  downloadJson,
  downloadMermaid,
  downloadPng,
  downloadXlsx,
  generateMermaid,
} from './utils/exportUtils';

export type { MermaidGraphType } from './utils/exportUtils';

// Graph traversal utilities
export {
  findConnectedElements,
  findConnectedElementsMultiple,
  findConnectedElementsDirectional,
  findConnectedElementsMultipleDirectional,
  findSearchMatchIds,
  filterGraphToHighlights,
  isTableNodeData,
  shouldIncludeNode,
  shouldIncludeEdge,
  applyTableFilter,
  buildTableLabelMap,
} from './utils/graphTraversal';

export type { ApplyTableFilterResult } from './utils/graphTraversal';
