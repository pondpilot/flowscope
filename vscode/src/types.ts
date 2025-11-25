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
  statements: StatementLineage[];
  globalLineage: GlobalLineage;
  issues: Issue[];
  summary: Summary;
}

export interface StatementLineage {
  statementIndex: number;
  statementType: string;
  sourceName?: string;
  nodes: Node[];
  edges: Edge[];
  span?: Span;
  joinCount: number;
  complexityScore: number;
}

export interface Node {
  id: string;
  type: NodeType;
  label: string;
  qualifiedName?: string;
  expression?: string;
  span?: Span;
  filters?: FilterPredicate[];
  joinType?: JoinType;
  joinCondition?: string;
  aggregation?: AggregationInfo;
}

export type NodeType = 'table' | 'cte' | 'column';

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
}

export type EdgeType = 'ownership' | 'data_flow' | 'derivation' | 'cross_statement';

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

export interface GlobalLineage {
  nodes: GlobalNode[];
  edges: GlobalEdge[];
}

export interface GlobalNode {
  id: string;
  type: NodeType;
  label: string;
  canonicalName: CanonicalName;
  statementRefs: StatementRef[];
}

export interface CanonicalName {
  catalog?: string;
  schema?: string;
  name: string;
  column?: string;
}

export interface StatementRef {
  statementIndex: number;
  nodeId?: string;
}

export interface GlobalEdge {
  id: string;
  from: string;
  to: string;
  type: EdgeType;
  producerStatement?: StatementRef;
  consumerStatement?: StatementRef;
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
