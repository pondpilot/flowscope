import { describe, expect, it, vi } from 'vitest';

import { splitIntoChunks, processPdf } from '../services/pdf-processor';
import type { PageText } from '../services/pdf-processor';

// Mock pdfjs-dist so extractTextFromPdf doesn't need a real PDF runtime
vi.mock('pdfjs-dist', () => ({
  GlobalWorkerOptions: { workerSrc: '' },
  getDocument: vi.fn(),
}));

function makePdfFile(name: string): File {
  const file = new File(['fake-pdf-content'], name, { type: 'application/pdf' });
  Object.defineProperty(file, 'arrayBuffer', {
    value: vi.fn().mockResolvedValue(new ArrayBuffer(8)),
  });
  return file;
}

describe('splitIntoChunks', () => {
  it('returns empty array for empty pages', () => {
    expect(splitIntoChunks([])).toEqual([]);
  });

  it('returns empty array for pages with empty text', () => {
    const pages: PageText[] = [{ pageNumber: 1, text: '' }];
    expect(splitIntoChunks(pages)).toEqual([]);
  });

  it('returns single chunk for text shorter than chunkSize', () => {
    const pages: PageText[] = [{ pageNumber: 1, text: 'Hello world' }];
    const chunks = splitIntoChunks(pages, 500, 50);
    expect(chunks).toHaveLength(1);
    expect(chunks[0]).toEqual({ pageNumber: 1, text: 'Hello world' });
  });

  it('returns single chunk for text equal to chunkSize', () => {
    const text = 'a'.repeat(100);
    const pages: PageText[] = [{ pageNumber: 1, text }];
    const chunks = splitIntoChunks(pages, 100, 10);
    expect(chunks).toHaveLength(1);
    expect(chunks[0].text).toBe(text);
  });

  it('splits long text into overlapping chunks', () => {
    const text = 'a'.repeat(250);
    const pages: PageText[] = [{ pageNumber: 1, text }];
    // chunkSize=100, overlap=20 => step=80
    // chunks: 0-100, 80-180, 160-250
    const chunks = splitIntoChunks(pages, 100, 20);
    expect(chunks).toHaveLength(3);
    expect(chunks[0].text).toBe('a'.repeat(100));
    expect(chunks[1].text).toBe('a'.repeat(100));
    expect(chunks[2].text).toBe('a'.repeat(90)); // 160-250
    // All retain the page number
    expect(chunks.every((c) => c.pageNumber === 1)).toBe(true);
  });

  it('handles multiple pages independently', () => {
    const pages: PageText[] = [
      { pageNumber: 1, text: 'Short text' },
      { pageNumber: 2, text: 'b'.repeat(200) },
    ];
    const chunks = splitIntoChunks(pages, 100, 10);
    expect(chunks[0]).toEqual({ pageNumber: 1, text: 'Short text' });
    // Page 2 should be split
    const page2Chunks = chunks.filter((c) => c.pageNumber === 2);
    expect(page2Chunks.length).toBeGreaterThan(1);
  });

  it('handles overlap larger than chunkSize gracefully', () => {
    const text = 'a'.repeat(200);
    const pages: PageText[] = [{ pageNumber: 1, text }];
    // overlap=150, chunkSize=100 => Math.min(150, 99)=99 => step=1
    const chunks = splitIntoChunks(pages, 100, 150);
    // Should still produce chunks without infinite loop
    expect(chunks.length).toBeGreaterThan(0);
    expect(chunks[0].text.length).toBe(100);
  });

  it('handles zero overlap', () => {
    const text = 'a'.repeat(200);
    const pages: PageText[] = [{ pageNumber: 1, text }];
    const chunks = splitIntoChunks(pages, 100, 0);
    expect(chunks).toHaveLength(2);
  });
});

describe('processPdf', () => {
  it('returns empty array when no text extracted', async () => {
    // We need to mock extractTextFromPdf indirectly via pdfjs-dist
    const pdfjs = await import('pdfjs-dist');
    const mockGetDocument = vi.mocked(pdfjs.getDocument);

    mockGetDocument.mockReturnValue({
      promise: Promise.resolve({
        numPages: 1,
        getPage: vi.fn().mockResolvedValue({
          getTextContent: vi.fn().mockResolvedValue({ items: [] }),
        }),
        destroy: vi.fn(),
      }),
    } as unknown as ReturnType<typeof pdfjs.getDocument>);

    const file = makePdfFile('test.pdf');
    const embedFn = vi.fn().mockResolvedValue([]);

    const result = await processPdf(file, 'file-1', embedFn);
    expect(result).toEqual([]);
    expect(embedFn).not.toHaveBeenCalled();
  });

  it('processes a PDF with text and returns embedded chunks', async () => {
    const pdfjs = await import('pdfjs-dist');
    const mockGetDocument = vi.mocked(pdfjs.getDocument);

    const textItems = [{ str: 'Hello ' }, { str: 'World' }];

    mockGetDocument.mockReturnValue({
      promise: Promise.resolve({
        numPages: 1,
        getPage: vi.fn().mockResolvedValue({
          getTextContent: vi.fn().mockResolvedValue({ items: textItems }),
        }),
        destroy: vi.fn(),
      }),
    } as unknown as ReturnType<typeof pdfjs.getDocument>);

    const file = makePdfFile('doc.pdf');
    const embedFn = vi.fn().mockResolvedValue([[0.1, 0.2, 0.3]]);

    const result = await processPdf(file, 'f1', embedFn);

    expect(result).toHaveLength(1);
    expect(result[0]).toEqual({
      id: 'f1-chunk-0',
      fileId: 'f1',
      fileName: 'doc.pdf',
      text: 'Hello  World',
      pageNumber: 1,
      embedding: [0.1, 0.2, 0.3],
    });
    expect(embedFn).toHaveBeenCalledWith(['Hello  World']);
  });

  it('throws on embedding count mismatch', async () => {
    const pdfjs = await import('pdfjs-dist');
    const mockGetDocument = vi.mocked(pdfjs.getDocument);

    mockGetDocument.mockReturnValue({
      promise: Promise.resolve({
        numPages: 1,
        getPage: vi.fn().mockResolvedValue({
          getTextContent: vi.fn().mockResolvedValue({
            items: [{ str: 'Some text' }],
          }),
        }),
        destroy: vi.fn(),
      }),
    } as unknown as ReturnType<typeof pdfjs.getDocument>);

    const file = makePdfFile('test.pdf');
    // Return wrong number of embeddings
    const embedFn = vi.fn().mockResolvedValue([[0.1], [0.2]]);

    await expect(processPdf(file, 'f1', embedFn)).rejects.toThrow('Embedding count mismatch');
  });
});
