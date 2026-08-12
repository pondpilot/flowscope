/**
 * GENERATED FILE — DO NOT EDIT.
 *
 * Source: docs/api_schema.json (generated from the authoritative Rust API types).
 * Regenerate with: just generate-ts-types
 *
 * Rust Option<T> fields are represented as optional TypeScript properties. Their
 * redundant JSON Schema null branch is omitted to preserve the public TS API's
 * ergonomic prop?: T surface; required nullable fields retain | null.
 */

/**
 * A request to analyze SQL for data lineage.
 *
 * This is the main entry point for the analysis API. It accepts SQL code along with
 * optional dialect and schema information to produce accurate lineage graphs.
 */
export interface AnalyzeRequest {
  /**
   * The SQL code to analyze (UTF-8 string, multi-statement supported)
   */
  sql: string;
  /**
   * Optional list of source files to analyze (alternative to single `sql` field)
   */
  files?: FileSource[];
  /**
   * SQL dialect
   */
  dialect: Dialect;
  /**
   * Optional source name (file path or script identifier) for grouping
   */
  sourceName?: string;
  /**
   * Optional analysis options
   */
  options?: AnalysisOptions;
  /**
   * Optional schema metadata for accurate column resolution
   */
  schema?: SchemaMetadata;
  /**
   * Optional template configuration for preprocessing Jinja/dbt-style SQL
   */
  templateConfig?: TemplateConfig;
}

export interface FileSource {
  name: string;
  content: string;
}

/**
 * SQL dialect for parsing and analysis.
 *
 * Different dialects have different syntax rules and identifier normalization behavior.
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
  | 'oracle'
  | 'postgres'
  | 'redshift'
  | 'snowflake'
  | 'sqlite';

/**
 * Options controlling the analysis behavior.
 */
export interface AnalysisOptions {
  /**
   * Enable column-level lineage (Phase 2+, default true when implemented)
   */
  enableColumnLineage?: boolean;
  /**
   * Preferred graph detail level for visualization (does not affect analysis)
   */
  graphDetailLevel?: GraphDetailLevel;
  /**
   * Hide CTEs from output, creating bypass edges (A→CTE→B becomes A→B)
   */
  hideCtes?: boolean;
  /**
   * SQL lint configuration
   */
  lint?: LintConfig;
}

/**
 * Graph detail level for visualization.
 *
 * Controls the granularity of the lineage graph returned by the analyzer.
 */
export type GraphDetailLevel = 'script' | 'table' | 'column';

/**
 * Configuration for the SQL linter.
 *
 * Controls which lint rules are enabled/disabled. By default, all rules are enabled.
 */
export interface LintConfig {
  /**
   * Master toggle for linting (default: true).
   */
  enabled?: boolean;
  /**
   * List of rule codes to disable (e.g., ["LINT_AM_008"]).
   */
  disabledRules?: string[];
  /**
   * Per-rule option objects keyed by rule reference (`LINT_*`, `AL01`,
   * `aliasing.table`, etc).
   */
  ruleConfigs?: Record<string, unknown>;
}

/**
 * Schema metadata for accurate column and table resolution.
 *
 * When provided, allows the analyzer to resolve ambiguous references and
 * produce more accurate lineage information.
 */
export interface SchemaMetadata {
  /**
   * Default catalog applied to unqualified identifiers
   */
  defaultCatalog?: string;
  /**
   * Default schema applied to unqualified identifiers
   */
  defaultSchema?: string;
  /**
   * Ordered list mirroring database search_path behavior
   */
  searchPath?: SchemaNamespaceHint[];
  /**
   * Override for identifier normalization (default 'dialect')
   */
  caseSensitivity?: CaseSensitivity;
  /**
   * Canonical table representations
   */
  tables?: SchemaTable[];
  /**
   * Global toggle for implied schema capture (default: true)
   * When false, only imported schema is used; workload DDL is ignored
   */
  allowImplied?: boolean;
}

export interface SchemaNamespaceHint {
  catalog?: string;
  schema: string;
}

/**
 * Case sensitivity for identifier normalization.
 */
export type CaseSensitivity = 'dialect' | 'lower' | 'upper' | 'exact';

export interface SchemaTable {
  catalog?: string;
  schema?: string;
  name: string;
  columns?: ColumnSchema[];
}

export interface ColumnSchema {
  name: string;
  dataType?: string;
  /**
   * True if this column is a primary key (or part of composite PK)
   */
  isPrimaryKey?: boolean;
  /**
   * Foreign key reference if this column references another table
   */
  foreignKey?: ForeignKeyRef;
}

/**
 * A foreign key reference to another table's column.
 */
export interface ForeignKeyRef {
  /**
   * The referenced table name (may be qualified)
   */
  table: string;
  /**
   * The referenced column name
   */
  column: string;
}

/**
 * Configuration for SQL template preprocessing.
 */
export interface TemplateConfig {
  /**
   * The templating mode to use.
   */
  mode?: TemplateMode;
  /**
   * Context variables available to the template.
   *
   * For dbt mode, variables under the "vars" key are accessible via `var()`.
   */
  context?: Record<string, unknown>;
}

/**
 * Templating mode for SQL preprocessing.
 */
export type TemplateMode = 'raw' | 'jinja' | 'dbt';

/**
 * The result of analyzing SQL for data lineage.
 *
 * Contains a single flat lineage graph spanning all statements, per-statement
 * metadata, any issues encountered during analysis, and summary statistics.
 * Each `Node` / `Edge` records the `statementIds` it participates in, so
 * consumers can filter down to a single statement or aggregate across all of
 * them without maintaining parallel collections.
 */
export interface AnalyzeResult {
  /**
   * Per-statement metadata (type, span, complexity, resolved SQL).
   * The graph itself lives in the top-level `nodes` / `edges`.
   */
  statements: StatementMeta[];
  /**
   * All nodes in the lineage graph. Nodes shared across statements
   * (for example, a table read by two queries) appear once with
   * `statementIds` listing every statement they participate in.
   */
  nodes: Node[];
  /**
   * All edges in the lineage graph. Intra-statement edges carry a single
   * entry in `statementIds`; `EdgeType::CrossStatement` edges connect nodes
   * whose statement groups differ.
   */
  edges: Edge[];
  /**
   * All issues encountered during analysis
   */
  issues: Issue[];
  /**
   * Summary statistics
   */
  summary: Summary;
  /**
   * Effective schema used during analysis (imported + implied)
   */
  resolvedSchema?: ResolvedSchemaMetadata;
}

/**
 * Per-statement metadata. The lineage graph itself is shared in
 * `AnalyzeResult.nodes` / `.edges`; this struct only carries facts about the
 * statement as a whole.
 */
export interface StatementMeta {
  /**
   * Zero-based index of the statement in the input SQL
   */
  statementIndex: number;
  /**
   * Type of SQL statement
   */
  statementType: string;
  /**
   * Optional source name (file path or script identifier) for grouping
   */
  sourceName?: string;
  /**
   * Optional span of the entire statement in source SQL
   */
  span?: Span;
  /**
   * Number of JOIN operations in this statement
   */
  joinCount: number;
  /**
   * Complexity score (1-100) based on query structure
   */
  complexityScore: number;
  /**
   * Resolved/compiled SQL after template expansion (e.g., dbt Jinja rendering).
   * Only present when templating was run in non-raw mode. May contain sensitive
   * values from template variables (e.g., database credentials).
   */
  resolvedSql?: string;
}

/**
 * A byte range in the source SQL string.
 */
export interface Span {
  /**
   * Byte offset from start of SQL string (inclusive)
   */
  start: number;
  /**
   * Byte offset from start of SQL string (exclusive)
   */
  end: number;
}

/**
 * A node in the lineage graph (table, CTE, or column).
 */
export interface Node {
  /**
   * Stable content-based hash ID
   */
  id: string;
  /**
   * Node type
   */
  type: NodeType;
  /**
   * Human-readable label (short name)
   */
  label: string;
  /**
   * Fully qualified display name when available.
   *
   * This is a cosmetic string intended for UI rendering. It is **not** a
   * stable identity — prefer `canonical_name` for cross-statement matching,
   * schema joins, or any equality comparison that must survive dialect
   * quoting, casing, or alias differences.
   */
  qualifiedName?: string;
  /**
   * Structured canonical identity (catalog.schema.name[.column]) used to
   * match the same entity across statements. Only populated for nodes
   * whose identity is globally meaningful — table-likes and columns owned
   * by them. Statement-scoped nodes (CTEs, CTE columns, self-join instance
   * columns) omit this.
   *
   * This is the authoritative identity for cross-statement matching.
   */
  canonicalName?: CanonicalName;
  /**
   * Zero-based indices of every statement this node participates in.
   *
   * Invariants:
   * - Always has at least one entry.
   * - Sorted ascending and deduplicated.
   * - A node shared across statements (e.g. a table referenced by two
   *   queries) lists every statement that references it.
   */
  statementIds?: number[];
  /**
   * SQL expression text for computed columns
   */
  expression?: string;
  /**
   * Source location in original SQL
   */
  span?: Span;
  /**
   * Source locations for this node's own relation-name occurrences.
   *
   * Ordered by lexical occurrence (left-to-right in the SQL text). Includes
   * the declaration plus relation occurrences we can associate with the
   * node (for example, a CTE name after `WITH` and each `FROM cte_name` /
   * `JOIN cte_name` usage). Self-joins intentionally produce distinct node
   * instances (one per lexical occurrence), each carrying its own
   * single-entry `name_spans`, so repeated table names map to the correct
   * node.
   *
   * Populated for table, view, and CTE nodes only. Column qualifier occurrences
   * are not yet included, so column nodes omit this field and callers should
   * fall back to `span` (use `Node::all_name_spans` for a unified view).
   */
  nameSpans?: Span[];
  /**
   * For CTE nodes: the source location of the CTE body (the parenthesized
   * subquery after `AS`). Enables the UI to highlight the definition body
   * separately from the CTE name.
   */
  bodySpan?: Span;
  /**
   * Extensible metadata for future use
   */
  metadata?: Record<string, unknown>;
  /**
   * How this table was resolved (imported, implied, or unknown)
   */
  resolutionSource?: ResolutionSource;
  /**
   * Filter predicates (WHERE clause conditions) that affect this table's rows
   */
  filters?: FilterPredicate[];
  /**
   * For column nodes: aggregation information if this column is aggregated or a grouping key.
   * Presence indicates the query uses GROUP BY; the fields indicate the column's role.
   */
  aggregation?: AggregationInfo;
  /**
   * Plain-text description harvested from SQL comments on the declaration.
   *
   * Sources: `COMMENT ON TABLE`, `COMMENT ON COLUMN`, and inline
   * `CREATE TABLE ... COMMENT '...'` clauses (column and table level).
   * Free-form SQL line/block comments are not considered.
   */
  description?: string;
}

/**
 * The type of a node in the lineage graph.
 */
export type NodeType = 'table' | 'view' | 'cte' | 'output' | 'column';

export interface CanonicalName {
  catalog?: string;
  schema?: string;
  name: string;
  column?: string;
}

/**
 * How a table reference was resolved during analysis.
 */
export type ResolutionSource = 'imported' | 'implied' | 'unknown';

/**
 * A filter predicate from a WHERE, HAVING, or JOIN ON clause.
 */
export interface FilterPredicate {
  /**
   * The SQL expression text of the predicate
   */
  expression: string;
  /**
   * Where this filter appears in the query
   */
  clauseType: FilterClauseType;
}

/**
 * The type of SQL clause where a filter predicate appears.
 */
export type FilterClauseType = 'WHERE' | 'HAVING' | 'JOIN_ON';

/**
 * Information about aggregation applied to a column.
 *
 * This tracks when a column is the result of an aggregation operation (like SUM, COUNT, AVG),
 * which indicates a cardinality reduction (1:many collapse) in the data flow.
 */
export interface AggregationInfo {
  /**
   * True if this column is a GROUP BY key (preserves row identity within groups)
   */
  isGroupingKey: boolean;
  /**
   * The aggregation function used (e.g., "SUM", "COUNT", "AVG")
   * None if this is a grouping key or non-aggregated column
   */
  function?: string;
  /**
   * True if this aggregation uses DISTINCT (e.g., COUNT(DISTINCT col))
   */
  distinct?: boolean;
}

/**
 * An edge connecting two nodes in the lineage graph.
 */
export interface Edge {
  /**
   * Stable content-based hash ID
   */
  id: string;
  /**
   * Source node ID
   */
  from: string;
  /**
   * Target node ID
   */
  to: string;
  /**
   * Edge type
   */
  type: EdgeType;
  /**
   * Optional: SQL expression if this edge represents a transformation
   */
  expression?: string;
  /**
   * Optional: operation label ('JOIN', 'UNION', 'AGGREGATE', etc.)
   */
  operation?: string;
  /**
   * Optional: specific join type for JOIN edges (INNER, LEFT, RIGHT, FULL, CROSS, etc.)
   */
  joinType?: JoinType;
  /**
   * Optional: join condition expression (ON clause)
   */
  joinCondition?: string;
  /**
   * Extensible metadata for future use
   */
  metadata?: Record<string, unknown>;
  /**
   * True if this edge represents approximate/uncertain lineage
   */
  approximate?: boolean;
  /**
   * Zero-based indices of the statement(s) this edge participates in.
   *
   * Invariants:
   * - Intra-statement edges (Ownership, DataFlow, Derivation, JoinDependency)
   *   list every statement in which the same structural `(from, to, kind)`
   *   edge appears, sorted ascending and deduplicated.
   * - `EdgeType::CrossStatement` edges are not merged across
   *   producer/consumer pairs: each edge carries exactly
   *   `[producer_index, consumer_index]` in that order, and the same
   *   `(from, to)` self-loop may appear multiple times with different
   *   pairs.
   */
  statementIds?: number[];
}

export type EdgeType =
  | 'ownership'
  | 'data_flow'
  | 'derivation'
  | 'join_dependency'
  | 'cross_statement';

/**
 * The type of SQL JOIN operation.
 */
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
 * An issue encountered during SQL analysis (error, warning, or info).
 */
export interface Issue {
  /**
   * Severity level
   */
  severity: Severity;
  /**
   * Machine-readable issue code
   */
  code: string;
  /**
   * Human-readable error message
   */
  message: string;
  /**
   * SQLFluff dotted rule name (e.g., `aliasing.table`).
   */
  sqlfluffName?: string;
  /**
   * Optional: location in source SQL where issue occurred
   */
  span?: Span;
  /**
   * Optional: which statement index this issue relates to
   */
  statementIndex?: number;
  /**
   * Optional: source file name where the issue occurred
   */
  sourceName?: string;
  /**
   * Optional: linter engine provenance (`semantic`, `lexical`, `document`).
   */
  lintEngine?: LintEngine;
  /**
   * Optional: confidence level for lint detection quality.
   */
  lintConfidence?: LintConfidence;
  /**
   * Optional: fallback mode used while evaluating this lint.
   */
  lintFallbackSource?: LintFallbackSource;
  /**
   * Optional: autofix metadata for this issue.
   */
  autofix?: IssueAutofix;
}

export type Severity = 'error' | 'warning' | 'info';

/**
 * Lint execution engine category.
 */
export type LintEngine = 'semantic' | 'lexical' | 'document';

/**
 * Confidence level attached to lint findings.
 */
export type LintConfidence = 'high' | 'medium' | 'low';

/**
 * Source of degraded lint confidence or fallback behavior.
 */
export type LintFallbackSource = 'parser_fallback' | 'tokenizer_fallback' | 'heuristic_rule';

/**
 * Autofix metadata attached to an issue.
 */
export interface IssueAutofix {
  /**
   * Applicability category for this fix.
   */
  applicability: IssueAutofixApplicability;
  /**
   * Edits required to apply this fix.
   */
  edits: IssuePatchEdit[];
}

/**
 * Autofix applicability metadata for an issue.
 */
export type IssueAutofixApplicability = 'safe' | 'unsafe' | 'displayOnly';

/**
 * A text patch edit associated with an issue autofix.
 */
export interface IssuePatchEdit {
  /**
   * Byte range in the source SQL to replace.
   */
  span: Span;
  /**
   * Replacement text for the target span.
   */
  replacement: string;
}

/**
 * Summary statistics for the analysis result.
 */
export interface Summary {
  /**
   * Total number of statements analyzed
   */
  statementCount: number;
  /**
   * Total unique tables/CTEs discovered across all statements
   */
  tableCount: number;
  /**
   * Total columns in output (Phase 2+)
   */
  columnCount: number;
  /**
   * Total number of JOIN operations
   */
  joinCount: number;
  /**
   * Complexity score (1-100) based on query structure
   */
  complexityScore: number;
  /**
   * Issue counts by severity
   */
  issueCount: IssueCount;
  /**
   * Quick check: true if any errors were encountered
   */
  hasErrors: boolean;
}

/**
 * Counts of issues by severity level.
 */
export interface IssueCount {
  /**
   * Number of error-level issues
   */
  errors: number;
  /**
   * Number of warning-level issues
   */
  warnings: number;
  /**
   * Number of info-level issues
   */
  infos: number;
}

/**
 * Resolved schema metadata showing the effective schema used during analysis.
 *
 * Combines imported (user-provided) and implied (inferred from DDL) schema.
 */
export interface ResolvedSchemaMetadata {
  /**
   * All tables used during analysis (imported + implied)
   */
  tables: ResolvedSchemaTable[];
}

/**
 * A table in the resolved schema with origin metadata.
 */
export interface ResolvedSchemaTable {
  catalog?: string;
  schema?: string;
  name: string;
  columns: ResolvedColumnSchema[];
  /**
   * Origin of this table's schema information
   */
  origin: SchemaOrigin;
  /**
   * For implied tables: which statement created it
   */
  sourceStatementIndex?: number;
  /**
   * Timestamp when this entry was created/updated (ISO 8601)
   */
  updatedAt: string;
  /**
   * True if this is a temporary table
   */
  temporary?: boolean;
  /**
   * Table-level constraints (composite PKs, FKs, etc.)
   */
  constraints?: TableConstraintInfo[];
}

/**
 * A column in the resolved schema with origin tracking.
 */
export interface ResolvedColumnSchema {
  name: string;
  dataType?: string;
  /**
   * Column-level origin (can differ from table origin in future merging)
   */
  origin?: SchemaOrigin;
  /**
   * True if this column is a primary key (or part of composite PK)
   */
  isPrimaryKey?: boolean;
  /**
   * Foreign key reference if this column references another table
   */
  foreignKey?: ForeignKeyRef;
}

/**
 * The origin of schema information.
 */
export type SchemaOrigin = 'imported' | 'implied';

/**
 * Information about a table-level constraint (composite PK, FK, etc.).
 */
export interface TableConstraintInfo {
  /**
   * Type of constraint
   */
  constraintType: ConstraintType;
  /**
   * Columns involved in this constraint
   */
  columns: string[];
  /**
   * For FK: the referenced table
   */
  referencedTable?: string;
  /**
   * For FK: the referenced columns
   */
  referencedColumns?: string[];
}

/**
 * Type of table constraint.
 *
 * This enum is marked `#[non_exhaustive]` to allow adding constraint types
 * (e.g., CHECK, EXCLUDE) in the future without breaking API compatibility.
 */
export type ConstraintType = 'primary_key' | 'foreign_key' | 'unique';
