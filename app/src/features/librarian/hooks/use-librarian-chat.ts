import { useCallback, useRef } from 'react';
import { useLineageState } from '@pondpilot/flowscope-react';

import { useProject } from '@/lib/project-store';

import { CHAT_HISTORY_LIMIT, VECTOR_SEARCH_TOP_K } from '../constants';
import { loadAIConfig, sendChatMessage } from '../services/ai-service';
import { buildContext, buildPrompt } from '../services/context-builder';
import { embedTexts } from '../services/embedding-service';
import { formatLineage } from '../services/lineage-formatter';
import { searchChunks } from '../services/vector-search';
import { useLibrarianStore } from '../store';

export function useLibrarianChat() {
  const abortRef = useRef<AbortController | null>(null);

  const addMessage = useLibrarianStore((s) => s.addMessage);
  const setLoading = useLibrarianStore((s) => s.setLoading);
  const { result } = useLineageState();
  const { currentProject } = useProject();

  const sendMessage = useCallback(
    async (userMessage: string) => {
      const config = loadAIConfig();
      if (!config) {
        addMessage('assistant', 'Please configure your AI settings first.');
        return;
      }

      const { activeProjectId } = useLibrarianStore.getState();
      if (!activeProjectId) {
        addMessage('assistant', 'Open or create a project to use Librarian.');
        return;
      }

      // Add user message to store
      addMessage('user', userMessage);
      setLoading(true);

      // Create abort controller for this request
      const controller = new AbortController();
      abortRef.current = controller;

      try {
        // Format lineage from current analysis result
        const lineage = formatLineage(result ?? null);

        // Get SQL from current project's active file
        let sqlSnippet = '';
        if (currentProject?.activeFileId) {
          const activeFile = currentProject.files.find((f) => f.id === currentProject.activeFileId);
          if (activeFile) {
            sqlSnippet = activeFile.content;
          }
        }

        // Vector search PDFs if chunks exist — read from active project bucket
        const pdfChunks =
          useLibrarianStore.getState().byProject[activeProjectId]?.pdfChunks ?? [];
        let pdfCitations = '';
        if (pdfChunks.length > 0) {
          try {
            const [queryEmbedding] = await embedTexts([userMessage], 'query');
            const relevantChunks = searchChunks(queryEmbedding, pdfChunks, VECTOR_SEARCH_TOP_K);
            if (relevantChunks.length > 0) {
              pdfCitations = relevantChunks
                .map((c) => `[${c.fileName} p.${c.pageNumber}]: ${c.text}`)
                .join('\n\n');
            }
          } catch {
            // Embedding failed - continue without PDF context
          }
        }

        // Build context and prompt — read messages from the active project bucket.
        // Exclude the last message (the user message just added) since it will
        // also be sent as the userMessage parameter to the LLM. Send only the
        // last CHAT_HISTORY_LIMIT messages as context to the AI.
        const allMessages =
          useLibrarianStore.getState().byProject[activeProjectId]?.messages ?? [];
        const recentHistory = allMessages.slice(0, -1).slice(-CHAT_HISTORY_LIMIT);
        const context = buildContext({
          lineage,
          pdfCitations,
          chatHistory: recentHistory,
          sqlSnippet,
        });
        const prompt = buildPrompt(context);

        // Send to AI
        const response = await sendChatMessage(config, prompt, userMessage, controller.signal);

        addMessage('assistant', response);
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') {
          addMessage('assistant', 'Request was cancelled.');
        } else {
          const message = err instanceof Error ? err.message : 'An unexpected error occurred.';
          addMessage('assistant', `Error: ${message}`);
        }
      } finally {
        setLoading(false);
        abortRef.current = null;
      }
    },
    [addMessage, setLoading, result, currentProject]
  );

  const cancel = useCallback(() => {
    if (abortRef.current) {
      abortRef.current.abort();
      abortRef.current = null;
    }
  }, []);

  return { sendMessage, cancel };
}
