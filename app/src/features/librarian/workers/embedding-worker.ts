/**
 * Embedding Web Worker
 *
 * Runs text embedding using Xenova/transformers in a separate thread.
 * Supports lazy model loading and batch embedding.
 */
import { EMBEDDING_MODEL } from '../constants';

export interface EmbeddingRequest {
  type: 'embed';
  id: number;
  texts: string[];
}

export interface EmbeddingSuccessResponse {
  id: number;
  success: true;
  result: number[][];
}

export interface EmbeddingErrorResponse {
  id: number;
  success: false;
  error: string;
}

export type EmbeddingResponse = EmbeddingSuccessResponse | EmbeddingErrorResponse;

let cachedPipeline: Awaited<ReturnType<typeof import('@xenova/transformers').pipeline>> | null =
  null;
let pipelinePromise: Promise<
  Awaited<ReturnType<typeof import('@xenova/transformers').pipeline>>
> | null = null;

async function getEmbeddingPipeline() {
  if (cachedPipeline) return cachedPipeline;

  if (!pipelinePromise) {
    pipelinePromise = (async () => {
      const { pipeline: createPipeline, env } = await import('@xenova/transformers');
      // Disable local models and browser cache to prevent Vite dev server
      // from intercepting model file requests and returning HTML instead of JSON
      env.allowLocalModels = false;
      env.allowRemoteModels = true;
      env.useBrowserCache = false;
      env.backends.onnx.wasm.wasmPaths =
        'https://cdn.jsdelivr.net/npm/@xenova/transformers@2.17.2/dist/';
      const p = await createPipeline('feature-extraction', EMBEDDING_MODEL);
      cachedPipeline = p;
      return p;
    })();
  }

  return pipelinePromise;
}

async function handleEmbed(request: EmbeddingRequest): Promise<void> {
  try {
    const model = await getEmbeddingPipeline();
    const results: number[][] = [];

    for (const text of request.texts) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const output = (await model(text, { pooling: 'mean', normalize: true } as any)) as {
        data: Float32Array;
      };
      results.push(Array.from(output.data));
    }

    globalThis.postMessage({
      id: request.id,
      success: true,
      result: results,
    } satisfies EmbeddingSuccessResponse);
  } catch (error) {
    globalThis.postMessage({
      id: request.id,
      success: false,
      error: error instanceof Error ? error.message : String(error),
    } satisfies EmbeddingErrorResponse);
  }
}

globalThis.onmessage = async (event: MessageEvent<EmbeddingRequest>) => {
  const request = event.data;

  switch (request.type) {
    case 'embed':
      await handleEmbed(request);
      break;
  }
};
