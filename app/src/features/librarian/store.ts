import { create } from 'zustand';

import { loadAIConfig } from './services/ai-service';
import type { ChatMessage, ChatRole, PdfChunk, PdfFile, PdfFileStatus, PromptStats } from './types';

// ============================================================================
// Types
// ============================================================================

export interface ProjectLibrarianState {
  messages: ChatMessage[];
  pdfFiles: PdfFile[];
  pdfChunks: PdfChunk[];
  lastPromptStats?: PromptStats;
}

interface LibrarianState {
  byProject: Record<string, ProjectLibrarianState>;
  activeProjectId: string | null;
  isLoading: boolean;
  hasConfig: boolean;

  // Flat-shape mirror of the active bucket. Kept in sync by every mutator so
  // existing consumers (librarian-panel, pdf-upload, use-librarian-chat) keep
  // typechecking until Task 3 migrates them to the selector hooks below.
  messages: ChatMessage[];
  pdfFiles: PdfFile[];
  pdfChunks: PdfChunk[];
  lastPromptStats: PromptStats | null;

  setActiveProjectId: (id: string | null) => void;
  addMessage: (role: ChatRole, content: string) => void;
  addMessageToProject: (projectId: string, role: ChatRole, content: string) => void;
  clearMessages: () => void;
  setLoading: (loading: boolean) => void;
  addPdfFile: (file: PdfFile) => void;
  addPdfFileToProject: (projectId: string, file: PdfFile) => void;
  addPdfChunks: (chunks: PdfChunk[]) => void;
  addPdfChunksToProject: (projectId: string, chunks: PdfChunk[]) => void;
  removePdf: (fileId: string) => void;
  setPdfStatus: (fileId: string, status: PdfFileStatus, error?: string) => void;
  setPdfStatusForProject: (
    projectId: string,
    fileId: string,
    status: PdfFileStatus,
    error?: string
  ) => void;
  hasPdfFile: (fileName: string) => boolean;
  setPromptStats: (stats: PromptStats) => void;
  setPromptStatsForProject: (projectId: string, stats: PromptStats) => void;
  pruneProjectBuckets: (validIds: ReadonlySet<string>) => void;
  refreshConfig: () => void;
}

// ============================================================================
// Helpers
// ============================================================================

const EMPTY_MESSAGES: ChatMessage[] = [];
const EMPTY_PDF_FILES: PdfFile[] = [];
const EMPTY_PDF_CHUNKS: PdfChunk[] = [];

const emptyBucket = (): ProjectLibrarianState => ({
  messages: [],
  pdfFiles: [],
  pdfChunks: [],
});

const getBucket = (
  byProject: Record<string, ProjectLibrarianState>,
  id: string
): ProjectLibrarianState => byProject[id] ?? emptyBucket();

// ============================================================================
// Store
// ============================================================================

export const useLibrarianStore = create<LibrarianState>()((set, get) => ({
  byProject: {},
  activeProjectId: null,
  isLoading: false,
  hasConfig: loadAIConfig() !== null,

  messages: EMPTY_MESSAGES,
  pdfFiles: EMPTY_PDF_FILES,
  pdfChunks: EMPTY_PDF_CHUNKS,
  lastPromptStats: null,

  setActiveProjectId: (id) => {
    const state = get();
    const bucket = id ? state.byProject[id] : null;
    set({
      activeProjectId: id,
      messages: bucket?.messages ?? EMPTY_MESSAGES,
      pdfFiles: bucket?.pdfFiles ?? EMPTY_PDF_FILES,
      pdfChunks: bucket?.pdfChunks ?? EMPTY_PDF_CHUNKS,
      lastPromptStats: bucket?.lastPromptStats ?? null,
    });
  },

  addMessage: (role, content) => {
    const { activeProjectId } = get();
    if (!activeProjectId) return;
    get().addMessageToProject(activeProjectId, role, content);
  },

  // Writes a message to an explicit project bucket regardless of which
  // project is currently active. Used by `useLibrarianChat` so that an
  // assistant response always lands in the project that originated the
  // request, even if the user switched projects mid-flight.
  addMessageToProject: (projectId, role, content) => {
    const message: ChatMessage = {
      id: crypto.randomUUID(),
      role,
      content,
      timestamp: Date.now(),
    };
    set((state) => {
      // If the bucket is missing and the project is no longer active, it
      // was pruned by a project deletion — drop the write rather than
      // resurrect a zombie bucket for a deleted project.
      if (!state.byProject[projectId] && state.activeProjectId !== projectId) {
        return state;
      }
      const prev = getBucket(state.byProject, projectId);
      const next: ProjectLibrarianState = { ...prev, messages: [...prev.messages, message] };
      const isActive = state.activeProjectId === projectId;
      return {
        byProject: { ...state.byProject, [projectId]: next },
        ...(isActive ? { messages: next.messages } : {}),
      };
    });
  },

  clearMessages: () => {
    const { activeProjectId } = get();
    if (!activeProjectId) return;
    set((state) => {
      const prev = getBucket(state.byProject, activeProjectId);
      const next: ProjectLibrarianState = { ...prev, messages: [] };
      return {
        byProject: { ...state.byProject, [activeProjectId]: next },
        messages: next.messages,
      };
    });
  },

  setLoading: (loading) => set({ isLoading: loading }),

  addPdfFile: (file) => {
    const { activeProjectId } = get();
    if (!activeProjectId) return;
    get().addPdfFileToProject(activeProjectId, file);
  },

  // Like `addMessageToProject`: writes to an explicit project bucket so a
  // mid-flight project switch during PDF processing still routes the file
  // (and its chunks/status updates) back to the originating project.
  addPdfFileToProject: (projectId, file) => {
    set((state) => {
      if (!state.byProject[projectId] && state.activeProjectId !== projectId) {
        return state;
      }
      const prev = getBucket(state.byProject, projectId);
      const next: ProjectLibrarianState = { ...prev, pdfFiles: [...prev.pdfFiles, file] };
      const isActive = state.activeProjectId === projectId;
      return {
        byProject: { ...state.byProject, [projectId]: next },
        ...(isActive ? { pdfFiles: next.pdfFiles } : {}),
      };
    });
  },

  addPdfChunks: (chunks) => {
    const { activeProjectId } = get();
    if (!activeProjectId) return;
    get().addPdfChunksToProject(activeProjectId, chunks);
  },

  addPdfChunksToProject: (projectId, chunks) => {
    set((state) => {
      if (!state.byProject[projectId] && state.activeProjectId !== projectId) {
        return state;
      }
      const prev = getBucket(state.byProject, projectId);
      const fileIds = new Set(prev.pdfFiles.map((file) => file.id));
      const chunksForExistingFiles = chunks.filter((chunk) => fileIds.has(chunk.fileId));
      const dropped = chunks.length - chunksForExistingFiles.length;
      if (dropped > 0) {
        // Chunk owner was removed mid-embed (e.g. user deleted the PDF before
        // embeddings finished). Surface a debug trace so this doesn't look
        // like "embeddings sometimes vanish" if it's ever investigated.
        console.debug(
          `[librarian] dropped ${dropped} chunk(s) for project ${projectId}: source file(s) no longer present`
        );
      }
      if (chunksForExistingFiles.length === 0) {
        return state;
      }
      const next: ProjectLibrarianState = {
        ...prev,
        pdfChunks: [...prev.pdfChunks, ...chunksForExistingFiles],
      };
      const isActive = state.activeProjectId === projectId;
      return {
        byProject: { ...state.byProject, [projectId]: next },
        ...(isActive ? { pdfChunks: next.pdfChunks } : {}),
      };
    });
  },

  removePdf: (fileId) => {
    const { activeProjectId } = get();
    if (!activeProjectId) return;
    set((state) => {
      const prev = getBucket(state.byProject, activeProjectId);
      const next: ProjectLibrarianState = {
        ...prev,
        pdfFiles: prev.pdfFiles.filter((f) => f.id !== fileId),
        pdfChunks: prev.pdfChunks.filter((c) => c.fileId !== fileId),
      };
      return {
        byProject: { ...state.byProject, [activeProjectId]: next },
        pdfFiles: next.pdfFiles,
        pdfChunks: next.pdfChunks,
      };
    });
  },

  setPdfStatus: (fileId, status, error) => {
    const { activeProjectId } = get();
    if (!activeProjectId) return;
    get().setPdfStatusForProject(activeProjectId, fileId, status, error);
  },

  setPdfStatusForProject: (projectId, fileId, status, error) => {
    set((state) => {
      if (!state.byProject[projectId] && state.activeProjectId !== projectId) {
        return state;
      }
      const prev = getBucket(state.byProject, projectId);
      const next: ProjectLibrarianState = {
        ...prev,
        pdfFiles: prev.pdfFiles.map((f) => (f.id === fileId ? { ...f, status, error } : f)),
      };
      const isActive = state.activeProjectId === projectId;
      return {
        byProject: { ...state.byProject, [projectId]: next },
        ...(isActive ? { pdfFiles: next.pdfFiles } : {}),
      };
    });
  },

  hasPdfFile: (fileName) => {
    const { activeProjectId, byProject } = get();
    if (!activeProjectId) return false;
    const bucket = byProject[activeProjectId];
    if (!bucket) return false;
    const target = fileName.toLowerCase();
    return bucket.pdfFiles.some((f) => f.name.toLowerCase() === target);
  },

  setPromptStats: (stats) => {
    const { activeProjectId } = get();
    if (!activeProjectId) return;
    get().setPromptStatsForProject(activeProjectId, stats);
  },

  setPromptStatsForProject: (projectId, stats) => {
    set((state) => {
      if (!state.byProject[projectId] && state.activeProjectId !== projectId) {
        return state;
      }
      const prev = getBucket(state.byProject, projectId);
      const next: ProjectLibrarianState = { ...prev, lastPromptStats: stats };
      const isActive = state.activeProjectId === projectId;
      return {
        byProject: { ...state.byProject, [projectId]: next },
        ...(isActive ? { lastPromptStats: stats } : {}),
      };
    });
  },

  // Drop buckets whose project id is no longer present. Called by
  // `useSyncActiveProject` when the project list changes — without this,
  // chat history and embedded PDF chunks for deleted projects accumulate
  // in RAM for the lifetime of the tab.
  pruneProjectBuckets: (validIds) => {
    set((state) => {
      const ids = Object.keys(state.byProject);
      const stale = ids.filter((id) => !validIds.has(id));
      if (stale.length === 0) return state;
      const next: Record<string, ProjectLibrarianState> = {};
      for (const id of ids) {
        if (validIds.has(id)) next[id] = state.byProject[id];
      }
      return { byProject: next };
    });
  },

  refreshConfig: () => {
    set({ hasConfig: loadAIConfig() !== null });
  },
}));

// ============================================================================
// Selectors
// ============================================================================

export const useLibrarianMessages = (): ChatMessage[] =>
  useLibrarianStore((s) => {
    const id = s.activeProjectId;
    if (!id) return EMPTY_MESSAGES;
    return s.byProject[id]?.messages ?? EMPTY_MESSAGES;
  });

export const useLibrarianPdfFiles = (): PdfFile[] =>
  useLibrarianStore((s) => {
    const id = s.activeProjectId;
    if (!id) return EMPTY_PDF_FILES;
    return s.byProject[id]?.pdfFiles ?? EMPTY_PDF_FILES;
  });

export const useLibrarianPdfChunks = (): PdfChunk[] =>
  useLibrarianStore((s) => {
    const id = s.activeProjectId;
    if (!id) return EMPTY_PDF_CHUNKS;
    return s.byProject[id]?.pdfChunks ?? EMPTY_PDF_CHUNKS;
  });

export const useLibrarianPromptStats = (): PromptStats | null =>
  useLibrarianStore((s) => {
    const id = s.activeProjectId;
    if (!id) return null;
    return s.byProject[id]?.lastPromptStats ?? null;
  });
