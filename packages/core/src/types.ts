/**
 * Types for the FlowScope SQL lineage analysis API.
 * @module types
 */

import type {
  AnalysisOptions as GeneratedAnalysisOptions,
  AnalyzeRequest as GeneratedAnalyzeRequest,
  AnalyzeResult as GeneratedAnalyzeResult,
  Dialect,
  Edge as GeneratedEdge,
  IssuePatchEdit,
  LintConfig as GeneratedLintConfig,
  Node as GeneratedNode,
  NodeType,
  SchemaMetadata,
  Span,
  TemplateConfig as GeneratedTemplateConfig,
  TemplateMode,
} from './generated/api-types';

export type {
  AggregationInfo,
  CanonicalName,
  CaseSensitivity,
  ColumnSchema,
  ConstraintType,
  Dialect,
  EdgeType,
  FileSource,
  FilterClauseType,
  FilterPredicate,
  ForeignKeyRef,
  GraphDetailLevel,
  Issue,
  IssueAutofix,
  IssueAutofixApplicability,
  IssueCount,
  IssuePatchEdit,
  JoinType,
  LintConfidence,
  LintEngine,
  LintFallbackSource,
  NodeType,
  ResolutionSource,
  ResolvedColumnSchema,
  ResolvedSchemaMetadata,
  ResolvedSchemaTable,
  SchemaMetadata,
  SchemaNamespaceHint,
  SchemaOrigin,
  SchemaTable,
  Severity,
  Span,
  StatementMeta,
  Summary,
  TableConstraintInfo,
  TemplateMode,
} from './generated/api-types';

// Request Types

/** SQL dialects supported for parsing and analysis. */
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
] as const satisfies readonly Dialect[];

/**
 * Configuration for template preprocessing.
 *
 * When provided, the SQL is preprocessed through a template engine before parsing.
 * This enables analysis of dbt models and Jinja-templated SQL files.
 */
export interface TemplateConfig extends GeneratedTemplateConfig {
  /** The template mode to use */
  mode: TemplateMode;
}

/** Lint configuration with object-valued rule settings for ergonomic callers. */
export interface LintConfig extends Omit<GeneratedLintConfig, 'ruleConfigs'> {
  ruleConfigs?: Record<string, Record<string, unknown>>;
}

/** Analysis options refined for the public TypeScript lint configuration. */
export interface AnalysisOptions extends Omit<GeneratedAnalysisOptions, 'lint'> {
  lint?: LintConfig;
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
export interface AnalyzeRequest extends Omit<
  GeneratedAnalyzeRequest,
  'options' | 'templateConfig'
> {
  /** Optional analysis options with object-valued lint rule settings. */
  options?: AnalysisOptions;
  /**
   * Text encoding for span offsets in the response.
   * When `'utf16'`, all Span offsets are converted to UTF-16 code units.
   * @default 'utf8'
   */
  encoding?: Encoding;
  /** Optional template configuration with an explicit preprocessing mode. */
  templateConfig?: TemplateConfig;
}

/** Mermaid export view modes. */
export type MermaidView = 'all' | 'script' | 'table' | 'column' | 'hybrid';

/** Export format identifiers. */
export type ExportFormat = 'json' | 'mermaid' | 'html' | 'sql' | 'csv' | 'xlsx' | 'duckdb' | 'png';

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

/** Analysis result with the graph invariants guaranteed by the public API. */
export interface AnalyzeResult extends Omit<GeneratedAnalyzeResult, 'nodes' | 'edges'> {
  nodes: Node[];
  edges: Edge[];
}

/** Lineage node with at least one participating statement. */
export interface Node extends GeneratedNode {
  statementIds: number[];
}

/** Lineage edge with its participating statement indices. */
export interface Edge extends GeneratedEdge {
  statementIds: number[];
}

/** Table-like node types that can contain columns and appear in FROM clauses. */
export type TableLikeNodeType = Extract<NodeType, 'table' | 'view' | 'cte'>;

/** Returns true if the node type is table-like (table, view, or CTE). */
export function isTableLikeType(type: NodeType): type is TableLikeNodeType {
  return type === 'table' || type === 'view' || type === 'cte';
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

// Utility Functions

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

// Shared TextDecoder instance for performance (avoid creating per-call)
const utf8Decoder = new TextDecoder();

/**
 * Apply a set of patch edits to a source string.
 *
 * Edits use UTF-8 byte offsets (matching the `Span` type). They are sorted
 * internally and applied in a single forward pass.
 *
 * @param source - The original source string
 * @param edits - The patch edits to apply (each with a byte-offset span and replacement text)
 * @returns The source string with all edits applied
 * @throws {RangeError} If any edit span is out of bounds or has start > end
 * @throws {Error} If edits overlap
 *
 * @example
 * ```typescript
 * const result = await analyzeSql({ sql: 'select ID from T', dialect: 'postgres' });
 * const issue = result.issues.find(i => i.autofix);
 * if (issue?.autofix) {
 *   const fixed = applyEdits('select ID from T', issue.autofix.edits);
 * }
 * ```
 */
export function applyEdits(source: string, edits: IssuePatchEdit[]): string {
  if (edits.length === 0) {
    return source;
  }

  // Encode source to bytes so we can work with byte offsets
  const sourceBytes = utf8Encoder.encode(source);

  // Sort edits by span.start ascending for a single forward pass
  const sorted = [...edits].sort((a, b) => a.span.start - b.span.start);

  // Pre-encode replacements so we can compute total size and avoid encoding twice
  const replacements = sorted.map((e) => utf8Encoder.encode(e.replacement));

  // Validate spans and detect overlaps; also compute result byte length
  let resultLength = sourceBytes.length;
  let previousEnd = 0;
  for (let i = 0; i < sorted.length; i++) {
    const { start, end } = sorted[i].span;
    if (start < 0 || end > sourceBytes.length || start > end) {
      throw new RangeError(
        `Invalid edit span [${start}, ${end}) for source of ${sourceBytes.length} bytes`
      );
    }
    if (start < previousEnd) {
      const prev = sorted[i - 1].span;
      throw new Error(
        `Overlapping edits: span [${start}, ${end}) overlaps with span [${prev.start}, ${prev.end})`
      );
    }
    resultLength += replacements[i].length - (end - start);
    previousEnd = end;
  }

  // Single-allocation forward pass: copy unchanged regions and replacements
  const result = new Uint8Array(resultLength);
  let readPos = 0;
  let writePos = 0;
  for (let i = 0; i < sorted.length; i++) {
    const { start, end } = sorted[i].span;

    // Copy unchanged bytes before this edit
    const unchanged = sourceBytes.subarray(readPos, start);
    result.set(unchanged, writePos);
    writePos += unchanged.length;

    // Copy replacement bytes
    result.set(replacements[i], writePos);
    writePos += replacements[i].length;

    readPos = end;
  }

  // Copy remaining bytes after the last edit
  const tail = sourceBytes.subarray(readPos);
  result.set(tail, writePos);

  return utf8Decoder.decode(result);
}
