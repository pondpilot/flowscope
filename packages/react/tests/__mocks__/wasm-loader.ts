/**
 * Mock wasm-loader for tests.
 * The react package tests only need types and utility functions from core,
 * not the actual WASM module.
 */

export async function initWasm() {
  throw new Error('WASM not available in test environment');
}

export function isWasmInitialized() {
  return false;
}

export function resetWasm() {
  // no-op
}

export function getEngineVersion() {
  return 'mock-test';
}
