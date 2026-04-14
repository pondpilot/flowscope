import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('../services/ai-service', () => ({
  loadAIConfig: vi.fn(() => null),
}));

import { loadAIConfig } from '../services/ai-service';

import { CHAT_HISTORY_LIMIT } from '../constants';
import { useLibrarianStore } from '../store';
import type { PdfChunk, PdfFile } from '../types';

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

describe('useLibrarianStore', () => {
  beforeEach(() => {
    vi.mocked(loadAIConfig).mockReturnValue(null);
    useLibrarianStore.setState({
      messages: [],
      isLoading: false,
      pdfFiles: [],
      pdfChunks: [],
      hasConfig: false,
    });
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
  });

  describe('clearMessages', () => {
    it('removes all messages', () => {
      const store = useLibrarianStore.getState();
      store.addMessage('user', 'a');
      store.addMessage('assistant', 'b');
      store.clearMessages();
      expect(useLibrarianStore.getState().messages).toHaveLength(0);
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
  });

  describe('addPdfChunks', () => {
    it('adds chunks to the store', () => {
      useLibrarianStore
        .getState()
        .addPdfChunks([makePdfChunk({ id: 'c1' }), makePdfChunk({ id: 'c2' })]);
      expect(useLibrarianStore.getState().pdfChunks).toHaveLength(2);
    });

    it('appends to existing chunks', () => {
      const store = useLibrarianStore.getState();
      store.addPdfChunks([makePdfChunk({ id: 'c1' })]);
      store.addPdfChunks([makePdfChunk({ id: 'c2' })]);
      expect(useLibrarianStore.getState().pdfChunks).toHaveLength(2);
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
    it('returns true when file exists', () => {
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
