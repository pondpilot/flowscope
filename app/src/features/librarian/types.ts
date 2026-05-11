export type ChatRole = 'user' | 'assistant';

export interface ChatMessage {
  id: string;
  role: ChatRole;
  content: string;
  timestamp: number;
}

export interface PdfChunk {
  id: string;
  fileId: string;
  fileName: string;
  text: string;
  pageNumber: number;
  embedding: number[];
}

export type PdfFileStatus = 'processing' | 'ready' | 'error';

export interface PdfFile {
  id: string;
  name: string;
  size: number;
  status: PdfFileStatus;
  error?: string;
  uploadedAt: number;
}

export interface LibrarianContext {
  lineage: string;
  pdfCitations: string;
  chatHistory: string;
  sqlSnippet: string;
}

export interface PromptStats {
  characters: number;
  bytes: number;
}
