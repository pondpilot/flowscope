import { initWasm, isWasmInitialized } from './wasm-loader';
import type { AnalyzeRequest, AnalyzeResult, Dialect } from './types';

// Import WASM functions (will be available after init)
let analyzeSqlJson: ((request: string) => string) | null = null;
let panicHookInstalled = false;

async function ensureWasmReady(): Promise<void> {
  const wasmModule = await initWasm();

  if (!isWasmInitialized()) {
    throw new Error('WASM module failed to initialize');
  }

  if (!analyzeSqlJson) {
    analyzeSqlJson = wasmModule.analyze_sql_json;
  }

  // Install panic hook for better error messages
  if (!panicHookInstalled && wasmModule.set_panic_hook) {
    wasmModule.set_panic_hook();
    panicHookInstalled = true;
  }
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
  const hasSqlString = typeof request.sql === 'string';

  if (!hasFiles) {
    if (!hasSqlString) {
      throw new Error('Invalid request: sql must be a string');
    }
    if (request.sql.trim().length === 0) {
      throw new Error('Invalid request: sql must be a non-empty string');
    }
  }

  if (!request.dialect) {
    throw new Error('Invalid request: dialect is required');
  }

  const validDialects: Dialect[] = ['generic', 'postgres', 'snowflake', 'bigquery'];
  if (!validDialects.includes(request.dialect)) {
    throw new Error(
      `Invalid dialect: ${request.dialect}. Must be one of: ${validDialects.join(', ')}`
    );
  }

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
