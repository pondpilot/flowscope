import { analyzeSql, initWasm, getEngineVersion, exportToDuckDbSql } from '@pondpilot/flowscope-core';
import type { AnalyzeResult, Dialect } from '@pondpilot/flowscope-core';
import { parseSchemaSQL } from '../lib/schema-parser';
import { readCachedAnalysisResult, writeCachedAnalysisResult } from '../lib/analysis-cache';
import { buildAnalysisCacheKey } from '../lib/analysis-hash';
import { ANALYSIS_CACHE_MAX_BYTES } from '../lib/constants';

export interface AnalysisWorkerPayload {
  files?: Array<{ name: string; content: string }>;
  fileNames?: string[];
  dialect: Dialect;
  schemaSQL: string;
  hideCTEs: boolean;
  enableColumnLineage: boolean;
}

export interface SyncFilesPayload {
  files: Array<{ name: string; content: string }>;
  replace?: boolean;
}

export interface ExportPayload {
  result: AnalyzeResult;
}

export interface AnalysisWorkerRequest {
  type: 'init' | 'analyze' | 'get-cache' | 'get-version' | 'sync-files' | 'clear-files' | 'export';
  requestId: string;
  payload?: AnalysisWorkerPayload;
  syncPayload?: SyncFilesPayload;
  exportPayload?: ExportPayload;
  cacheMaxBytes?: number;
  knownCacheKey?: string | null;
}


export interface AnalysisWorkerTimings {
  totalMs: number;
  cacheReadMs: number;
  schemaParseMs: number;
  analyzeMs: number;
}

/**
 * Error codes for worker operations.
 * Must be kept in sync with AnalysisErrorCode in types/index.ts
 */
export const WorkerErrorCode = {
  MISSING_FILE_CONTENT: 'MISSING_FILE_CONTENT',
  NO_FILES_AVAILABLE: 'NO_FILES_AVAILABLE',
} as const;

export type WorkerErrorCode = (typeof WorkerErrorCode)[keyof typeof WorkerErrorCode];

export interface AnalysisWorkerResponse {
  type: 'init-result' | 'analyze-result' | 'cache-result' | 'version-result' | 'sync-result' | 'export-result';
  requestId: string;
  result?: AnalyzeResult | null;
  cacheKey?: string;
  cacheHit?: boolean;
  skipResult?: boolean;
  timings?: AnalysisWorkerTimings;
  version?: string;
  /** SQL statements for DuckDB export */
  exportSql?: string;
  error?: string;
  /** Structured error code for programmatic handling */
  errorCode?: WorkerErrorCode;
}


let wasmReady = false;
const fileCache = new Map<string, string>();

/**
 * Worker-side error with structured error code.
 * The code is extracted in the message handler and sent in the response.
 */
class WorkerError extends Error {
  code: WorkerErrorCode;

  constructor(code: WorkerErrorCode, message: string) {
    super(message);
    this.code = code;
    this.name = 'WorkerError';
  }
}

function nowMs(): number {
  if (typeof performance !== 'undefined' && typeof performance.now === 'function') {
    return performance.now();
  }
  return Date.now();
}

async function ensureWasmReady(): Promise<void> {
  if (wasmReady) {
    return;
  }
  await initWasm();
  wasmReady = true;
}

function resolveFiles(payload: AnalysisWorkerPayload): Array<{ name: string; content: string }> {
  if (payload.files && payload.files.length > 0) {
    return payload.files;
  }

  if (!payload.fileNames || payload.fileNames.length === 0) {
    return [];
  }

  return payload.fileNames.map((name) => {
    const content = fileCache.get(name);
    if (content === undefined) {
      throw new WorkerError(
        WorkerErrorCode.MISSING_FILE_CONTENT,
        `Missing file content for ${name}`
      );
    }
    return { name, content };
  });
}

function resolvePayload(payload: AnalysisWorkerPayload): AnalysisWorkerPayload & { files: Array<{ name: string; content: string }> } {
  const files = resolveFiles(payload);
  if (files.length === 0) {
    throw new WorkerError(
      WorkerErrorCode.NO_FILES_AVAILABLE,
      'No files available for analysis'
    );
  }
  return {
    ...payload,
    files,
  };
}

async function buildImportedSchema(payload: AnalysisWorkerPayload & { files: Array<{ name: string; content: string }> }): Promise<{
  schema: { allowImplied: boolean; tables: Array<{ name: string; schema?: string; catalog?: string; columns?: Array<{ name: string; dataType?: string }> }> } | undefined;
  schemaErrors: string[];
}> {
  if (!payload.schemaSQL.trim()) {
    return { schema: undefined, schemaErrors: [] };
  }

  const { tables, errors } = await parseSchemaSQL(payload.schemaSQL, payload.dialect, analyzeSql);
  const schema = tables.length > 0
    ? {
        allowImplied: true,
        tables,
      }
    : undefined;

  return { schema, schemaErrors: errors };
}

async function runAnalysis(
  payload: AnalysisWorkerPayload,
  cacheMaxBytes: number,
  knownCacheKey?: string | null
): Promise<AnalysisWorkerResponse> {
  const totalStart = nowMs();
  const resolvedPayload = resolvePayload(payload);

  const cacheKey = buildAnalysisCacheKey({
    files: resolvedPayload.files,
    dialect: resolvedPayload.dialect,
    schemaSQL: resolvedPayload.schemaSQL,
    hideCTEs: resolvedPayload.hideCTEs,
    enableColumnLineage: resolvedPayload.enableColumnLineage,
  });

  if (knownCacheKey && knownCacheKey === cacheKey) {
    return {
      type: 'analyze-result',
      requestId: '',
      cacheKey,
      cacheHit: true,
      skipResult: true,
      timings: {
        totalMs: nowMs() - totalStart,
        cacheReadMs: 0,
        schemaParseMs: 0,
        analyzeMs: 0,
      },
    };
  }

  const cacheReadStart = nowMs();
  let cached: AnalyzeResult | null = null;
  try {
    cached = await readCachedAnalysisResult(cacheKey);
  } catch {
    cached = null;
  }
  const cacheReadMs = nowMs() - cacheReadStart;

  if (cached) {
    return {
      type: 'analyze-result',
      requestId: '',
      result: cached,
      cacheKey,
      cacheHit: true,
      timings: {
        totalMs: nowMs() - totalStart,
        cacheReadMs,
        schemaParseMs: 0,
        analyzeMs: 0,
      },
    };
  }

  const schemaStart = nowMs();
  const { schema, schemaErrors } = await buildImportedSchema(resolvedPayload);
  const schemaParseMs = nowMs() - schemaStart;

  const analyzeStart = nowMs();
  const result = await analyzeSql({
    sql: '',
    files: resolvedPayload.files,
    dialect: resolvedPayload.dialect,
    schema,
    options: {
      enableColumnLineage: resolvedPayload.enableColumnLineage,
      hideCtes: resolvedPayload.hideCTEs,
    },
  });
  const analyzeMs = nowMs() - analyzeStart;

  if (schemaErrors.length > 0) {
    const schemaIssues = schemaErrors.map((errorMessage) => ({
      severity: 'warning' as const,
      code: 'SCHEMA_PARSE_ERROR',
      message: `Schema DDL: ${errorMessage}`,
      locations: [],
    }));

    result.issues = [...(result.issues || []), ...schemaIssues];
  }

  try {
    await writeCachedAnalysisResult(cacheKey, result, cacheMaxBytes);
  } catch {
    // Cache write failure is non-critical
  }

  return {
    type: 'analyze-result',
    requestId: '',
    result,
    cacheKey,
    cacheHit: false,
    timings: {
      totalMs: nowMs() - totalStart,
      cacheReadMs,
      schemaParseMs,
      analyzeMs,
    },
  };
}

async function getCachedAnalysis(payload: AnalysisWorkerPayload): Promise<AnalysisWorkerResponse> {
  const totalStart = nowMs();
  const resolvedPayload = resolvePayload(payload);

  const cacheKey = buildAnalysisCacheKey({
    files: resolvedPayload.files,
    dialect: resolvedPayload.dialect,
    schemaSQL: resolvedPayload.schemaSQL,
    hideCTEs: resolvedPayload.hideCTEs,
    enableColumnLineage: resolvedPayload.enableColumnLineage,
  });

  const cacheReadStart = nowMs();
  let cached: AnalyzeResult | null = null;
  try {
    cached = await readCachedAnalysisResult(cacheKey);
  } catch {
    cached = null;
  }
  const cacheReadMs = nowMs() - cacheReadStart;

  return {
    type: 'cache-result',
    requestId: '',
    result: cached,
    cacheKey,
    cacheHit: Boolean(cached),
    timings: {
      totalMs: nowMs() - totalStart,
      cacheReadMs,
      schemaParseMs: 0,
      analyzeMs: 0,
    },
  };
}

self.onmessage = async (event: MessageEvent<AnalysisWorkerRequest>) => {
  const { type, requestId, payload, syncPayload, exportPayload, cacheMaxBytes, knownCacheKey } = event.data;

  try {
    if (type === 'sync-files') {
      if (!syncPayload) {
        const response: AnalysisWorkerResponse = {
          type: 'sync-result',
          requestId,
          error: 'Missing sync payload',
        };
        self.postMessage(response);
        return;
      }

      if (syncPayload.replace) {
        fileCache.clear();
      }

      for (const file of syncPayload.files) {
        fileCache.set(file.name, file.content);
      }

      const response: AnalysisWorkerResponse = {
        type: 'sync-result',
        requestId,
      };
      self.postMessage(response);
      return;
    }

    if (type === 'clear-files') {
      fileCache.clear();
      const response: AnalysisWorkerResponse = {
        type: 'sync-result',
        requestId,
      };
      self.postMessage(response);
      return;
    }

    if (type === 'init') {
      await ensureWasmReady();
      const response: AnalysisWorkerResponse = {
        type: 'init-result',
        requestId,
      };
      self.postMessage(response);
      return;
    }

    if (type === 'get-version') {
      await ensureWasmReady();
      const response: AnalysisWorkerResponse = {
        type: 'version-result',
        requestId,
        version: getEngineVersion(),
      };
      self.postMessage(response);
      return;
    }

    if (type === 'export') {
      if (!exportPayload) {
        const response: AnalysisWorkerResponse = {
          type: 'export-result',
          requestId,
          error: 'Missing export payload',
        };
        self.postMessage(response);
        return;
      }

      await ensureWasmReady();
      const sql = await exportToDuckDbSql(exportPayload.result);
      const response: AnalysisWorkerResponse = {
        type: 'export-result',
        requestId,
        exportSql: sql,
      };
      self.postMessage(response);
      return;
    }

    if (!payload) {
      const response: AnalysisWorkerResponse = {
        type: type === 'get-cache' ? 'cache-result' : 'analyze-result',
        requestId,
        error: 'Missing analysis payload',
      };
      self.postMessage(response);
      return;
    }

    await ensureWasmReady();

    if (type === 'get-cache') {
      const cacheResponse = await getCachedAnalysis(payload);
      cacheResponse.requestId = requestId;
      self.postMessage(cacheResponse);
      return;
    }

    const resolvedCacheMaxBytes = cacheMaxBytes ?? ANALYSIS_CACHE_MAX_BYTES;
    const analysisResponse = await runAnalysis(payload, resolvedCacheMaxBytes, knownCacheKey);
    analysisResponse.requestId = requestId;
    self.postMessage(analysisResponse);
  } catch (error) {
    const response: AnalysisWorkerResponse = {
      type: type === 'get-cache' ? 'cache-result' : 'analyze-result',
      requestId,
      error: error instanceof Error ? error.message : String(error),
      // Include error code for programmatic handling without string matching
      errorCode: error instanceof WorkerError ? error.code : undefined,
    };
    self.postMessage(response);
  }
};
