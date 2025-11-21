let wasmModule: typeof import('../wasm/flowscope_wasm') | null = null;
let initPromise: Promise<typeof import('../wasm/flowscope_wasm')> | null = null;

export interface InitWasmOptions {
  wasmUrl?: string;
}

/**
 * Initialize the WASM module. Safe to call multiple times (idempotent).
 * Returns the initialized WASM module.
 */
export async function initWasm(
  _options: InitWasmOptions = {}
): Promise<typeof import('../wasm/flowscope_wasm')> {
  // Return cached module if already initialized
  if (wasmModule) {
    return wasmModule;
  }

  // Return existing promise if initialization is in progress
  if (initPromise) {
    return initPromise;
  }

  initPromise = (async () => {
    try {
      // Dynamic import of the wasm module
      // With vite-plugin-wasm, the module auto-initializes on import
      const wasm = await import('../wasm/flowscope_wasm');

      // For bundler target with vite-plugin-wasm, the module is already initialized
      // No need to call default() - the import statement handles initialization

      wasmModule = wasm;
      return wasmModule;
    } catch (error) {
      initPromise = null; // Allow retry on failure
      throw new Error(
        `Failed to initialize WASM module: ${error instanceof Error ? error.message : String(error)}`
      );
    }
  })();

  return initPromise;
}

/**
 * Check if WASM module is initialized
 */
export function isWasmInitialized(): boolean {
  return wasmModule !== null;
}

/**
 * Get the initialized WASM module. Throws if not initialized.
 */
export function getWasmModule(): typeof import('../wasm/flowscope_wasm') {
  if (!wasmModule) {
    throw new Error('WASM module not initialized. Call initWasm() first.');
  }
  return wasmModule;
}

/**
 * Reset the WASM module (mainly for testing)
 */
export function resetWasm(): void {
  wasmModule = null;
  initPromise = null;
}
