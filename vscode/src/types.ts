/**
 * Types for the FlowScope SQL lineage analysis API.
 * Copied from @pondpilot/flowscope-core for standalone VSCode extension use.
 */

export const VALID_DIALECTS = [
  'generic',
  'ansi',
  'bigquery',
  'clickhouse',
  'databricks',
  'duckdb',
  'hive',
  'mssql',
  'mysql',
  'oracle',
  'postgres',
  'redshift',
  'snowflake',
  'sqlite',
] as const;

export type Dialect = (typeof VALID_DIALECTS)[number];

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

export type Encoding = 'utf8' | 'utf16';

export interface CompletionRequest {
  sql: string;
  dialect: Dialect;
  cursorOffset: number;
  encoding?: Encoding;
}

export type CompletionClause =
  | 'select'
  | 'from'
  | 'where'
  | 'join'
  | 'on'
  | 'groupBy'
  | 'having'
  | 'orderBy'
  | 'limit'
  | 'qualify'
  | 'window'
  | 'insert'
  | 'update'
  | 'delete'
  | 'with'
  | 'unknown';

export type CompletionTokenKind =
  | 'keyword'
  | 'identifier'
  | 'literal'
  | 'operator'
  | 'symbol'
  | 'unknown';

export interface CompletionToken {
  value: string;
  kind: CompletionTokenKind;
  span: Span;
}

export type CompletionItemKind =
  | 'keyword'
  | 'operator'
  | 'function'
  | 'snippet'
  | 'table'
  | 'column'
  | 'schemaTable';

export interface CompletionItem {
  label: string;
  insertText: string;
  kind: CompletionItemKind;
  category: string;
  score: number;
  clauseSpecific: boolean;
  detail?: string;
}

export interface CompletionItemsResult {
  clause: CompletionClause;
  token?: CompletionToken;
  shouldShow: boolean;
  items: CompletionItem[];
  error?: string;
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
