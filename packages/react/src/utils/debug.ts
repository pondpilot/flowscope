/**
 * Shared debugging utilities for the flowscope-react package.
 */

/**
 * Debug flag for graph-related logging.
 * Only enabled in development mode.
 */
export const GRAPH_DEBUG = !!(import.meta as { env?: { DEV?: boolean } }).env?.DEV;

/**
 * Debug flag for layout-related logging.
 * Only enabled in development mode.
 */
export const LAYOUT_DEBUG = !!(import.meta as { env?: { DEV?: boolean } }).env?.DEV;

/**
 * Get current time in milliseconds with fallback for environments
 * where performance.now() is unavailable (tests, SSR, older browsers).
 */
export function nowMs(): number {
  if (typeof performance !== 'undefined' && typeof performance.now === 'function') {
    return performance.now();
  }
  return Date.now();
}
