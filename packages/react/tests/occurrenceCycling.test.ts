import { describe, it, expect, beforeEach } from 'vitest';
import type { StoreApi } from 'zustand/vanilla';
import type { AnalyzeResult } from '@pondpilot/flowscope-core';

import { createLineageStore, type LineageState } from '../src/store';

function buildResult(): AnalyzeResult {
  // Two nodes: a table with three name occurrences, and a CTE with one. The
  // CTE node lets us cover the "<2 spans" no-op branch.
  return {
    statements: [
      {
        statementIndex: 0,
        statementType: 'SELECT',
        joinCount: 0,
        complexityScore: 1,
      },
    ],
    nodes: [
      {
        id: 'table:users',
        type: 'table',
        label: 'users',
        statementIds: [0],
        span: { start: 14, end: 19 },
        nameSpans: [
          { start: 14, end: 19 },
          { start: 30, end: 35 },
          { start: 60, end: 65 },
        ],
      },
      {
        id: 'cte:active',
        type: 'cte',
        label: 'active',
        statementIds: [0],
        span: { start: 5, end: 11 },
        nameSpans: [{ start: 5, end: 11 }],
        bodySpan: { start: 15, end: 60 },
      },
    ],
    edges: [],
    issues: [],
    summary: {
      statementCount: 1,
      tableCount: 1,
      columnCount: 0,
      joinCount: 0,
      complexityScore: 1,
      issueCount: { errors: 0, warnings: 0, infos: 0 },
      hasErrors: false,
    },
  };
}

function buildMultiStatementResult(): AnalyzeResult {
  return {
    statements: [
      {
        statementIndex: 0,
        statementType: 'SELECT',
        sourceName: 'models/users_a.sql',
        joinCount: 0,
        complexityScore: 1,
      },
      {
        statementIndex: 1,
        statementType: 'SELECT',
        sourceName: 'models/users_b.sql',
        joinCount: 0,
        complexityScore: 1,
      },
    ],
    // In the flat model a node referenced by two statements appears once with
    // statementIds listing both. We emit two distinct Node instances here to
    // preserve the original test's per-statement `span` / `nameSpans` so the
    // merge logic has two occurrences to cycle through.
    nodes: [
      {
        id: 'table:users',
        type: 'table',
        label: 'users',
        statementIds: [0],
        span: { start: 10, end: 15 },
        nameSpans: [{ start: 10, end: 15 }],
      },
      {
        id: 'table:users',
        type: 'table',
        label: 'users',
        statementIds: [1],
        span: { start: 40, end: 45 },
        nameSpans: [
          { start: 40, end: 45 },
          { start: 70, end: 75 },
        ],
      },
    ],
    edges: [],
    issues: [],
    summary: {
      statementCount: 2,
      tableCount: 1,
      columnCount: 0,
      joinCount: 0,
      complexityScore: 1,
      issueCount: { errors: 0, warnings: 0, infos: 0 },
      hasErrors: false,
    },
  };
}

function buildSharedNodeResult(): AnalyzeResult {
  return {
    statements: [
      {
        statementIndex: 0,
        statementType: 'SELECT',
        sourceName: 'models/users_a.sql',
        joinCount: 0,
        complexityScore: 1,
      },
      {
        statementIndex: 1,
        statementType: 'SELECT',
        sourceName: 'models/users_b.sql',
        joinCount: 0,
        complexityScore: 1,
      },
    ],
    nodes: [
      {
        id: 'table:users',
        type: 'table',
        label: 'users',
        statementIds: [0, 1],
        span: { start: 10, end: 15 },
        nameSpans: [
          { start: 10, end: 15 },
          { start: 40, end: 45 },
        ],
      },
    ],
    edges: [],
    issues: [],
    summary: {
      statementCount: 2,
      tableCount: 1,
      columnCount: 0,
      joinCount: 0,
      complexityScore: 1,
      issueCount: { errors: 0, warnings: 0, infos: 0 },
      hasErrors: false,
    },
  };
}

describe('occurrence cycling', () => {
  let store: StoreApi<LineageState>;

  beforeEach(() => {
    store = createLineageStore();
    store.getState().setResult(buildResult());
  });

  it('selecting a node resets focusedOccurrenceIndex to 0', () => {
    store.getState().selectNode('table:users');
    expect(store.getState().focusedOccurrenceIndex).toBe(0);
  });

  it('cycleOccurrence("next") advances and updates highlightedSpan', () => {
    store.getState().selectNode('table:users');
    store.getState().cycleOccurrence('next');

    const state = store.getState();
    expect(state.focusedOccurrenceIndex).toBe(1);
    expect(state.highlightedSpan).toEqual({ start: 30, end: 35 });
  });

  it('cycleOccurrence wraps forward past the last occurrence', () => {
    store.getState().selectNode('table:users');
    store.getState().cycleOccurrence('next');
    store.getState().cycleOccurrence('next');
    store.getState().cycleOccurrence('next'); // wraps back to 0

    const state = store.getState();
    expect(state.focusedOccurrenceIndex).toBe(0);
    expect(state.highlightedSpan).toEqual({ start: 14, end: 19 });
  });

  it('cycleOccurrence("prev") wraps backward from index 0', () => {
    store.getState().selectNode('table:users');
    store.getState().cycleOccurrence('prev');

    const state = store.getState();
    expect(state.focusedOccurrenceIndex).toBe(2);
    expect(state.highlightedSpan).toEqual({ start: 60, end: 65 });
  });

  it('is a no-op when the selected node has fewer than 2 nameSpans', () => {
    store.getState().selectNode('cte:active');
    const before = store.getState();
    store.getState().cycleOccurrence('next');
    const after = store.getState();

    expect(after.focusedOccurrenceIndex).toBe(before.focusedOccurrenceIndex);
    expect(after.highlightedSpan).toBe(before.highlightedSpan);
  });

  it('is a no-op when no node is selected', () => {
    const before = store.getState();
    store.getState().cycleOccurrence('next');
    const after = store.getState();

    expect(after.focusedOccurrenceIndex).toBe(before.focusedOccurrenceIndex);
    expect(after.highlightedSpan).toBe(before.highlightedSpan);
  });

  it('focusOccurrence sets a specific index and highlights the matching span', () => {
    store.getState().selectNode('table:users');
    store.getState().focusOccurrence(2);

    const state = store.getState();
    expect(state.focusedOccurrenceIndex).toBe(2);
    expect(state.highlightedSpan).toEqual({ start: 60, end: 65 });
  });

  it('focusOccurrence ignores out-of-range indices', () => {
    store.getState().selectNode('table:users');
    store.getState().focusOccurrence(99);

    const state = store.getState();
    expect(state.focusedOccurrenceIndex).toBe(0);
    // selectNode leaves highlightedSpan untouched (null on a fresh store);
    // a rejected focusOccurrence must not alter that.
    expect(state.highlightedSpan ?? null).toBe(null);
  });

  it('cycles across occurrences merged from multiple statements', () => {
    store.getState().setResult(buildMultiStatementResult());
    store.getState().selectNode('table:users');

    store.getState().cycleOccurrence('next');
    expect(store.getState().highlightedSpan).toEqual({ start: 40, end: 45 });

    store.getState().cycleOccurrence('next');
    const state = store.getState();
    expect(state.focusedOccurrenceIndex).toBe(2);
    expect(state.highlightedSpan).toEqual({ start: 70, end: 75 });
  });

  it('does not duplicate occurrences when a flat node is already shared across statements', () => {
    store.getState().setResult(buildSharedNodeResult());
    store.getState().selectNode('table:users');

    store.getState().cycleOccurrence('next');
    expect(store.getState().highlightedSpan).toEqual({ start: 40, end: 45 });

    store.getState().cycleOccurrence('next');
    const state = store.getState();
    expect(state.focusedOccurrenceIndex).toBe(0);
    expect(state.highlightedSpan).toEqual({ start: 10, end: 15 });
  });
});
