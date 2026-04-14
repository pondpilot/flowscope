/* eslint-disable @typescript-eslint/no-explicit-any */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';

import { useLibrarianStore } from '../store';
import type { PdfChunk } from '../types';

// ---------- Mocks ----------

vi.mock('../services/ai-service', () => ({
  loadAIConfig: vi.fn(),
  sendChatMessage: vi.fn(),
}));

vi.mock('../services/lineage-formatter', () => ({
  formatLineage: vi.fn(),
}));

vi.mock('../services/context-builder', () => ({
  buildContext: vi.fn(),
  buildPrompt: vi.fn(),
}));

vi.mock('../services/embedding-service', () => ({
  embedTexts: vi.fn(),
}));

vi.mock('../services/vector-search', () => ({
  searchChunks: vi.fn(),
}));

// Mock lineage state
const mockResult = { globalLineage: { nodes: [], edges: [] } };
vi.mock('@pondpilot/flowscope-react', () => ({
  useLineageState: () => ({ result: mockResult }),
}));

// Mock project store
const mockCurrentProject = {
  id: 'proj-1',
  name: 'Test Project',
  files: [
    { id: 'file-1', name: 'query.sql', path: 'query.sql', content: 'SELECT 1', language: 'sql' },
  ],
  activeFileId: 'file-1',
  dialect: 'generic',
  runMode: 'all',
  selectedFileIds: [],
  schemaSQL: '',
  templateMode: 'raw',
};
vi.mock('@/lib/project-store', () => ({
  useProject: () => ({ currentProject: mockCurrentProject }),
}));

// Import mocked modules after vi.mock
import { loadAIConfig, sendChatMessage } from '../services/ai-service';
import { formatLineage } from '../services/lineage-formatter';
import { buildContext, buildPrompt } from '../services/context-builder';
import { embedTexts } from '../services/embedding-service';
import { searchChunks } from '../services/vector-search';
import { useLibrarianChat } from '../hooks/use-librarian-chat';

// Typed mocks
const mockedLoadAIConfig = vi.mocked(loadAIConfig);
const mockedSendChatMessage = vi.mocked(sendChatMessage);
const mockedFormatLineage = vi.mocked(formatLineage);
const mockedBuildContext = vi.mocked(buildContext);
const mockedBuildPrompt = vi.mocked(buildPrompt);
const mockedEmbedTexts = vi.mocked(embedTexts);
const mockedSearchChunks = vi.mocked(searchChunks);

// ---------- Setup ----------

beforeEach(() => {
  useLibrarianStore.setState({
    messages: [],
    isLoading: false,
    pdfFiles: [],
    pdfChunks: [],
  });
  vi.clearAllMocks();

  // Restore default implementations
  mockedLoadAIConfig.mockReturnValue({
    provider: 'openai',
    apiKey: 'sk-test',
    model: 'gpt-4o',
  });
  mockedSendChatMessage.mockResolvedValue('AI response');
  mockedFormatLineage.mockReturnValue('formatted lineage');
  mockedBuildContext.mockReturnValue({
    lineage: 'lineage',
    pdfCitations: '',
    chatHistory: '',
    sqlSnippet: '',
  });
  mockedBuildPrompt.mockReturnValue('system prompt');
  mockedEmbedTexts.mockResolvedValue([[0.1, 0.2, 0.3]]);
  mockedSearchChunks.mockReturnValue([]);
});

afterEach(() => {
  vi.restoreAllMocks();
});

// ---------- Tests ----------

describe('useLibrarianChat', () => {
  it('adds user and assistant messages on successful send', async () => {
    const { result } = renderHook(() => useLibrarianChat());

    await act(async () => {
      await result.current.sendMessage('Hello');
    });

    const state = useLibrarianStore.getState();
    expect(state.messages).toHaveLength(2);
    expect(state.messages[0].role).toBe('user');
    expect(state.messages[0].content).toBe('Hello');
    expect(state.messages[1].role).toBe('assistant');
    expect(state.messages[1].content).toBe('AI response');
  });

  it('shows config message when AI is not configured', async () => {
    mockedLoadAIConfig.mockReturnValue(null);
    const { result } = renderHook(() => useLibrarianChat());

    await act(async () => {
      await result.current.sendMessage('Hello');
    });

    const state = useLibrarianStore.getState();
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0].role).toBe('assistant');
    expect(state.messages[0].content).toContain('configure');
  });

  it('calls formatLineage with the analysis result', async () => {
    const { result } = renderHook(() => useLibrarianChat());

    await act(async () => {
      await result.current.sendMessage('test');
    });

    expect(mockedFormatLineage).toHaveBeenCalledWith(mockResult);
  });

  it('passes SQL from active file to context builder', async () => {
    const { result } = renderHook(() => useLibrarianChat());

    await act(async () => {
      await result.current.sendMessage('test');
    });

    expect(mockedBuildContext).toHaveBeenCalledWith(
      expect.objectContaining({ sqlSnippet: 'SELECT 1' })
    );
  });

  it('sends prompt and user message to AI service', async () => {
    const { result } = renderHook(() => useLibrarianChat());

    await act(async () => {
      await result.current.sendMessage('my question');
    });

    expect(mockedSendChatMessage).toHaveBeenCalledWith(
      expect.objectContaining({ provider: 'openai' }),
      'system prompt',
      'my question',
      expect.any(AbortSignal)
    );
  });

  it('sets loading state during request', async () => {
    let loadingDuringRequest = false;
    mockedSendChatMessage.mockImplementation(async () => {
      loadingDuringRequest = useLibrarianStore.getState().isLoading;
      return 'response';
    });

    const { result } = renderHook(() => useLibrarianChat());

    await act(async () => {
      await result.current.sendMessage('test');
    });

    expect(loadingDuringRequest).toBe(true);
    expect(useLibrarianStore.getState().isLoading).toBe(false);
  });

  it('handles AI service errors gracefully', async () => {
    mockedSendChatMessage.mockRejectedValue(new Error('API limit reached'));
    const { result } = renderHook(() => useLibrarianChat());

    await act(async () => {
      await result.current.sendMessage('test');
    });

    const state = useLibrarianStore.getState();
    expect(state.messages).toHaveLength(2);
    expect(state.messages[1].role).toBe('assistant');
    expect(state.messages[1].content).toContain('API limit reached');
  });

  it('handles non-Error exceptions', async () => {
    mockedSendChatMessage.mockRejectedValue('string error');
    const { result } = renderHook(() => useLibrarianChat());

    await act(async () => {
      await result.current.sendMessage('test');
    });

    const state = useLibrarianStore.getState();
    expect(state.messages[1].content).toContain('unexpected error');
  });

  it('handles abort/cancellation', async () => {
    mockedSendChatMessage.mockImplementation(async () => {
      const error = new Error('Aborted');
      error.name = 'AbortError';
      throw error;
    });

    const { result } = renderHook(() => useLibrarianChat());

    await act(async () => {
      await result.current.sendMessage('test');
    });

    const state = useLibrarianStore.getState();
    expect(state.messages[1].content).toContain('cancelled');
    expect(state.isLoading).toBe(false);
  });

  it('cancel aborts the current request', async () => {
    let capturedSignal: AbortSignal | undefined;
    mockedSendChatMessage.mockImplementation(
      async (_config: any, _prompt: any, _msg: any, signal?: AbortSignal) => {
        capturedSignal = signal;
        await new Promise((resolve) => setTimeout(resolve, 100));
        if (signal?.aborted) {
          const err = new Error('Aborted');
          err.name = 'AbortError';
          throw err;
        }
        return 'response';
      }
    );

    const { result } = renderHook(() => useLibrarianChat());

    let sendPromise: Promise<void>;
    act(() => {
      sendPromise = result.current.sendMessage('test');
    });

    act(() => {
      result.current.cancel();
    });

    await act(async () => {
      await sendPromise!;
    });

    expect(capturedSignal?.aborted).toBe(true);
  });

  describe('PDF vector search', () => {
    const pdfChunks: PdfChunk[] = [
      {
        id: 'c1',
        fileId: 'f1',
        fileName: 'doc.pdf',
        text: 'relevant content',
        pageNumber: 3,
        embedding: [0.5, 0.5, 0.5],
      },
    ];

    it('searches PDF chunks when available', async () => {
      useLibrarianStore.setState({ pdfChunks });
      mockedSearchChunks.mockReturnValue(pdfChunks);

      const { result } = renderHook(() => useLibrarianChat());

      await act(async () => {
        await result.current.sendMessage('question');
      });

      expect(mockedEmbedTexts).toHaveBeenCalledWith(['question']);
      expect(mockedSearchChunks).toHaveBeenCalled();
      expect(mockedBuildContext).toHaveBeenCalledWith(
        expect.objectContaining({
          pdfCitations: expect.stringContaining('doc.pdf'),
        })
      );
    });

    it('skips vector search when no PDF chunks', async () => {
      const { result } = renderHook(() => useLibrarianChat());

      await act(async () => {
        await result.current.sendMessage('question');
      });

      expect(mockedEmbedTexts).not.toHaveBeenCalled();
      expect(mockedSearchChunks).not.toHaveBeenCalled();
    });

    it('continues without PDF context if embedding fails', async () => {
      useLibrarianStore.setState({ pdfChunks });
      mockedEmbedTexts.mockRejectedValue(new Error('embedding error'));

      const { result } = renderHook(() => useLibrarianChat());

      await act(async () => {
        await result.current.sendMessage('question');
      });

      expect(mockedSendChatMessage).toHaveBeenCalled();
      expect(mockedBuildContext).toHaveBeenCalledWith(
        expect.objectContaining({ pdfCitations: '' })
      );
    });
  });
});
