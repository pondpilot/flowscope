import type { AnalyzeResult } from '@pondpilot/flowscope-core';
import type { AnalysisWorkerPayload, AnalysisWorkerRequest, AnalysisWorkerResponse, AnalysisWorkerTimings, SyncFilesPayload, WorkerErrorCode } from '../workers/analysis.worker';
import { buildFileSyncKey } from './analysis-hash';
import { AnalysisError, AnalysisErrorCode } from '../types';

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
let lastSyncedFileKey: string | null = null;

function isWorkerSupported(): boolean {
  return typeof Worker !== 'undefined';
}

function getWorker(): Worker {
  if (!workerInstance) {
    workerInstance = new Worker(new URL('../workers/analysis.worker.ts', import.meta.url), {
      type: 'module',
    });

    workerInstance.onmessage = (event: MessageEvent<AnalysisWorkerResponse>) => {
      const response = event.data;
      const pending = pendingRequests.get(response.requestId);
      if (!pending) {
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

    workerInstance.onerror = (error) => {
      for (const [requestId, pending] of pendingRequests) {
        pending.reject(new Error(`Worker error: ${error.message}`));
        pendingRequests.delete(requestId);
      }
    };
  }

  return workerInstance;
}

function sendRequest(message: Omit<AnalysisWorkerRequest, 'requestId'>): Promise<AnalysisWorkerResponse> {
  if (!isWorkerSupported()) {
    return Promise.reject(new Error('Web Workers are not supported in this environment'));
  }

  const requestId = `analysis-${requestCounter += 1}`;
  const worker = getWorker();

  return new Promise((resolve, reject) => {
    pendingRequests.set(requestId, { resolve, reject });
    worker.postMessage({ ...message, requestId });
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

export async function syncAnalysisFiles(files: SyncFilesPayload['files']): Promise<void> {
  const nextKey = buildFileSyncKey({ files });
  if (nextKey === lastSyncedFileKey) {
    return;
  }

  if (files.length === 0) {
    await sendRequest({ type: 'clear-files' });
    lastSyncedFileKey = nextKey;
    return;
  }

  const chunkSize = 5;
  for (let index = 0; index < files.length; index += chunkSize) {
    const chunk = files.slice(index, index + chunkSize);
    await sendRequest({
      type: 'sync-files',
      syncPayload: {
        files: chunk,
        replace: index === 0,
      },
    });
    await yieldToMainThread();
  }

  lastSyncedFileKey = nextKey;
}

export async function initializeAnalysisWorker(): Promise<void> {
  await sendRequest({ type: 'init' });
}

export interface AnalyzeWorkerOptions {
  cacheMaxBytes?: number;
  knownCacheKey?: string | null;
}

export async function analyzeWithWorker(
  payload: AnalysisWorkerPayload,
  options?: AnalyzeWorkerOptions
): Promise<AnalysisWorkerResult> {
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
}

export async function getCachedAnalysis(
  payload: AnalysisWorkerPayload
): Promise<AnalysisWorkerResult | null> {
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
}

export async function getAnalysisWorkerVersion(): Promise<string | null> {
  const response = await sendRequest({ type: 'get-version' });
  return response.version ?? null;
}

export function terminateAnalysisWorker(): void {
  if (workerInstance) {
    workerInstance.terminate();
    workerInstance = null;
  }
  lastSyncedFileKey = null;
  for (const [requestId, pending] of pendingRequests) {
    pending.reject(new Error('Worker terminated'));
    pendingRequests.delete(requestId);
  }
}
