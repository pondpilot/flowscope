import { useCallback, useRef } from 'react';
import { useLineageState } from '@pondpilot/flowscope-react';

import { useProject } from '@/lib/project-store';

import { CHAT_HISTORY_LIMIT, VECTOR_SEARCH_TOP_K } from '../constants';
import { loadAIConfig, sendChatMessage } from '../services/ai-service';
import { buildContext, buildPrompt, getPromptStats } from '../services/context-builder';
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

      const { activeProjectId, addMessageToProject, setPromptStatsForProject } =
        useLibrarianStore.getState();
      if (!activeProjectId) {
        // The chat input UI shows the "Open or create a project" hint
        // (via the `noActiveProject` prop), so we just bail. We can't
        // write a chat message here because the bucket doesn't exist.
        return;
      }

      // Capture the project id for the duration of this request so a
      // mid-flight project switch routes the assistant response back to
      // the originating project's bucket, not whichever project is
      // active when the network call returns.
      const projectId = activeProjectId;

      addMessageToProject(projectId, 'user', userMessage);
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

        // Vector search PDFs if chunks exist — read from the originating
        // project's bucket (captured projectId, not the live active id).
        const pdfChunks = useLibrarianStore.getState().byProject[projectId]?.pdfChunks ?? [];
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
          } catch (err) {
            const message = err instanceof Error ? err.message : 'Unknown embedding error';
            throw new Error(`Failed to search uploaded PDFs: ${message}`);
          }
        }

        // Build context and prompt — read messages from the originating
        // project's bucket. Exclude the last message (the user message just
        // added) since it will also be sent as the userMessage parameter to
        // the LLM. Send only the last CHAT_HISTORY_LIMIT messages as context.
        const allMessages = useLibrarianStore.getState().byProject[projectId]?.messages ?? [];
        const recentHistory = allMessages.slice(0, -1).slice(-CHAT_HISTORY_LIMIT);
        const context = buildContext({
          lineage,
          pdfCitations,
          chatHistory: recentHistory,
          sqlSnippet,
        });
        const prompt = buildPrompt(context, { systemPrompt: config.systemPrompt });
        setPromptStatsForProject(projectId, getPromptStats(prompt));

        // Send to AI
        const response = await sendChatMessage(config, prompt, userMessage, controller.signal);

        addMessageToProject(projectId, 'assistant', response);
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') {
          addMessageToProject(projectId, 'assistant', 'Request was cancelled.');
        } else {
          const message = err instanceof Error ? err.message : 'An unexpected error occurred.';
          addMessageToProject(projectId, 'assistant', `Error: ${message}`);
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
