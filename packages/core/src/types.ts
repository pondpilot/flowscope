/**
 * Types for the FlowScope SQL lineage analysis API.
 * @module types
 */

// Request Types

/** SQL dialect for parsing and analysis. */
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

/** Case sensitivity mode for identifier normalization. */
export type CaseSensitivity = 'dialect' | 'lower' | 'upper' | 'exact';

/**
 * A request to analyze SQL for data lineage.
 *
 * This is the main entry point for the analysis API. It accepts SQL code along with
 * optional dialect and schema information to produce accurate lineage graphs.
 */
export interface AnalyzeRequest {
  /** The SQL code to analyze (UTF-8 string, multi-statement supported) */
  sql: string;
  /** Optional list of source files to analyze (alternative to single `sql` field) */
  files?: FileSource[];
  /** SQL dialect */
  dialect: Dialect;
  /** Optional source name (file path or script identifier) for grouping */
  sourceName?: string;
  /** Optional analysis options */
  options?: AnalysisOptions;
  /** Optional schema metadata for accurate column resolution */
  schema?: SchemaMetadata;
}

export interface FileSource {
  name: string;
  content: string;
}

/** Graph detail level for visualization. */
export type GraphDetailLevel = 'script' | 'table' | 'column';

/** Options controlling the analysis behavior. */
export interface AnalysisOptions {
  /** Enable column-level lineage (default: true) */
  enableColumnLineage?: boolean;
  /** Preferred graph detail level for visualization (does not affect analysis) */
  graphDetailLevel?: GraphDetailLevel;
}

/**
 * Schema metadata for accurate column and table resolution.
 *
 * When provided, allows the analyzer to resolve ambiguous references and
 * produce more accurate lineage information.
 */
export interface SchemaMetadata {
  /** Default catalog applied to unqualified identifiers */
  defaultCatalog?: string;
  /** Default schema applied to unqualified identifiers */
  defaultSchema?: string;
  /** Ordered list mirroring database search_path behavior */
  searchPath?: SchemaNamespaceHint[];
  /** Override for identifier normalization (default 'dialect') */
  caseSensitivity?: CaseSensitivity;
  /** Canonical table representations */
  tables?: SchemaTable[];
  /** Global toggle for implied schema capture (default: true) */
  allowImplied?: boolean;
}

export interface SchemaNamespaceHint {
  catalog?: string;
  schema: string;
}

export interface SchemaTable {
  catalog?: string;
  schema?: string;
  name: string;
  columns?: ColumnSchema[];
}

export interface ColumnSchema {
  name: string;
  dataType?: string;
  /** True if this column is a primary key (or part of composite PK) */
  isPrimaryKey?: boolean;
  /** Foreign key reference if this column references another table */
  foreignKey?: ForeignKeyRef;
}

/** A foreign key reference to another table's column. */
export interface ForeignKeyRef {
  /** The referenced table name (may be qualified) */
  table: string;
  /** The referenced column name */
  column: string;
}

// Response Types

/**
 * The result of analyzing SQL for data lineage.
 *
 * Contains per-statement lineage graphs, a global lineage graph spanning all statements,
 * any issues encountered during analysis, and summary statistics.
 */
export interface AnalyzeResult {
  /** Per-statement lineage analysis results */
  statements: StatementLineage[];
  /** Global lineage graph spanning all statements */
  globalLineage: GlobalLineage;
  /** All issues encountered during analysis */
  issues: Issue[];
  /** Summary statistics */
  summary: Summary;
  /** Effective schema used during analysis (imported + implied) */
  resolvedSchema?: ResolvedSchemaMetadata;
}

/** Lineage information for a single SQL statement. */
export interface StatementLineage {
  /** Zero-based index of the statement in the input SQL */
  statementIndex: number;
  /** Type of SQL statement */
  statementType: string;
  /** Optional source name (file path or script identifier) for grouping */
  sourceName?: string;
  /** All nodes in the lineage graph for this statement */
  nodes: Node[];
  /** All edges connecting nodes in the lineage graph */
  edges: Edge[];
  /** Optional span of the entire statement in source SQL */
  span?: Span;
  /** Number of JOIN operations in this statement */
  joinCount: number;
  /** Complexity score (1-100) based on query structure */
  complexityScore: number;
}

/** A node in the lineage graph (table, CTE, or column). */
export interface Node {
  /** Stable content-based hash ID */
  id: string;
  /** Node type */
  type: NodeType;
  /** Human-readable label (short name) */
  label: string;
  /** Fully qualified name when available */
  qualifiedName?: string;
  /** SQL expression text for computed columns */
  expression?: string;
  /** Source location in original SQL */
  span?: Span;
  /** Extensible metadata for future use */
  metadata?: Record<string, unknown>;
  /** How this table was resolved (imported, implied, or unknown) */
  resolutionSource?: ResolutionSource;
  /** Filter predicates (WHERE clause conditions) that affect this table's rows */
  filters?: FilterPredicate[];
  /** For table nodes that are JOINed: the type of join used to include this table */
  joinType?: JoinType;
  /** For table nodes that are JOINed: the join condition (ON clause) */
  joinCondition?: string;
  /** For column nodes: aggregation information if this column is aggregated or a grouping key */
  aggregation?: AggregationInfo;
}

/** The type of a node in the lineage graph. */
export type NodeType = 'table' | 'view' | 'cte' | 'column';

/** Table-like node types that can contain columns and appear in FROM clauses. */
export type TableLikeNodeType = 'table' | 'view' | 'cte';

/** Returns true if the node type is table-like (table, view, or CTE). */
export function isTableLikeType(type: NodeType): type is TableLikeNodeType {
  return type === 'table' || type === 'view' || type === 'cte';
}

/** A filter predicate from a WHERE, HAVING, or JOIN ON clause. */
export interface FilterPredicate {
  /** The SQL expression text of the predicate */
  expression: string;
  /** Where this filter appears in the query */
  clauseType: FilterClauseType;
}

/** The type of SQL clause where a filter predicate appears. */
export type FilterClauseType = 'WHERE' | 'HAVING' | 'JOIN_ON';

/**
 * Information about aggregation applied to a column.
 *
 * This tracks when a column is the result of an aggregation operation (like SUM, COUNT, AVG),
 * which indicates a cardinality reduction (1:many collapse) in the data flow.
 */
export interface AggregationInfo {
  /** True if this column is a GROUP BY key (preserves row identity within groups) */
  isGroupingKey: boolean;
  /** The aggregation function used (e.g., "SUM", "COUNT", "AVG"). Undefined if this is a grouping key. */
  function?: string;
  /** True if this aggregation uses DISTINCT (e.g., COUNT(DISTINCT col)) */
  distinct?: boolean;
}

/** An edge connecting two nodes in the lineage graph. */
export interface Edge {
  /** Stable content-based hash ID */
  id: string;
  /** Source node ID */
  from: string;
  /** Target node ID */
  to: string;
  /** Edge type */
  type: EdgeType;
  /** Optional: SQL expression if this edge represents a transformation */
  expression?: string;
  /** Optional: operation label ('JOIN', 'UNION', 'AGGREGATE', etc.) */
  operation?: string;
  /** Optional: specific join type for JOIN edges */
  joinType?: JoinType;
  /** Optional: join condition expression (ON clause) */
  joinCondition?: string;
  /** Extensible metadata for future use */
  metadata?: Record<string, unknown>;
  /** True if this edge represents approximate/uncertain lineage */
  approximate?: boolean;
}

/** The type of an edge in the lineage graph. */
export type EdgeType = 'ownership' | 'data_flow' | 'derivation' | 'cross_statement';

/** The type of SQL JOIN operation. */
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

/**
 * Global lineage graph spanning all statements in the analyzed SQL.
 *
 * Provides a unified view of data flow across multiple statements.
 */
export interface GlobalLineage {
  /** All unique nodes across all statements */
  nodes: GlobalNode[];
  /** All edges representing cross-statement data flow */
  edges: GlobalEdge[];
}

export interface GlobalNode {
  /** Stable ID derived from canonical identifier */
  id: string;
  /** Node type */
  type: NodeType;
  /** Human-readable label */
  label: string;
  /** Canonical name for cross-statement matching */
  canonicalName: CanonicalName;
  /** References to statements that use this node */
  statementRefs: StatementRef[];
  /** Extensible metadata */
  metadata?: Record<string, unknown>;
  /** How this table was resolved (imported, implied, or unknown) */
  resolutionSource?: ResolutionSource;
}

export interface CanonicalName {
  catalog?: string;
  schema?: string;
  name: string;
  column?: string;
}

export interface StatementRef {
  /** Statement index in the original request */
  statementIndex: number;
  /** ID of the local node inside that statement graph (if available) */
  nodeId?: string;
}

export interface GlobalEdge {
  id: string;
  from: string;
  to: string;
  type: EdgeType;
  producerStatement?: StatementRef;
  consumerStatement?: StatementRef;
  metadata?: Record<string, unknown>;
}

/** An issue encountered during SQL analysis (error, warning, or info). */
export interface Issue {
  /** Severity level */
  severity: Severity;
  /** Machine-readable issue code */
  code: string;
  /** Human-readable error message */
  message: string;
  /** Optional: location in source SQL where issue occurred */
  span?: Span;
  /** Optional: which statement index this issue relates to */
  statementIndex?: number;
}

export type Severity = 'error' | 'warning' | 'info';

/** A byte range in the source SQL string. */
export interface Span {
  /** Byte offset from start of SQL string (inclusive) */
  start: number;
  /** Byte offset from start of SQL string (exclusive) */
  end: number;
}

/** Summary statistics for the analysis result. */
export interface Summary {
  /** Total number of statements analyzed */
  statementCount: number;
  /** Total unique tables/CTEs discovered across all statements */
  tableCount: number;
  /** Total unique columns discovered across all statements */
  columnCount: number;
  /** Total number of JOIN operations */
  joinCount: number;
  /** Complexity score (1-100) based on query structure */
  complexityScore: number;
  /** Issue counts by severity */
  issueCount: IssueCount;
  /** Quick check: true if any errors were encountered */
  hasErrors: boolean;
}

/** Counts of issues by severity level. */
export interface IssueCount {
  /** Number of error-level issues */
  errors: number;
  /** Number of warning-level issues */
  warnings: number;
  /** Number of info-level issues */
  infos: number;
}

/** Machine-readable issue codes. */
export const IssueCodes = {
  PARSE_ERROR: 'PARSE_ERROR',
  INVALID_REQUEST: 'INVALID_REQUEST',
  DIALECT_FALLBACK: 'DIALECT_FALLBACK',
  UNSUPPORTED_SYNTAX: 'UNSUPPORTED_SYNTAX',
  UNSUPPORTED_RECURSIVE_CTE: 'UNSUPPORTED_RECURSIVE_CTE',
  APPROXIMATE_LINEAGE: 'APPROXIMATE_LINEAGE',
  UNKNOWN_COLUMN: 'UNKNOWN_COLUMN',
  UNKNOWN_TABLE: 'UNKNOWN_TABLE',
  UNRESOLVED_REFERENCE: 'UNRESOLVED_REFERENCE',
  CANCELLED: 'CANCELLED',
  PAYLOAD_SIZE_WARNING: 'PAYLOAD_SIZE_WARNING',
  MEMORY_LIMIT_EXCEEDED: 'MEMORY_LIMIT_EXCEEDED',
} as const;

// Resolved Schema Types

/** Resolved schema metadata showing the effective schema used during analysis. */
export interface ResolvedSchemaMetadata {
  /** All tables used during analysis (imported + implied) */
  tables: ResolvedSchemaTable[];
}

/** A table in the resolved schema with origin metadata. */
export interface ResolvedSchemaTable {
  catalog?: string;
  schema?: string;
  name: string;
  columns: ResolvedColumnSchema[];
  /** Origin of this table's schema information */
  origin: SchemaOrigin;
  /** For implied tables: which statement created it */
  sourceStatementIndex?: number;
  /** Timestamp when this entry was created/updated (ISO 8601) */
  updatedAt: string;
  /** True if this is a temporary table */
  temporary?: boolean;
  /** Table-level constraints (composite PKs, FKs, etc.) */
  constraints?: TableConstraintInfo[];
}

/** A column in the resolved schema with origin tracking. */
export interface ResolvedColumnSchema {
  name: string;
  dataType?: string;
  /** Column-level origin (can differ from table origin in future merging) */
  origin?: SchemaOrigin;
  /** True if this column is a primary key (or part of composite PK) */
  isPrimaryKey?: boolean;
  /** Foreign key reference if this column references another table */
  foreignKey?: ForeignKeyRef;
}

/** Information about a table-level constraint (composite PK, FK, etc.). */
export interface TableConstraintInfo {
  /** Type of constraint */
  constraintType: ConstraintType;
  /** Columns involved in this constraint */
  columns: string[];
  /** For FK: the referenced table */
  referencedTable?: string;
  /** For FK: the referenced columns */
  referencedColumns?: string[];
}

/** Type of table constraint. */
export type ConstraintType = 'primary_key' | 'foreign_key' | 'unique';

/** The origin of schema information. */
export type SchemaOrigin = 'imported' | 'implied';

/** How a table reference was resolved during analysis. */
export type ResolutionSource = 'imported' | 'implied' | 'unknown';
