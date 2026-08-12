import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  analyzeWithWorker,
  getCachedAnalysis,
  syncAnalysisFiles,
  terminateAnalysisWorker,
} from '../analysis-worker';
import type { AnalysisWorkerRequest, AnalysisWorkerResponse } from '../../workers/analysis.worker';

class TestWorker {
  static instances: TestWorker[] = [];
  static autoRespond = true;

  onmessage: ((event: MessageEvent<AnalysisWorkerResponse>) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  readonly messages: AnalysisWorkerRequest[] = [];
  readonly terminate = vi.fn();

  constructor() {
    TestWorker.instances.push(this);
  }

  postMessage(message: AnalysisWorkerRequest): void {
    this.messages.push(message);
    if (TestWorker.autoRespond) {
      this.respond(message);
    }
  }

  respond(message = this.messages.at(-1)): void {
    if (!message) {
      throw new Error('No worker message to respond to');
    }
    this.onmessage?.({
      data: {
        type: 'sync-result',
        requestId: message.requestId,
      },
    } as MessageEvent<AnalysisWorkerResponse>);
  }

  respondWith(
    response: Omit<AnalysisWorkerResponse, 'requestId'>,
    message = this.messages.at(-1)
  ): void {
    if (!message) {
      throw new Error('No worker message to respond to');
    }
    this.onmessage?.({
      data: { ...response, requestId: message.requestId },
    } as MessageEvent<AnalysisWorkerResponse>);
  }
}

const workerPayload = (fileName: string) => ({
  fileNames: [fileName],
  dialect: 'generic' as const,
  schemaSQL: '',
  hideCTEs: false,
  enableColumnLineage: true,
});

describe('syncAnalysisFiles', () => {
  beforeEach(() => {
    TestWorker.instances = [];
    TestWorker.autoRespond = true;
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

  it('sends only changed files and skips unchanged snapshots', async () => {
    const originalFiles = [{ name: 'query.sql', content: 'SELECT 1' }];

    await syncAnalysisFiles(originalFiles);
    await syncAnalysisFiles(originalFiles.map((file) => ({ ...file })));
    await syncAnalysisFiles([{ name: 'query.sql', content: 'SELECT 2' }]);

    expect(TestWorker.instances).toHaveLength(1);
    const syncMessages = TestWorker.instances[0].messages.filter(
      (message) => message.type === 'sync-files'
    );
    expect(syncMessages).toHaveLength(2);
    expect(syncMessages[0].syncPayload).toEqual({
      files: originalFiles,
      deletedFileNames: [],
      replace: true,
    });
    expect(syncMessages[1].syncPayload?.files).toEqual([
      { name: 'query.sql', content: 'SELECT 2' },
    ]);
    expect(syncMessages[1].syncPayload?.replace).toBe(false);
  });

  it('tracks additions, deletions, renames, and project switches incrementally', async () => {
    await syncAnalysisFiles([
      { name: 'models/orders.sql', content: 'SELECT 1' },
      { name: 'models/users.sql', content: 'SELECT 2' },
    ]);
    await syncAnalysisFiles([
      { name: 'models/orders.sql', content: 'SELECT 10' },
      { name: 'models/customers.sql', content: 'SELECT 3' },
    ]);
    await syncAnalysisFiles([{ name: 'other/project.sql', content: 'SELECT 4' }]);
    await syncAnalysisFiles([]);

    const syncMessages = TestWorker.instances[0].messages.filter(
      (message) => message.type === 'sync-files'
    );
    expect(syncMessages).toHaveLength(4);
    expect(syncMessages[1].syncPayload).toEqual({
      files: [
        { name: 'models/orders.sql', content: 'SELECT 10' },
        { name: 'models/customers.sql', content: 'SELECT 3' },
      ],
      deletedFileNames: ['models/users.sql'],
      replace: false,
    });
    expect(syncMessages[2].syncPayload).toEqual({
      files: [{ name: 'other/project.sql', content: 'SELECT 4' }],
      deletedFileNames: ['models/orders.sql', 'models/customers.sql'],
      replace: false,
    });
    expect(syncMessages[3].syncPayload).toEqual({
      files: [],
      deletedFileNames: ['other/project.sql'],
      replace: false,
    });
  });

  it('orders overlapping snapshots instead of racing full replacements', async () => {
    const first = syncAnalysisFiles([{ name: 'query.sql', content: 'SELECT 1' }]);
    const second = syncAnalysisFiles([{ name: 'query.sql', content: 'SELECT 2' }]);

    await Promise.all([first, second]);

    const syncMessages = TestWorker.instances[0].messages.filter(
      (message) => message.type === 'sync-files'
    );
    expect(syncMessages.map((message) => message.syncPayload?.replace)).toEqual([true, false]);
    expect(syncMessages[1].syncPayload?.files).toEqual([
      { name: 'query.sql', content: 'SELECT 2' },
    ]);
  });

  it('keeps analysis bound to its snapshot when an older cache lookup queues later', async () => {
    TestWorker.autoRespond = false;
    const filesA = [{ name: 'query.sql', content: 'SELECT 1' }];
    const filesB = [{ name: 'query.sql', content: 'SELECT 2' }];
    const initialSync = syncAnalysisFiles(filesA);
    await vi.waitFor(() => expect(TestWorker.instances[0]?.messages).toHaveLength(1));
    const worker = TestWorker.instances[0];

    const analysisB = analyzeWithWorker(workerPayload('query.sql'), { fileSnapshot: filesB });
    worker.respond(worker.messages[0]);
    await initialSync;
    const staleLookupA = getCachedAnalysis(workerPayload('query.sql'), filesA);

    await vi.waitFor(() => expect(worker.messages).toHaveLength(2));
    expect(worker.messages[1].type).toBe('sync-files');
    expect(worker.messages[1].syncPayload?.files).toEqual(filesB);
    worker.respond(worker.messages[1]);

    await vi.waitFor(() => expect(worker.messages).toHaveLength(3));
    expect(worker.messages[2].type).toBe('analyze');
    worker.respondWith(
      {
        type: 'analyze-result',
        result: {
          nodes: [],
          edges: [],
          statements: [],
          issues: [],
          summary: {
            statementCount: 0,
            tableCount: 0,
            columnCount: 0,
            joinCount: 0,
            complexityScore: 0,
            issueCount: { errors: 0, warnings: 0, infos: 0 },
            hasErrors: false,
          },
        },
        cacheKey: 'snapshot-b',
      },
      worker.messages[2]
    );
    await expect(analysisB).resolves.toEqual(expect.objectContaining({ cacheKey: 'snapshot-b' }));

    await vi.waitFor(() => expect(worker.messages).toHaveLength(4));
    expect(worker.messages[3].syncPayload?.files).toEqual(filesA);
    worker.respond(worker.messages[3]);
    await vi.waitFor(() => expect(worker.messages).toHaveLength(5));
    expect(worker.messages[4].type).toBe('get-cache');
    worker.respondWith({ type: 'cache-result' }, worker.messages[4]);
    await expect(staleLookupA).resolves.toBeNull();
  });

  it('replaces the full snapshot after the worker restarts', async () => {
    const files = [{ name: 'query.sql', content: 'SELECT 1' }];
    await syncAnalysisFiles(files);

    terminateAnalysisWorker();
    await syncAnalysisFiles(files);

    expect(TestWorker.instances).toHaveLength(2);
    expect(TestWorker.instances[1].messages[0].syncPayload).toEqual({
      files,
      deletedFileNames: [],
      replace: true,
    });
  });

  it('forces a full replacement when worker state is known to be incomplete', async () => {
    const files = [{ name: 'query.sql', content: 'SELECT 1' }];
    await syncAnalysisFiles(files);
    await syncAnalysisFiles(files, { forceReplace: true });

    const syncMessages = TestWorker.instances[0].messages.filter(
      (message) => message.type === 'sync-files'
    );
    expect(syncMessages).toHaveLength(2);
    expect(syncMessages[1].syncPayload).toEqual({
      files,
      deletedFileNames: [],
      replace: true,
    });
  });

  it('invalidates the client snapshot when the worker crashes', async () => {
    const files = [{ name: 'query.sql', content: 'SELECT 1' }];
    await syncAnalysisFiles(files);

    TestWorker.instances[0].onerror?.({ message: 'crashed' } as ErrorEvent);
    await syncAnalysisFiles(files);

    expect(TestWorker.instances).toHaveLength(2);
    expect(TestWorker.instances[0].terminate).toHaveBeenCalledTimes(1);
    expect(TestWorker.instances[1].messages[0].syncPayload?.replace).toBe(true);
  });

  it('does not recreate a terminated worker from queued synchronization', async () => {
    TestWorker.autoRespond = false;
    const first = syncAnalysisFiles([{ name: 'query.sql', content: 'SELECT 1' }]);
    const second = syncAnalysisFiles([{ name: 'query.sql', content: 'SELECT 2' }]);
    const outcomes = Promise.allSettled([first, second]);

    await vi.waitFor(() => expect(TestWorker.instances[0]?.messages).toHaveLength(1));
    terminateAnalysisWorker();

    expect(await outcomes).toEqual([
      expect.objectContaining({ status: 'rejected' }),
      expect.objectContaining({ status: 'rejected' }),
    ]);
    expect(TestWorker.instances).toHaveLength(1);
  });

  it('ignores a stale error from a worker that has already been replaced', async () => {
    await syncAnalysisFiles([{ name: 'query.sql', content: 'SELECT 1' }]);
    const staleWorker = TestWorker.instances[0];
    terminateAnalysisWorker();

    TestWorker.autoRespond = false;
    const replacementSync = syncAnalysisFiles([{ name: 'query.sql', content: 'SELECT 2' }]);
    await vi.waitFor(() => expect(TestWorker.instances).toHaveLength(2));
    const replacementWorker = TestWorker.instances[1];

    staleWorker.onerror?.({ message: 'late crash' } as ErrorEvent);
    replacementWorker.respond();

    await expect(replacementSync).resolves.toBeUndefined();
    expect(replacementWorker.terminate).not.toHaveBeenCalled();
  });
});
