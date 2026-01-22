/**
 * Backend adapter abstraction for FlowScope analysis.
 *
 * Provides a unified interface for running lineage analysis either via:
 * - REST API (serve mode with CLI backend)
 * - WASM (client-side analysis in a web worker)
 *
 * The factory function detects available backends and returns the appropriate adapter.
 */

import type { AnalyzeResult, Dialect } from '@pondpilot/flowscope-core';
import type { TemplateMode } from '@/types';
import {
  analyzeWithWorker,
  syncAnalysisFiles,
  initializeAnalysisWorker,
  getCachedAnalysis,
  getAnalysisWorkerVersion,
  clearAnalysisWorkerCache,
} from './analysis-worker';
import type { AnalysisWorkerResult, AnalyzeWorkerOptions } from './analysis-worker';

/**
 * Payload for running analysis.
 */
export interface AnalysisPayload {
  files: Array<{ name: string; content: string }>;
  dialect: Dialect;
  schemaSQL: string;
  hideCTEs: boolean;
  enableColumnLineage: boolean;
  templateMode?: TemplateMode;
}

/**
 * Result from analysis operations.
 */
export interface AnalysisResult {
  result: AnalyzeResult | null;
  cacheKey: string;
  cacheHit: boolean;
  skipped: boolean;
  timings: {
    totalMs: number;
    cacheReadMs: number;
    schemaParseMs: number;
    analyzeMs: number;
  } | null;
}

/**
 * Backend adapter interface for analysis operations.
 */
export interface BackendAdapter {
  /** Unique identifier for this backend type */
  readonly type: 'rest' | 'wasm';

  /** Initialize the backend (load WASM, check server health, etc.) */
  initialize(): Promise<void>;

  /** Run lineage analysis */
  analyze(payload: AnalysisPayload, options?: AnalyzeWorkerOptions): Promise<AnalysisResult>;

  /** Get cached analysis result (if available) */
  getCached(payload: AnalysisPayload): Promise<AnalysisResult | null>;

  /** Get the engine version */
  getVersion(): Promise<string | null>;

  /** Sync files to the backend (for WASM worker file cache) */
  syncFiles(files: Array<{ name: string; content: string }>): Promise<void>;

  /** Clear the analysis cache */
  clearCache(): Promise<void>;
}

/**
 * REST backend adapter for serve mode.
 * Calls the CLI server's REST API endpoints.
 */
export class RestBackendAdapter implements BackendAdapter {
  readonly type = 'rest' as const;
  private baseUrl: string;
  private version: string | null = null;

  constructor(baseUrl: string = '') {
    this.baseUrl = baseUrl;
  }

  async initialize(): Promise<void> {
    const response = await fetch(`${this.baseUrl}/api/health`);
    if (!response.ok) {
      throw new Error(`Health check failed: ${response.status}`);
    }
    const data = await response.json();
    this.version = data.version || null;
  }

  async analyze(payload: AnalysisPayload): Promise<AnalysisResult> {
    const startTime = performance.now();

    const response = await fetch(`${this.baseUrl}/api/analyze`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        sql: '',
        files: payload.files,
        dialect: payload.dialect,
        schema_sql: payload.schemaSQL,
        hide_ctes: payload.hideCTEs,
        enable_column_lineage: payload.enableColumnLineage,
        template_mode: payload.templateMode,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Analysis failed: ${error}`);
    }

    const result = (await response.json()) as AnalyzeResult;
    const totalMs = performance.now() - startTime;

    return {
      result,
      cacheKey: '',
      cacheHit: false,
      skipped: false,
      timings: {
        totalMs,
        cacheReadMs: 0,
        schemaParseMs: 0,
        analyzeMs: totalMs,
      },
    };
  }

  async getCached(): Promise<AnalysisResult | null> {
    // REST backend doesn't support caching in the same way
    return null;
  }

  async getVersion(): Promise<string | null> {
    return this.version;
  }

  async syncFiles(): Promise<void> {
    // REST backend doesn't need file syncing - files are sent with each request
  }

  async clearCache(): Promise<void> {
    // REST backend doesn't maintain a client-side cache
  }
}

/**
 * WASM backend adapter.
 * Wraps the existing analysis worker for client-side analysis.
 */
export class WasmBackendAdapter implements BackendAdapter {
  readonly type = 'wasm' as const;

  async initialize(): Promise<void> {
    await initializeAnalysisWorker();
  }

  async analyze(payload: AnalysisPayload, options?: AnalyzeWorkerOptions): Promise<AnalysisResult> {
    // Ensure files are synced before analysis
    await this.syncFiles(payload.files);

    const workerResult: AnalysisWorkerResult = await analyzeWithWorker(
      {
        fileNames: payload.files.map((f) => f.name),
        dialect: payload.dialect,
        schemaSQL: payload.schemaSQL,
        hideCTEs: payload.hideCTEs,
        enableColumnLineage: payload.enableColumnLineage,
        templateMode: payload.templateMode,
      },
      options
    );

    return {
      result: workerResult.result,
      cacheKey: workerResult.cacheKey,
      cacheHit: workerResult.cacheHit,
      skipped: workerResult.skipped,
      timings: workerResult.timings,
    };
  }

  async getCached(payload: AnalysisPayload): Promise<AnalysisResult | null> {
    // Ensure files are synced before checking cache
    await this.syncFiles(payload.files);

    const cached = await getCachedAnalysis({
      fileNames: payload.files.map((f) => f.name),
      dialect: payload.dialect,
      schemaSQL: payload.schemaSQL,
      hideCTEs: payload.hideCTEs,
      enableColumnLineage: payload.enableColumnLineage,
      templateMode: payload.templateMode,
    });

    if (!cached) {
      return null;
    }

    return {
      result: cached.result,
      cacheKey: cached.cacheKey,
      cacheHit: cached.cacheHit,
      skipped: cached.skipped,
      timings: cached.timings,
    };
  }

  async getVersion(): Promise<string | null> {
    return getAnalysisWorkerVersion();
  }

  async syncFiles(files: Array<{ name: string; content: string }>): Promise<void> {
    await syncAnalysisFiles(files);
  }

  async clearCache(): Promise<void> {
    await clearAnalysisWorkerCache();
  }
}

/**
 * Backend detection result.
 */
export interface BackendDetectionResult {
  adapter: BackendAdapter;
  detectedType: 'rest' | 'wasm';
}

/**
 * Create a backend adapter with automatic detection.
 *
 * Detection logic:
 * 1. Try to reach /api/health endpoint
 * 2. If successful, use REST backend (serve mode)
 * 3. If failed, fall back to WASM backend (client-side)
 *
 * @param preferWasm - Force WASM backend even if REST is available
 * @param restBaseUrl - Base URL for REST API (defaults to same origin)
 */
export async function createBackendAdapter(
  preferWasm = false,
  restBaseUrl = ''
): Promise<BackendDetectionResult> {
  // If explicitly preferring WASM, skip REST detection
  if (preferWasm) {
    const adapter = new WasmBackendAdapter();
    await adapter.initialize();
    return { adapter, detectedType: 'wasm' };
  }

  // Try REST backend first
  try {
    const adapter = new RestBackendAdapter(restBaseUrl);
    await adapter.initialize();
    return { adapter, detectedType: 'rest' };
  } catch {
    // REST not available, fall back to WASM
  }

  // Fall back to WASM
  const adapter = new WasmBackendAdapter();
  await adapter.initialize();
  return { adapter, detectedType: 'wasm' };
}

/**
 * Check if REST backend is available.
 * Useful for UI to show backend status.
 */
export async function isRestBackendAvailable(baseUrl = ''): Promise<boolean> {
  try {
    const response = await fetch(`${baseUrl}/api/health`, {
      method: 'GET',
      signal: AbortSignal.timeout(2000),
    });
    return response.ok;
  } catch {
    return false;
  }
}
