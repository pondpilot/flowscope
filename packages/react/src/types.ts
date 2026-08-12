import type { RefObject } from 'react';
import type {
  AnalyzeResult,
  Node,
  Edge,
  Issue,
  Span,
  SchemaTable,
  SchemaMetadata,
  Dialect,
  FilterPredicate,
  AggregationInfo,
} from '@pondpilot/flowscope-core';

/**
 * View mode for the lineage graph visualization.
 * Controls the level of detail displayed in the graph.
 */
export type LineageViewMode = 'script' | 'table';

/**
 * Sub-mode for the matrix view.
 * Controls whether to show table or script level dependencies.
 */
export type MatrixSubMode = 'tables' | 'scripts';

/**
 * Layout algorithm for the graph visualization.
 */
export type LayoutAlgorithm = 'dagre' | 'elk';

/**
 * Performance metrics for layout computation.
 */
export interface LayoutMetrics {
  lastDurationMs: number | null;
  nodeCount: number;
  edgeCount: number;
  algorithm: LayoutAlgorithm | null;
  lastUpdatedAt: number | null;
}

/**
 * Performance metrics for graph build computation.
 */
export interface GraphBuildMetrics {
  lastDurationMs: number | null;
  nodeCount: number;
  edgeCount: number;
  lastUpdatedAt: number | null;
}

/**
 * Direction for table filter lineage traversal.
 */
export type TableFilterDirection = 'upstream' | 'downstream' | 'both';

/**
 * Configuration for filtering the graph by selected tables.
 */
export interface TableFilter {
  /** Set of table/view/CTE labels (names) to filter by */
  selectedTableLabels: Set<string>;
  /** Direction of lineage to show: upstream, downstream, or both */
  direction: TableFilterDirection;
}

/**
 * Props for the SchemaView component.
 */
export interface SchemaViewProps {
  /** Array of schema tables to display */
  schema: SchemaTable[];
}

/**
 * Request to navigate to a specific file and location.
 */
export interface NavigationRequest {
  sourceName: string;
  span?: Span;
  targetName?: string;
  targetType?: 'table' | 'view' | 'cte' | 'column' | 'script';
}

/**
 * State shape for the lineage context.
 * Contains all the stateful values managed by the LineageProvider.
 */
export interface LineageState {
  /** The current analysis result containing lineage data */
  result: AnalyzeResult | null;
  /** The SQL text being analyzed */
  sql: string;
  /** ID of the currently selected node in the graph, or null if none selected */
  selectedNodeId: string | null;
  /** Set of IDs for nodes that are currently collapsed */
  collapsedNodeIds: Set<string>;
  /** Set of IDs for tables with all columns shown */
  expandedTableIds: Set<string>;
  /** Whether tables are collapsed by default */
  defaultCollapsed: boolean;
  /** Index of the currently selected SQL statement */
  selectedStatementIndex: number;
  /** The currently highlighted span in the SQL editor, or null if none */
  highlightedSpan: Span | null;
  /** Index of the selected node occurrence currently highlighted in the editor */
  focusedOccurrenceIndex: number;
  /** Search term for filtering/highlighting nodes in the graph */
  searchTerm: string;
  /** Current view mode for the lineage graph */
  viewMode: LineageViewMode;
  /** Current sub-mode for the matrix view */
  matrixSubMode: MatrixSubMode;
  /** Current layout algorithm */
  layoutAlgorithm: LayoutAlgorithm;
  /** Layout performance metrics */
  layoutMetrics: LayoutMetrics;
  /** Graph build performance metrics */
  graphMetrics: GraphBuildMetrics;
  /** Whether to show column-level edges */
  showColumnEdges: boolean;
  /** Whether to hide CTEs and show bypass edges */
  hideCTEs: boolean;
  /** Whether to show table details in script nodes */
  showScriptTables: boolean;
  /** Request to navigate to a specific file and location */
  navigationRequest: NavigationRequest | null;
  /**
   * Node ids currently rendered in the graph after filtering. Exposed as a
   * `ReadonlySet` because consumers should not mutate the stored set; call
   * `setVisibleGraphNodeIds` to replace it.
   */
  visibleGraphNodeIds: ReadonlySet<string>;
  /**
   * Active reveal-in-graph request. The `nonce` changes on every call so the
   * graph re-triggers its pulse animation even when the same node id is
   * revealed twice in a row. `suppressNavigation` tells the graph→editor bounce
   * effect to skip the navigation side effect for this reveal (consumed once
   * per nonce). Consumers typically observe this field rather than writing to
   * it; use the `revealNodeInGraph` / `clearRevealRequest` actions to drive it.
   */
  revealRequest: { nodeId: string; nonce: number; suppressNavigation: boolean } | null;
  /** Table filter configuration */
  tableFilter: TableFilter;
  /** Whether the graph layout is currently running */
  isLayouting: boolean;
  /** Whether the graph view model is currently being built */
  isBuilding: boolean;
  /**
   * Snapshot of the SQL text each path held when the most recent analysis
   * ran, keyed by the analyzer's `sourceName`. `null` before any analysis
   * has run. Consumers diff this against live file content to gate graph↔
   * text navigation on staleness (#22).
   */
  analyzedContentByPath: ReadonlyMap<string, string> | null;
  /**
   * Paths whose live content has diverged from `analyzedContentByPath`.
   * Written by the app layer; read by components that need to disable nav
   * (OccurrenceCycler, useOccurrenceShortcuts, SqlView reveal button).
   */
  stalePaths: ReadonlySet<string>;
}

/**
 * Actions available in the lineage context.
 * These functions allow components to update the lineage state.
 */
export interface LineageActions {
  /** Update the analysis result */
  setResult: (result: AnalyzeResult | null) => void;
  /** Update the SQL text */
  setSql: (sql: string) => void;
  /** Select a node by ID, or null to deselect */
  selectNode: (nodeId: string | null) => void;
  /** Cycle the highlighted occurrence of the selected node */
  cycleOccurrence: (direction: 'next' | 'prev') => void;
  /** Focus a specific occurrence of the selected node */
  focusOccurrence: (index: number) => void;
  /** Toggle the collapsed state of a node */
  toggleNodeCollapse: (nodeId: string) => void;
  /** Explicitly set whether a node is collapsed */
  setNodeCollapsed: (nodeId: string, collapsed: boolean) => void;
  /** Toggle the expansion state of a table (show/hide all columns) */
  toggleTableExpansion: (tableId: string) => void;
  /** Explicitly set whether a table is expanded to show all columns */
  setTableExpanded: (tableId: string, expanded: boolean) => void;
  /** Set all nodes to collapsed or expanded state */
  setAllNodesCollapsed: (collapsed: boolean) => void;
  /** Select a statement by index */
  selectStatement: (index: number) => void;
  /** Highlight a span in the SQL editor, or null to clear */
  highlightSpan: (span: Span | null) => void;
  /** Update the search term for node filtering */
  setSearchTerm: (term: string) => void;
  /** Update the view mode for the lineage graph */
  setViewMode: (mode: LineageViewMode) => void;
  /** Update the sub-mode for the matrix view */
  setMatrixSubMode: (mode: MatrixSubMode) => void;
  /** Update the layout algorithm */
  setLayoutAlgorithm: (algorithm: LayoutAlgorithm) => void;
  /** Update layout performance metrics */
  setLayoutMetrics: (metrics: LayoutMetrics) => void;
  /** Update graph build performance metrics */
  setGraphMetrics: (metrics: GraphBuildMetrics) => void;
  /** Toggle column-level edge visibility */
  toggleColumnEdges: () => void;
  /** Toggle hiding of CTEs (showing bypass edges) */
  toggleHideCTEs: () => void;
  /** Toggle showing tables in script nodes */
  toggleShowScriptTables: () => void;
  /** Request navigation to a file/location */
  requestNavigation: (request: NavigationRequest | null) => void;
  /**
   * Replace the set of graph node ids currently rendered in the viewport.
   * Called by `GraphView` whenever its filtered graph changes; external callers
   * normally don't need to invoke this.
   */
  setVisibleGraphNodeIds: (nodeIds: Iterable<string>) => void;
  /**
   * Select a graph node from the SQL editor (e.g. "Reveal in lineage"),
   * triggering the pulse animation without bouncing the selection back into
   * the editor.
   */
  revealNodeInGraph: (nodeId: string) => void;
  /** Clear any pending reveal request. */
  clearRevealRequest: () => void;
  /** Set the table filter */
  setTableFilter: (filter: TableFilter) => void;
  /** Toggle selection of a table in the filter */
  toggleTableFilterSelection: (tableLabel: string) => void;
  /** Set the direction of the table filter */
  setTableFilterDirection: (direction: TableFilterDirection) => void;
  /** Clear the table filter */
  clearTableFilter: () => void;
  /** Set whether the graph layout is currently running */
  setIsLayouting: (isLayouting: boolean) => void;
  /** Set whether the graph view model is currently being built */
  setIsBuilding: (isBuilding: boolean) => void;
  /** Replace the analyzed-content snapshot (or clear with null). */
  setAnalyzedContent: (map: ReadonlyMap<string, string> | null) => void;
  /** Replace the set of paths whose content has diverged from the snapshot. */
  setStalePaths: (paths: Iterable<string>) => void;
}

/**
 * The complete lineage context value combining state and actions.
 */
export interface LineageContextValue {
  /** The current state */
  state: LineageState;
  /** Available actions for updating state */
  actions: LineageActions;
}

/**
 * Viewport state for graph visualization.
 */
export interface ViewportState {
  x: number;
  y: number;
  zoom: number;
}

/**
 * Namespace filter for filtering nodes by database/schema.
 *
 * Note: This interface mirrors NamespaceFilterState in app/src/lib/view-state-store.ts.
 * They are intentionally separate because this is a library type while the app has its
 * own persistence layer. If you change one, update the other.
 */
export interface NamespaceFilter {
  /** Selected schemas to filter by (empty = show all) */
  schemas: string[];
  /** Selected databases/catalogs to filter by (empty = show all) */
  databases: string[];
}

/**
 * Props for the GraphView component.
 */
export interface GraphViewProps {
  /** Optional CSS class name */
  className?: string;
  /** Callback when a node is clicked */
  onNodeClick?: (node: Node) => void;
  /** Ref to the graph container div for export functionality */
  graphContainerRef?: RefObject<HTMLDivElement | null>;
  /** Node ID to focus/zoom to (will pan and zoom to center this node) */
  focusNodeId?: string;
  /** Callback when focus has been applied (so parent can clear the focusNodeId) */
  onFocusApplied?: () => void;
  /** Controlled search term - when provided, uses this instead of internal state */
  controlledSearchTerm?: string;
  /** Callback when search term changes - called with the new search term */
  onSearchTermChange?: (searchTerm: string) => void;
  /** Initial viewport to restore (zoom/pan position) */
  initialViewport?: ViewportState;
  /** Callback when viewport changes (zoom/pan) - debounced */
  onViewportChange?: (viewport: ViewportState) => void;
  /** Trigger to fit view to all nodes (increment to trigger) */
  fitViewTrigger?: number;
  /** Namespace filter - when provided, only shows nodes matching the filter */
  namespaceFilter?: NamespaceFilter;
}

/**
 * Props for the SqlView component.
 */
export interface SqlViewProps {
  /** Optional CSS class name */
  className?: string;
  /** Whether the editor should be editable */
  editable?: boolean;
  /** Callback when the SQL content changes */
  onChange?: (sql: string) => void;
  /** Controlled value for the SQL editor. When provided, uses controlled mode. */
  value?: string;
  /** Whether dark mode is active (for editor theming) */
  isDark?: boolean;
  /** Span to highlight and scroll to in the editor (for controlled mode navigation) */
  highlightedSpan?: Span | null;
  /** Source file currently shown in controlled mode; used to scope reveal lookups safely */
  analyzedSourceName?: string;
  /**
   * SQL dialect used when the built-in completion source is active. Defaults
   * to `'generic'`. Only consulted while `editable` is true.
   */
  dialect?: Dialect;
  /**
   * Schema catalog forwarded to the completion engine for column resolution.
   * Optional; omit when no catalog is available.
   */
  completionSchema?: SchemaMetadata;
  /**
   * Disable the built-in SQL completion source. Defaults to `false`.
   * Useful when the embedder wants to supply its own completion extension.
   */
  disableCompletion?: boolean;
  /**
   * Callback invoked when the completion engine throws. If omitted, errors
   * are logged via `console.warn` with only the message (no full error
   * object) to avoid leaking SQL/schema details into shared consoles.
   */
  onCompletionError?: (error: unknown) => void;
}

/**
 * Props for the ColumnPanel component.
 */
export interface ColumnPanelProps {
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for the IssuesPanel component.
 */
export interface IssuesPanelProps {
  /** Optional CSS class name */
  className?: string;
  /** Callback when an issue is clicked */
  onIssueClick?: (issue: Issue) => void;
}

/**
 * Props for the LineageExplorer component.
 */
export interface LineageExplorerProps {
  /** The analysis result to display */
  result: AnalyzeResult | null;
  /** The SQL text to display */
  sql: string;
  /** Optional CSS class name */
  className?: string;
  /** Callback when SQL content changes in editable mode */
  onSqlChange?: (sql: string) => void;
  /** Visual theme (default: 'light') */
  theme?: 'light' | 'dark';
  /** Preferred default layout algorithm when the explorer first renders */
  defaultLayoutAlgorithm?: LayoutAlgorithm;
  /** SQL dialect used by the built-in completion source in editable mode. */
  dialect?: Dialect;
  /** Optional schema catalog forwarded to the built-in completion source. */
  completionSchema?: SchemaMetadata;
  /** Disable the built-in SQL completion source in the embedded editor. */
  disableCompletion?: boolean;
  /** Optional hook invoked when the built-in completion source throws. */
  onCompletionError?: (error: unknown) => void;
}

/**
 * Data structure for script/file nodes in the graph visualization (script-level view).
 */
export interface ScriptNodeData extends Record<string, unknown> {
  /** Display name of the script or file */
  label: string;
  /** Source name (file path or identifier) */
  sourceName: string;
  /** Tables read by this script */
  tablesRead: string[];
  /** Tables written by this script */
  tablesWritten: string[];
  /** Number of statements in this script */
  statementCount: number;
  /** Whether this node is currently selected */
  isSelected: boolean;
  /** Whether this node matches the current search term */
  isHighlighted: boolean;
}

/**
 * Data structure for table/view/CTE nodes in the graph visualization.
 */
export interface TableNodeData extends Record<string, unknown> {
  /** Display name of the table, view, or CTE */
  label: string;
  /** Type of node: regular table, view, CTE, or virtual output */
  nodeType: 'table' | 'view' | 'cte' | 'virtualOutput';
  /** Whether this CTE is recursive (self-referential) */
  isRecursive?: boolean;
  /** List of columns belonging to this table */
  columns: ColumnNodeInfo[];
  /** Whether this node is currently selected */
  isSelected: boolean;
  /** Whether this node is collapsed */
  isCollapsed: boolean;
  /** Whether this node matches the current search term */
  isHighlighted: boolean;
  /** True if this table is the primary FROM/base table in a join */
  isBaseTable?: boolean;
  /** Optional source file name */
  sourceName?: string;
  /** Number of columns hidden from resolvedSchema (0 if none) */
  hiddenColumnCount?: number;
  /** Number of columns hidden in column-lineage mode because they have no visible lineage */
  lineageHiddenColumnCount?: number;
  /** Filter predicates (WHERE/HAVING clauses) affecting this table */
  filters?: FilterPredicate[];
  /** Fully qualified name (e.g., "catalog.schema.table") */
  qualifiedName?: string;
  /** Schema name extracted from qualified name */
  schema?: string;
  /** Database/catalog name extracted from qualified name */
  database?: string;
  /** Plain-text description from a SQL `COMMENT ON TABLE` / inline table comment */
  description?: string;
}

/**
 * Information about a column node.
 */
export interface ColumnNodeInfo {
  /** Unique identifier for the column */
  id: string;
  /** Column name */
  name: string;
  /** Optional SQL expression for computed columns */
  expression?: string;
  /** Whether this column is part of a highlighted path */
  isHighlighted?: boolean;
  /** Optional source file name */
  sourceName?: string;
  /** Aggregation information if this column is aggregated or a grouping key */
  aggregation?: AggregationInfo;
  /** Plain-text description from a SQL `COMMENT ON COLUMN` / inline column comment */
  description?: string;
}

/**
 * Data structure for standalone column nodes in the graph visualization (column-level view).
 */
export interface ColumnNodeData extends Record<string, unknown> {
  /** Display name of the column */
  label: string;
  /** Parent table name */
  tableName: string;
  /** Optional SQL expression for computed columns */
  expression?: string;
  /** Whether this node is currently selected */
  isSelected: boolean;
  /** Whether this node matches the current search term */
  isHighlighted: boolean;
  /** Optional source file name */
  sourceName?: string;
}

export type { AnalyzeResult, Node, Edge, Issue, Span };
