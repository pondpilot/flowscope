/**
 * Application-level type definitions
 */

// Re-export TemplateMode from core to maintain single source of truth
// Application-specific utilities (validation, UI options) are defined below
export type { TemplateMode } from '@pondpilot/flowscope-core';
import type { TemplateMode } from '@pondpilot/flowscope-core';

/**
 * Error codes for analysis worker operations.
 * Using structured error codes instead of string matching for reliability.
 */
export const AnalysisErrorCode = {
  /** File content required for analysis is not available in the worker's cache */
  MISSING_FILE_CONTENT: 'MISSING_FILE_CONTENT',
  /** No files provided for analysis */
  NO_FILES_AVAILABLE: 'NO_FILES_AVAILABLE',
  /** Worker returned empty result unexpectedly */
  EMPTY_RESULT: 'EMPTY_RESULT',
  /** WASM initialization failed */
  WASM_INIT_FAILED: 'WASM_INIT_FAILED',
  /** Generic analysis error */
  ANALYSIS_FAILED: 'ANALYSIS_FAILED',
} as const;

export type AnalysisErrorCode = (typeof AnalysisErrorCode)[keyof typeof AnalysisErrorCode];

/**
 * Structured error for analysis operations.
 * Includes an error code for programmatic handling without string parsing.
 */
export class AnalysisError extends Error {
  code: AnalysisErrorCode;

  constructor(code: AnalysisErrorCode, message: string) {
    super(message);
    this.code = code;
    this.name = 'AnalysisError';
  }
}

/**
 * Check if an error is an AnalysisError with a specific code.
 */
export function isAnalysisError(error: unknown, code?: AnalysisErrorCode): error is AnalysisError {
  if (!(error instanceof Error) || !('code' in error)) return false;
  if (code && (error as AnalysisError).code !== code) return false;
  return true;
}

export interface WasmState {
  ready: boolean;
  error: string | null;
  isRetrying: boolean;
}

export interface AnalysisState {
  isAnalyzing: boolean;
  error: string | null;
  lastAnalyzedAt: number | null;
}

export interface KeyboardShortcutHandler {
  key: string;
  modifiers: ReadonlyArray<'metaKey' | 'ctrlKey' | 'shiftKey' | 'altKey'>;
  handler: () => void;
  description: string;
}

export interface FileValidationResult {
  valid: boolean;
  error?: string;
}

export interface AnalysisContext {
  description: string;
  fileCount: number;
  files: Array<{ name: string; content: string }>;
}

/** Valid template mode values for runtime validation */
export const VALID_TEMPLATE_MODES: readonly TemplateMode[] = ['raw', 'jinja', 'dbt'] as const;

/** Template mode options for UI dropdowns with display labels */
export const TEMPLATE_MODE_OPTIONS: readonly { value: TemplateMode; label: string }[] = [
  { value: 'raw', label: 'No Template' },
  { value: 'jinja', label: 'Jinja' },
  { value: 'dbt', label: 'dbt' },
] as const;

/**
 * Validates and parses a template mode value.
 * Returns the validated mode or 'raw' as a safe fallback for invalid values.
 */
export function parseTemplateMode(value: unknown): TemplateMode {
  if (typeof value === 'string' && VALID_TEMPLATE_MODES.includes(value as TemplateMode)) {
    return value as TemplateMode;
  }
  return 'raw';
}

/**
 * Type guard to check if a value is a valid TemplateMode.
 */
export function isValidTemplateMode(value: unknown): value is TemplateMode {
  return typeof value === 'string' && VALID_TEMPLATE_MODES.includes(value as TemplateMode);
}
