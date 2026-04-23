/**
 * Shared embedding service that manages a singleton Web Worker
 * for text embedding. Used by both PDF processing and query embedding.
 */

let worker: Worker | null = null;
let requestId = 0;

function getWorker(): Worker {
  if (!worker) {
    worker = new Worker(new URL('../workers/embedding-worker.ts', import.meta.url), {
      type: 'module',
    });
  }
  return worker;
}

/**
 * Embed an array of texts using the shared embedding worker.
 * @param mode - 'query' for user questions, 'passage' for documents (PDF chunks)
 */
export function embedTexts(
  texts: string[],
  mode: 'query' | 'passage' = 'passage'
): Promise<number[][]> {
  const w = getWorker();
  const id = requestId;
  requestId += 1;

  return new Promise((resolve, reject) => {
    const handler = (e: MessageEvent) => {
      if (e.data.id === id) {
        w.removeEventListener('message', handler);
        w.removeEventListener('error', errorHandler);
        if (e.data.success) {
          resolve(e.data.result);
        } else {
          reject(new Error(e.data.error));
        }
      }
    };
    const errorHandler = (e: ErrorEvent) => {
      w.removeEventListener('message', handler);
      w.removeEventListener('error', errorHandler);
      worker = null;
      reject(new Error(e.message || 'Embedding worker error'));
    };
    w.addEventListener('message', handler);
    w.addEventListener('error', errorHandler);
    w.postMessage({ type: 'embed', id, texts, mode });
  });
}

/**
 * Terminate the shared embedding worker and release resources.
 */
export function terminateEmbeddingWorker(): void {
  if (worker) {
    worker.terminate();
    worker = null;
  }
}
