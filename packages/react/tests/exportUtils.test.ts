import { describe, it, expect } from 'vitest';
import type { AnalyzeResult } from '@pondpilot/flowscope-core';
import { generateMermaid } from '../src/utils/exportUtils';

describe('exportUtils', () => {
  it('surfaces wasm availability errors', async () => {
    const dummyResult = {
      statements: [],
      globalLineage: { nodes: [], edges: [] },
      issues: [],
      summary: {
        statementCount: 0,
        tableCount: 0,
        columnCount: 0,
        joinCount: 0,
        complexityScore: 0,
        issueCount: { errors: 0, warnings: 0, infos: 0 },
        hasErrors: false,
      },
    } satisfies AnalyzeResult;

    await expect(generateMermaid(dummyResult, 'table')).rejects.toThrow(/WASM/);
  });
});
