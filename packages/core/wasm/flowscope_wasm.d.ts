/* tslint:disable */
/* eslint-disable */

/**
 * Analyze SQL and export to DuckDB SQL statements in one step.
 *
 * Convenience function that combines analyze_sql_json + export_to_duckdb_sql.
 * Takes a JSON AnalyzeRequest and returns SQL statements for duckdb-wasm.
 *
 * Note: This function does not support the schema parameter. Use
 * analyze_sql_json + export_to_duckdb_sql separately for schema support.
 */
export function analyze_and_export_sql(request_json: string): string;

/**
 * Legacy simple API - accepts SQL string, returns JSON with table names
 * Kept for backwards compatibility
 */
export function analyze_sql(sql_input: string): string;

/**
 * Main analysis entry point - accepts JSON request, returns JSON result
 * This function never throws - errors are returned in the result's issues array
 *
 * Supports optional `encoding` field in request:
 * - `"utf8"` (default): All span offsets are UTF-8 byte offsets
 * - `"utf16"`: All span offsets are converted to UTF-16 code units
 */
export function analyze_sql_json(request_json: string): string;

/**
 * Compute completion context for a cursor position.
 * Returns JSON-serialized CompletionContext.
 *
 * Supports optional `encoding` field in request:
 * - `"utf8"` (default): cursor_offset is UTF-8 bytes, spans are UTF-8 bytes
 * - `"utf16"`: cursor_offset is UTF-16 code units, spans are UTF-16 code units
 */
export function completion_context_json(request_json: string): string;

/**
 * Compute ranked completion items for a cursor position.
 *
 * Supports optional `encoding` field in request:
 * - `"utf8"` (default): cursor_offset is UTF-8 bytes, spans are UTF-8 bytes
 * - `"utf16"`: cursor_offset is UTF-16 code units, spans are UTF-16 code units
 */
export function completion_items_json(request_json: string): string;

/**
 * Enable tracing logs to the browser console (requires `tracing` feature).
 */
export function enable_tracing(): void;

export function export_csv_bundle(request_json: string): Uint8Array;

export function export_filename(request_json: string): string;

export function export_html(request_json: string): string;

export function export_json(request_json: string): string;

export function export_mermaid(request_json: string): string;

/**
 * Export analysis result to SQL statements for DuckDB-WASM.
 *
 * Takes a JSON object with:
 * - `result`: The AnalyzeResult to export
 * - `schema` (optional): Schema name to prefix all tables/views (e.g., "lineage")
 *
 * Returns SQL statements (DDL + INSERT) that can be executed by duckdb-wasm.
 *
 * This is the WASM-compatible export path - generates SQL text that
 * duckdb-wasm can execute to create a queryable database in the browser.
 */
export function export_to_duckdb_sql(request_json: string): string;

export function export_xlsx(request_json: string): Uint8Array;

/**
 * Get version information
 */
export function get_version(): string;

/**
 * Install panic hook for better error messages in browser console
 */
export function set_panic_hook(): void;

/**
 * Split SQL into statement spans.
 *
 * Supports optional `encoding` field in request:
 * - `"utf8"` (default): All span offsets are UTF-8 byte offsets
 * - `"utf16"`: All span offsets are converted to UTF-16 code units
 */
export function split_statements_json(request_json: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly analyze_and_export_sql: (a: number, b: number) => [number, number, number, number];
  readonly analyze_sql: (a: number, b: number) => [number, number, number, number];
  readonly analyze_sql_json: (a: number, b: number) => [number, number];
  readonly completion_context_json: (a: number, b: number) => [number, number];
  readonly completion_items_json: (a: number, b: number) => [number, number];
  readonly enable_tracing: () => void;
  readonly export_csv_bundle: (a: number, b: number) => [number, number, number, number];
  readonly export_filename: (a: number, b: number) => [number, number, number, number];
  readonly export_html: (a: number, b: number) => [number, number, number, number];
  readonly export_json: (a: number, b: number) => [number, number, number, number];
  readonly export_mermaid: (a: number, b: number) => [number, number, number, number];
  readonly export_to_duckdb_sql: (a: number, b: number) => [number, number, number, number];
  readonly export_xlsx: (a: number, b: number) => [number, number, number, number];
  readonly get_version: () => [number, number];
  readonly split_statements_json: (a: number, b: number) => [number, number];
  readonly set_panic_hook: () => void;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
