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
 * Template preprocessing mode for SQL analysis.
 * - 'raw': No templating, SQL passed through unchanged
 * - 'jinja': Standard Jinja2 template rendering
 * - 'dbt': dbt-style templating with builtin macros (ref, source, config, var)
 *
 * This is the canonical definition. The app re-exports this type and adds
 * validation utilities (parseTemplateMode, isValidTemplateMode) and UI
 * options (TEMPLATE_MODE_OPTIONS).
 */
export type TemplateMode = 'raw' | 'jinja' | 'dbt';

/**
 * Configuration for template preprocessing.
 *
 * When provided, the SQL is preprocessed through a template engine before parsing.
 * This enables analysis of dbt models and Jinja-templated SQL files.
 */
export interface TemplateConfig {
  /** The template mode to use */
  mode: TemplateMode;
  /**
   * Context variables available to the template.
   * For dbt mode, include a 'vars' key with dbt project variables.
   */
  context?: Record<string, unknown>;
}

/**
 * Text encoding for offset interpretation in API requests/responses.
 *
 * - `'utf8'` (default): All offsets are UTF-8 byte offsets. This is the native
 *   encoding used internally. Use this when working directly with byte positions.
 *
 * - `'utf16'`: All offsets are UTF-16 code units. This matches JavaScript's native
 *   string indexing (string.length, indexOf, etc.) and Monaco editor positions.
 *   When this is set:
 *   - `cursorOffset` in requests is interpreted as UTF-16 code units
 *   - All `Span` offsets in responses are converted to UTF-16 code units
 *
 * @example
 * ```typescript
 * // With UTF-16 encoding, use JavaScript string indices directly
 * const sql = "SELECT '日本語'";
 * const cursorPos = sql.indexOf("'") + 1; // JavaScript string index
 * const result = await completionItems({
 *   sql,
 *   dialect: 'postgres',
 *   cursorOffset: cursorPos,
 *   encoding: 'utf16'  // No conversion needed!
 * });
 * // Response spans are also in UTF-16 code units
 * const text = sql.slice(result.token.span.start, result.token.span.end);
 * ```
 */
export type Encoding = 'utf8' | 'utf16';

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
  /**
   * Text encoding for span offsets in the response.
   * When `'utf16'`, all Span offsets are converted to UTF-16 code units.
   * @default 'utf8'
   */
  encoding?: Encoding;
  /**
   * Optional template configuration for preprocessing SQL.
   * When provided, SQL is rendered through the template engine before parsing.
   * Enables analysis of dbt models and Jinja-templated SQL files.
   */
  templateConfig?: TemplateConfig;
}

export interface FileSource {
  name: string;
  content: string;
}

/** Graph detail level for visualization. */
export type GraphDetailLevel = 'script' | 'table' | 'column';

/** Mermaid export view modes. */
export type MermaidView = 'all' | 'script' | 'table' | 'column' | 'hybrid';

/** Export format identifiers. */
export type ExportFormat = 'json' | 'mermaid' | 'html' | 'sql' | 'csv' | 'xlsx' | 'duckdb' | 'png';

/** Options controlling the analysis behavior. */
export interface AnalysisOptions {
  /** Enable column-level lineage (default: true) */
  enableColumnLineage?: boolean;
  /** Preferred graph detail level for visualization (does not affect analysis) */
  graphDetailLevel?: GraphDetailLevel;
  /** Hide CTEs from output, creating bypass edges (A→CTE→B becomes A→B) */
  hideCtes?: boolean;
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

export interface CompletionRequest {
  /** The SQL code to analyze (UTF-8 string, multi-statement supported) */
  sql: string;
  /** SQL dialect */
  dialect: Dialect;
  /**
   * Cursor offset in the SQL string.
   *
   * The interpretation depends on the `encoding` field:
   * - `'utf8'` (default): UTF-8 byte offset. Use `charOffsetToByteOffset()` to convert
   *   JavaScript string indices.
   * - `'utf16'`: UTF-16 code units (JavaScript's native string indexing). Use JavaScript
   *   string indices directly (e.g., from `indexOf()` or Monaco cursor position).
   *
   * @example
   * ```typescript
   * // Option 1: UTF-8 mode (default) - requires conversion
   * const byteOffset = charOffsetToByteOffset(sql, charIndex);
   * const result = await completionItems({ sql, dialect: 'postgres', cursorOffset: byteOffset });
   *
   * // Option 2: UTF-16 mode - use JS indices directly
   * const result = await completionItems({
   *   sql,
   *   dialect: 'postgres',
   *   cursorOffset: charIndex,
   *   encoding: 'utf16'
   * });
   * ```
   */
  cursorOffset: number;
  /** Optional schema metadata for accurate column resolution */
  schema?: SchemaMetadata;
  /**
   * Text encoding for cursor offset and response spans.
   * When `'utf16'`, cursorOffset is UTF-16 code units and response spans are converted.
   * @default 'utf8'
   */
  encoding?: Encoding;
}

export interface StatementSplitRequest {
  /** The SQL code to split (UTF-8 string, multi-statement supported) */
  sql: string;
  /**
   * SQL dialect (optional; reserved for future dialect-specific splitting).
   *
   * The current implementation uses a universal tokenizer that handles common SQL
   * constructs (strings, comments, dollar-quoting) across all dialects. Defaults to 'generic'.
   */
  dialect?: Dialect;
  /**
   * Text encoding for span offsets in the response.
   * When `'utf16'`, all Span offsets are converted to UTF-16 code units.
   * @default 'utf8'
   */
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

export interface CompletionTable {
  name: string;
  canonical: string;
  alias?: string;
  matchedSchema: boolean;
}

export interface CompletionColumn {
  name: string;
  dataType?: string;
  table?: string;
  canonicalTable?: string;
  isAmbiguous: boolean;
}

export interface CompletionKeywordSet {
  keywords: string[];
  operators: string[];
  aggregates: string[];
  snippets: string[];
}

export interface CompletionKeywordHints {
  global: CompletionKeywordSet;
  clause: CompletionKeywordSet;
}

export interface CompletionContext {
  statementIndex: number;
  statementSpan: Span;
  clause: CompletionClause;
  token?: CompletionToken;
  tablesInScope: CompletionTable[];
  columnsInScope: CompletionColumn[];
  keywordHints: CompletionKeywordHints;
  /** Error message if the request could not be processed */
  error?: string;
}

export type CompletionItemKind =
  | 'keyword'
  | 'operator'
  | 'function'
  | 'snippet'
  | 'table'
  | 'column'
  | 'schemaTable';

export type CompletionItemCategory =
  | 'keyword'
  | 'operator'
  | 'aggregate'
  | 'snippet'
  | 'table'
  | 'column'
  | 'schemaTable'
  | 'function';

export interface CompletionItem {
  label: string;
  insertText: string;
  kind: CompletionItemKind;
  category: CompletionItemCategory;
  score: number;
  clauseSpecific: boolean;
  detail?: string;
}

export interface CompletionItemsResult {
  clause: CompletionClause;
  token?: CompletionToken;
  shouldShow: boolean;
  items: CompletionItem[];
  /** Error message if the request could not be processed */
  error?: string;
}

export interface StatementSplitResult {
  statements: Span[];
  /** Error message if the request could not be processed */
  error?: string;
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
  /**
   * Resolved/compiled SQL after template expansion (e.g., dbt Jinja rendering).
   * Only present when templating was applied and the result differs from the original.
   */
  resolvedSql?: string;
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
export type NodeType = 'table' | 'view' | 'cte' | 'output' | 'column';

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
export type EdgeType =
  | 'ownership'
  | 'data_flow'
  | 'derivation'
  | 'join_dependency'
  | 'cross_statement';

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
  /** Optional: source file name where the issue occurred */
  sourceName?: string;
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

// Utility Functions

// Shared TextEncoder instance for performance (avoid creating per-call)
const utf8Encoder = new TextEncoder();

// UTF-16 surrogate pair constants
const UTF16_HIGH_SURROGATE_START = 0xd800;
const UTF16_HIGH_SURROGATE_END = 0xdbff;
const UTF16_LOW_SURROGATE_START = 0xdc00;
const UTF16_LOW_SURROGATE_END = 0xdfff;

/**
 * Calculate the UTF-8 byte length of a UTF-16 code unit.
 * This avoids re-encoding each character.
 */
function utf8ByteLength(charCode: number): number {
  if (charCode < 0x80) return 1;
  if (charCode < 0x800) return 2;
  return 3;
}

/**
 * Check if a character code is a high surrogate (first half of a surrogate pair).
 */
function isHighSurrogate(charCode: number): boolean {
  return charCode >= UTF16_HIGH_SURROGATE_START && charCode <= UTF16_HIGH_SURROGATE_END;
}

/**
 * Check if a character code is a low surrogate (second half of a surrogate pair).
 */
function isLowSurrogate(charCode: number): boolean {
  return charCode >= UTF16_LOW_SURROGATE_START && charCode <= UTF16_LOW_SURROGATE_END;
}

/**
 * Convert a JavaScript string character index (UTF-16 code units) to a UTF-8 byte offset.
 *
 * JavaScript strings use UTF-16 internally, but the FlowScope WASM API expects
 * UTF-8 byte offsets. This function converts a character index (as returned by
 * methods like `indexOf()` or cursor position in editors) to the corresponding
 * byte offset in the UTF-8 encoded string.
 *
 * **Note**: The charOffset is in UTF-16 code units (what JavaScript uses for string indexing).
 * For characters outside the Basic Multilingual Plane (like emoji), a single character
 * takes 2 code units (a surrogate pair).
 *
 * @param str - The string to convert within
 * @param charOffset - The character index in UTF-16 code units (0-based)
 * @returns The UTF-8 byte offset corresponding to the character index
 * @throws Error if charOffset is out of bounds
 *
 * @example
 * ```typescript
 * const sql = "SELECT '日本語'"; // Contains multi-byte characters
 * const charIndex = 8; // Position of first Japanese character
 * const byteOffset = charOffsetToByteOffset(sql, charIndex);
 * // byteOffset will be 8 (ASCII chars) vs charIndex 8
 * // But for position after '日', byteOffset would be 11 (8 + 3 bytes for '日')
 * ```
 */
export function charOffsetToByteOffset(str: string, charOffset: number): number {
  if (charOffset < 0) {
    throw new Error(`Character offset cannot be negative: ${charOffset}`);
  }
  if (charOffset > str.length) {
    throw new Error(`Character offset ${charOffset} exceeds string length ${str.length}`);
  }

  // Fast path: check if prefix is pure ASCII
  let hasNonAscii = false;
  for (let i = 0; i < charOffset; i++) {
    if (str.charCodeAt(i) > 0x7f) {
      hasNonAscii = true;
      break;
    }
  }
  if (!hasNonAscii) {
    return charOffset;
  }

  // Slower path: calculate byte offset accounting for multi-byte characters
  let byteOffset = 0;
  for (let i = 0; i < charOffset; i++) {
    const charCode = str.charCodeAt(i);

    // Handle surrogate pairs (characters outside BMP like emoji)
    if (isHighSurrogate(charCode) && i + 1 < charOffset) {
      const nextCode = str.charCodeAt(i + 1);
      if (isLowSurrogate(nextCode)) {
        // Surrogate pair encodes to 4 UTF-8 bytes
        byteOffset += 4;
        i++; // Skip the low surrogate
        continue;
      }
    }

    byteOffset += utf8ByteLength(charCode);
  }

  return byteOffset;
}

/**
 * Convert a UTF-8 byte offset to a JavaScript string character index (UTF-16 code units).
 *
 * This is the inverse of `charOffsetToByteOffset()`. Use this when converting
 * byte offsets from the WASM API back to JavaScript string indices.
 *
 * **Note**: The returned index is in UTF-16 code units (what JavaScript uses for string indexing).
 * For characters outside the Basic Multilingual Plane (like emoji), a single character
 * takes 2 code units (a surrogate pair).
 *
 * @param str - The string to convert within
 * @param byteOffset - The UTF-8 byte offset
 * @returns The character index in UTF-16 code units corresponding to the byte offset
 * @throws Error if byteOffset is out of bounds or doesn't land on a character boundary
 *
 * @example
 * ```typescript
 * const sql = "SELECT '日本語'";
 * const span = result.statementSpan; // { start: 0, end: 17 } in bytes
 * const startChar = byteOffsetToCharOffset(sql, span.start);
 * const endChar = byteOffsetToCharOffset(sql, span.end);
 * const statement = sql.slice(startChar, endChar);
 * ```
 */
export function byteOffsetToCharOffset(str: string, byteOffset: number): number {
  if (byteOffset < 0) {
    throw new Error(`Byte offset cannot be negative: ${byteOffset}`);
  }

  // Get total byte length to validate
  const totalBytes = utf8Encoder.encode(str).length;

  if (byteOffset > totalBytes) {
    throw new Error(`Byte offset ${byteOffset} exceeds UTF-8 length ${totalBytes}`);
  }

  // Fast path for zero offset
  if (byteOffset === 0) {
    return 0;
  }

  // O(n) scan: iterate through string tracking byte position
  let currentByteOffset = 0;
  let charIndex = 0;

  while (charIndex < str.length) {
    if (currentByteOffset === byteOffset) {
      return charIndex;
    }
    if (currentByteOffset > byteOffset) {
      throw new Error(`Byte offset ${byteOffset} does not land on a character boundary`);
    }

    const charCode = str.charCodeAt(charIndex);

    // Handle surrogate pairs (characters outside BMP like emoji)
    if (isHighSurrogate(charCode) && charIndex + 1 < str.length) {
      const nextCode = str.charCodeAt(charIndex + 1);
      if (isLowSurrogate(nextCode)) {
        // Surrogate pair encodes to 4 UTF-8 bytes
        currentByteOffset += 4;
        charIndex += 2; // Skip both surrogates
        continue;
      }
    }

    currentByteOffset += utf8ByteLength(charCode);
    charIndex++;
  }

  // Handle end-of-string case
  if (currentByteOffset === byteOffset) {
    return charIndex;
  }

  throw new Error(`Byte offset ${byteOffset} does not land on a character boundary`);
}
