import { create } from 'zustand';

import { loadAIConfig } from './services/ai-service';
import type { ChatMessage, ChatRole, PdfChunk, PdfFile, PdfFileStatus } from './types';

// ============================================================================
// Types
// ============================================================================

interface LibrarianState {
  messages: ChatMessage[];
  isLoading: boolean;
  pdfFiles: PdfFile[];
  pdfChunks: PdfChunk[];
  hasConfig: boolean;

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
// Store
// ============================================================================

export const useLibrarianStore = create<LibrarianState>()((set, get) => ({
  messages: [],
  isLoading: false,
  pdfFiles: [],
  pdfChunks: [],
  hasConfig: loadAIConfig() !== null,

  addMessage: (role, content) => {
    const message: ChatMessage = {
      id: crypto.randomUUID(),
      role,
      content,
      timestamp: Date.now(),
    };
    set((state) => ({ messages: [...state.messages, message] }));
  },

  clearMessages: () => set({ messages: [] }),

  setLoading: (loading) => set({ isLoading: loading }),

  addPdfFile: (file) => set((state) => ({ pdfFiles: [...state.pdfFiles, file] })),

  addPdfChunks: (chunks) => set((state) => ({ pdfChunks: [...state.pdfChunks, ...chunks] })),

  removePdf: (fileId) =>
    set((state) => ({
      pdfFiles: state.pdfFiles.filter((f) => f.id !== fileId),
      pdfChunks: state.pdfChunks.filter((c) => c.fileId !== fileId),
    })),

  setPdfStatus: (fileId, status, error) =>
    set((state) => ({
      pdfFiles: state.pdfFiles.map((f) => (f.id === fileId ? { ...f, status, error } : f)),
    })),

  hasPdfFile: (fileName) => {
    return get().pdfFiles.some((f) => f.name === fileName);
  },

  refreshConfig: () => {
    set({ hasConfig: loadAIConfig() !== null });
  },
}));
