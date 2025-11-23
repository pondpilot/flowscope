/**
 * Types for the FlowScope SQL lineage analysis API.
 * @module types
 */

// Request Types

/** SQL dialect for parsing and analysis. */
export type Dialect = 'generic' | 'postgres' | 'snowflake' | 'bigquery';

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
}

/** The type of a node in the lineage graph. */
export type NodeType = 'table' | 'cte' | 'column';

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
  /** Extensible metadata for future use */
  metadata?: Record<string, unknown>;
}

/** The type of an edge in the lineage graph. */
export type EdgeType = 'ownership' | 'data_flow' | 'derivation' | 'cross_statement';

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
