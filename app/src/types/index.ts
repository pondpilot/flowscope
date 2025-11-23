/**
 * Application-level type definitions
 */

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
