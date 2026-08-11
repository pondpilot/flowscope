import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { syncAnalysisFiles, terminateAnalysisWorker } from '../analysis-worker';
import type { AnalysisWorkerRequest, AnalysisWorkerResponse } from '../../workers/analysis.worker';

class TestWorker {
  static instances: TestWorker[] = [];

  onmessage: ((event: MessageEvent<AnalysisWorkerResponse>) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  readonly messages: AnalysisWorkerRequest[] = [];
  readonly terminate = vi.fn();

  constructor() {
    TestWorker.instances.push(this);
  }

  postMessage(message: AnalysisWorkerRequest): void {
    this.messages.push(message);
    this.onmessage?.({
      data: {
        type: 'sync-result',
        requestId: message.requestId,
      },
    } as MessageEvent<AnalysisWorkerResponse>);
  }
}

describe('syncAnalysisFiles', () => {
  beforeEach(() => {
    TestWorker.instances = [];
    vi.spyOn(console, 'log').mockImplementation(() => undefined);
    vi.stubGlobal('Worker', TestWorker);
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      callback(0);
      return 0;
    });
  });

  afterEach(() => {
    terminateAnalysisWorker();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('resyncs same-length edits and skips unchanged files', async () => {
    const originalFiles = [{ name: 'query.sql', content: 'SELECT 1' }];

    await syncAnalysisFiles(originalFiles);
    await syncAnalysisFiles(originalFiles.map((file) => ({ ...file })));
    await syncAnalysisFiles([{ name: 'query.sql', content: 'SELECT 2' }]);

    expect(TestWorker.instances).toHaveLength(1);
    const syncMessages = TestWorker.instances[0].messages.filter(
      (message) => message.type === 'sync-files'
    );
    expect(syncMessages).toHaveLength(2);
    expect(syncMessages[1].syncPayload?.files).toEqual([
      { name: 'query.sql', content: 'SELECT 2' },
    ]);
  });
});
