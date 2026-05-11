import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('../services/ai-service', () => ({
  loadAIConfig: vi.fn(() => null),
}));

import { loadAIConfig } from '../services/ai-service';

import { CHAT_HISTORY_LIMIT } from '../constants';
import {
  useLibrarianStore,
  useLibrarianMessages,
  useLibrarianPdfFiles,
  useLibrarianPdfChunks,
} from '../store';
import type { PdfChunk, PdfFile } from '../types';

const PROJECT_A = 'proj-a';
const PROJECT_B = 'proj-b';

function makePdfFile(overrides: Partial<PdfFile> = {}): PdfFile {
  return {
    id: 'file-1',
    name: 'test.pdf',
    size: 1024,
    status: 'processing',
    uploadedAt: Date.now(),
    ...overrides,
  };
}

function makePdfChunk(overrides: Partial<PdfChunk> = {}): PdfChunk {
  return {
    id: 'chunk-1',
    fileId: 'file-1',
    fileName: 'test.pdf',
    text: 'chunk text',
    pageNumber: 1,
    embedding: [0.1, 0.2],
    ...overrides,
  };
}

function resetStore(activeProjectId: string | null = PROJECT_A) {
  useLibrarianStore.setState({
    byProject: {},
    activeProjectId,
    isLoading: false,
    hasConfig: false,
    messages: [],
    pdfFiles: [],
    pdfChunks: [],
  });
}

describe('useLibrarianStore', () => {
  beforeEach(() => {
    vi.mocked(loadAIConfig).mockReturnValue(null);
    resetStore(PROJECT_A);
  });

  // ---------- messages ----------

  describe('addMessage', () => {
    it('adds a user message', () => {
      useLibrarianStore.getState().addMessage('user', 'hello');
      const { messages } = useLibrarianStore.getState();
      expect(messages).toHaveLength(1);
      expect(messages[0].role).toBe('user');
      expect(messages[0].content).toBe('hello');
      expect(messages[0].id).toBeTruthy();
      expect(messages[0].timestamp).toBeGreaterThan(0);
    });

    it('adds an assistant message', () => {
      useLibrarianStore.getState().addMessage('assistant', 'hi there');
      const { messages } = useLibrarianStore.getState();
      expect(messages).toHaveLength(1);
      expect(messages[0].role).toBe('assistant');
    });

    it('keeps all messages without truncating', () => {
      const store = useLibrarianStore.getState();
      const total = CHAT_HISTORY_LIMIT + 5;
      for (let i = 0; i < total; i++) {
        store.addMessage('user', `msg-${i}`);
      }
      const { messages } = useLibrarianStore.getState();
      expect(messages).toHaveLength(total);
      expect(messages[0].content).toBe('msg-0');
      expect(messages[messages.length - 1].content).toBe(`msg-${total - 1}`);
    });

    it('no-ops when activeProjectId is null', () => {
      resetStore(null);
      useLibrarianStore.getState().addMessage('user', 'orphan');
      const state = useLibrarianStore.getState();
      expect(state.messages).toHaveLength(0);
      expect(state.byProject).toEqual({});
    });

    it('lazily initializes the bucket on first write', () => {
      // No bucket exists yet; addMessage should create one for the active project.
      expect(useLibrarianStore.getState().byProject[PROJECT_A]).toBeUndefined();
      useLibrarianStore.getState().addMessage('user', 'hi');
      const bucket = useLibrarianStore.getState().byProject[PROJECT_A];
      expect(bucket).toBeDefined();
      expect(bucket.messages).toHaveLength(1);
      expect(bucket.pdfFiles).toEqual([]);
      expect(bucket.pdfChunks).toEqual([]);
    });
  });

  describe('clearMessages', () => {
    it('removes all messages for the active project', () => {
      const store = useLibrarianStore.getState();
      store.addMessage('user', 'a');
      store.addMessage('assistant', 'b');
      store.clearMessages();
      expect(useLibrarianStore.getState().messages).toHaveLength(0);
      expect(useLibrarianStore.getState().byProject[PROJECT_A].messages).toHaveLength(0);
    });

    it('does not touch other projects', () => {
      useLibrarianStore.getState().addMessage('user', 'a-msg');
      useLibrarianStore.getState().setActiveProjectId(PROJECT_B);
      useLibrarianStore.getState().addMessage('user', 'b-msg');
      useLibrarianStore.getState().clearMessages();
      const { byProject } = useLibrarianStore.getState();
      expect(byProject[PROJECT_A].messages).toHaveLength(1);
      expect(byProject[PROJECT_B].messages).toHaveLength(0);
    });

    it('no-ops when activeProjectId is null', () => {
      useLibrarianStore.getState().addMessage('user', 'a-msg');
      resetStore(null);
      useLibrarianStore.setState({
        byProject: { [PROJECT_A]: { messages: [], pdfFiles: [], pdfChunks: [] } },
      });
      // No active id; clearMessages should not throw and not modify state.
      expect(() => useLibrarianStore.getState().clearMessages()).not.toThrow();
    });
  });

  // ---------- loading ----------

  describe('setLoading', () => {
    it('sets isLoading to true', () => {
      useLibrarianStore.getState().setLoading(true);
      expect(useLibrarianStore.getState().isLoading).toBe(true);
    });

    it('sets isLoading to false', () => {
      useLibrarianStore.getState().setLoading(true);
      useLibrarianStore.getState().setLoading(false);
      expect(useLibrarianStore.getState().isLoading).toBe(false);
    });

    it('isLoading is global (not per project)', () => {
      useLibrarianStore.getState().setLoading(true);
      useLibrarianStore.getState().setActiveProjectId(PROJECT_B);
      expect(useLibrarianStore.getState().isLoading).toBe(true);
    });
  });

  // ---------- PDF files ----------

  describe('addPdfFile', () => {
    it('adds a PDF file', () => {
      useLibrarianStore.getState().addPdfFile(makePdfFile());
      expect(useLibrarianStore.getState().pdfFiles).toHaveLength(1);
      expect(useLibrarianStore.getState().pdfFiles[0].name).toBe('test.pdf');
    });

    it('adds multiple PDF files', () => {
      const store = useLibrarianStore.getState();
      store.addPdfFile(makePdfFile({ id: 'f1', name: 'a.pdf' }));
      store.addPdfFile(makePdfFile({ id: 'f2', name: 'b.pdf' }));
      expect(useLibrarianStore.getState().pdfFiles).toHaveLength(2);
    });

    it('no-ops when activeProjectId is null', () => {
      resetStore(null);
      useLibrarianStore.getState().addPdfFile(makePdfFile());
      expect(useLibrarianStore.getState().pdfFiles).toHaveLength(0);
    });
  });

  describe('addPdfChunks', () => {
    it('adds chunks to the store', () => {
      useLibrarianStore.getState().addPdfFile(makePdfFile({ id: 'file-1' }));
      useLibrarianStore
        .getState()
        .addPdfChunks([makePdfChunk({ id: 'c1' }), makePdfChunk({ id: 'c2' })]);
      expect(useLibrarianStore.getState().pdfChunks).toHaveLength(2);
    });

    it('appends to existing chunks', () => {
      const store = useLibrarianStore.getState();
      store.addPdfFile(makePdfFile({ id: 'file-1' }));
      store.addPdfChunks([makePdfChunk({ id: 'c1' })]);
      store.addPdfChunks([makePdfChunk({ id: 'c2' })]);
      expect(useLibrarianStore.getState().pdfChunks).toHaveLength(2);
    });

    it('drops chunks for files that no longer exist', () => {
      const store = useLibrarianStore.getState();
      store.addPdfFile(makePdfFile({ id: 'f1' }));
      store.removePdf('f1');
      store.addPdfChunks([makePdfChunk({ id: 'c1', fileId: 'f1' })]);
      expect(useLibrarianStore.getState().pdfChunks).toEqual([]);
    });

    it('logs a debug trace when chunks are dropped for missing files', () => {
      const debugSpy = vi.spyOn(console, 'debug').mockImplementation(() => {});
      const store = useLibrarianStore.getState();
      store.addPdfFile(makePdfFile({ id: 'f1' }));
      store.addPdfChunks([
        makePdfChunk({ id: 'c1', fileId: 'f1' }),
        makePdfChunk({ id: 'c2', fileId: 'ghost' }),
      ]);
      expect(debugSpy).toHaveBeenCalledWith(expect.stringContaining('dropped 1 chunk(s)'));
      debugSpy.mockRestore();
    });
  });

  describe('removePdf', () => {
    it('removes the file and its associated chunks', () => {
      const store = useLibrarianStore.getState();
      store.addPdfFile(makePdfFile({ id: 'f1' }));
      store.addPdfFile(makePdfFile({ id: 'f2', name: 'other.pdf' }));
      store.addPdfChunks([
        makePdfChunk({ id: 'c1', fileId: 'f1' }),
        makePdfChunk({ id: 'c2', fileId: 'f1' }),
        makePdfChunk({ id: 'c3', fileId: 'f2' }),
      ]);

      useLibrarianStore.getState().removePdf('f1');

      const state = useLibrarianStore.getState();
      expect(state.pdfFiles).toHaveLength(1);
      expect(state.pdfFiles[0].id).toBe('f2');
      expect(state.pdfChunks).toHaveLength(1);
      expect(state.pdfChunks[0].id).toBe('c3');
    });

    it('does nothing when fileId does not exist', () => {
      const store = useLibrarianStore.getState();
      store.addPdfFile(makePdfFile({ id: 'f1' }));
      store.removePdf('nonexistent');
      expect(useLibrarianStore.getState().pdfFiles).toHaveLength(1);
    });
  });

  describe('setPdfStatus', () => {
    it('updates the status of a PDF file', () => {
      const store = useLibrarianStore.getState();
      store.addPdfFile(makePdfFile({ id: 'f1', status: 'processing' }));
      store.setPdfStatus('f1', 'ready');
      expect(useLibrarianStore.getState().pdfFiles[0].status).toBe('ready');
    });

    it('sets an error message', () => {
      const store = useLibrarianStore.getState();
      store.addPdfFile(makePdfFile({ id: 'f1' }));
      store.setPdfStatus('f1', 'error', 'something went wrong');
      const file = useLibrarianStore.getState().pdfFiles[0];
      expect(file.status).toBe('error');
      expect(file.error).toBe('something went wrong');
    });

    it('does not affect other files', () => {
      const store = useLibrarianStore.getState();
      store.addPdfFile(makePdfFile({ id: 'f1', status: 'processing' }));
      store.addPdfFile(makePdfFile({ id: 'f2', name: 'b.pdf', status: 'processing' }));
      store.setPdfStatus('f1', 'ready');
      expect(useLibrarianStore.getState().pdfFiles[1].status).toBe('processing');
    });
  });

  describe('hasPdfFile', () => {
    it('returns true when file exists in the active project', () => {
      useLibrarianStore.getState().addPdfFile(makePdfFile({ name: 'test.pdf' }));
      expect(useLibrarianStore.getState().hasPdfFile('test.pdf')).toBe(true);
    });

    it('returns false when file does not exist', () => {
      expect(useLibrarianStore.getState().hasPdfFile('nope.pdf')).toBe(false);
    });

    it('matches by name, not id', () => {
      useLibrarianStore.getState().addPdfFile(makePdfFile({ id: 'f1', name: 'report.pdf' }));
      expect(useLibrarianStore.getState().hasPdfFile('report.pdf')).toBe(true);
      expect(useLibrarianStore.getState().hasPdfFile('f1')).toBe(false);
    });

    it('matches names case-insensitively', () => {
      useLibrarianStore.getState().addPdfFile(makePdfFile({ id: 'f1', name: 'Report.pdf' }));
      expect(useLibrarianStore.getState().hasPdfFile('report.pdf')).toBe(true);
      expect(useLibrarianStore.getState().hasPdfFile('REPORT.PDF')).toBe(true);
    });

    it('is scoped to the active project', () => {
      // Upload report.pdf to project A
      useLibrarianStore.getState().addPdfFile(makePdfFile({ id: 'f1', name: 'report.pdf' }));
      expect(useLibrarianStore.getState().hasPdfFile('report.pdf')).toBe(true);

      // Switch to project B — A's file must not be visible
      useLibrarianStore.getState().setActiveProjectId(PROJECT_B);
      expect(useLibrarianStore.getState().hasPdfFile('report.pdf')).toBe(false);
    });

    it('returns false when activeProjectId is null', () => {
      resetStore(null);
      expect(useLibrarianStore.getState().hasPdfFile('anything.pdf')).toBe(false);
    });
  });

  // ---------- per-project isolation ----------

  describe('per-project isolation', () => {
    it('isolates messages and PDFs across two project ids', () => {
      const store = useLibrarianStore.getState();
      // Project A
      store.addMessage('user', 'hello from A');
      store.addPdfFile(makePdfFile({ id: 'a1', name: 'a.pdf' }));
      store.addPdfChunks([makePdfChunk({ id: 'ac1', fileId: 'a1', fileName: 'a.pdf' })]);

      // Switch to Project B and add different content
      useLibrarianStore.getState().setActiveProjectId(PROJECT_B);
      const stateAfterSwitch = useLibrarianStore.getState();
      expect(stateAfterSwitch.messages).toEqual([]);
      expect(stateAfterSwitch.pdfFiles).toEqual([]);
      expect(stateAfterSwitch.pdfChunks).toEqual([]);

      useLibrarianStore.getState().addMessage('user', 'hello from B');
      useLibrarianStore.getState().addPdfFile(makePdfFile({ id: 'b1', name: 'b.pdf' }));
      useLibrarianStore
        .getState()
        .addPdfChunks([makePdfChunk({ id: 'bc1', fileId: 'b1', fileName: 'b.pdf' })]);

      const { byProject } = useLibrarianStore.getState();
      expect(byProject[PROJECT_A].messages.map((m) => m.content)).toEqual(['hello from A']);
      expect(byProject[PROJECT_A].pdfFiles[0].name).toBe('a.pdf');
      expect(byProject[PROJECT_A].pdfChunks).toHaveLength(1);
      expect(byProject[PROJECT_B].messages.map((m) => m.content)).toEqual(['hello from B']);
      expect(byProject[PROJECT_B].pdfFiles[0].name).toBe('b.pdf');
      expect(byProject[PROJECT_B].pdfChunks).toHaveLength(1);
    });

    it('switching back to a previous project restores its data', () => {
      const store = useLibrarianStore.getState();
      store.addMessage('user', 'A says hi');
      store.addPdfFile(makePdfFile({ id: 'a1', name: 'a.pdf' }));

      useLibrarianStore.getState().setActiveProjectId(PROJECT_B);
      useLibrarianStore.getState().addMessage('user', 'B says hi');

      useLibrarianStore.getState().setActiveProjectId(PROJECT_A);
      const state = useLibrarianStore.getState();
      expect(state.messages.map((m) => m.content)).toEqual(['A says hi']);
      expect(state.pdfFiles[0].name).toBe('a.pdf');
    });

    it('setActiveProjectId(null) blanks the flat mirror without dropping buckets', () => {
      const store = useLibrarianStore.getState();
      store.addMessage('user', 'A says hi');
      useLibrarianStore.getState().setActiveProjectId(null);
      const state = useLibrarianStore.getState();
      expect(state.messages).toEqual([]);
      expect(state.pdfFiles).toEqual([]);
      expect(state.pdfChunks).toEqual([]);
      expect(state.byProject[PROJECT_A].messages).toHaveLength(1);
    });
  });

  // ---------- explicit-bucket writes ----------

  describe('addMessageToProject', () => {
    it('writes to a non-active bucket that already exists', () => {
      // Seed B's bucket while it's active, then switch back to A.
      useLibrarianStore.getState().setActiveProjectId(PROJECT_B);
      useLibrarianStore.getState().addMessage('user', 'B existing');
      useLibrarianStore.getState().setActiveProjectId(PROJECT_A);

      // Now write to B from A's context — bucket exists, write succeeds.
      useLibrarianStore.getState().addMessageToProject(PROJECT_B, 'assistant', 'for B');
      const state = useLibrarianStore.getState();
      expect(state.byProject[PROJECT_B].messages.map((m) => m.content)).toEqual([
        'B existing',
        'for B',
      ]);
      // A is untouched; flat mirror reflects the active project (A), which is empty.
      expect(state.byProject[PROJECT_A]).toBeUndefined();
      expect(state.messages).toEqual([]);
    });

    it('updates the flat mirror when writing to the active project', () => {
      useLibrarianStore.getState().addMessageToProject(PROJECT_A, 'assistant', 'reply');
      const state = useLibrarianStore.getState();
      expect(state.byProject[PROJECT_A].messages).toHaveLength(1);
      expect(state.messages.map((m) => m.content)).toEqual(['reply']);
    });

    it('preserves cross-project routing after a mid-flight switch', () => {
      // Simulates use-librarian-chat: capture the project id, switch active,
      // then write the assistant response back to the captured id.
      useLibrarianStore.getState().addMessageToProject(PROJECT_A, 'user', 'A asked');
      useLibrarianStore.getState().setActiveProjectId(PROJECT_B);
      useLibrarianStore.getState().addMessageToProject(PROJECT_A, 'assistant', 'A answer');

      const state = useLibrarianStore.getState();
      expect(state.byProject[PROJECT_A].messages.map((m) => m.content)).toEqual([
        'A asked',
        'A answer',
      ]);
      // B's bucket is untouched (no leakage).
      expect(state.byProject[PROJECT_B]?.messages ?? []).toEqual([]);
    });

    it('drops the write when the bucket was pruned (project deleted mid-flight)', () => {
      // Seed A's bucket as active, then simulate deletion: switch away and
      // prune. A late-arriving response must not resurrect a zombie bucket.
      useLibrarianStore.getState().addMessage('user', 'A asked');
      useLibrarianStore.getState().setActiveProjectId(PROJECT_B);
      useLibrarianStore.getState().pruneProjectBuckets(new Set([PROJECT_B]));
      expect(useLibrarianStore.getState().byProject[PROJECT_A]).toBeUndefined();

      useLibrarianStore.getState().addMessageToProject(PROJECT_A, 'assistant', 'late reply');
      const state = useLibrarianStore.getState();
      expect(state.byProject[PROJECT_A]).toBeUndefined();
    });
  });

  describe('addPdfFileToProject / addPdfChunksToProject / setPdfStatusForProject', () => {
    it('routes the full PDF lifecycle to the originating project after a mid-flight switch', () => {
      // Simulates handlePdfUpload capturing projectId at upload time:
      // user uploads in A, switches to B mid-process, chunks/status arrive late.
      useLibrarianStore
        .getState()
        .addPdfFileToProject(PROJECT_A, makePdfFile({ id: 'f1', status: 'processing' }));
      useLibrarianStore.getState().setActiveProjectId(PROJECT_B);

      useLibrarianStore
        .getState()
        .addPdfChunksToProject(PROJECT_A, [makePdfChunk({ id: 'c1', fileId: 'f1' })]);
      useLibrarianStore.getState().setPdfStatusForProject(PROJECT_A, 'f1', 'ready');

      const state = useLibrarianStore.getState();
      expect(state.byProject[PROJECT_A].pdfFiles[0].status).toBe('ready');
      expect(state.byProject[PROJECT_A].pdfChunks).toHaveLength(1);
      // B's bucket is untouched.
      expect(state.byProject[PROJECT_B]?.pdfFiles ?? []).toEqual([]);
      expect(state.byProject[PROJECT_B]?.pdfChunks ?? []).toEqual([]);
      // Flat mirror tracks active project B.
      expect(state.pdfFiles).toEqual([]);
      expect(state.pdfChunks).toEqual([]);
    });

    it('drops PDF writes when the bucket was pruned', () => {
      useLibrarianStore.getState().addPdfFile(makePdfFile({ id: 'f1' }));
      useLibrarianStore.getState().setActiveProjectId(PROJECT_B);
      useLibrarianStore.getState().pruneProjectBuckets(new Set([PROJECT_B]));

      useLibrarianStore
        .getState()
        .addPdfChunksToProject(PROJECT_A, [makePdfChunk({ id: 'c1', fileId: 'f1' })]);
      useLibrarianStore.getState().setPdfStatusForProject(PROJECT_A, 'f1', 'ready');

      expect(useLibrarianStore.getState().byProject[PROJECT_A]).toBeUndefined();
    });
  });

  describe('pruneProjectBuckets', () => {
    it('drops buckets whose id is not in the valid set', () => {
      useLibrarianStore.getState().addMessage('user', 'A msg');
      useLibrarianStore.getState().setActiveProjectId(PROJECT_B);
      useLibrarianStore.getState().addMessage('user', 'B msg');

      // Only PROJECT_A is valid — B's bucket should be dropped.
      useLibrarianStore.getState().pruneProjectBuckets(new Set([PROJECT_A]));
      const state = useLibrarianStore.getState();
      expect(state.byProject[PROJECT_A]).toBeDefined();
      expect(state.byProject[PROJECT_B]).toBeUndefined();
    });

    it('is a no-op when all buckets are valid', () => {
      useLibrarianStore.getState().addMessage('user', 'A msg');
      const before = useLibrarianStore.getState().byProject;
      useLibrarianStore.getState().pruneProjectBuckets(new Set([PROJECT_A, PROJECT_B]));
      // Reference equality: state was returned unchanged.
      expect(useLibrarianStore.getState().byProject).toBe(before);
    });

    it('does not modify the flat mirror when the active project survives', () => {
      useLibrarianStore.getState().addMessage('user', 'A msg');
      useLibrarianStore.getState().pruneProjectBuckets(new Set([PROJECT_A]));
      const state = useLibrarianStore.getState();
      expect(state.activeProjectId).toBe(PROJECT_A);
      expect(state.messages.map((m) => m.content)).toEqual(['A msg']);
    });
  });

  // ---------- selector hooks ----------

  describe('selector hooks', () => {
    it('useLibrarianMessages returns the active project bucket', () => {
      useLibrarianStore.getState().addMessage('user', 'a');
      // Hooks return state by reading the store synchronously when not inside a component.
      // Here we just verify the selector logic against getState().
      const id = useLibrarianStore.getState().activeProjectId!;
      expect(useLibrarianStore.getState().byProject[id].messages).toHaveLength(1);
      // Indirect: stable empty array when project absent
      useLibrarianStore.getState().setActiveProjectId('unknown-id');
      const state = useLibrarianStore.getState();
      expect(state.byProject['unknown-id']).toBeUndefined();
      // The selector hooks themselves are exercised via component tests; here we
      // confirm they're exported and typed correctly.
      expect(useLibrarianMessages).toBeTypeOf('function');
      expect(useLibrarianPdfFiles).toBeTypeOf('function');
      expect(useLibrarianPdfChunks).toBeTypeOf('function');
    });
  });

  // ---------- hasConfig / refreshConfig ----------

  describe('hasConfig', () => {
    it('defaults to false when no config exists', () => {
      expect(useLibrarianStore.getState().hasConfig).toBe(false);
    });

    it('updates to true after refreshConfig when config exists', () => {
      vi.mocked(loadAIConfig).mockReturnValue({
        provider: 'openai',
        apiKey: 'sk-test',
        model: 'gpt-4o',
      });
      useLibrarianStore.getState().refreshConfig();
      expect(useLibrarianStore.getState().hasConfig).toBe(true);
    });

    it('updates to false after refreshConfig when config is removed', () => {
      useLibrarianStore.setState({ hasConfig: true });
      vi.mocked(loadAIConfig).mockReturnValue(null);
      useLibrarianStore.getState().refreshConfig();
      expect(useLibrarianStore.getState().hasConfig).toBe(false);
    });
  });
});
