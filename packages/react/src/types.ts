import type { RefObject } from 'react';
import type {
  AnalyzeResult,
  Node,
  Edge,
  Issue,
  Span,
  StatementLineage,
  SchemaTable,
} from '@pondpilot/flowscope-core';

/**
 * View mode for the lineage graph visualization.
 * Controls the level of detail displayed in the graph.
 */
export type LineageViewMode = 'script' | 'table' | 'column';

/**
 * Props for the SchemaView component.
 */
export interface SchemaViewProps {
  /** Array of schema tables to display */
  schema: SchemaTable[];
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
  /** Index of the currently selected SQL statement */
  selectedStatementIndex: number;
  /** The currently highlighted span in the SQL editor, or null if none */
  highlightedSpan: Span | null;
  /** Search term for filtering/highlighting nodes in the graph */
  searchTerm: string;
  /** Current view mode for the lineage graph */
  viewMode: LineageViewMode;
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
  /** Select a statement by index */
  selectStatement: (index: number) => void;
  /** Highlight a span in the SQL editor, or null to clear */
  highlightSpan: (span: Span | null) => void;
  /** Update the search term for node filtering */
  setSearchTerm: (term: string) => void;
  /** Update the view mode for the lineage graph */
  setViewMode: (mode: LineageViewMode) => void;
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
 * Props for the GraphView component.
 */
export interface GraphViewProps {
  /** Optional CSS class name */
  className?: string;
  /** Callback when a node is clicked */
  onNodeClick?: (node: Node) => void;
  /** Ref to the graph container div for export functionality */
  graphContainerRef?: RefObject<HTMLDivElement>;
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
 * Data structure for table/CTE nodes in the graph visualization.
 */
export interface TableNodeData extends Record<string, unknown> {
  /** Display name of the table or CTE */
  label: string;
  /** Type of node: regular table or CTE */
  nodeType: 'table' | 'cte';
  /** List of columns belonging to this table */
  columns: ColumnNodeInfo[];
  /** Whether this node is currently selected */
  isSelected: boolean;
  /** Whether this node matches the current search term */
  isHighlighted: boolean;
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
}

export { AnalyzeResult, Node, Edge, Issue, Span, StatementLineage };
