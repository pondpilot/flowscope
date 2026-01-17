import { initWasm, isWasmInitialized } from './wasm-loader';
import type {
  AnalyzeRequest,
  AnalyzeResult,
  CompletionItemsResult,
  CompletionRequest,
  Dialect,
  StatementSplitRequest,
  StatementSplitResult,
} from './types';

// Shared reserved keywords (single source of truth for Rust and TypeScript)
import reservedKeywordsJson from './reserved-keywords.json';

// Import WASM functions (will be available after init)
let analyzeSqlJson: ((request: string) => string) | null = null;
let exportToDuckDbSqlFn: ((resultJson: string) => string) | null = null;
let completionItemsJson: ((requestJson: string) => string) | null = null;
let splitStatementsJson: ((requestJson: string) => string) | null = null;
let panicHookInstalled = false;

// Initialization guard to prevent race conditions
let wasmInitPromise: Promise<void> | null = null;

/** Maximum length for schema identifiers (PostgreSQL/DuckDB limit). */
const MAX_SCHEMA_NAME_LENGTH = 63;

/**
 * Maximum SQL input size in characters to prevent memory exhaustion.
 * This is a character count, not byte count - for typical SQL (mostly ASCII),
 * this roughly corresponds to 10MB. The WASM layer enforces the actual 10MB byte limit.
 */
const MAX_SQL_LENGTH = 10 * 1024 * 1024;

/** Valid SQL dialects. */
const VALID_DIALECTS: readonly Dialect[] = [
  'generic',
  'ansi',
  'bigquery',
  'clickhouse',
  'databricks',
  'duckdb',
  'hive',
  'mssql',
  'mysql',
  'postgres',
  'redshift',
  'snowflake',
  'sqlite',
] as const;

/**
 * Validate that a dialect value is valid.
 * @param dialect - The dialect to validate
 * @throws Error if dialect is undefined, null, or not a valid dialect
 */
function validateDialect(dialect: Dialect | undefined | null): asserts dialect is Dialect {
  if (dialect === undefined || dialect === null) {
    throw new Error('Invalid request: dialect is required');
  }
  if (!VALID_DIALECTS.includes(dialect)) {
    throw new Error(`Invalid dialect: ${dialect}. Must be one of: ${VALID_DIALECTS.join(', ')}`);
  }
}

/**
 * Validate SQL input string.
 * @param sql - The SQL to validate
 * @param allowEmpty - Whether empty SQL is allowed (default: false)
 * @throws Error if sql is not a string or exceeds size limits
 */
function validateSqlInput(sql: unknown, allowEmpty = false): asserts sql is string {
  if (typeof sql !== 'string') {
    throw new Error('Invalid request: sql must be a string');
  }
  if (!allowEmpty && sql.trim().length === 0) {
    throw new Error('Invalid request: sql must be a non-empty string');
  }
  if (sql.length > MAX_SQL_LENGTH) {
    throw new Error(
      `SQL exceeds maximum length of ${MAX_SQL_LENGTH} characters (${sql.length} characters provided)`
    );
  }
}

/**
 * SQL reserved keywords that cannot be used as unquoted schema names.
 * Loaded from shared JSON file to keep Rust and TypeScript in sync.
 */
const RESERVED_KEYWORDS = new Set(reservedKeywordsJson.keywords);

/**
 * Validate a schema name for use as a SQL identifier.
 * Returns an error reason if invalid, or undefined if valid.
 *
 * The returned message is just the reason (e.g., "must start with a letter or underscore"),
 * without a "Schema name" prefix. Callers should format for display as needed.
 *
 * @param schema - The schema name to validate
 * @returns Error reason if invalid, undefined if valid
 */
export function validateSchemaName(schema: string): string | undefined {
  if (!schema) return undefined; // Empty is valid (means no schema)

  // Must start with letter or underscore
  if (!/^[a-zA-Z_]/.test(schema)) {
    return 'must start with a letter or underscore';
  }

  // Must contain only valid identifier characters
  if (!/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(schema)) {
    return 'can only contain letters, numbers, and underscores';
  }

  // Length check
  if (schema.length > MAX_SCHEMA_NAME_LENGTH) {
    return `must be ${MAX_SCHEMA_NAME_LENGTH} characters or fewer`;
  }

  // Reserved keyword check (case-insensitive)
  if (RESERVED_KEYWORDS.has(schema.toLowerCase())) {
    return 'cannot be a SQL reserved keyword';
  }

  return undefined;
}

/**
 * Format a schema validation error for user display.
 *
 * @param reason - The error reason from validateSchemaName
 * @returns Formatted error message with "Schema name" prefix
 */
export function formatSchemaError(reason: string): string {
  return `Schema name ${reason}`;
}

/**
 * Validate a schema name for use as a SQL identifier.
 * Throws an error if the schema name is invalid.
 *
 * @param schema - The schema name to validate
 * @throws Error if the schema name is invalid
 * @internal
 */
function validateSchemaNameOrThrow(schema: string): void {
  const error = validateSchemaName(schema);
  if (error) {
    throw new Error(`Invalid schema name: ${error}`);
  }
}

async function ensureWasmReady(): Promise<void> {
  // Use initialization guard to prevent race conditions when called concurrently
  if (wasmInitPromise) {
    return wasmInitPromise;
  }

  wasmInitPromise = (async () => {
    const wasmModule = await initWasm();

    if (!isWasmInitialized()) {
      throw new Error('WASM module failed to initialize');
    }

    if (!analyzeSqlJson) {
      analyzeSqlJson = wasmModule.analyze_sql_json;
    }

    if (!exportToDuckDbSqlFn) {
      exportToDuckDbSqlFn = wasmModule.export_to_duckdb_sql;
    }

    if (!completionItemsJson && typeof wasmModule.completion_items_json === 'function') {
      completionItemsJson = wasmModule.completion_items_json;
    }

    if (!splitStatementsJson && typeof wasmModule.split_statements_json === 'function') {
      splitStatementsJson = wasmModule.split_statements_json;
    }

    // Install panic hook for better error messages
    if (!panicHookInstalled && wasmModule.set_panic_hook) {
      wasmModule.set_panic_hook();
      panicHookInstalled = true;
    }
  })();

  return wasmInitPromise;
}

/**
 * Analyze SQL and return lineage information.
 *
 * @param request - The analysis request containing SQL and options
 * @returns The analysis result with lineage graphs and issues
 *
 * @example
 * ```typescript
 * const result = await analyzeSql({
 *   sql: 'SELECT * FROM users JOIN orders ON users.id = orders.user_id',
 *   dialect: 'postgres'
 * });
 *
 * console.log(result.statements[0].nodes); // Tables: users, orders
 * console.log(result.summary.hasErrors); // false
 * ```
 */
export async function analyzeSql(request: AnalyzeRequest): Promise<AnalyzeResult> {
  await ensureWasmReady();

  if (!analyzeSqlJson) {
    throw new Error('WASM module not properly initialized');
  }

  // Validate request
  const hasFiles = Array.isArray(request.files) && request.files.length > 0;

  if (!hasFiles) {
    validateSqlInput(request.sql);
  } else if (request.files) {
    // Validate each file's SQL content
    for (const file of request.files) {
      validateSqlInput(file.content, true);
    }
  }

  validateDialect(request.dialect);

  // Serialize request to JSON
  const requestJson = JSON.stringify(request);

  // Call WASM function
  const resultJson = analyzeSqlJson(requestJson);

  // Parse result
  try {
    return JSON.parse(resultJson) as AnalyzeResult;
  } catch (error) {
    throw new Error(
      `Failed to parse analysis result: ${error instanceof Error ? error.message : String(error)}`
    );
  }
}

export async function completionItems(request: CompletionRequest): Promise<CompletionItemsResult> {
  await ensureWasmReady();

  if (!completionItemsJson) {
    throw new Error('WASM module not properly initialized');
  }

  // Validate SQL input (allow empty - Rust handles empty SQL gracefully)
  validateSqlInput(request.sql, true);
  validateDialect(request.dialect);

  // Validate cursor offset
  if (typeof request.cursorOffset !== 'number' || !Number.isInteger(request.cursorOffset)) {
    throw new Error('Invalid request: cursorOffset must be an integer');
  }
  if (request.cursorOffset < 0) {
    throw new Error(`Invalid request: cursorOffset cannot be negative (${request.cursorOffset})`);
  }
  // Note: Rust validates that cursorOffset doesn't exceed SQL length and lands on UTF-8 boundary

  const requestJson = JSON.stringify(request);
  const resultJson = completionItemsJson(requestJson);

  try {
    return JSON.parse(resultJson) as CompletionItemsResult;
  } catch (error) {
    throw new Error(
      `Failed to parse completion items: ${error instanceof Error ? error.message : String(error)}`
    );
  }
}

export async function splitStatements(
  request: StatementSplitRequest
): Promise<StatementSplitResult> {
  await ensureWasmReady();

  if (!splitStatementsJson) {
    throw new Error('WASM module not properly initialized');
  }

  // Validate SQL input (allow empty - Rust returns empty array for empty SQL)
  validateSqlInput(request.sql, true);

  // Dialect is optional for splitStatements (defaults to 'generic' in Rust)
  // but if provided, it must be valid
  if (request.dialect !== undefined) {
    validateDialect(request.dialect);
  }

  const requestJson = JSON.stringify(request);
  const resultJson = splitStatementsJson(requestJson);

  try {
    return JSON.parse(resultJson) as StatementSplitResult;
  } catch (error) {
    throw new Error(
      `Failed to parse statement split result: ${error instanceof Error ? error.message : String(error)}`
    );
  }
}

/**
 * Convenience function to analyze SQL with minimal options.
 *
 * @param sql - The SQL to analyze
 * @param dialect - The SQL dialect (defaults to 'generic')
 * @returns The analysis result
 */
export async function analyzeSimple(
  sql: string,
  dialect: Dialect = 'generic'
): Promise<AnalyzeResult> {
  return analyzeSql({ sql, dialect });
}

/**
 * Export an analysis result to SQL statements for DuckDB.
 *
 * Returns DDL (CREATE TABLE/VIEW) + INSERT statements that can be
 * executed by duckdb-wasm in the browser to create a queryable database.
 *
 * @param result - The analysis result to export
 * @param schema - Optional schema name to prefix all tables/views (e.g., "lineage")
 * @returns SQL statements as a string
 *
 * @example
 * ```typescript
 * const result = await analyzeSql({ sql: 'SELECT * FROM users', dialect: 'postgres' });
 * const sql = await exportToDuckDbSql(result);
 * // Execute 'sql' with duckdb-wasm to create a queryable lineage database
 *
 * // With schema prefix:
 * const sqlWithSchema = await exportToDuckDbSql(result, 'lineage');
 * // Creates: lineage._meta, lineage.statements, etc.
 * ```
 */
export async function exportToDuckDbSql(result: AnalyzeResult, schema?: string): Promise<string> {
  // Validate schema first (fail fast on user input errors)
  if (schema !== undefined) {
    validateSchemaNameOrThrow(schema);
  }

  await ensureWasmReady();

  if (!exportToDuckDbSqlFn) {
    throw new Error('WASM module not properly initialized');
  }

  // Validate result structure before serialization
  if (!result || typeof result !== 'object') {
    throw new Error('Invalid result: expected an object');
  }
  if (!Array.isArray(result.statements)) {
    throw new Error(`Invalid result: expected statements array, got ${typeof result.statements}`);
  }

  // Serialize request to JSON (WASM expects { result, schema? })
  const requestJson = JSON.stringify({ result, schema });

  // Call WASM function
  return exportToDuckDbSqlFn(requestJson);
}
