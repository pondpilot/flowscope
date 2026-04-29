import { describe, expect, it } from 'vitest';

import {
  CHAT_HISTORY_LIMIT,
  EMBEDDING_MODEL,
  MAX_MESSAGE_LENGTH,
  MAX_PDF_SIZE_BYTES,
  MAX_PDF_SIZE_MB,
  PDF_CHUNK_OVERLAP,
  PDF_CHUNK_SIZE,
  STORAGE_KEY_AI_API_KEY,
  STORAGE_KEY_AI_MODEL,
  STORAGE_KEY_AI_PROVIDER,
  VECTOR_SEARCH_TOP_K,
} from '../constants';

describe('constants', () => {
  it('CHAT_HISTORY_LIMIT is 10', () => {
    expect(CHAT_HISTORY_LIMIT).toBe(10);
  });

  it('MAX_MESSAGE_LENGTH is 4000', () => {
    expect(MAX_MESSAGE_LENGTH).toBe(4000);
  });

  it('MAX_PDF_SIZE_BYTES equals MAX_PDF_SIZE_MB * 1024 * 1024', () => {
    expect(MAX_PDF_SIZE_BYTES).toBe(MAX_PDF_SIZE_MB * 1024 * 1024);
  });

  it('PDF_CHUNK_OVERLAP is less than PDF_CHUNK_SIZE', () => {
    expect(PDF_CHUNK_OVERLAP).toBeLessThan(PDF_CHUNK_SIZE);
  });

  it('VECTOR_SEARCH_TOP_K is positive', () => {
    expect(VECTOR_SEARCH_TOP_K).toBeGreaterThan(0);
  });

  it('EMBEDDING_MODEL is a valid model identifier', () => {
    expect(EMBEDDING_MODEL).toBe('Xenova/multilingual-e5-small');
  });

  it('storage keys are unique strings', () => {
    const keys = [STORAGE_KEY_AI_PROVIDER, STORAGE_KEY_AI_API_KEY, STORAGE_KEY_AI_MODEL];
    expect(new Set(keys).size).toBe(keys.length);
    keys.forEach((key) => expect(typeof key).toBe('string'));
  });
});
