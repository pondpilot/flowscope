import { describe, expect, it } from 'vitest';

import type { ChatMessage } from '../types';
import {
  DEFAULT_LIBRARIAN_SYSTEM_PROMPT,
  buildContext,
  buildPrompt,
  getPromptStats,
} from '../services/context-builder';

function makeMessage(role: 'user' | 'assistant', content: string): ChatMessage {
  return { id: `msg-${Date.now()}`, role, content, timestamp: Date.now() };
}

describe('buildContext', () => {
  it('passes through lineage and pdfCitations unchanged', () => {
    const ctx = buildContext({
      lineage: 'some lineage',
      pdfCitations: 'citation text',
      chatHistory: [],
      sqlSnippet: '',
    });
    expect(ctx.lineage).toBe('some lineage');
    expect(ctx.pdfCitations).toBe('citation text');
  });

  it('formats chat history as role: content lines', () => {
    const ctx = buildContext({
      lineage: '',
      pdfCitations: '',
      chatHistory: [makeMessage('user', 'Hello'), makeMessage('assistant', 'Hi there')],
      sqlSnippet: '',
    });
    expect(ctx.chatHistory).toContain('user: Hello');
    expect(ctx.chatHistory).toContain('assistant: Hi there');
  });

  it('returns empty chatHistory for no messages', () => {
    const ctx = buildContext({
      lineage: '',
      pdfCitations: '',
      chatHistory: [],
      sqlSnippet: '',
    });
    expect(ctx.chatHistory).toBe('');
  });

  it('truncates SQL beyond 3000 characters', () => {
    const longSql = 'SELECT ' + 'x'.repeat(3000);
    expect(longSql.length).toBeGreaterThan(3000);
    const ctx = buildContext({
      lineage: '',
      pdfCitations: '',
      chatHistory: [],
      sqlSnippet: longSql,
    });
    expect(ctx.sqlSnippet).toContain('... (truncated)');
    // First 3000 chars are kept, then the suffix is appended
    expect(ctx.sqlSnippet.startsWith(longSql.slice(0, 3000))).toBe(true);
    expect(ctx.sqlSnippet.endsWith('... (truncated)')).toBe(true);
  });

  it('does not truncate SQL within 3000 characters', () => {
    const shortSql = 'SELECT 1';
    const ctx = buildContext({
      lineage: '',
      pdfCitations: '',
      chatHistory: [],
      sqlSnippet: shortSql,
    });
    expect(ctx.sqlSnippet).toBe(shortSql);
  });
});

describe('buildPrompt', () => {
  it('includes system instruction', () => {
    const prompt = buildPrompt({
      lineage: '',
      pdfCitations: '',
      chatHistory: '',
      sqlSnippet: '',
    });
    expect(prompt).toContain('expert on SQL lineage and data flow');
  });

  it('uses a custom system prompt when provided', () => {
    const prompt = buildPrompt(
      {
        lineage: 'lineage data',
        pdfCitations: '',
        chatHistory: '',
        sqlSnippet: '',
      },
      { systemPrompt: 'Custom instructions' }
    );
    expect(prompt).toContain('Custom instructions');
    expect(prompt).not.toContain(DEFAULT_LIBRARIAN_SYSTEM_PROMPT);
    expect(prompt).toContain('lineage data');
  });

  it('falls back to the default prompt when the custom prompt is blank', () => {
    const prompt = buildPrompt(
      {
        lineage: '',
        pdfCitations: '',
        chatHistory: '',
        sqlSnippet: '',
      },
      { systemPrompt: '   ' }
    );
    expect(prompt).toContain('expert on SQL lineage and data flow');
  });

  it('includes lineage section when present', () => {
    const prompt = buildPrompt({
      lineage: 'table: users',
      pdfCitations: '',
      chatHistory: '',
      sqlSnippet: '',
    });
    expect(prompt).toContain('## DATA SOURCE: Data Lineage (from SQL analysis)');
    expect(prompt).toContain('table: users');
  });

  it('includes SQL section when present', () => {
    const prompt = buildPrompt({
      lineage: '',
      pdfCitations: '',
      chatHistory: '',
      sqlSnippet: 'SELECT 1',
    });
    expect(prompt).toContain('## DATA SOURCE: SQL Code (from SQL analysis)');
    expect(prompt).toContain('SELECT 1');
  });

  it('includes documentation section when present', () => {
    const prompt = buildPrompt({
      lineage: '',
      pdfCitations: 'From doc.pdf page 3: ...',
      chatHistory: '',
      sqlSnippet: '',
    });
    expect(prompt).toContain('## DATA SOURCE: Documentation (from uploaded PDFs)');
    expect(prompt).toContain('From doc.pdf page 3');
  });

  it('includes conversation history when present', () => {
    const prompt = buildPrompt({
      lineage: '',
      pdfCitations: '',
      chatHistory: 'user: hi\nassistant: hello',
      sqlSnippet: '',
    });
    expect(prompt).toContain('## Conversation History');
    expect(prompt).toContain('user: hi');
  });

  it('omits empty sections', () => {
    const prompt = buildPrompt({
      lineage: '',
      pdfCitations: '',
      chatHistory: '',
      sqlSnippet: '',
    });
    expect(prompt).not.toContain('## DATA SOURCE: Data Lineage');
    expect(prompt).not.toContain('## DATA SOURCE: SQL Code');
    expect(prompt).not.toContain('## DATA SOURCE: Documentation');
    expect(prompt).not.toContain('## Conversation History');
  });

  it('includes all sections when all present', () => {
    const prompt = buildPrompt({
      lineage: 'lineage data',
      pdfCitations: 'pdf data',
      chatHistory: 'chat data',
      sqlSnippet: 'sql data',
    });
    expect(prompt).toContain('## DATA SOURCE: Data Lineage (from SQL analysis)');
    expect(prompt).toContain('## DATA SOURCE: SQL Code (from SQL analysis)');
    expect(prompt).toContain('## DATA SOURCE: Documentation (from uploaded PDFs)');
    expect(prompt).toContain('## Conversation History');
  });

  it('includes source attribution instructions in system prompt', () => {
    const prompt = buildPrompt({
      lineage: 'lineage data',
      pdfCitations: 'pdf data',
      chatHistory: '',
      sqlSnippet: '',
    });
    expect(prompt).toContain('answer from Data Lineage and SQL Code sources ONLY');
    expect(prompt).toContain('answer from Documentation ONLY');
    expect(prompt).toContain('Never mix sources between sections');
  });

  it('includes the off-topic refusal rule with the exact canned response', () => {
    const prompt = buildPrompt({
      lineage: '',
      pdfCitations: '',
      chatHistory: '',
      sqlSnippet: '',
    });
    expect(prompt).toContain('Politely decline off-topic questions');
    expect(prompt).toContain('I can only answer questions related to your data.');
  });

  it('keeps the "no information" data fallback distinct from the off-topic refusal', () => {
    const prompt = buildPrompt({
      lineage: '',
      pdfCitations: '',
      chatHistory: '',
      sqlSnippet: '',
    });
    expect(prompt).toContain('If a source has no relevant information, write "No information"');
  });

  it('includes Summary format guidance with technical names in parentheses', () => {
    const prompt = buildPrompt({
      lineage: '',
      pdfCitations: '',
      chatHistory: '',
      sqlSnippet: '',
    });
    expect(prompt).toContain('document number (BELNR)');
  });

  it('instructs the model to render identifiers as inline code', () => {
    const prompt = buildPrompt({
      lineage: '',
      pdfCitations: '',
      chatHistory: '',
      sqlSnippet: '',
    });
    expect(prompt).toContain('Write table and column names as inline code');
  });
});

describe('getPromptStats', () => {
  it('returns character and byte counts', () => {
    expect(getPromptStats('abc')).toEqual({ characters: 3, bytes: 3 });
    expect(getPromptStats('é')).toEqual({ characters: 1, bytes: 2 });
  });
});
