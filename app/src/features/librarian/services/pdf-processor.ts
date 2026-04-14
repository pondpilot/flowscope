import type { TextItem } from 'pdfjs-dist/types/src/display/api';

import { PDF_CHUNK_OVERLAP, PDF_CHUNK_SIZE } from '../constants';
import type { PdfChunk } from '../types';

export interface PageText {
  pageNumber: number;
  text: string;
}

let pdfjsConfigured = false;

/**
 * Get the PDF.js worker URL using import.meta.url for Vite compatibility.
 */
function getPdfWorkerUrl(): string {
  return new URL('pdfjs-dist/build/pdf.worker.mjs', import.meta.url).toString();
}

/**
 * Extract text content from each page of a PDF file.
 */
export async function extractTextFromPdf(file: File): Promise<PageText[]> {
  const pdfjs = await import('pdfjs-dist');

  if (!pdfjsConfigured) {
    pdfjs.GlobalWorkerOptions.workerSrc = getPdfWorkerUrl();
    pdfjsConfigured = true;
  }

  const arrayBuffer = await file.arrayBuffer();
  const pdf = await pdfjs.getDocument({ data: arrayBuffer }).promise;
  const pages: PageText[] = [];

  try {
    for (let i = 1; i <= pdf.numPages; i += 1) {
      const page = await pdf.getPage(i);
      const content = await page.getTextContent();
      const text = content.items
        .filter((item): item is TextItem => 'str' in item)
        .map((item) => item.str)
        .join(' ');
      pages.push({ pageNumber: i, text });
    }
  } finally {
    pdf.destroy();
  }

  return pages;
}

/**
 * Split page texts into overlapping chunks with page boundary tracking.
 */
export function splitIntoChunks(
  pages: PageText[],
  chunkSize: number = PDF_CHUNK_SIZE,
  overlap: number = PDF_CHUNK_OVERLAP
): PageText[] {
  const chunks: PageText[] = [];

  for (const page of pages) {
    const { text, pageNumber } = page;
    if (text.length === 0) continue;

    if (text.length <= chunkSize) {
      chunks.push({ text, pageNumber });
      continue;
    }

    const step = chunkSize - Math.min(overlap, chunkSize - 1);
    let start = 0;
    while (start < text.length) {
      const end = Math.min(start + chunkSize, text.length);
      chunks.push({ text: text.slice(start, end), pageNumber });
      if (end === text.length) break;
      start += step;
    }
  }

  return chunks;
}

/**
 * Full PDF processing pipeline: extract text, chunk, and embed.
 */
export async function processPdf(
  file: File,
  fileId: string,
  embedFn: (texts: string[]) => Promise<number[][]>
): Promise<PdfChunk[]> {
  const pages = await extractTextFromPdf(file);
  const chunks = splitIntoChunks(pages);

  if (chunks.length === 0) return [];

  const texts = chunks.map((c) => c.text);
  const embeddings = await embedFn(texts);

  if (embeddings.length !== chunks.length) {
    throw new Error(
      `Embedding count mismatch: expected ${chunks.length}, got ${embeddings.length}`
    );
  }

  return chunks.map((chunk, i) => ({
    id: `${fileId}-chunk-${i}`,
    fileId,
    fileName: file.name,
    text: chunk.text,
    pageNumber: chunk.pageNumber,
    embedding: embeddings[i],
  }));
}
