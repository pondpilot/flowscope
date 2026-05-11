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
  getPromptStats: vi.fn(),
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
  useProject: () => ({ currentProject: mockCurrentProject, activeProjectId: 'proj-1' }),
}));

// Import mocked modules after vi.mock
import { loadAIConfig, sendChatMessage } from '../services/ai-service';
import { formatLineage } from '../services/lineage-formatter';
import { buildContext, buildPrompt, getPromptStats } from '../services/context-builder';
import { embedTexts } from '../services/embedding-service';
import { searchChunks } from '../services/vector-search';
import { useLibrarianChat } from '../hooks/use-librarian-chat';

// Typed mocks
const mockedLoadAIConfig = vi.mocked(loadAIConfig);
const mockedSendChatMessage = vi.mocked(sendChatMessage);
const mockedFormatLineage = vi.mocked(formatLineage);
const mockedBuildContext = vi.mocked(buildContext);
const mockedBuildPrompt = vi.mocked(buildPrompt);
const mockedGetPromptStats = vi.mocked(getPromptStats);
const mockedEmbedTexts = vi.mocked(embedTexts);
const mockedSearchChunks = vi.mocked(searchChunks);

// ---------- Setup ----------

beforeEach(() => {
  useLibrarianStore.setState({
    byProject: { 'proj-1': { messages: [], pdfFiles: [], pdfChunks: [] } },
    activeProjectId: 'proj-1',
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
  mockedGetPromptStats.mockReturnValue({ characters: 13, bytes: 13 });
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

  it('passes configured prompt override to the prompt builder', async () => {
    mockedLoadAIConfig.mockReturnValue({
      provider: 'openai',
      apiKey: 'sk-test',
      model: 'gpt-4o',
      systemPrompt: 'Custom prompt',
    });
    const { result } = renderHook(() => useLibrarianChat());

    await act(async () => {
      await result.current.sendMessage('my question');
    });

    expect(mockedBuildPrompt).toHaveBeenCalledWith(expect.any(Object), {
      systemPrompt: 'Custom prompt',
    });
  });

  it('stores the final prompt size for the originating project', async () => {
    const { result } = renderHook(() => useLibrarianChat());

    await act(async () => {
      await result.current.sendMessage('my question');
    });

    expect(mockedGetPromptStats).toHaveBeenCalledWith('system prompt');
    expect(useLibrarianStore.getState().byProject['proj-1'].lastPromptStats).toEqual({
      characters: 13,
      bytes: 13,
    });
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
      useLibrarianStore.setState({
        byProject: { 'proj-1': { messages: [], pdfFiles: [], pdfChunks } },
      });
      mockedSearchChunks.mockReturnValue(pdfChunks);

      const { result } = renderHook(() => useLibrarianChat());

      await act(async () => {
        await result.current.sendMessage('question');
      });

      expect(mockedEmbedTexts).toHaveBeenCalledWith(['question'], 'query');
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

    it('reports an error if PDF embedding fails', async () => {
      useLibrarianStore.setState({
        byProject: { 'proj-1': { messages: [], pdfFiles: [], pdfChunks } },
      });
      mockedEmbedTexts.mockRejectedValue(new Error('embedding error'));

      const { result } = renderHook(() => useLibrarianChat());

      await act(async () => {
        await result.current.sendMessage('question');
      });

      expect(mockedSendChatMessage).not.toHaveBeenCalled();
      expect(mockedBuildContext).not.toHaveBeenCalled();
      expect(useLibrarianStore.getState().messages[1].content).toContain(
        'Failed to search uploaded PDFs: embedding error'
      );
    });

    it('does not search project A PDF chunks while project B is active', async () => {
      // Seed project A with chunks; project B has none. Active project is A.
      useLibrarianStore.setState({
        activeProjectId: 'proj-a',
        byProject: {
          'proj-a': { messages: [], pdfFiles: [], pdfChunks },
          'proj-b': { messages: [], pdfFiles: [], pdfChunks: [] },
        },
      });

      // Switch active project to B before sending.
      useLibrarianStore.setState({ activeProjectId: 'proj-b' });

      const { result } = renderHook(() => useLibrarianChat());

      await act(async () => {
        await result.current.sendMessage('question');
      });

      expect(mockedEmbedTexts).not.toHaveBeenCalled();
      expect(mockedSearchChunks).not.toHaveBeenCalled();
      expect(mockedBuildContext).toHaveBeenCalledWith(
        expect.objectContaining({ pdfCitations: '' })
      );
    });
  });

  describe('mid-flight project switch', () => {
    it('routes the assistant response to the originating project, not the live active one', async () => {
      // Hold the network call open until we explicitly resolve it.
      let resolveSend: (value: string) => void = () => {};
      mockedSendChatMessage.mockImplementation(
        () =>
          new Promise<string>((resolve) => {
            resolveSend = resolve;
          })
      );

      useLibrarianStore.setState({
        activeProjectId: 'proj-1',
        byProject: { 'proj-1': { messages: [], pdfFiles: [], pdfChunks: [] } },
      });

      const { result } = renderHook(() => useLibrarianChat());

      let sendPromise!: Promise<void>;
      act(() => {
        sendPromise = result.current.sendMessage('question for proj-1');
      });

      // User switches to a different project before the response arrives.
      // Seed the destination bucket so it's a realistic switch.
      act(() => {
        useLibrarianStore.setState({
          activeProjectId: 'proj-2',
          byProject: {
            ...useLibrarianStore.getState().byProject,
            'proj-2': { messages: [], pdfFiles: [], pdfChunks: [] },
          },
        });
      });

      await act(async () => {
        resolveSend('answer for proj-1');
        await sendPromise;
      });

      const state = useLibrarianStore.getState();
      const proj1Messages = state.byProject['proj-1'].messages.map((m) => m.content);
      const proj2Messages = state.byProject['proj-2']?.messages.map((m) => m.content) ?? [];
      expect(proj1Messages).toEqual(['question for proj-1', 'answer for proj-1']);
      expect(proj2Messages).toEqual([]);
    });

    it('routes errors to the originating project too', async () => {
      let rejectSend: (reason: Error) => void = () => {};
      mockedSendChatMessage.mockImplementation(
        () =>
          new Promise<string>((_resolve, reject) => {
            rejectSend = reject;
          })
      );

      useLibrarianStore.setState({
        activeProjectId: 'proj-1',
        byProject: { 'proj-1': { messages: [], pdfFiles: [], pdfChunks: [] } },
      });

      const { result } = renderHook(() => useLibrarianChat());

      let sendPromise!: Promise<void>;
      act(() => {
        sendPromise = result.current.sendMessage('question');
      });

      act(() => {
        useLibrarianStore.setState({
          activeProjectId: 'proj-2',
          byProject: {
            ...useLibrarianStore.getState().byProject,
            'proj-2': { messages: [], pdfFiles: [], pdfChunks: [] },
          },
        });
      });

      await act(async () => {
        rejectSend(new Error('network down'));
        await sendPromise;
      });

      const state = useLibrarianStore.getState();
      const proj1Messages = state.byProject['proj-1'].messages.map((m) => m.content);
      const proj2Messages = state.byProject['proj-2']?.messages.map((m) => m.content) ?? [];
      expect(proj1Messages[0]).toBe('question');
      expect(proj1Messages[1]).toContain('network down');
      expect(proj2Messages).toEqual([]);
    });
  });

  describe('no active project', () => {
    it('bails silently when activeProjectId is null', async () => {
      useLibrarianStore.setState({ activeProjectId: null, byProject: {} });

      const { result } = renderHook(() => useLibrarianChat());

      await act(async () => {
        await result.current.sendMessage('question');
      });

      // No bucket exists to write to — the chat input UI shows the
      // "Open or create a project" hint via its `noActiveProject` prop,
      // so the hook just bails without producing a message.
      expect(mockedSendChatMessage).not.toHaveBeenCalled();
      expect(mockedBuildContext).not.toHaveBeenCalled();
      expect(useLibrarianStore.getState().byProject).toEqual({});
    });
  });
});
