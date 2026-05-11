import { beforeEach, describe, expect, it, vi } from 'vitest';

const transformersMock = vi.hoisted(() => {
  const model = vi.fn(async () => ({ data: new Float32Array([1, 2, 3]) }));
  const pipeline = vi.fn(async () => model);
  const env = {
    allowLocalModels: true,
    allowRemoteModels: false,
    useBrowserCache: false,
    backends: {
      onnx: {
        wasm: {
          wasmPaths: '',
        },
      },
    },
  };

  return { env, model, pipeline };
});

vi.mock('@xenova/transformers', () => transformersMock);

describe('embedding worker', () => {
  beforeEach(() => {
    vi.resetModules();
    transformersMock.model.mockClear();
    transformersMock.pipeline.mockReset();
    transformersMock.pipeline.mockResolvedValue(transformersMock.model);
  });

  it('retries model loading after a transient failure', async () => {
    const responses: unknown[] = [];
    const originalPostMessage = globalThis.postMessage;
    const originalOnMessage = globalThis.onmessage;

    Object.defineProperty(globalThis, 'postMessage', {
      configurable: true,
      value: vi.fn((message: unknown) => responses.push(message)),
    });

    transformersMock.pipeline
      .mockRejectedValueOnce(new Error('network interrupted'))
      .mockResolvedValue(transformersMock.model);

    try {
      await import('../workers/embedding-worker');

      const sendMessage = globalThis.onmessage as (event: MessageEvent) => Promise<void>;
      await sendMessage({
        data: { type: 'embed', id: 1, texts: ['first'], mode: 'passage' },
      } as MessageEvent);
      await sendMessage({
        data: { type: 'embed', id: 2, texts: ['second'], mode: 'passage' },
      } as MessageEvent);
    } finally {
      Object.defineProperty(globalThis, 'postMessage', {
        configurable: true,
        value: originalPostMessage,
      });
      globalThis.onmessage = originalOnMessage;
    }

    expect(transformersMock.pipeline).toHaveBeenCalledTimes(2);
    expect(responses).toEqual([
      {
        id: 1,
        success: false,
        error: 'network interrupted',
      },
      {
        id: 2,
        success: true,
        result: [[1, 2, 3]],
      },
    ]);
  });
});
