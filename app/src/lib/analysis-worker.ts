import type { AnalyzeResult } from '@pondpilot/flowscope-core';
import type {
  AnalysisWorkerPayload,
  AnalysisWorkerRequest,
  AnalysisWorkerResponse,
  AnalysisWorkerTimings,
  SyncFilesPayload,
  WorkerErrorCode,
} from '../workers/analysis.worker';
import { AnalysisError, AnalysisErrorCode } from '../types';

// Debug flag for analysis worker logging - only enabled in development
const ANALYSIS_WORKER_DEBUG = !!(import.meta as { env?: { DEV?: boolean } }).env?.DEV;

// Safe time measurement function with fallback
function nowMs(): number {
  if (typeof performance !== 'undefined' && typeof performance.now === 'function') {
    return performance.now();
  }
  return Date.now();
}

/**
 * Map worker error codes to application error codes.
 * This keeps the error handling consistent across the application.
 */
function mapWorkerErrorCode(code: WorkerErrorCode | undefined): AnalysisErrorCode | undefined {
  if (!code) return undefined;
  // WorkerErrorCode values match AnalysisErrorCode values by design
  return code as AnalysisErrorCode;
}

interface PendingRequest {
  resolve: (value: AnalysisWorkerResponse) => void;
  reject: (error: Error) => void;
  worker: Worker;
}

export interface AnalysisWorkerResult {
  result: AnalyzeResult | null;
  cacheKey: string;
  cacheHit: boolean;
  skipped: boolean;
  timings: AnalysisWorkerTimings | null;
}

let workerInstance: Worker | null = null;
let requestCounter = 0;
const pendingRequests = new Map<string, PendingRequest>();
let syncedFiles: Map<string, string> | null = null;
let fileSyncQueue = Promise.resolve();
let fileSyncGeneration = 0;

function resetFileSyncState(): void {
  syncedFiles = null;
  fileSyncGeneration += 1;
  fileSyncQueue = Promise.resolve();
}

function rejectWorkerRequests(worker: Worker, error: Error): void {
  for (const [requestId, pending] of pendingRequests) {
    if (pending.worker !== worker) {
      continue;
    }
    pending.reject(error);
    pendingRequests.delete(requestId);
  }
}

function isWorkerSupported(): boolean {
  return typeof Worker !== 'undefined';
}

function getWorker(): Worker {
  if (!workerInstance) {
    const worker = new Worker(new URL('../workers/analysis.worker.ts', import.meta.url), {
      type: 'module',
    });
    workerInstance = worker;

    worker.onmessage = (event: MessageEvent<AnalysisWorkerResponse>) => {
      const response = event.data;
      const pending = pendingRequests.get(response.requestId);
      if (!pending || pending.worker !== worker) {
        return;
      }
      pendingRequests.delete(response.requestId);

      if (response.error) {
        // Create structured error with code for programmatic handling
        const errorCode = mapWorkerErrorCode(response.errorCode);
        if (errorCode) {
          pending.reject(new AnalysisError(errorCode, response.error));
        } else {
          pending.reject(new Error(response.error));
        }
        return;
      }

      pending.resolve(response);
    };

    worker.onerror = (error) => {
      if (workerInstance === worker) {
        worker.terminate();
        workerInstance = null;
        resetFileSyncState();
      }
      rejectWorkerRequests(worker, new Error(`Worker error: ${error.message}`));
    };
  }

  return workerInstance;
}

function sendRequest(
  message: Omit<AnalysisWorkerRequest, 'requestId'>
): Promise<AnalysisWorkerResponse> {
  if (!isWorkerSupported()) {
    return Promise.reject(new Error('Web Workers are not supported in this environment'));
  }

  const requestId = `analysis-${(requestCounter += 1)}`;
  const worker = getWorker();

  return new Promise((resolve, reject) => {
    pendingRequests.set(requestId, { resolve, reject, worker });
    try {
      worker.postMessage({ ...message, requestId });
    } catch (error) {
      pendingRequests.delete(requestId);
      reject(error instanceof Error ? error : new Error(String(error)));
    }
  });
}

async function yieldToMainThread(): Promise<void> {
  await new Promise<void>((resolve) => {
    if (typeof requestAnimationFrame !== 'undefined') {
      requestAnimationFrame(() => resolve());
      return;
    }
    setTimeout(resolve, 0);
  });
}

async function sendFileChanges(
  files: SyncFilesPayload['files'],
  deletedFileNames: string[],
  replace: boolean,
  generation: number
): Promise<void> {
  const assertCurrentGeneration = () => {
    if (generation !== fileSyncGeneration) {
      throw new Error('Worker terminated');
    }
  };

  if (files.length === 0) {
    assertCurrentGeneration();
    await sendRequest({
      type: 'sync-files',
      syncPayload: { files: [], deletedFileNames, replace },
    });
    assertCurrentGeneration();
    return;
  }

  const chunkSize = 5;
  for (let index = 0; index < files.length; index += chunkSize) {
    assertCurrentGeneration();
    const isFirstChunk = index === 0;
    await sendRequest({
      type: 'sync-files',
      syncPayload: {
        files: files.slice(index, index + chunkSize),
        deletedFileNames: isFirstChunk ? deletedFileNames : [],
        replace: isFirstChunk && replace,
      },
    });
    assertCurrentGeneration();
    if (index + chunkSize < files.length) {
      await yieldToMainThread();
    }
  }
}

async function applyFileSnapshot(
  files: SyncFilesPayload['files'],
  generation: number
): Promise<void> {
  const syncStart = nowMs();
  const targetFilesByName = new Map(files.map((file) => [file.name, file]));
  const targetFiles = [...targetFilesByName.values()];
  const targetContents = new Map(targetFiles.map((file) => [file.name, file.content]));

  if (ANALYSIS_WORKER_DEBUG)
    console.log(`[syncAnalysisFiles] Comparing ${targetFiles.length} files`);

  if (syncedFiles === null) {
    await sendFileChanges(targetFiles, [], true, generation);
    syncedFiles = targetContents;
    if (ANALYSIS_WORKER_DEBUG)
      console.log(
        `[syncAnalysisFiles] Replaced worker snapshot in ${(nowMs() - syncStart).toFixed(1)}ms`
      );
    return;
  }

  const changedFiles = targetFiles.filter((file) => syncedFiles?.get(file.name) !== file.content);
  const deletedFileNames = [...syncedFiles.keys()].filter(
    (fileName) => !targetContents.has(fileName)
  );

  if (changedFiles.length === 0 && deletedFileNames.length === 0) {
    if (ANALYSIS_WORKER_DEBUG) console.log(`[syncAnalysisFiles] Skipped unchanged snapshot`);
    return;
  }

  await sendFileChanges(changedFiles, deletedFileNames, false, generation);
  syncedFiles = targetContents;
  if (ANALYSIS_WORKER_DEBUG)
    console.log(
      `[syncAnalysisFiles] Applied ${changedFiles.length} updates and ${deletedFileNames.length} deletions in ${(nowMs() - syncStart).toFixed(1)}ms`
    );
}

export interface SyncAnalysisFilesOptions {
  /** Replace the worker snapshot even when the client snapshot appears current. */
  forceReplace?: boolean;
}

function withFileSnapshot<T>(
  files: SyncFilesPayload['files'],
  options: SyncAnalysisFilesOptions | undefined,
  task: () => Promise<T>
): Promise<T> {
  const snapshot = files.map((file) => ({ ...file }));
  const generation = fileSyncGeneration;
  const operation = fileSyncQueue
    .catch(() => undefined)
    .then(async () => {
      if (generation !== fileSyncGeneration) {
        throw new Error('Worker terminated');
      }
      if (options?.forceReplace) {
        syncedFiles = null;
      }
      await applyFileSnapshot(snapshot, generation);
      if (generation !== fileSyncGeneration) {
        throw new Error('Worker terminated');
      }
      return task();
    });
  fileSyncQueue = operation.then(
    () => undefined,
    () => undefined
  );
  return operation;
}

/**
 * Synchronize an exact file snapshot to the worker in call order. The first
 * snapshot after worker creation/restart replaces all files; later snapshots
 * send only added, changed, renamed, or deleted paths. The returned promise is
 * a barrier: analysis started after it resolves observes this snapshot.
 */
export function syncAnalysisFiles(
  files: SyncFilesPayload['files'],
  options?: SyncAnalysisFilesOptions
): Promise<void> {
  return withFileSnapshot(files, options, async () => undefined);
}

export async function initializeAnalysisWorker(): Promise<void> {
  await sendRequest({ type: 'init' });
}

export async function clearAnalysisWorkerCache(): Promise<void> {
  await sendRequest({ type: 'clear-cache' });
}

export interface AnalyzeWorkerOptions {
  cacheMaxBytes?: number;
  knownCacheKey?: string | null;
  /** Exact worker file snapshot that must remain bound to this analysis. */
  fileSnapshot?: SyncFilesPayload['files'];
}

export async function analyzeWithWorker(
  payload: AnalysisWorkerPayload,
  options?: AnalyzeWorkerOptions
): Promise<AnalysisWorkerResult> {
  const analyze = async () => {
    const response = await sendRequest({
      type: 'analyze',
      payload,
      cacheMaxBytes: options?.cacheMaxBytes,
      knownCacheKey: options?.knownCacheKey,
    });

    if (!response.cacheKey) {
      throw new Error('Worker returned an empty cache key');
    }

    const skipped = Boolean(response.skipResult);
    if (!response.result && !skipped) {
      throw new Error('Worker returned an empty analysis result');
    }

    return {
      result: response.result ?? null,
      cacheKey: response.cacheKey,
      cacheHit: Boolean(response.cacheHit),
      skipped,
      timings: response.timings ?? null,
    };
  };

  if (options?.fileSnapshot) {
    return withFileSnapshot(options.fileSnapshot, undefined, analyze);
  }
  await fileSyncQueue;
  return analyze();
}

export async function getCachedAnalysis(
  payload: AnalysisWorkerPayload,
  fileSnapshot?: SyncFilesPayload['files']
): Promise<AnalysisWorkerResult | null> {
  const getCached = async () => {
    const response = await sendRequest({ type: 'get-cache', payload });

    if (!response.result || !response.cacheKey) {
      return null;
    }

    return {
      result: response.result,
      cacheKey: response.cacheKey,
      cacheHit: Boolean(response.cacheHit),
      skipped: false,
      timings: response.timings ?? null,
    };
  };

  if (fileSnapshot) {
    return withFileSnapshot(fileSnapshot, undefined, getCached);
  }
  await fileSyncQueue;
  return getCached();
}

export async function getAnalysisWorkerVersion(): Promise<string | null> {
  const response = await sendRequest({ type: 'get-version' });
  return response.version ?? null;
}

export function terminateAnalysisWorker(): void {
  const worker = workerInstance;
  if (worker) {
    worker.terminate();
    workerInstance = null;
  }
  resetFileSyncState();
  for (const [requestId, pending] of pendingRequests) {
    pending.reject(new Error('Worker terminated'));
    pendingRequests.delete(requestId);
  }
}

/**
 * Export analysis result to SQL statements for DuckDB.
 *
 * @param result - The analysis result to export
 * @param schema - Optional schema name to prefix all tables/views (e.g., "lineage")
 * @returns SQL statements (DDL + INSERT) for DuckDB
 */
export async function exportToDuckDbSql(result: AnalyzeResult, schema?: string): Promise<string> {
  const response = await sendRequest({
    type: 'export',
    exportPayload: { result, schema },
  });

  if (!response.exportSql) {
    throw new Error('Worker returned empty export SQL');
  }

  return response.exportSql;
}
