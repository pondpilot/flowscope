import type {
  AnalyzeResult,
  Node,
  Edge,
  Issue,
  Span,
  StatementLineage,
} from '@pondpilot/flowscope-core';

export interface LineageState {
  result: AnalyzeResult | null;
  sql: string;
  selectedNodeId: string | null;
  selectedStatementIndex: number;
  highlightedSpan: Span | null;
}

export interface LineageActions {
  setResult: (result: AnalyzeResult | null) => void;
  setSql: (sql: string) => void;
  selectNode: (nodeId: string | null) => void;
  selectStatement: (index: number) => void;
  highlightSpan: (span: Span | null) => void;
}

export interface LineageContextValue {
  state: LineageState;
  actions: LineageActions;
}

export interface GraphViewProps {
  className?: string;
  onNodeClick?: (node: Node) => void;
}

export interface SqlViewProps {
  className?: string;
  editable?: boolean;
  onChange?: (sql: string) => void;
}

export interface ColumnPanelProps {
  className?: string;
}

export interface IssuesPanelProps {
  className?: string;
  onIssueClick?: (issue: Issue) => void;
}

export interface LineageExplorerProps {
  result: AnalyzeResult | null;
  sql: string;
  className?: string;
  onSqlChange?: (sql: string) => void;
}

export interface TableNodeData extends Record<string, unknown> {
  label: string;
  nodeType: 'table' | 'cte';
  columns: ColumnNodeInfo[];
  isSelected: boolean;
}

export interface ColumnNodeInfo {
  id: string;
  name: string;
  expression?: string;
}

export { AnalyzeResult, Node, Edge, Issue, Span, StatementLineage };
