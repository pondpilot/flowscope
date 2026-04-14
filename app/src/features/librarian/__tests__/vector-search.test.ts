import { describe, expect, it } from 'vitest';

import type { PdfChunk } from '../types';
import { cosineSimilarity, searchChunks } from '../services/vector-search';

function makeChunk(id: string, embedding: number[]): PdfChunk {
  return {
    id,
    fileId: 'file-1',
    fileName: 'test.pdf',
    text: `chunk ${id}`,
    pageNumber: 1,
    embedding,
  };
}

describe('cosineSimilarity', () => {
  it('returns 1 for identical unit vectors', () => {
    const v = [1, 0, 0];
    expect(cosineSimilarity(v, v)).toBeCloseTo(1);
  });

  it('returns 0 for orthogonal vectors', () => {
    expect(cosineSimilarity([1, 0], [0, 1])).toBeCloseTo(0);
  });

  it('returns -1 for opposite vectors', () => {
    expect(cosineSimilarity([1, 0], [-1, 0])).toBeCloseTo(-1);
  });

  it('returns 0 for empty vectors', () => {
    expect(cosineSimilarity([], [])).toBe(0);
  });

  it('returns 0 for mismatched lengths', () => {
    expect(cosineSimilarity([1, 2], [1, 2, 3])).toBe(0);
  });

  it('returns 0 for zero vectors', () => {
    expect(cosineSimilarity([0, 0, 0], [1, 2, 3])).toBe(0);
  });

  it('computes correct similarity for non-unit vectors', () => {
    // [3, 4] and [4, 3]: dot=24, |a|=5, |b|=5 => 24/25 = 0.96
    expect(cosineSimilarity([3, 4], [4, 3])).toBeCloseTo(0.96);
  });
});

describe('searchChunks', () => {
  const chunks = [
    makeChunk('a', [1, 0, 0]),
    makeChunk('b', [0, 1, 0]),
    makeChunk('c', [0.9, 0.1, 0]),
    makeChunk('d', [0.5, 0.5, 0]),
  ];

  it('returns empty array for empty chunks', () => {
    expect(searchChunks([1, 0, 0], [], 5)).toEqual([]);
  });

  it('returns empty array for topK <= 0', () => {
    expect(searchChunks([1, 0, 0], chunks, 0)).toEqual([]);
    expect(searchChunks([1, 0, 0], chunks, -1)).toEqual([]);
  });

  it('returns top-K chunks sorted by similarity descending', () => {
    const result = searchChunks([1, 0, 0], chunks, 2);
    expect(result).toHaveLength(2);
    expect(result[0].id).toBe('a'); // exact match
    expect(result[1].id).toBe('c'); // close to [1,0,0]
  });

  it('returns all chunks when topK exceeds chunk count', () => {
    const result = searchChunks([1, 0, 0], chunks, 100);
    expect(result).toHaveLength(4);
    expect(result[0].id).toBe('a');
  });

  it('ranks by similarity correctly for different query', () => {
    const result = searchChunks([0, 1, 0], chunks, 2);
    expect(result[0].id).toBe('b'); // exact match to [0,1,0]
  });
});
