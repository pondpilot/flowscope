import type { PdfChunk } from '../types';

/**
 * Compute cosine similarity between two vectors.
 * Returns 0 for zero-length vectors.
 */
export function cosineSimilarity(a: number[], b: number[]): number {
  if (a.length !== b.length || a.length === 0) return 0;

  let dot = 0;
  let normA = 0;
  let normB = 0;

  for (let i = 0; i < a.length; i += 1) {
    dot += a[i] * b[i];
    normA += a[i] * a[i];
    normB += b[i] * b[i];
  }

  const denominator = Math.sqrt(normA) * Math.sqrt(normB);
  if (denominator === 0) return 0;

  return dot / denominator;
}

/**
 * Search chunks by cosine similarity to the query embedding.
 * Returns the top-K chunks sorted by similarity descending.
 */
export function searchChunks(
  queryEmbedding: number[],
  chunks: PdfChunk[],
  topK: number
): PdfChunk[] {
  if (chunks.length === 0 || topK <= 0) return [];

  const scored = chunks.map((chunk) => ({
    chunk,
    score: cosineSimilarity(queryEmbedding, chunk.embedding),
  }));

  scored.sort((a, b) => b.score - a.score);

  return scored.slice(0, topK).map((s) => s.chunk);
}
