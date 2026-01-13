/* tslint:disable */
/* eslint-disable */

/**
 * Legacy simple API - accepts SQL string, returns JSON with table names
 * Kept for backwards compatibility
 */
export function analyze_sql(sql_input: string): string;

/**
 * Main analysis entry point - accepts JSON request, returns JSON result
 * This function never throws - errors are returned in the result's issues array
 */
export function analyze_sql_json(request_json: string): string;

/**
 * Enable tracing logs to the browser console (requires `tracing` feature).
 */
export function enable_tracing(): void;

/**
 * Get version information
 */
export function get_version(): string;

/**
 * Install panic hook for better error messages in browser console
 */
export function set_panic_hook(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly analyze_sql: (a: number, b: number) => [number, number, number, number];
  readonly analyze_sql_json: (a: number, b: number) => [number, number];
  readonly enable_tracing: () => void;
  readonly get_version: () => [number, number];
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
