/**
 * Service for communicating with the matrix Web Worker.
 * Offloads matrix computation to keep the UI responsive.
 */
import type { StatementLineage } from '@pondpilot/flowscope-core';
import type { MatrixData } from './matrixUtils';
import type {
  MatrixBuildRequest,
  MatrixBuildResponse,
  MatrixMetrics,
} from '../workers/matrix.worker';

interface PendingRequest {
  resolve: (result: MatrixBuildResult) => void;
  reject: (error: Error) => void;
}

export interface MatrixBuildResult {
  tableMatrix: MatrixData;
  scriptMatrix: MatrixData;
  allColumnNames: string[];
  tableMetrics: MatrixMetrics;
  scriptMetrics: MatrixMetrics;
  tableItemCount: number;
  tableItemsRendered: number;
  scriptItemCount: number;
  scriptItemsRendered: number;
}

const MATRIX_DEBUG = !!(import.meta as { env?: { DEV?: boolean } }).env?.DEV;

let worker: Worker | null = null;
let requestIdCounter = 0;
const pendingRequests = new Map<string, PendingRequest>();

function getWorker(): Worker {
  if (!worker) {
    worker = new Worker(
      new URL('../workers/matrix.worker.ts', import.meta.url),
      { type: 'module' }
    );

    worker.onmessage = (event: MessageEvent<MatrixBuildResponse>) => {
      const handlerStart = MATRIX_DEBUG ? performance.now() : 0;
      const {
        requestId,
        tableMatrix,
        scriptMatrix,
        allColumnNames,
        tableMetrics,
        scriptMetrics,
        tableItemCount,
        tableItemsRendered,
        scriptItemCount,
        scriptItemsRendered,
        error,
      } = event.data;
      const pending = pendingRequests.get(requestId);
      if (!pending) return;
      pendingRequests.delete(requestId);

      if (error) {
        pending.reject(new Error(error));
      } else {
        pending.resolve({
          tableMatrix,
          scriptMatrix,
          allColumnNames,
          tableMetrics,
          scriptMetrics,
          tableItemCount,
          tableItemsRendered,
          scriptItemCount,
          scriptItemsRendered,
        });
      }

      if (MATRIX_DEBUG) {
        const handlerDuration = performance.now() - handlerStart;
        if (handlerDuration > 8) {
          console.log(`[Matrix Worker] onmessage handler: ${handlerDuration.toFixed(1)}ms`);
        }
      }
    };

    worker.onerror = (error) => {
      console.error('[Matrix Worker] Worker error:', error);
      for (const pending of pendingRequests.values()) {
        pending.reject(new Error('Worker error'));
      }
      pendingRequests.clear();
    };
  }

  return worker;
}

export async function buildMatrixInWorker(
  statements: StatementLineage[],
  options?: { maxItems?: number }
): Promise<MatrixBuildResult> {
  const requestId = `matrix-${++requestIdCounter}`;
  const workerInstance = getWorker();

  const request: MatrixBuildRequest = {
    type: 'build-matrix',
    requestId,
    statements,
    maxItems: options?.maxItems,
  };

  return new Promise((resolve, reject) => {
    pendingRequests.set(requestId, { resolve, reject });
    workerInstance.postMessage(request);
  });
}

export function cancelPendingMatrixBuilds(): void {
  for (const pending of pendingRequests.values()) {
    pending.reject(new Error('Build cancelled'));
  }
  pendingRequests.clear();
}

export function terminateMatrixWorker(): void {
  if (worker) {
    cancelPendingMatrixBuilds();
    worker.terminate();
    worker = null;
  }
}
