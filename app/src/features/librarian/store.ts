import { create } from 'zustand';

import { loadAIConfig } from './services/ai-service';
import type { ChatMessage, ChatRole, PdfChunk, PdfFile, PdfFileStatus } from './types';

// ============================================================================
// Types
// ============================================================================

export interface ProjectLibrarianState {
  messages: ChatMessage[];
  pdfFiles: PdfFile[];
  pdfChunks: PdfChunk[];
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

  setActiveProjectId: (id: string | null) => void;
  addMessage: (role: ChatRole, content: string) => void;
  clearMessages: () => void;
  setLoading: (loading: boolean) => void;
  addPdfFile: (file: PdfFile) => void;
  addPdfChunks: (chunks: PdfChunk[]) => void;
  removePdf: (fileId: string) => void;
  setPdfStatus: (fileId: string, status: PdfFileStatus, error?: string) => void;
  hasPdfFile: (fileName: string) => boolean;
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

  setActiveProjectId: (id) => {
    const state = get();
    const bucket = id ? state.byProject[id] : null;
    set({
      activeProjectId: id,
      messages: bucket?.messages ?? EMPTY_MESSAGES,
      pdfFiles: bucket?.pdfFiles ?? EMPTY_PDF_FILES,
      pdfChunks: bucket?.pdfChunks ?? EMPTY_PDF_CHUNKS,
    });
  },

  addMessage: (role, content) => {
    const { activeProjectId } = get();
    if (!activeProjectId) return;
    const message: ChatMessage = {
      id: crypto.randomUUID(),
      role,
      content,
      timestamp: Date.now(),
    };
    set((state) => {
      const prev = getBucket(state.byProject, activeProjectId);
      const next: ProjectLibrarianState = { ...prev, messages: [...prev.messages, message] };
      return {
        byProject: { ...state.byProject, [activeProjectId]: next },
        messages: next.messages,
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
    set((state) => {
      const prev = getBucket(state.byProject, activeProjectId);
      const next: ProjectLibrarianState = { ...prev, pdfFiles: [...prev.pdfFiles, file] };
      return {
        byProject: { ...state.byProject, [activeProjectId]: next },
        pdfFiles: next.pdfFiles,
      };
    });
  },

  addPdfChunks: (chunks) => {
    const { activeProjectId } = get();
    if (!activeProjectId) return;
    set((state) => {
      const prev = getBucket(state.byProject, activeProjectId);
      const next: ProjectLibrarianState = {
        ...prev,
        pdfChunks: [...prev.pdfChunks, ...chunks],
      };
      return {
        byProject: { ...state.byProject, [activeProjectId]: next },
        pdfChunks: next.pdfChunks,
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
    set((state) => {
      const prev = getBucket(state.byProject, activeProjectId);
      const next: ProjectLibrarianState = {
        ...prev,
        pdfFiles: prev.pdfFiles.map((f) => (f.id === fileId ? { ...f, status, error } : f)),
      };
      return {
        byProject: { ...state.byProject, [activeProjectId]: next },
        pdfFiles: next.pdfFiles,
      };
    });
  },

  hasPdfFile: (fileName) => {
    const { activeProjectId, byProject } = get();
    if (!activeProjectId) return false;
    const bucket = byProject[activeProjectId];
    return bucket ? bucket.pdfFiles.some((f) => f.name === fileName) : false;
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
