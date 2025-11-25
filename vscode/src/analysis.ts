import * as path from 'path';
import type { AnalyzeRequest, AnalyzeResult, Dialect } from './types';

// WASM module - loaded lazily
let wasmModule: typeof import('../wasm-node/flowscope_wasm') | null = null;

/**
 * Initialize the WASM module.
 */
export async function initWasm(extensionPath: string): Promise<void> {
  if (wasmModule) {
    return;
  }

  // The Node.js WASM bindings handle everything automatically
  // We just need to require the module
  // The wasm-node folder is copied to dist/ during build
  const wasmNodePath = path.join(extensionPath, 'dist', 'wasm-node', 'flowscope_wasm.js');

  try {
    // Use dynamic require for the WASM module
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    wasmModule = require(wasmNodePath);

    // Install panic hook for better error messages
    if (wasmModule.set_panic_hook) {
      wasmModule.set_panic_hook();
    }
  } catch (error) {
    throw new Error(
      `Failed to load WASM module from ${wasmNodePath}: ${error instanceof Error ? error.message : String(error)}`
    );
  }
}

/**
 * Check if WASM is initialized.
 */
export function isWasmInitialized(): boolean {
  return wasmModule !== null;
}

/**
 * Analyze SQL and return lineage information.
 */
export function analyzeSql(request: AnalyzeRequest): AnalyzeResult {
  if (!wasmModule) {
    throw new Error('WASM not initialized. Call initWasm() first.');
  }

  const requestJson = JSON.stringify(request);
  const resultJson = wasmModule.analyze_sql_json(requestJson);
  return JSON.parse(resultJson) as AnalyzeResult;
}

/**
 * Convenience function to analyze SQL with minimal options.
 */
export function analyzeSimple(sql: string, dialect: Dialect = 'generic'): AnalyzeResult {
  return analyzeSql({ sql, dialect });
}

/**
 * Get the engine version.
 */
export function getEngineVersion(): string {
  if (!wasmModule) {
    return 'not initialized';
  }

  if (wasmModule.get_version) {
    return wasmModule.get_version();
  }
  return 'unknown';
}
