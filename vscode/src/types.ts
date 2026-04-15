/**
 * Types for the FlowScope SQL lineage analysis API.
 * Copied from @pondpilot/flowscope-core for standalone VSCode extension use.
 */

export type Dialect =
  | 'generic'
  | 'ansi'
  | 'bigquery'
  | 'clickhouse'
  | 'databricks'
  | 'duckdb'
  | 'hive'
  | 'mssql'
  | 'mysql'
  | 'postgres'
  | 'redshift'
  | 'snowflake'
  | 'sqlite';

export interface AnalyzeRequest {
  sql: string;
  dialect: Dialect;
  sourceName?: string;
  options?: AnalysisOptions;
}

export interface AnalysisOptions {
  enableColumnLineage?: boolean;
}

export interface AnalyzeResult {
  statements: StatementMeta[];
  nodes: Node[];
  edges: Edge[];
  issues: Issue[];
  summary: Summary;
}

export interface StatementMeta {
  statementIndex: number;
  statementType: string;
  sourceName?: string;
  span?: Span;
  joinCount: number;
  complexityScore: number;
}

export interface Node {
  id: string;
  type: NodeType;
  label: string;
  qualifiedName?: string;
  canonicalName?: CanonicalName;
  statementIds: number[];
  expression?: string;
  span?: Span;
  nameSpans?: Span[];
  bodySpan?: Span;
  filters?: FilterPredicate[];
  aggregation?: AggregationInfo;
  metadata?: Record<string, unknown>;
}

export type NodeType = 'table' | 'view' | 'cte' | 'output' | 'column';

export interface FilterPredicate {
  expression: string;
  clauseType: FilterClauseType;
}

export type FilterClauseType = 'WHERE' | 'HAVING' | 'JOIN_ON';

export interface AggregationInfo {
  isGroupingKey: boolean;
  function?: string;
  distinct?: boolean;
}

export interface Edge {
  id: string;
  from: string;
  to: string;
  type: EdgeType;
  expression?: string;
  operation?: string;
  joinType?: JoinType;
  joinCondition?: string;
  approximate?: boolean;
  statementIds: number[];
}

export type EdgeType =
  | 'ownership'
  | 'data_flow'
  | 'derivation'
  | 'join_dependency'
  | 'cross_statement';

export type JoinType =
  | 'INNER'
  | 'LEFT'
  | 'RIGHT'
  | 'FULL'
  | 'CROSS'
  | 'LEFT_SEMI'
  | 'RIGHT_SEMI'
  | 'LEFT_ANTI'
  | 'RIGHT_ANTI'
  | 'CROSS_APPLY'
  | 'OUTER_APPLY'
  | 'AS_OF';

export interface CanonicalName {
  catalog?: string;
  schema?: string;
  name: string;
  column?: string;
}

export interface Issue {
  severity: Severity;
  code: string;
  message: string;
  span?: Span;
  statementIndex?: number;
}

export type Severity = 'error' | 'warning' | 'info';

export interface Span {
  start: number;
  end: number;
}

export interface Summary {
  statementCount: number;
  tableCount: number;
  columnCount: number;
  joinCount: number;
  complexityScore: number;
  issueCount: IssueCount;
  hasErrors: boolean;
}

export interface IssueCount {
  errors: number;
  warnings: number;
  infos: number;
}

/**
 * Return the nodes from an `AnalyzeResult` that participate in the given
 * statement index. Uses the flat `result.nodes` collection; matching is
 * by `statementIds.includes(statementIndex)`.
 */
export function nodesInStatement(result: AnalyzeResult, statementIndex: number): Node[] {
  return result.nodes.filter((n) => n.statementIds.includes(statementIndex));
}

/**
 * Return the edges from an `AnalyzeResult` that participate in the given
 * statement index. Uses the flat `result.edges` collection; matching is
 * by `statementIds.includes(statementIndex)`.
 */
export function edgesInStatement(result: AnalyzeResult, statementIndex: number): Edge[] {
  return result.edges.filter((e) => e.statementIds.includes(statementIndex));
}
